// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    global::Table,
    hir::{
        BinOp, Block, Expr, ExprKind, Field, Ident, Lit, Mutability, NodeId, Pat, PatKind,
        PrimField, Res, Stmt, StmtKind, UnOp,
    },
    mut_visit::{MutVisitor, walk_expr},
    ty::{GenericArg, Prim, SizeKind, Ty},
};
use rustc_hash::FxHashSet;

use crate::{
    CORE_NAMESPACE,
    common::{create_gen_core_ref, generated_name},
    logic_sep::{Error, find_quantum_stmts},
};

pub(crate) fn adj_invert_block(
    core: &Table,
    assigner: &mut Assigner,
    block: &mut Block,
) -> Result<(), Vec<Error>> {
    let quantum_stmts = find_quantum_stmts(block)?;
    let mut pass = BlockInverter {
        core,
        assigner,
        quantum_stmts,
        should_reverse_loop: false,
    };
    pass.visit_block(block);
    Ok(())
}

struct BlockInverter<'a> {
    core: &'a Table,
    assigner: &'a mut Assigner,
    quantum_stmts: FxHashSet<NodeId>,
    should_reverse_loop: bool,
}

impl MutVisitor for BlockInverter<'_> {
    fn visit_block(&mut self, block: &mut Block) {
        // Each block is split into classical and quantum statements based on the presence of operation
        // calls, so that the quantum statements can be reversed.
        let mut classical_stmts = Vec::new();
        let mut quantum_stmts = Vec::new();
        for mut stmt in block.stmts.drain(..) {
            if self.quantum_stmts.contains(&stmt.id) {
                self.should_reverse_loop = true;
                self.visit_stmt(&mut stmt);
                quantum_stmts.push(stmt);
                self.should_reverse_loop = false;
            } else {
                self.visit_stmt(&mut stmt);
                classical_stmts.push(stmt);
            }
        }
        quantum_stmts.reverse();
        block.stmts.append(&mut classical_stmts);
        block.stmts.append(&mut quantum_stmts);
    }

    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::For(pat, iterable, block) if self.should_reverse_loop => {
                self.visit_block(block);
                *expr = self.reverse_loop(pat, iterable, block);
            }
            ExprKind::Conjugate(_, apply) => {
                // Only invert the apply block, within block inversion handled by a different pass.
                self.visit_block(apply);
            }
            _ => walk_expr(self, expr),
        }
    }
}

impl BlockInverter<'_> {
    fn reverse_loop(&mut self, pat: &Pat, iterable: &Expr, block: &Block) -> Expr {
        let mut wrapper = Block {
            id: NodeId::default(),
            span: Span::default(),
            ty: Ty::UNIT,
            stmts: Vec::new(),
        };
        match &iterable.ty {
            Ty::Prim(Prim::Range) => self.reverse_range_loop(&mut wrapper, iterable, pat, block),

            Ty::Array(arr_ty, size_kind) => {
                self.reverse_array_loop(
                    &mut wrapper,
                    arr_ty,
                    size_kind,
                    iterable.clone(),
                    pat.clone(),
                    block.clone(),
                );
            }

            _ => panic!("iterable should be array or range"),
        }

        Expr {
            id: NodeId::default(),
            span: Span::default(),
            ty: Ty::UNIT,
            kind: ExprKind::Block(wrapper),
        }
    }

    fn reverse_array_loop(
        &mut self,
        wrapper: &mut Block,
        arr_ty: &Ty,
        size_kind: &SizeKind,
        iterable: Expr,
        pat: Pat,
        mut block: Block,
    ) {
        // Create a new binding for the array expr.
        let new_arr_id = self.assigner.next_node();
        wrapper.stmts.push(Stmt {
            id: NodeId::default(),
            span: Span::default(),
            kind: StmtKind::Local(
                Mutability::Immutable,
                Pat {
                    id: NodeId::default(),
                    span: Span::default(),
                    ty: Ty::Array(Box::new(arr_ty.clone()), *size_kind),
                    kind: PatKind::Bind(Ident {
                        id: new_arr_id,
                        span: Span::default(),
                        name: generated_name("array"),
                    }),
                },
                iterable,
            ),
        });

        // Create a pattern for binding the index iterator.
        let index_id = self.assigner.next_node();
        let index_pat = Pat {
            id: NodeId::default(),
            span: Span::default(),
            ty: Ty::Prim(Prim::Int),
            kind: PatKind::Bind(Ident {
                id: index_id,
                span: Span::default(),
                name: generated_name("index"),
            }),
        };

        // Create a binding from the previous loop iterator variable and the array index expr.
        let iterator_bind = Stmt {
            id: NodeId::default(),
            span: Span::default(),
            kind: StmtKind::Local(
                Mutability::Immutable,
                pat,
                Expr {
                    id: NodeId::default(),
                    span: Span::default(),
                    ty: arr_ty.clone(),
                    kind: ExprKind::Index(
                        Box::new(Expr {
                            id: NodeId::default(),
                            span: Span::default(),
                            ty: Ty::Array(Box::new(arr_ty.clone()), *size_kind),
                            kind: ExprKind::Var(Res::Local(new_arr_id), Vec::new()),
                        }),
                        Box::new(Expr {
                            id: NodeId::default(),
                            span: Span::default(),
                            ty: Ty::Prim(Prim::Int),
                            kind: ExprKind::Var(Res::Local(index_id), Vec::new()),
                        }),
                    ),
                },
            ),
        };

        // Add the new binding to the front of the block statements.
        let mut new_stmts = vec![iterator_bind];
        new_stmts.append(&mut block.stmts);
        block.stmts = new_stmts;

        // Put in the new for-loop
        wrapper.stmts.push(Stmt {
            id: NodeId::default(),
            span: Span::default(),
            kind: StmtKind::Expr(Expr {
                id: NodeId::default(),
                span: Span::default(),
                ty: Ty::UNIT,
                kind: ExprKind::For(
                    index_pat,
                    Box::new(make_array_index_range_reverse(
                        self.core, new_arr_id, arr_ty, size_kind,
                    )),
                    block,
                ),
            }),
        });
    }

    fn reverse_range_loop(
        &mut self,
        wrapper: &mut Block,
        iterable: &Expr,
        pat: &Pat,
        block: &Block,
    ) {
        // Create a new binding for the range expr.
        let new_range_id = self.assigner.next_node();
        wrapper.stmts.push(Stmt {
            id: NodeId::default(),
            span: Span::default(),
            kind: StmtKind::Local(
                Mutability::Immutable,
                Pat {
                    id: NodeId::default(),
                    span: Span::default(),
                    ty: Ty::Prim(Prim::Range),
                    kind: PatKind::Bind(Ident {
                        id: new_range_id,
                        span: Span::default(),
                        name: generated_name("range"),
                    }),
                },
                iterable.clone(),
            ),
        });

        // Create the new for-loop that iterates over the reversed range.
        wrapper.stmts.push(Stmt {
            id: NodeId::default(),
            span: Span::default(),
            kind: StmtKind::Expr(Expr {
                id: NodeId::default(),
                span: Span::default(),
                ty: Ty::UNIT,
                kind: ExprKind::For(
                    pat.clone(),
                    Box::new(make_range_reverse_expr(new_range_id)),
                    block.clone(),
                ),
            }),
        });
    }
}

fn make_range_reverse_expr(range_id: NodeId) -> Expr {
    let start = make_range_field(range_id, PrimField::Start);
    let step = make_range_field(range_id, PrimField::Step);
    let end = make_range_field(range_id, PrimField::End);

    // A reversed range is `(start + (end - start) / step * step) .. -step .. start`.
    let new_start = Box::new(Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::BinOp(
            BinOp::Add,
            Box::new(start.clone()),
            Box::new(Expr {
                id: NodeId::default(),
                span: Span::default(),
                ty: Ty::Prim(Prim::Int),
                kind: ExprKind::BinOp(
                    BinOp::Mul,
                    Box::new(Expr {
                        id: NodeId::default(),
                        span: Span::default(),
                        ty: Ty::Prim(Prim::Int),
                        kind: ExprKind::BinOp(
                            BinOp::Div,
                            Box::new(Expr {
                                id: NodeId::default(),
                                span: Span::default(),
                                ty: Ty::Prim(Prim::Int),
                                kind: ExprKind::BinOp(
                                    BinOp::Sub,
                                    Box::new(end),
                                    Box::new(start.clone()),
                                ),
                            }),
                            Box::new(step.clone()),
                        ),
                    }),
                    Box::new(step.clone()),
                ),
            }),
        ),
    });
    let new_step = Box::new(Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::UnOp(UnOp::Neg, Box::new(step)),
    });
    let new_end = Box::new(start);

    Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Range),
        kind: ExprKind::Range(Some(new_start), Some(new_step), Some(new_end)),
    }
}

fn make_range_field(range_id: NodeId, field: PrimField) -> Expr {
    Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::Field(
            Box::new(Expr {
                id: NodeId::default(),
                span: Span::default(),
                ty: Ty::Prim(Prim::Range),
                kind: ExprKind::Var(Res::Local(range_id), Vec::new()),
            }),
            Field::Prim(field),
        ),
    }
}

fn make_array_index_range_reverse(
    core: &Table,
    arr_id: NodeId,
    arr_ty: &Ty,
    size_kind: &SizeKind,
) -> Expr {
    let ns = core
        .find_namespace(CORE_NAMESPACE.iter().copied())
        .expect("prelude namespaces should exist");
    let len_expr_kind = match size_kind {
        SizeKind::Known(n) => ExprKind::Lit(Lit::Int(
            (*n).try_into().expect("array size should fit in i64"),
        )),
        SizeKind::Unknown => {
            // For unknown or inferred sizes, we can't create a literal, so we'll use a call to the Length function
            ExprKind::Call(
                Box::new(create_gen_core_ref(
                    core,
                    ns,
                    "Length",
                    vec![GenericArg::Ty(arr_ty.clone())],
                    Span::default(),
                )),
                Box::new(Expr {
                    id: NodeId::default(),
                    span: Span::default(),
                    ty: Ty::Array(Box::new(arr_ty.clone()), *size_kind),
                    kind: ExprKind::Var(Res::Local(arr_id), Vec::new()),
                }),
            )
        }
    };
    let len = Box::new(Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: len_expr_kind,
    });
    let start = Box::new(Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::BinOp(
            BinOp::Sub,
            len,
            Box::new(Expr {
                id: NodeId::default(),
                span: Span::default(),
                ty: Ty::Prim(Prim::Int),
                kind: ExprKind::Lit(Lit::Int(1)),
            }),
        ),
    });
    let step = Box::new(Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::Lit(Lit::Int(-1)),
    });
    let end = Box::new(Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::Lit(Lit::Int(0)),
    });
    Expr {
        id: NodeId::default(),
        span: Span::default(),
        ty: Ty::Prim(Prim::Range),
        kind: ExprKind::Range(Some(start), Some(step), Some(end)),
    }
}
