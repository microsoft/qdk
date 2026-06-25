// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Eliminates `break` and `continue` expressions by rewriting their enclosing
//! `while` loops with boolean flag guards.
//!
//! This pass runs after [`crate::loop_unification`], so every loop is already a
//! `while` (every `for`/`repeat` has been desugared). For a loop body that
//! contains a `break`/`continue` bound to that loop, this pass:
//!
//! * Replaces each `break` with `set __broke = true;` and folds `not __broke`
//!   into the loop condition (`while not __broke and cond`) so the loop exits on
//!   the next condition check. `__broke` is initialized in a block wrapping the
//!   loop. This mirrors the `while`-rewrite used for early `return` elimination.
//! * Replaces each `continue` with `set __continued = true;` and re-arms
//!   `__continued` to `false` at the top of every iteration.
//! * Guards the statements following a `break`/`continue` site with
//!   `if not __broke and not __continued { ... }` so the rest of the iteration is
//!   skipped once a flag is set.
//!
//! The pass is a structural no-op for any input that contains no `break`/
//! `continue`: it mints no nodes and leaves the loop unchanged.

use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    hir::{BinOp, Block, Expr, ExprKind, Lit, Mutability, NodeId, Stmt, StmtKind, UnOp},
    mut_visit::{MutVisitor, walk_expr},
    ty::{Prim, Ty},
};
use rustc_hash::FxHashMap;

use crate::common::{IdentTemplate, gen_ident};

pub(crate) struct ControlFlowUnification<'a> {
    assigner: &'a mut Assigner,
    /// Maps each desugared `for`-body block to the loop-update statement that
    /// [`crate::loop_unification`] appended to it. That statement must keep
    /// running so `continue` still advances the loop variable, so it is left
    /// outside the per-iteration suffix guard.
    update_steps: FxHashMap<NodeId, NodeId>,
    /// Stack of enclosing loops, innermost last, so each `break`/`continue`
    /// binds to the innermost loop (`OpenQASM` loop control is unlabeled).
    loops: Vec<LoopCtx>,
}

/// Per-loop flag bindings, lazily minted on the first `break`/`continue`.
#[derive(Default)]
struct LoopCtx {
    broke: Option<IdentTemplate>,
    continued: Option<IdentTemplate>,
}

impl<'a> ControlFlowUnification<'a> {
    pub(crate) fn new(assigner: &'a mut Assigner, update_steps: FxHashMap<NodeId, NodeId>) -> Self {
        Self {
            assigner,
            update_steps,
            loops: Vec::new(),
        }
    }

    /// Rewrites a `while` loop in place, eliminating any `break`/`continue` bound
    /// to it and recursing into its body (including nested loops).
    fn transform_while(&mut self, while_expr: &mut Expr) {
        let span = while_expr.span;
        let ExprKind::While(cond, mut body) =
            std::mem::replace(&mut while_expr.kind, ExprKind::Err)
        else {
            unreachable!("transform_while requires a While expression");
        };

        // A desugared `for` body ends with a loop-update statement that must run
        // every iteration; set it aside so `continue` cannot skip it.
        let exempt_id = self.update_steps.get(&body.id).copied();

        self.loops.push(LoopCtx::default());

        let exempt_stmt = if exempt_id.is_some() && body.stmts.last().map(|s| s.id) == exempt_id {
            body.stmts.pop()
        } else {
            None
        };

        let (stmts, _) = self.transform_stmts(std::mem::take(&mut body.stmts));
        body.stmts = stmts;

        let ctx = self.loops.pop().expect("loop context was pushed above");

        // Re-arm `continue` at the top of every iteration.
        if let Some(continued) = &ctx.continued {
            let reset = continued.gen_id_init(
                Mutability::Mutable,
                bool_lit(self.assigner, false, continued.span),
                self.assigner,
            );
            body.stmts.insert(0, reset);
        }

        // The loop-update statement always runs, unguarded.
        if let Some(update) = exempt_stmt {
            body.stmts.push(update);
        }

        if let Some(broke) = ctx.broke {
            // Fold `not __broke` into the condition so a set `break` exits on the
            // next check, and scope `__broke` to a block wrapping the loop.
            let new_cond = Expr {
                id: self.assigner.next_node(),
                span: cond.span,
                ty: Ty::Prim(Prim::Bool),
                kind: ExprKind::BinOp(BinOp::AndL, Box::new(not_flag(self.assigner, &broke)), cond),
            };
            let while_stmt = Stmt {
                id: self.assigner.next_node(),
                span: Span::default(),
                kind: StmtKind::Expr(Expr {
                    id: self.assigner.next_node(),
                    span,
                    ty: Ty::UNIT,
                    kind: ExprKind::While(Box::new(new_cond), body),
                }),
            };
            let broke_init = broke.gen_id_init(
                Mutability::Mutable,
                bool_lit(self.assigner, false, broke.span),
                self.assigner,
            );
            while_expr.kind = ExprKind::Block(Block {
                id: self.assigner.next_node(),
                span,
                ty: Ty::UNIT,
                stmts: vec![broke_init, while_stmt],
            });
        } else {
            // Continue-only or no control: the condition is unchanged.
            while_expr.kind = ExprKind::While(cond, body);
        }
    }

    /// Transforms a statement list, guarding every statement after the first one
    /// that may `break`/`continue` the current loop. Returns the rewritten
    /// statements and whether any of them controls the loop.
    fn transform_stmts(&mut self, stmts: Vec<Stmt>) -> (Vec<Stmt>, bool) {
        let mut out = Vec::with_capacity(stmts.len());
        let mut saw_control = false;
        let mut stmts = stmts.into_iter();
        while let Some(stmt) = stmts.next() {
            let (stmt, controls) = self.transform_stmt(stmt);
            out.push(stmt);
            if controls {
                saw_control = true;
                let suffix: Vec<Stmt> = stmts.by_ref().collect();
                if !suffix.is_empty() {
                    let (guarded, _) = self.transform_stmts(suffix);
                    out.push(self.build_guard_stmt(guarded));
                }
                break;
            }
        }
        (out, saw_control)
    }

    /// Transforms a single statement. Returns the rewritten statement and whether
    /// it may `break`/`continue` the current loop.
    fn transform_stmt(&mut self, mut stmt: Stmt) -> (Stmt, bool) {
        match &mut stmt.kind {
            StmtKind::Semi(expr) | StmtKind::Expr(expr) => match &mut expr.kind {
                ExprKind::Break => {
                    let broke = self.ensure_broke();
                    expr.kind = ExprKind::Assign(
                        Box::new(broke.gen_local_ref(self.assigner)),
                        Box::new(bool_lit(self.assigner, true, broke.span)),
                    );
                    (stmt, true)
                }
                ExprKind::Continue => {
                    let continued = self.ensure_continued();
                    expr.kind = ExprKind::Assign(
                        Box::new(continued.gen_local_ref(self.assigner)),
                        Box::new(bool_lit(self.assigner, true, continued.span)),
                    );
                    (stmt, true)
                }
                ExprKind::While(..) => {
                    // A nested loop fully handles its own break/continue; they do
                    // not control this loop.
                    self.transform_while(expr);
                    (stmt, false)
                }
                ExprKind::If(..) => {
                    let controls = self.transform_if(expr);
                    (stmt, controls)
                }
                ExprKind::Block(block) => {
                    let (stmts, controls) = self.transform_stmts(std::mem::take(&mut block.stmts));
                    block.stmts = stmts;
                    (stmt, controls)
                }
                _ => {
                    // No statement-level break/continue can appear here; descend
                    // only to rewrite nested loops in operand positions.
                    walk_expr(self, expr);
                    (stmt, false)
                }
            },
            StmtKind::Local(_, _, init) => {
                walk_expr(self, init);
                (stmt, false)
            }
            StmtKind::Qubit(_, _, init, block) => {
                // A qubit-allocation block is a statement sequence that may
                // contain a `break`/`continue` bound to the current loop, so it
                // is transformed like any other block (the init cannot).
                self.visit_qubit_init(init);
                let controls = if let Some(block) = block {
                    let (stmts, controls) = self.transform_stmts(std::mem::take(&mut block.stmts));
                    block.stmts = stmts;
                    controls
                } else {
                    false
                };
                (stmt, controls)
            }
            StmtKind::Item(_) => (stmt, false),
        }
    }

    /// Transforms both branches of an `if` expression and reports whether either
    /// branch may `break`/`continue` the current loop.
    fn transform_if(&mut self, if_expr: &mut Expr) -> bool {
        let ExprKind::If(cond, then_branch, else_branch) = &mut if_expr.kind else {
            unreachable!("transform_if requires an If expression");
        };
        walk_expr(self, cond);
        let then_controls = self.transform_branch(then_branch);
        let else_controls = match else_branch {
            Some(branch) => self.transform_branch(branch),
            None => false,
        };
        then_controls || else_controls
    }

    /// Transforms an `if` branch, which is a block expression or, for `elif`, a
    /// nested `if`.
    fn transform_branch(&mut self, branch: &mut Expr) -> bool {
        match &mut branch.kind {
            ExprKind::Block(block) => {
                let (stmts, controls) = self.transform_stmts(std::mem::take(&mut block.stmts));
                block.stmts = stmts;
                controls
            }
            ExprKind::If(..) => self.transform_if(branch),
            _ => {
                walk_expr(self, branch);
                false
            }
        }
    }

    /// Wraps guarded statements in `if not __broke and not __continued { ... }`,
    /// using whichever flags the current loop has minted.
    fn build_guard_stmt(&mut self, stmts: Vec<Stmt>) -> Stmt {
        let cond = self.guard_condition();
        let then_block = Block {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::UNIT,
            stmts,
        };
        let then_expr = Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::UNIT,
            kind: ExprKind::Block(then_block),
        };
        let if_expr = Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: Ty::UNIT,
            kind: ExprKind::If(Box::new(cond), Box::new(then_expr), None),
        };
        Stmt {
            id: self.assigner.next_node(),
            span: Span::default(),
            kind: StmtKind::Expr(if_expr),
        }
    }

    /// Builds the guard condition from the flags minted in the current loop. The
    /// conjunction skips the suffix when either a `break` or `continue` fired;
    /// when only one flag exists, the guard tests just that flag.
    fn guard_condition(&mut self) -> Expr {
        let Self {
            assigner, loops, ..
        } = self;
        let ctx = loops.last().expect("guard must be inside a loop");
        let mut conds = Vec::new();
        if let Some(broke) = &ctx.broke {
            conds.push(not_flag(assigner, broke));
        }
        if let Some(continued) = &ctx.continued {
            conds.push(not_flag(assigner, continued));
        }
        let mut conds = conds.into_iter();
        let mut cond = conds
            .next()
            .expect("a guard requires at least one control flag");
        for next in conds {
            cond = Expr {
                id: assigner.next_node(),
                span: Span::default(),
                ty: Ty::Prim(Prim::Bool),
                kind: ExprKind::BinOp(BinOp::AndL, Box::new(cond), Box::new(next)),
            };
        }
        cond
    }

    /// Lazily mints the current loop's `break` flag and returns it.
    fn ensure_broke(&mut self) -> IdentTemplate {
        let Self {
            assigner, loops, ..
        } = self;
        let ctx = loops.last_mut().expect("break must be inside a loop");
        if ctx.broke.is_none() {
            ctx.broke = Some(gen_ident(
                assigner,
                "broke",
                Ty::Prim(Prim::Bool),
                Span::default(),
            ));
        }
        ctx.broke.clone().expect("broke flag was just set")
    }

    /// Lazily mints the current loop's `continue` flag and returns it.
    fn ensure_continued(&mut self) -> IdentTemplate {
        let Self {
            assigner, loops, ..
        } = self;
        let ctx = loops.last_mut().expect("continue must be inside a loop");
        if ctx.continued.is_none() {
            ctx.continued = Some(gen_ident(
                assigner,
                "continued",
                Ty::Prim(Prim::Bool),
                Span::default(),
            ));
        }
        ctx.continued.clone().expect("continued flag was just set")
    }
}

impl MutVisitor for ControlFlowUnification<'_> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        if matches!(expr.kind, ExprKind::While(..)) {
            // `transform_while` drives its own recursion into the loop body so it
            // controls break/continue binding and never double-processes.
            self.transform_while(expr);
        } else {
            walk_expr(self, expr);
        }
    }
}

/// Builds a `not __flag` expression referencing a flag binding.
fn not_flag(assigner: &mut Assigner, flag: &IdentTemplate) -> Expr {
    Expr {
        id: assigner.next_node(),
        span: flag.span,
        ty: Ty::Prim(Prim::Bool),
        kind: ExprKind::UnOp(UnOp::NotL, Box::new(flag.gen_local_ref(assigner))),
    }
}

/// Builds a boolean literal expression.
fn bool_lit(assigner: &mut Assigner, value: bool, span: Span) -> Expr {
    Expr {
        id: assigner.next_node(),
        span,
        ty: Ty::Prim(Prim::Bool),
        kind: ExprKind::Lit(Lit::Bool(value)),
    }
}
