// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::mem::take;

use miette::Diagnostic;
use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    global::Table,
    hir::{
        BinOp, Block, Expr, ExprKind, Lit, Mutability, Package, Pat, Pauli, PrimField, QubitInit,
        QubitInitKind, Result, Stmt, StmtKind, UnOp,
    },
    mut_visit::{MutVisitor, walk_expr},
    ty::{GenericArg, Prim, Ty},
    visit::{self, Visitor},
};
use thiserror::Error;

use crate::CORE_NAMESPACE;
use crate::common::{IdentTemplate, LoopDepthScan, create_gen_core_ref, gen_ident};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_utils;

#[cfg(test)]
mod break_continue_tests;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error(
        "cannot desugar `break`/`continue` nested in a value of type `{0}` that has no classical default"
    )]
    #[diagnostic(code("Qdk.Qsc.LoopUnification.UnsupportedBreakContinueType"))]
    #[diagnostic(help(
        "on the branch that breaks or continues, this value is never produced, but its type has \
         no classical default to stand in for it while the surrounding effects are skipped. \
         Operand, `let`-binding, short-circuit-operand, and discarded value-block positions are \
         all supported; if you reach this, bind the value with a `let` or restructure so it is \
         not produced on the branch that breaks or continues"
    ))]
    UnsupportedType(
        String,
        #[label("break/continue in a value with no classical default")] Span,
    ),

    #[error("internal error: `break`/`continue` was not eliminated by loop desugaring")]
    #[diagnostic(code("Qdk.Qsc.LoopUnification.ResidualBreakContinue"))]
    #[diagnostic(help(
        "this indicates a compiler invariant was violated; every `break`/`continue` should be \
         rewritten to loop-flag writes during desugaring"
    ))]
    ResidualBreakContinue(#[label("`break`/`continue` survived loop desugaring")] Span),
}

pub(crate) struct LoopUni<'a> {
    pub(crate) core: &'a Table,
    pub(crate) assigner: &'a mut Assigner,
    pub(crate) errors: Vec<Error>,
}

impl LoopUni<'_> {
    /// Rewrites `repeat`/`until` into a `while` driven by `.continue_cond_<id>`.
    ///
    /// # Before
    /// ```text
    /// repeat { body } until cond fixup { fixup }
    /// ```
    ///
    /// # After
    /// ```text
    /// mutable .continue_cond_<id> = true;
    /// while .continue_cond_<id> {
    ///     body
    ///     .continue_cond_<id> = not cond;
    ///     if .continue_cond_<id> { fixup }
    /// }
    /// ```
    ///
    /// If `body` contains `break`, the condition and tail update are also
    /// guarded by the `not .broke_<id>` flag; if it contains `continue`, the tail still runs
    /// before the next condition check.
    #[allow(clippy::too_many_lines)]
    fn visit_repeat(
        &mut self,
        mut block: Block,
        cond: Box<Expr>,
        fixup: Option<Block>,
        span: Span,
    ) -> Expr {
        let cond_span = cond.span;
        let continue_cond_id = gen_ident(
            self.assigner,
            "continue_cond",
            Ty::Prim(Prim::Bool),
            cond_span,
        );
        let continue_cond_init = continue_cond_id.gen_id_init(
            Mutability::Mutable,
            Expr {
                id: self.assigner.next_node(),
                span: cond_span,
                ty: Ty::Prim(Prim::Bool),
                kind: ExprKind::Lit(Lit::Bool(true)),
            },
            self.assigner,
        );

        let flags = self.desugar_loop_body(&mut block);

        let update = Stmt {
            id: self.assigner.next_node(),
            span: cond_span,
            kind: StmtKind::Semi(Expr {
                id: self.assigner.next_node(),
                span: cond_span,
                ty: Ty::UNIT,
                kind: ExprKind::Assign(
                    Box::new(continue_cond_id.gen_local_ref(self.assigner)),
                    Box::new(Expr {
                        id: self.assigner.next_node(),
                        span: cond_span,
                        ty: Ty::Prim(Prim::Bool),
                        kind: ExprKind::UnOp(UnOp::NotL, cond),
                    }),
                ),
            }),
        };

        let fix_if = if let Some(fix_body) = fixup {
            Some(Stmt {
                id: self.assigner.next_node(),
                span: fix_body.span,
                kind: StmtKind::Expr(Expr {
                    id: self.assigner.next_node(),
                    span: fix_body.span,
                    ty: Ty::UNIT,
                    kind: ExprKind::If(
                        Box::new(continue_cond_id.gen_local_ref(self.assigner)),
                        Box::new(Expr {
                            id: self.assigner.next_node(),
                            span: fix_body.span,
                            ty: Ty::UNIT,
                            kind: ExprKind::Block(fix_body),
                        }),
                        None,
                    ),
                }),
            })
        } else {
            None
        };

        let new_block = match flags {
            None => {
                block.stmts.push(update);
                if let Some(fix_if) = fix_if {
                    block.stmts.push(fix_if);
                }
                Block {
                    id: self.assigner.next_node(),
                    span,
                    ty: Ty::UNIT,
                    stmts: vec![
                        continue_cond_init,
                        Stmt {
                            id: self.assigner.next_node(),
                            span: Span::default(),
                            kind: StmtKind::Expr(Expr {
                                id: self.assigner.next_node(),
                                span,
                                ty: Ty::UNIT,
                                kind: ExprKind::While(
                                    Box::new(continue_cond_id.gen_local_ref(self.assigner)),
                                    block,
                                ),
                            }),
                        },
                    ],
                }
            }
            Some(flags) => {
                // `.cont_<id>` resets each iteration; declare it as the first body statement.
                if let Some(cont_decl) = flags.cont_decl {
                    block.stmts.insert(0, cont_decl);
                }
                // The tail, the `until`/`fixup` step, runs on `continue`; when a
                // `break` is present it is skipped on `break` under
                // `if not .broke_<id> { ... }`, otherwise it runs unconditionally.
                if let Some(broke) = &flags.broke {
                    let mut tail_stmts = vec![update];
                    if let Some(fix_if) = fix_if {
                        tail_stmts.push(fix_if);
                    }
                    let tail_block = Block {
                        id: self.assigner.next_node(),
                        span: Span::default(),
                        ty: Ty::UNIT,
                        stmts: tail_stmts,
                    };
                    let tail_if = self.gen_broke_guarded_block(broke, tail_block);
                    block.stmts.push(tail_if);
                } else {
                    block.stmts.push(update);
                    if let Some(fix_if) = fix_if {
                        block.stmts.push(fix_if);
                    }
                }
                let cc_ref = continue_cond_id.gen_local_ref(self.assigner);
                let while_cond = match &flags.broke {
                    Some(broke) => self.gen_broke_guard_cond(broke, cc_ref),
                    None => cc_ref,
                };
                let while_stmt = Stmt {
                    id: self.assigner.next_node(),
                    span: Span::default(),
                    kind: StmtKind::Expr(Expr {
                        id: self.assigner.next_node(),
                        span,
                        ty: Ty::UNIT,
                        kind: ExprKind::While(Box::new(while_cond), block),
                    }),
                };
                let mut stmts = vec![continue_cond_init];
                if let Some(broke_decl) = flags.broke_decl {
                    stmts.push(broke_decl);
                }
                stmts.push(while_stmt);
                Block {
                    id: self.assigner.next_node(),
                    span,
                    ty: Ty::UNIT,
                    stmts,
                }
            }
        };

        Expr {
            id: self.assigner.next_node(),
            span,
            ty: Ty::UNIT,
            kind: ExprKind::Block(new_block),
        }
    }

    /// Rewrites an array `for` loop to an index-driven `while` loop.
    ///
    /// # Before
    /// ```text
    /// for pat in array { body }
    /// ```
    ///
    /// # After
    /// ```text
    /// let .array_id_<id> = array;
    /// let .len_id_<id> = Length(.array_id_<id>);
    /// mutable .index_id_<id> = 0;
    /// while .index_id_<id> < .len_id_<id> {
    ///     let pat = .array_id_<id>[.index_id_<id>];
    ///     body
    ///     .index_id_<id> += 1;
    /// }
    /// ```
    ///
    /// A `break` adds a persistent `.broke_<id>` flag that guards the condition and
    /// step; a `continue` adds a per-iteration `.cont_<id>` flag that skips the rest
    /// of the body but still runs the step.
    #[allow(clippy::too_many_lines)]
    fn visit_for_array(
        &mut self,
        iter: Pat,
        iterable: Box<Expr>,
        mut block: Block,
        span: Span,
    ) -> Expr {
        let iterable_span = iterable.span;

        let flags = self.desugar_loop_body(&mut block);

        let array_id = gen_ident(
            self.assigner,
            "array_id",
            iterable.ty.clone(),
            iterable_span,
        );
        let array_capture = array_id.gen_id_init(Mutability::Immutable, *iterable, self.assigner);

        let item_ty = match &array_id.ty {
            Ty::Array(inner) => (**inner).clone(),
            // If the type is not array, this is likely the special case where a short-circuiting expression is the iterable
            // and the type is thus unknown. In that case, we can just use the type of the iteration variable pattern.
            _ => iter.ty.clone(),
        };
        let ns = self
            .core
            .find_namespace(CORE_NAMESPACE.iter().copied())
            .expect("prelude namespaces should exist");
        let mut len_callee = create_gen_core_ref(
            self.core,
            ns,
            "Length",
            vec![GenericArg::Ty(item_ty)],
            array_id.span,
        );
        len_callee.id = self.assigner.next_node();
        let len_id = gen_ident(self.assigner, "len_id", Ty::Prim(Prim::Int), iterable_span);
        let len_capture = len_id.gen_id_init(
            Mutability::Immutable,
            Expr {
                id: self.assigner.next_node(),
                span: array_id.span,
                ty: Ty::Prim(Prim::Int),
                kind: ExprKind::Call(
                    Box::new(len_callee),
                    Box::new(array_id.gen_local_ref(self.assigner)),
                ),
            },
            self.assigner,
        );

        let index_id = gen_ident(
            self.assigner,
            "index_id",
            Ty::Prim(Prim::Int),
            iterable_span,
        );
        let index_init = index_id.gen_steppable_id_init(
            Mutability::Mutable,
            Expr {
                id: self.assigner.next_node(),
                span: iterable_span,
                ty: Ty::Prim(Prim::Int),
                kind: ExprKind::Lit(Lit::Int(0)),
            },
            self.assigner,
        );

        let pat_ty = iter.ty.clone();
        let pat_init = Stmt {
            id: self.assigner.next_node(),
            span: iter.span,
            kind: StmtKind::Local(
                Mutability::Immutable,
                iter,
                Expr {
                    id: self.assigner.next_node(),
                    span: iterable_span,
                    ty: pat_ty,
                    kind: ExprKind::Index(
                        Box::new(array_id.gen_local_ref(self.assigner)),
                        Box::new(index_id.gen_local_ref(self.assigner)),
                    ),
                },
            ),
        };

        let update_expr = Expr {
            id: self.assigner.next_node(),
            span: iterable_span,
            ty: Ty::Prim(Prim::Int),
            kind: ExprKind::Lit(Lit::Int(1)),
        };
        let update_index = gen_id_add_update(self.assigner, &index_id, update_expr);

        let cond = Expr {
            id: self.assigner.next_node(),
            span: iterable_span,
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::BinOp(
                BinOp::Lt,
                Box::new(index_id.gen_local_ref(self.assigner)),
                Box::new(len_id.gen_local_ref(self.assigner)),
            ),
        };

        let new_block_stmts = match flags {
            None => {
                block.stmts.insert(0, pat_init);
                block.stmts.push(update_index);
                let while_stmt = Stmt {
                    id: self.assigner.next_node(),
                    span: Span::default(),
                    kind: StmtKind::Expr(Expr {
                        id: self.assigner.next_node(),
                        span,
                        ty: Ty::UNIT,
                        kind: ExprKind::While(Box::new(cond), block),
                    }),
                };
                vec![array_capture, len_capture, index_init, while_stmt]
            }
            Some(flags) => {
                // `.cont_<id>` resets each iteration; declare it before the loop variable.
                block.stmts.insert(0, pat_init);
                if let Some(cont_decl) = flags.cont_decl {
                    block.stmts.insert(0, cont_decl);
                }
                // The index step runs on `continue` but is skipped on `break`.
                let step_stmt = match &flags.broke {
                    Some(broke) => self.gen_broke_guarded_step(broke, update_index),
                    None => update_index,
                };
                block.stmts.push(step_stmt);
                let while_cond = match &flags.broke {
                    Some(broke) => self.gen_broke_guard_cond(broke, cond),
                    None => cond,
                };
                let while_stmt = Stmt {
                    id: self.assigner.next_node(),
                    span: Span::default(),
                    kind: StmtKind::Expr(Expr {
                        id: self.assigner.next_node(),
                        span,
                        ty: Ty::UNIT,
                        kind: ExprKind::While(Box::new(while_cond), block),
                    }),
                };
                let mut stmts = vec![array_capture, len_capture, index_init];
                if let Some(broke_decl) = flags.broke_decl {
                    stmts.push(broke_decl);
                }
                stmts.push(while_stmt);
                stmts
            }
        };

        Expr {
            id: self.assigner.next_node(),
            span,
            ty: Ty::UNIT,
            kind: ExprKind::Block(Block {
                id: self.assigner.next_node(),
                span,
                ty: Ty::UNIT,
                stmts: new_block_stmts,
            }),
        }
    }

    /// Rewrites a range `for` loop to an index-driven `while` loop.
    ///
    /// # Before
    /// ```text
    /// for pat in start..step..end { body }
    /// ```
    ///
    /// # After
    /// ```text
    /// let .range_id_<id> = start..step..end;
    /// mutable .index_id_<id> = .range_id_<id>::Start;
    /// let .step_id_<id> = .range_id_<id>::Step;
    /// let .end_id_<id> = .range_id_<id>::End;
    /// while range_cond(.index_id_<id>, .step_id_<id>, .end_id_<id>) {
    ///     let pat = .index_id_<id>;
    ///     body
    ///     .index_id_<id> += .step_id_<id>;
    /// }
    /// ```
    ///
    /// Uses the same `.broke_<id>`/`.cont_<id>` guards as array loops.
    #[allow(clippy::too_many_lines)]
    fn visit_for_range(
        &mut self,
        iter: Pat,
        iterable: Box<Expr>,
        mut block: Block,
        span: Span,
    ) -> Expr {
        let iterable_span = iterable.span;

        let flags = self.desugar_loop_body(&mut block);

        let range_id = gen_ident(
            self.assigner,
            "range_id",
            Ty::Prim(Prim::Range),
            iterable_span,
        );
        let range_capture = range_id.gen_id_init(Mutability::Immutable, *iterable, self.assigner);

        let index_id = gen_ident(
            self.assigner,
            "index_id",
            Ty::Prim(Prim::Int),
            iterable_span,
        );
        let index_init = index_id.gen_steppable_id_init(
            Mutability::Mutable,
            range_id.gen_field_access(PrimField::Start, self.assigner),
            self.assigner,
        );

        let step_id = gen_ident(self.assigner, "step_id", Ty::Prim(Prim::Int), iterable_span);
        let step_init = step_id.gen_id_init(
            Mutability::Immutable,
            range_id.gen_field_access(PrimField::Step, self.assigner),
            self.assigner,
        );

        let end_id = gen_ident(self.assigner, "end_id", Ty::Prim(Prim::Int), iterable_span);
        let end_init = end_id.gen_id_init(
            Mutability::Immutable,
            range_id.gen_field_access(PrimField::End, self.assigner),
            self.assigner,
        );

        let pat_init = Stmt {
            id: self.assigner.next_node(),
            span: iter.span,
            kind: StmtKind::Local(
                Mutability::Immutable,
                iter,
                index_id.gen_local_ref(self.assigner),
            ),
        };

        let update_expr = step_id.gen_local_ref(self.assigner);
        let update_index = gen_id_add_update(self.assigner, &index_id, update_expr);

        let cond = gen_range_cond(self.assigner, &index_id, &step_id, &end_id, iterable_span);

        let new_block_stmts = match flags {
            None => {
                block.stmts.insert(0, pat_init);
                block.stmts.push(update_index);
                let while_stmt = Stmt {
                    id: self.assigner.next_node(),
                    span: Span::default(),
                    kind: StmtKind::Expr(Expr {
                        id: self.assigner.next_node(),
                        span,
                        ty: Ty::UNIT,
                        kind: ExprKind::While(Box::new(cond), block),
                    }),
                };
                vec![range_capture, index_init, step_init, end_init, while_stmt]
            }
            Some(flags) => {
                // `.cont_<id>` resets each iteration; declare it before the loop variable.
                block.stmts.insert(0, pat_init);
                if let Some(cont_decl) = flags.cont_decl {
                    block.stmts.insert(0, cont_decl);
                }
                // The index step runs on `continue` but is skipped on `break`.
                let step_stmt = match &flags.broke {
                    Some(broke) => self.gen_broke_guarded_step(broke, update_index),
                    None => update_index,
                };
                block.stmts.push(step_stmt);
                let while_cond = match &flags.broke {
                    Some(broke) => self.gen_broke_guard_cond(broke, cond),
                    None => cond,
                };
                let while_stmt = Stmt {
                    id: self.assigner.next_node(),
                    span: Span::default(),
                    kind: StmtKind::Expr(Expr {
                        id: self.assigner.next_node(),
                        span,
                        ty: Ty::UNIT,
                        kind: ExprKind::While(Box::new(while_cond), block),
                    }),
                };
                let mut stmts = vec![range_capture, index_init, step_init, end_init];
                if let Some(broke_decl) = flags.broke_decl {
                    stmts.push(broke_decl);
                }
                stmts.push(while_stmt);
                stmts
            }
        };

        Expr {
            id: self.assigner.next_node(),
            span,
            ty: Ty::UNIT,
            kind: ExprKind::Block(Block {
                id: self.assigner.next_node(),
                span,
                ty: Ty::UNIT,
                stmts: new_block_stmts,
            }),
        }
    }

    /// Rebuilds a plain `while` whose body contains `break`/`continue`.
    ///
    /// # Before
    /// ```text
    /// while cond { body }
    /// ```
    ///
    /// # After
    /// ```text
    /// mutable .broke_<id> = false;
    /// while not .broke_<id> and cond { body' }
    /// ```
    ///
    /// `body'` is the body after [`desugar_loop_body`]. Continue-only loops do
    /// not need `.broke_<id>`, so they stay as `while cond { body' }`; a plain
    /// `while` has no loop step, so `continue` falls through to the next
    /// condition check.
    fn visit_while(&mut self, cond: Box<Expr>, mut block: Block, span: Span) -> Expr {
        let flags = self
            .desugar_loop_body(&mut block)
            .expect("while body should contain break/continue when rebuilt");
        if let Some(cont_decl) = flags.cont_decl {
            block.stmts.insert(0, cont_decl);
        }
        let while_cond = match &flags.broke {
            Some(broke) => self.gen_broke_guard_cond(broke, *cond),
            None => *cond,
        };
        match flags.broke_decl {
            Some(broke_decl) => {
                let while_stmt = Stmt {
                    id: self.assigner.next_node(),
                    span: Span::default(),
                    kind: StmtKind::Expr(Expr {
                        id: self.assigner.next_node(),
                        span,
                        ty: Ty::UNIT,
                        kind: ExprKind::While(Box::new(while_cond), block),
                    }),
                };
                Expr {
                    id: self.assigner.next_node(),
                    span,
                    ty: Ty::UNIT,
                    kind: ExprKind::Block(Block {
                        id: self.assigner.next_node(),
                        span,
                        ty: Ty::UNIT,
                        stmts: vec![broke_decl, while_stmt],
                    }),
                }
            }
            None => Expr {
                id: self.assigner.next_node(),
                span,
                ty: Ty::UNIT,
                kind: ExprKind::While(Box::new(while_cond), block),
            },
        }
    }

    /// Detects `break`/`continue` binding to this loop, creates only the needed
    /// `.broke_<id>`/`.cont_<id>` flags, and rewrites the body in place.
    ///
    /// # Before
    /// ```text
    /// break;
    /// stmt;
    /// continue;
    /// next;
    /// ```
    ///
    /// # After
    /// ```text
    /// .broke_<id> = true;
    /// if not .broke_<id> { stmt; }
    /// if not .broke_<id> { .cont_<id> = true; }
    /// if not .broke_<id> and not .cont_<id> { next; }
    /// ```
    ///
    /// # Mutations
    /// - Rewrites bare `break`/`continue` statements to flag assignments.
    /// - Guards statements that follow a loop-control statement.
    /// - Allocates only the flags required by the body.
    fn desugar_loop_body(&mut self, body: &mut Block) -> Option<LoopFlags> {
        let presence = body_break_continue(body);
        if !presence.any() {
            return None;
        }
        let (broke, broke_decl) = if presence.has_break {
            let (ident, decl) = self.gen_flag_decl("broke");
            (Some(ident), Some(decl))
        } else {
            (None, None)
        };
        let (cont, cont_decl) = if presence.has_continue {
            let (ident, decl) = self.gen_flag_decl("cont");
            (Some(ident), Some(decl))
        } else {
            (None, None)
        };
        let mut desugar_errors = {
            let mut desugar = BreakContinueDesugar {
                assigner: self.assigner,
                broke: broke.as_ref(),
                cont: cont.as_ref(),
                errors: Vec::new(),
            };
            desugar.visit_block(body);
            desugar.errors
        };
        self.errors.append(&mut desugar_errors);
        Some(LoopFlags {
            broke,
            broke_decl,
            cont_decl,
        })
    }

    /// Creates a `mutable .<label>_<id> = false;` declaration with a non-steppable
    /// span, returning the flag's [`IdentTemplate`] and its declaration.
    fn gen_flag_decl(&mut self, label: &str) -> (IdentTemplate, Stmt) {
        let ident = gen_ident(self.assigner, label, Ty::Prim(Prim::Bool), Span::default());
        let init = Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::Lit(Lit::Bool(false)),
        };
        let decl = ident.gen_id_init(Mutability::Mutable, init, self.assigner);
        (ident, decl)
    }

    /// Rewrites a loop condition to `(not .broke_<id>) and <cond>`. The synthetic
    /// `(not .broke_<id>) and` prefix is non-steppable; `cond` keeps its own span.
    fn gen_broke_guard_cond(&mut self, broke: &IdentTemplate, cond: Expr) -> Expr {
        let not_broke = gen_not_flag(self.assigner, broke);
        Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::BinOp(BinOp::AndL, Box::new(not_broke), Box::new(cond)),
        }
    }

    /// Wraps a single loop-step statement in `if not .broke_<id> { <step> }`. The
    /// step runs on `continue` but is skipped on `break`.
    fn gen_broke_guarded_step(&mut self, broke: &IdentTemplate, step: Stmt) -> Stmt {
        let block = Block {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::UNIT,
            stmts: vec![step],
        };
        self.gen_broke_guarded_block(broke, block)
    }

    /// Wraps a block in `if not .broke_<id> { <block> }` as a `StmtKind::Expr`. The
    /// guard `if` is non-steppable; the block's statements keep their spans.
    fn gen_broke_guarded_block(&mut self, broke: &IdentTemplate, block: Block) -> Stmt {
        let not_broke = gen_not_flag(self.assigner, broke);
        let block_expr = Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::UNIT,
            kind: ExprKind::Block(block),
        };
        let if_expr = Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::UNIT,
            kind: ExprKind::If(Box::new(not_broke), Box::new(block_expr), None),
        };
        Stmt {
            id: self.assigner.next_node(),
            span: Span::default(),
            kind: StmtKind::Expr(if_expr),
        }
    }
}

/// The flags and declarations produced when desugaring a loop body that
/// contains `break`/`continue`. Each field is minted only when its keyword is
/// present: `broke.is_some() == broke_decl.is_some()`, and both are `Some` iff
/// the body has a `break`, while `cont_decl.is_some()` iff the body has a
/// `continue`.
struct LoopFlags {
    broke: Option<IdentTemplate>,
    broke_decl: Option<Stmt>,
    cont_decl: Option<Stmt>,
}

impl MutVisitor for LoopUni<'_> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        walk_expr(self, expr);
        match take(&mut expr.kind) {
            ExprKind::Repeat(block, cond, fixup) => {
                *expr = self.visit_repeat(block, cond, fixup, expr.span);
            }
            ExprKind::For(iter, iterable, block) => {
                match iterable.ty {
                    Ty::Array(_) => *expr = self.visit_for_array(iter, iterable, block, expr.span),
                    Ty::Prim(Prim::Range) => {
                        *expr = self.visit_for_range(iter, iterable, block, expr.span);
                    }
                    Ty::Tuple(ref inner) if inner.is_empty() => {
                        // The type checking would only allow unit in here in the case where the iterable expression is
                        // short-circuiting (an explicit `fail` or `return`), so treat this as if it were an array
                        // of the type defined by the iteration variable.
                        *expr = self.visit_for_array(iter, iterable, block, expr.span);
                    }
                    a => {
                        // This scenario should have been caught by type-checking earlier
                        panic!(
                            "The type of the iterable must be either array or range, but it is an {a:?}"
                        )
                    }
                }
            }
            ExprKind::While(cond, block) => {
                // A plain `while` is rewritten only when its body contains
                // `break`/`continue`; otherwise it is left byte-for-byte
                // unchanged. A `while` has no loop step, so `continue` simply
                // falls through to the next condition check.
                if body_has_break_continue(&block) {
                    *expr = self.visit_while(cond, block, expr.span);
                } else {
                    expr.kind = ExprKind::While(cond, block);
                }
            }
            kind => expr.kind = kind,
        }
    }
}

fn gen_range_cond(
    assigner: &mut Assigner,
    index: &IdentTemplate,
    step: &IdentTemplate,
    end: &IdentTemplate,
    span: Span,
) -> Expr {
    Expr {
        id: assigner.next_node(),
        span,
        ty: Ty::Prim(Prim::Bool),
        kind: ExprKind::BinOp(
            BinOp::OrL,
            Box::new(Expr {
                id: assigner.next_node(),
                span,
                ty: Ty::Prim(Prim::Bool),
                kind: ExprKind::BinOp(
                    BinOp::AndL,
                    Box::new(Expr {
                        id: assigner.next_node(),
                        span,
                        ty: Ty::Prim(Prim::Bool),
                        kind: ExprKind::BinOp(
                            BinOp::Gt,
                            Box::new(step.gen_local_ref(assigner)),
                            Box::new(Expr {
                                id: assigner.next_node(),
                                span,
                                ty: Ty::Prim(Prim::Int),
                                kind: ExprKind::Lit(Lit::Int(0)),
                            }),
                        ),
                    }),
                    Box::new(Expr {
                        id: assigner.next_node(),
                        span,
                        ty: Ty::Prim(Prim::Bool),
                        kind: ExprKind::BinOp(
                            BinOp::Lte,
                            Box::new(index.gen_local_ref(assigner)),
                            Box::new(end.gen_local_ref(assigner)),
                        ),
                    }),
                ),
            }),
            Box::new(Expr {
                id: assigner.next_node(),
                span,
                ty: Ty::Prim(Prim::Bool),
                kind: ExprKind::BinOp(
                    BinOp::AndL,
                    Box::new(Expr {
                        id: assigner.next_node(),
                        span,
                        ty: Ty::Prim(Prim::Bool),
                        kind: ExprKind::BinOp(
                            BinOp::Lt,
                            Box::new(step.gen_local_ref(assigner)),
                            Box::new(Expr {
                                id: assigner.next_node(),
                                span,
                                ty: Ty::Prim(Prim::Int),
                                kind: ExprKind::Lit(Lit::Int(0)),
                            }),
                        ),
                    }),
                    Box::new(Expr {
                        id: assigner.next_node(),
                        span,
                        ty: Ty::Prim(Prim::Bool),
                        kind: ExprKind::BinOp(
                            BinOp::Gte,
                            Box::new(index.gen_local_ref(assigner)),
                            Box::new(end.gen_local_ref(assigner)),
                        ),
                    }),
                ),
            }),
        ),
    }
}

fn gen_id_add_update(assigner: &mut Assigner, ident: &IdentTemplate, expr: Expr) -> Stmt {
    Stmt {
        id: assigner.next_node(),
        span: ident.span,
        kind: StmtKind::Semi(Expr {
            id: assigner.next_node(),
            span: ident.span,
            ty: Ty::UNIT,
            kind: ExprKind::AssignOp(
                BinOp::Add,
                Box::new(ident.gen_local_ref(assigner)),
                Box::new(expr),
            ),
        }),
    }
}

/// Rewrites `break`/`continue` in a single loop body to `.broke_<id>`/`.cont_<id>` flag
/// writes, guarding the statements that follow a `break`/`continue` so they do
/// not run once the loop is exiting or skipping to the next iteration.
///
/// Nested loops are opaque: their own `break`/`continue` bind to them and were
/// already desugared, so this walker never descends into them. This covers
/// `while` as well as the `for`/`repeat` forms already lowered to `while` by
/// the post-order [`LoopUni`] traversal.
struct BreakContinueDesugar<'a> {
    assigner: &'a mut Assigner,
    broke: Option<&'a IdentTemplate>,
    cont: Option<&'a IdentTemplate>,
    errors: Vec<Error>,
}

impl MutVisitor for BreakContinueDesugar<'_> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        let is_break = match &expr.kind {
            ExprKind::Break => true,
            ExprKind::Continue => false,
            // Nested loops bind their own `break`/`continue`; do not descend.
            ExprKind::While(..) | ExprKind::For(..) | ExprKind::Repeat(..) => return,
            _ => {
                walk_expr(self, expr);
                return;
            }
        };
        // A `break`/`continue` reached here sits in operand or value position;
        // a statement-position one is intercepted by `visit_block`. Replace it
        // with a flag write that yields a value of its type.
        let ty = expr.ty.clone();
        let kw_span = expr.span;
        *expr = self.gen_flag_value(is_break, kw_span, &ty);
    }

    fn visit_block(&mut self, block: &mut Block) {
        let block_ty = block.ty.clone();
        let stmts = take(&mut block.stmts);
        let mut iter = stmts.into_iter().peekable();
        let mut out = Vec::with_capacity(iter.size_hint().0 + 1);
        let mut seen_control_flow = false;
        while let Some(mut stmt) = iter.next() {
            let is_last = iter.peek().is_none();
            let fires = stmt_escapes(&stmt);
            let always_fires = stmt_always_escapes(&stmt);
            let bare = bare_break_continue(&stmt);
            if let Some((is_break, kw_span)) = bare {
                let flag = if is_break {
                    self.broke.expect("break implies a broke flag was minted")
                } else {
                    self.cont.expect("continue implies a cont flag was minted")
                };
                let flag_set = gen_flag_set_stmt(self.assigner, flag, kw_span);
                let flag_set = if seen_control_flow {
                    self.guard_stmt(flag_set)
                } else {
                    flag_set
                };
                out.push(flag_set);
                // A value-block whose trailing value was a bare `break`/
                // `continue` still needs to produce a value on the exit path.
                if is_last && block_ty != Ty::UNIT {
                    let default =
                        build_default_or_err(self.assigner, &mut self.errors, &block_ty, kw_span);
                    out.push(expr_stmt(self.assigner, default));
                }
            } else {
                if seen_control_flow && requires_suffix_relocation(&stmt) {
                    // A qubit allocation or a non-defaultable `let` binding cannot
                    // be guarded in place after a flag set: the qubit's binding
                    // scope must stay intact, and a non-defaultable binding has no
                    // classical default to seed the divergence path. Relocate this
                    // statement and the rest of the block into a single guarded
                    // block so the binding runs only on the fall-through path.
                    let mut suffix_stmts = Vec::with_capacity(iter.size_hint().0 + 1);
                    suffix_stmts.push(stmt);
                    suffix_stmts.extend(iter);
                    out.push(self.guard_suffix_block(suffix_stmts, block_ty.clone()));
                    break;
                }
                self.visit_stmt(&mut stmt);
                let stmt = if seen_control_flow {
                    self.guard_stmt(stmt)
                } else {
                    stmt
                };
                out.push(stmt);
            }
            if always_fires && bare.is_none() {
                break;
            }
            seen_control_flow = seen_control_flow || fires;
        }
        block.stmts = out;
    }
}

impl BreakContinueDesugar<'_> {
    /// Guards a suffix that starts with a binding that cannot be guarded in
    /// place after a loop-control flag has been set: a qubit allocation, whose
    /// binding scope must stay intact, or a non-defaultable `let`, which has no
    /// classical default to seed the divergence path. The binding and every
    /// statement that can refer to it must remain in the same block, so
    /// `break; use q = Qubit(); Foo(q);` becomes
    /// `.broke_<id> = true; if not .broke_<id> { use q = Qubit(); Foo(q); }`.
    fn guard_suffix_block(&mut self, stmts: Vec<Stmt>, ty: Ty) -> Stmt {
        let mut block = Block {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: ty.clone(),
            stmts,
        };
        self.visit_block(&mut block);
        let then = Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: ty.clone(),
            kind: ExprKind::Block(block),
        };
        let els = if ty == Ty::UNIT {
            None
        } else {
            let default =
                build_default_or_err(self.assigner, &mut self.errors, &ty, Span::default());
            Some(block_wrap_expr(self.assigner, default, ty.clone()))
        };
        let guard_if = flag_guard_if(self.assigner, self.broke, self.cont, then, els, ty);
        expr_stmt(self.assigner, guard_if)
    }

    /// Wraps a statement that follows a `break`/`continue` so it runs only while
    /// neither flag is set: `if not .broke_<id> and not .cont_<id> { <stmt> }`.
    ///
    /// A `Local` keeps its binding visible to later statements, so its
    /// initializer is guarded instead of the whole statement; a value-position
    /// trailing expression is guarded with a defaulting `else` so the block
    /// still yields a value. Item statements are left as is. A qubit allocation
    /// after a flag set is handled earlier by [`Self::guard_suffix_block`] so
    /// its binding scope stays intact.
    fn guard_stmt(&mut self, stmt: Stmt) -> Stmt {
        match stmt.kind {
            StmtKind::Local(mutability, pat, init) => {
                let ty = init.ty.clone();
                let span = init.span;
                let default = build_default_or_err(self.assigner, &mut self.errors, &ty, span);
                let then = block_wrap_expr(self.assigner, init, ty.clone());
                let els = block_wrap_expr(self.assigner, default, ty.clone());
                let guarded =
                    flag_guard_if(self.assigner, self.broke, self.cont, then, Some(els), ty);
                Stmt {
                    id: stmt.id,
                    span: stmt.span,
                    kind: StmtKind::Local(mutability, pat, guarded),
                }
            }
            StmtKind::Expr(e) if e.ty != Ty::UNIT => {
                let ty = e.ty.clone();
                let span = e.span;
                let default = build_default_or_err(self.assigner, &mut self.errors, &ty, span);
                let then = block_wrap_expr(self.assigner, e, ty.clone());
                let els = block_wrap_expr(self.assigner, default, ty.clone());
                let guard_if =
                    flag_guard_if(self.assigner, self.broke, self.cont, then, Some(els), ty);
                expr_stmt(self.assigner, guard_if)
            }
            StmtKind::Qubit(..) | StmtKind::Item(_) => stmt,
            StmtKind::Expr(_) | StmtKind::Semi(_) => {
                let then = block_wrap_stmt(self.assigner, stmt);
                let guard_if =
                    flag_guard_if(self.assigner, self.broke, self.cont, then, None, Ty::UNIT);
                expr_stmt(self.assigner, guard_if)
            }
        }
    }

    /// Builds a value of type `ty` that also performs the `break`/`continue`
    /// flag write, for a `break`/`continue` sitting in operand or value
    /// position: `set .flag_<id> = true` alone for a `Unit` value, or
    /// `{ set .flag_<id> = true; <default> }` otherwise.
    fn gen_flag_value(&mut self, is_break: bool, kw_span: Span, ty: &Ty) -> Expr {
        let flag = if is_break {
            self.broke.expect("break implies a broke flag was minted")
        } else {
            self.cont.expect("continue implies a cont flag was minted")
        };
        let assign = gen_flag_assign_expr(self.assigner, flag, kw_span);
        if *ty == Ty::UNIT {
            return assign;
        }
        let assign_semi = Stmt {
            id: self.assigner.next_node(),
            span: kw_span,
            kind: StmtKind::Semi(assign),
        };
        let default = build_default_or_err(self.assigner, &mut self.errors, ty, kw_span);
        let default_stmt = expr_stmt(self.assigner, default);
        Expr {
            id: self.assigner.next_node(),
            span: kw_span,
            ty: ty.clone(),
            kind: ExprKind::Block(Block {
                id: self.assigner.next_node(),
                span: kw_span,
                ty: ty.clone(),
                stmts: vec![assign_semi, default_stmt],
            }),
        }
    }
}

/// Returns `(is_break, keyword_span)` when `stmt` is a bare `break;`/`continue;`,
/// with or without a trailing semicolon, or `None` otherwise.
fn bare_break_continue(stmt: &Stmt) -> Option<(bool, Span)> {
    match &stmt.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => match e.kind {
            ExprKind::Break => Some((true, e.span)),
            ExprKind::Continue => Some((false, e.span)),
            _ => None,
        },
        _ => None,
    }
}

/// Builds a steppable `set .flag_<id> = true;` statement carrying the original
/// `break`/`continue` keyword span, so the debugger steps on it and runtime
/// errors point there.
fn gen_flag_set_stmt(assigner: &mut Assigner, flag: &IdentTemplate, span: Span) -> Stmt {
    let assign = gen_flag_assign_expr(assigner, flag, span);
    Stmt {
        id: assigner.next_node(),
        span,
        kind: StmtKind::Semi(assign),
    }
}

/// Builds the `set .flag_<id> = true` assignment expression.
fn gen_flag_assign_expr(assigner: &mut Assigner, flag: &IdentTemplate, span: Span) -> Expr {
    let flag_ref = flag.gen_local_ref(assigner);
    let true_lit = Expr {
        id: assigner.next_node(),
        span,
        ty: Ty::Prim(Prim::Bool),
        kind: ExprKind::Lit(Lit::Bool(true)),
    };
    Expr {
        id: assigner.next_node(),
        span,
        ty: Ty::UNIT,
        kind: ExprKind::Assign(Box::new(flag_ref), Box::new(true_lit)),
    }
}

/// Builds `not .flag_<id>` with a non-steppable span.
fn gen_not_flag(assigner: &mut Assigner, flag: &IdentTemplate) -> Expr {
    Expr {
        id: assigner.next_node(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Bool),
        kind: ExprKind::UnOp(UnOp::NotL, Box::new(flag.gen_local_ref(assigner))),
    }
}

/// Builds the post-control-flow guard condition from whichever flags are
/// present: `(not .broke_<id>) and (not .cont_<id>)` when both exist, a single
/// `not .flag_<id>` when only one does. Both non-steppable.
fn flag_guard_cond(
    assigner: &mut Assigner,
    broke: Option<&IdentTemplate>,
    cont: Option<&IdentTemplate>,
) -> Expr {
    match (broke, cont) {
        (Some(broke), Some(cont)) => {
            let not_broke = gen_not_flag(assigner, broke);
            let not_cont = gen_not_flag(assigner, cont);
            Expr {
                id: assigner.next_node(),
                span: Span::default(),
                ty: Ty::Prim(Prim::Bool),
                kind: ExprKind::BinOp(BinOp::AndL, Box::new(not_broke), Box::new(not_cont)),
            }
        }
        (Some(broke), None) => gen_not_flag(assigner, broke),
        (None, Some(cont)) => gen_not_flag(assigner, cont),
        (None, None) => unreachable!("guard_stmt runs only after seen_control_flow"),
    }
}

/// Builds `if <guard> { <then> } else { <els> }` with a non-steppable guard
/// `if`, per the debugging span discipline. The guard is the conjunction of
/// whichever of `broke`/`cont` are present.
fn flag_guard_if(
    assigner: &mut Assigner,
    broke: Option<&IdentTemplate>,
    cont: Option<&IdentTemplate>,
    then: Expr,
    els: Option<Expr>,
    ty: Ty,
) -> Expr {
    let guard_cond = flag_guard_cond(assigner, broke, cont);
    Expr {
        id: assigner.next_node(),
        span: Span::default(),
        ty,
        kind: ExprKind::If(Box::new(guard_cond), Box::new(then), els.map(Box::new)),
    }
}

/// Wraps `expr` as the trailing value of a fresh block `{ <expr> }`; `expr`
/// keeps its own span, the synthetic block is non-steppable.
fn block_wrap_expr(assigner: &mut Assigner, expr: Expr, ty: Ty) -> Expr {
    let stmt = Stmt {
        id: assigner.next_node(),
        span: Span::default(),
        kind: StmtKind::Expr(expr),
    };
    Expr {
        id: assigner.next_node(),
        span: Span::default(),
        ty: ty.clone(),
        kind: ExprKind::Block(Block {
            id: assigner.next_node(),
            span: Span::default(),
            ty,
            stmts: vec![stmt],
        }),
    }
}

/// Wraps a `Unit` statement in a fresh block `{ <stmt> }`; the statement keeps
/// its own id and span, the synthetic block is non-steppable.
fn block_wrap_stmt(assigner: &mut Assigner, stmt: Stmt) -> Expr {
    Expr {
        id: assigner.next_node(),
        span: Span::default(),
        ty: Ty::UNIT,
        kind: ExprKind::Block(Block {
            id: assigner.next_node(),
            span: Span::default(),
            ty: Ty::UNIT,
            stmts: vec![stmt],
        }),
    }
}

/// Wraps an expression as a non-steppable `StmtKind::Expr` statement.
fn expr_stmt(assigner: &mut Assigner, expr: Expr) -> Stmt {
    Stmt {
        id: assigner.next_node(),
        span: Span::default(),
        kind: StmtKind::Expr(expr),
    }
}

/// Builds a classical default value of `ty`, recording an
/// [`Error::UnsupportedType`] and substituting a typed `Err` placeholder when
/// `ty` has no synthesizable default.
fn build_default_or_err(
    assigner: &mut Assigner,
    errors: &mut Vec<Error>,
    ty: &Ty,
    span: Span,
) -> Expr {
    if let Some(default) = build_default(assigner, ty) {
        return default;
    }
    errors.push(Error::UnsupportedType(format!("{ty}"), span));
    Expr {
        id: assigner.next_node(),
        span: Span::default(),
        ty: ty.clone(),
        kind: ExprKind::Err,
    }
}

/// Builds a classical default value of `ty`, or `None` when `ty` has no
/// synthesizable default, such as `Qubit`, an arrow type, or a user-defined
/// type, which this desugar does not attempt to construct.
fn build_default(assigner: &mut Assigner, ty: &Ty) -> Option<Expr> {
    let kind = build_default_kind(assigner, ty)?;
    Some(Expr {
        id: assigner.next_node(),
        span: Span::default(),
        ty: ty.clone(),
        kind,
    })
}

fn build_default_kind(assigner: &mut Assigner, ty: &Ty) -> Option<ExprKind> {
    match ty {
        Ty::Prim(Prim::Bool) => Some(ExprKind::Lit(Lit::Bool(false))),
        Ty::Prim(Prim::Int) => Some(ExprKind::Lit(Lit::Int(0))),
        Ty::Prim(Prim::BigInt) => Some(ExprKind::Lit(Lit::BigInt(BigInt::from(0)))),
        Ty::Prim(Prim::Double) => Some(ExprKind::Lit(Lit::Double(0.0))),
        Ty::Prim(Prim::Pauli) => Some(ExprKind::Lit(Lit::Pauli(Pauli::I))),
        Ty::Prim(Prim::Result) => Some(ExprKind::Lit(Lit::Result(Result::Zero))),
        Ty::Prim(Prim::String) => Some(ExprKind::String(Vec::new())),
        // Each range type's default must present exactly the bounds its shape
        // requires, so the synthesized value's kind matches its type tag: `...`
        // for `RangeFull`, `0...` for `RangeFrom`, `...0` for `RangeTo`, and
        // `0..0` for a fully-bounded `Range`. Emitting the `RangeFull` shape
        // (`...`) for every variant would tag the value with a range type it
        // does not structurally match. This default only seeds a never-observed
        // divergence path, so the concrete bounds are immaterial.
        Ty::Prim(Prim::RangeFull) => Some(ExprKind::Range(None, None, None)),
        Ty::Prim(Prim::RangeFrom) => Some(ExprKind::Range(
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
            None,
            None,
        )),
        Ty::Prim(Prim::RangeTo) => Some(ExprKind::Range(
            None,
            None,
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
        )),
        Ty::Prim(Prim::Range) => Some(ExprKind::Range(
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
            None,
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
        )),
        Ty::Array(_) => Some(ExprKind::Array(Vec::new())),
        Ty::Tuple(elems) => {
            let exprs = elems
                .iter()
                .map(|elem| build_default(assigner, elem))
                .collect::<Option<Vec<_>>>()?;
            Some(ExprKind::Tuple(exprs))
        }
        Ty::Prim(Prim::Qubit)
        | Ty::Arrow(_)
        | Ty::Udt(_, _)
        | Ty::Infer(_)
        | Ty::Param { .. }
        | Ty::Err => None,
    }
}

/// Returns `true` when a statement following a set loop-control flag must be
/// relocated into a guarded suffix block rather than guarded in place: a qubit
/// allocation, whose binding scope must stay intact, or a non-defaultable `let`
/// binding, which has no classical default to seed the divergence path. Any
/// other statement is guarded in place by [`LoopUni::guard_stmt`].
fn requires_suffix_relocation(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Qubit(..) => true,
        StmtKind::Local(_, _, init) => !is_defaultable(&init.ty),
        _ => false,
    }
}

/// Read-only check whether `ty` has a classical default the desugar can
/// materialize directly. Kept in exact agreement with [`build_default_kind`]
/// so the relocation decision matches what [`build_default_or_err`] can build;
/// `Qubit`, `Arrow`, and every user-defined type are not defaultable.
fn is_defaultable(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(Prim::Qubit)
        | Ty::Arrow(_)
        | Ty::Udt(..)
        | Ty::Infer(_)
        | Ty::Param { .. }
        | Ty::Err => false,
        Ty::Prim(_) | Ty::Array(_) => true,
        Ty::Tuple(elems) => elems.iter().all(is_defaultable),
    }
}

/// Per-keyword presence of a `break`/`continue` binding to the enclosing loop.
struct BreakContinuePresence {
    has_break: bool,
    has_continue: bool,
}

impl BreakContinuePresence {
    /// Returns `true` when the body contains either a `break` or a `continue`.
    fn any(&self) -> bool {
        self.has_break || self.has_continue
    }
}

/// Returns the per-keyword presence of a `break`/`continue` that binds to the
/// enclosing loop, not one nested in an inner loop, within `block`.
fn body_break_continue(block: &Block) -> BreakContinuePresence {
    let mut scan = BreakContinueScan {
        loop_depth: 0,
        has_break: false,
        has_continue: false,
    };
    scan.visit_block(block);
    BreakContinuePresence {
        has_break: scan.has_break,
        has_continue: scan.has_continue,
    }
}

/// Returns `true` when `block` directly contains a `break`/`continue` that
/// binds to the enclosing loop, not one nested in an inner loop.
fn body_has_break_continue(block: &Block) -> bool {
    body_break_continue(block).any()
}

/// Returns `true` when `stmt` contains a `break`/`continue` that binds to the
/// enclosing loop, not one nested in an inner loop.
fn stmt_escapes(stmt: &Stmt) -> bool {
    let mut scan = BreakContinueScan {
        loop_depth: 0,
        has_break: false,
        has_continue: false,
    };
    scan.visit_stmt(stmt);
    scan.has_break || scan.has_continue
}

/// Returns `true` when evaluating `stmt` necessarily executes a `break` or
/// `continue` bound to the enclosing loop. Normalization exposes operand
/// control flow in sequential blocks, so this only needs to model blocks and
/// conditionals; nested loops remain opaque.
fn stmt_always_escapes(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Expr(expr) | StmtKind::Semi(expr) | StmtKind::Local(_, _, expr) => {
            expr_always_escapes(expr)
        }
        StmtKind::Qubit(_, _, init, _) => qubit_init_always_escapes(init),
        StmtKind::Item(_) => false,
    }
}

fn qubit_init_always_escapes(init: &QubitInit) -> bool {
    match &init.kind {
        QubitInitKind::Array(size) => expr_always_escapes(size),
        QubitInitKind::Tuple(items) => items.iter().any(qubit_init_always_escapes),
        QubitInitKind::Single | QubitInitKind::Err => false,
    }
}

fn expr_always_escapes(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Break | ExprKind::Continue => true,
        ExprKind::Block(block) => block.stmts.iter().any(stmt_always_escapes),
        ExprKind::If(_, then_branch, Some(else_branch)) => {
            expr_always_escapes(then_branch) && expr_always_escapes(else_branch)
        }
        _ => false,
    }
}

/// Scans for a `break`/`continue` that binds to the loop being desugared,
/// tracking loop nesting so a `break`/`continue` bound to an inner loop is not
/// counted.
struct BreakContinueScan {
    loop_depth: u32,
    has_break: bool,
    has_continue: bool,
}

impl<'a> Visitor<'a> for BreakContinueScan {
    fn visit_expr(&mut self, expr: &'a Expr) {
        self.walk_loop_depth(expr);
    }
}

impl LoopDepthScan<'_> for BreakContinueScan {
    fn loop_depth(&self) -> u32 {
        self.loop_depth
    }

    fn enter_loop(&mut self) {
        self.loop_depth += 1;
    }

    fn exit_loop(&mut self) {
        self.loop_depth -= 1;
    }

    fn is_done(&self) -> bool {
        self.has_break && self.has_continue
    }

    fn record_break_continue(&mut self, expr: &Expr, at_enclosing_loop: bool) {
        if at_enclosing_loop {
            match &expr.kind {
                ExprKind::Break => self.has_break = true,
                ExprKind::Continue => self.has_continue = true,
                _ => {}
            }
        }
    }
}

/// Scans `package` for any raw `break`/`continue` node that survived loop
/// desugaring and returns one [`Error::ResidualBreakContinue`] per occurrence.
///
/// After [`LoopUni`] runs, every `break`/`continue` should have been rewritten to
/// loop-flag writes, so any surviving raw node signals a violated compiler
/// invariant. Unlike [`BreakContinueScan`], this walk is intentionally not
/// loop-depth aware: once desugaring is complete, no raw `break`/`continue`
/// should remain anywhere, so every occurrence is a violation.
pub(crate) fn check_no_break_continue(package: &Package) -> Vec<Error> {
    let mut scan = ResidualBreakContinueScan { errors: Vec::new() };
    scan.visit_package(package);
    scan.errors
}

/// Read-only visitor that records a [`Error::ResidualBreakContinue`] for each raw
/// `break`/`continue` node it encounters.
struct ResidualBreakContinueScan {
    errors: Vec<Error>,
}

impl<'a> Visitor<'a> for ResidualBreakContinueScan {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.kind {
            ExprKind::Break | ExprKind::Continue => {
                self.errors.push(Error::ResidualBreakContinue(expr.span));
            }
            _ => {}
        }
        visit::walk_expr(self, expr);
    }
}
