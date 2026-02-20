// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc_data_structures::namespaces::NamespaceId;
use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    global::Table,
    hir::{
        Block, Expr, ExprKind, Mutability, Pat, PatKind, QubitInit, QubitInitKind, QubitSource,
        Stmt, StmtKind,
    },
    mut_visit::{MutVisitor, walk_expr, walk_stmt},
    ty::{Prim, Ty},
};
use std::mem::take;

use crate::QIR_RUNTIME_NAMESPACE;
use crate::common::{IdentTemplate, create_gen_core_ref, generated_name};

#[derive(Debug, Clone)]
struct QubitIdent {
    id: IdentTemplate,
    is_array: bool,
}

pub(crate) struct ReplaceQubitAllocation<'a> {
    assigner: &'a mut Assigner,
    core: &'a Table,
    qubits_curr_callable: Vec<Vec<QubitIdent>>,
    qubits_curr_block: Vec<QubitIdent>,
    prefix_qubits: Vec<QubitIdent>,
}

impl<'a> ReplaceQubitAllocation<'a> {
    pub(crate) fn new(core: &'a Table, assigner: &'a mut Assigner) -> Self {
        Self {
            assigner,
            core,
            qubits_curr_callable: Vec::new(),
            qubits_curr_block: Vec::new(),
            prefix_qubits: Vec::new(),
        }
    }

    fn generate_qubit_alloc_stmts(
        &mut self,
        stmt_span: Span,
        pat: Pat,
        mut init: QubitInit,
        qubit_source: QubitSource,
    ) -> (Vec<QubitIdent>, Vec<Stmt>) {
        fn is_non_tuple(init: &mut QubitInit) -> (bool, Option<Expr>) {
            match &mut init.kind {
                QubitInitKind::Array(e) => (true, Some(take(e))),
                QubitInitKind::Single => (true, None),
                QubitInitKind::Tuple(_) => (false, None),
                QubitInitKind::Err => panic!("QubitInitKind::Err"),
            }
        }

        let mut new_stmts: Vec<Stmt> = vec![];
        let mut new_ids: Vec<QubitIdent> = vec![];

        if let (true, opt) = is_non_tuple(&mut init) {
            if let PatKind::Bind(id) = pat.kind {
                let id = IdentTemplate {
                    id: id.id,
                    span: stmt_span,
                    name: id.name,
                    ty: pat.ty,
                };
                let is_array = opt.is_some();
                new_stmts.push(match opt {
                    Some(mut size) => {
                        self.visit_expr(&mut size);
                        self.create_array_alloc_stmt(&id, size, qubit_source)
                    }
                    None => self.create_alloc_stmt(&id, qubit_source),
                });
                new_ids.push(QubitIdent { id, is_array });
            } else {
                panic!("Shape of identifier pattern doesn't match shape of initializer");
            }
        } else {
            let (assignment_expr, mut ids) = self.process_qubit_init(init);
            new_stmts = ids
                .iter_mut()
                .map(|(id, size)| match size {
                    Some(size) => {
                        self.visit_expr(size);
                        self.create_array_alloc_stmt(id, size.clone(), qubit_source)
                    }
                    None => self.create_alloc_stmt(id, qubit_source),
                })
                .collect();
            new_ids = ids
                .into_iter()
                .map(|(id, expr)| QubitIdent {
                    id,
                    is_array: expr.is_some(),
                })
                .collect();
            new_stmts.push(Stmt {
                id: self.assigner.next_node(),
                span: stmt_span,
                kind: StmtKind::Local(Mutability::Immutable, pat, assignment_expr),
            });
        }

        (new_ids, new_stmts)
    }

    fn process_qubit_stmt(
        &mut self,
        stmt_span: Span,
        pat: Pat,
        init: QubitInit,
        block: Option<Block>,
        qubit_source: QubitSource,
    ) -> Vec<Stmt> {
        let (new_ids, new_stmts) =
            self.generate_qubit_alloc_stmts(stmt_span, pat, init, qubit_source);
        if let Some(block) = block {
            vec![self.generate_block_stmt(stmt_span, new_ids, block, new_stmts)]
        } else {
            self.qubits_curr_block.extend(new_ids);
            new_stmts
        }
    }

    fn generate_block_stmt(
        &mut self,
        stmt_span: Span,
        new_ids: Vec<QubitIdent>,
        mut block: Block,
        new_stmts: Vec<Stmt>,
    ) -> Stmt {
        self.prefix_qubits = new_ids;
        block.stmts.splice(0..0, new_stmts);
        self.visit_block(&mut block);
        Stmt {
            id: self.assigner.next_node(),
            span: stmt_span,
            kind: StmtKind::Expr(Expr {
                id: self.assigner.next_node(),
                span: stmt_span,
                ty: block.ty.clone(),
                kind: ExprKind::Block(block),
            }),
        }
    }

    fn process_qubit_init(
        &mut self,
        init: QubitInit,
    ) -> (Expr, Vec<(IdentTemplate, Option<Expr>)>) {
        match init.kind {
            QubitInitKind::Array(size) => {
                let gen_id = self.gen_ident(Ty::Array(Box::new(Ty::Prim(Prim::Qubit))), init.span);
                let expr = gen_id.gen_local_ref(self.assigner);
                (expr, vec![(gen_id, Some(*size))])
            }
            QubitInitKind::Single => {
                let gen_id = self.gen_ident(Ty::Prim(Prim::Qubit), init.span);
                let expr = gen_id.gen_local_ref(self.assigner);
                (expr, vec![(gen_id, None)])
            }
            QubitInitKind::Tuple(inits) => {
                let mut exprs: Vec<Expr> = vec![];
                let mut ids: Vec<(IdentTemplate, Option<Expr>)> = vec![];
                for i in inits {
                    let (sub_expr, sub_ids) = self.process_qubit_init(i);
                    exprs.push(sub_expr);
                    ids.extend(sub_ids);
                }
                let tuple_expr = Expr {
                    id: self.assigner.next_node(),
                    span: init.span,
                    ty: Ty::Tuple(exprs.iter().map(|e| e.ty.clone()).collect()),
                    kind: ExprKind::Tuple(exprs),
                };
                (tuple_expr, ids)
            }
            QubitInitKind::Err => panic!("QubitInitKind::Err"),
        }
    }

    fn gen_ident(&mut self, ty: Ty, span: Span) -> IdentTemplate {
        let id = self.assigner.next_node();
        IdentTemplate {
            id,
            span,
            name: generated_name(&format!("generated_ident_{id}")),
            ty,
        }
    }

    fn is_qubits_empty(&self) -> bool {
        self.qubits_curr_block.is_empty()
            && self
                .qubits_curr_callable
                .iter()
                .all(std::vec::Vec::is_empty)
    }

    fn get_dealloc_stmts(&mut self, qubits: &[QubitIdent]) -> Vec<Stmt> {
        qubits
            .iter()
            .rev()
            .map(|qubit| self.get_dealloc_stmt(qubit))
            .collect()
    }

    fn get_dealloc_stmt(&mut self, qubit: &QubitIdent) -> Stmt {
        if qubit.is_array {
            self.create_array_dealloc_stmt(&qubit.id)
        } else {
            self.create_dealloc_stmt(&qubit.id)
        }
    }

    fn get_dealloc_stmts_for_block(&mut self) -> Vec<Stmt> {
        let mut stmts = vec![];
        for qubit in self.qubits_curr_block.clone().iter().rev() {
            stmts.push(self.get_dealloc_stmt(qubit));
        }
        stmts
    }

    fn get_dealloc_stmts_for_callable(&mut self) -> Vec<Stmt> {
        let mut stmts = self.get_dealloc_stmts(&self.qubits_curr_block.clone());
        stmts.extend(
            self.qubits_curr_callable
                .clone()
                .iter()
                .rev()
                .flat_map(|q| self.get_dealloc_stmts(q)),
        );
        stmts
    }

    fn create_alloc_stmt(&mut self, ident: &IdentTemplate, qubit_source: QubitSource) -> Stmt {
        let ns = self.get_qir_runtime_namespace();
        let api = match qubit_source {
            QubitSource::Fresh => "__quantum__rt__qubit_allocate",
            QubitSource::Dirty => "__quantum__rt__qubit_borrow",
        };
        let mut call_expr = create_gen_core_ref(self.core, ns, api, Vec::new(), ident.span);
        call_expr.id = self.assigner.next_node();
        create_general_alloc_stmt(self.assigner, ident, call_expr, None)
    }

    fn create_array_alloc_stmt(
        &mut self,
        ident: &IdentTemplate,
        array_size: Expr,
        qubit_source: QubitSource,
    ) -> Stmt {
        let ns = self.get_qir_runtime_namespace();
        let api = match qubit_source {
            QubitSource::Fresh => "AllocateQubitArray",
            QubitSource::Dirty => "BorrowQubitArray",
        };
        let mut call_expr = create_gen_core_ref(self.core, ns, api, Vec::new(), ident.span);
        call_expr.id = self.assigner.next_node();
        create_general_alloc_stmt(self.assigner, ident, call_expr, Some(array_size))
    }

    fn create_dealloc_stmt(&mut self, ident: &IdentTemplate) -> Stmt {
        let ns = self.get_qir_runtime_namespace();
        let mut call_expr = create_gen_core_ref(
            self.core,
            ns,
            "__quantum__rt__qubit_release",
            Vec::new(),
            ident.span,
        );
        call_expr.id = self.assigner.next_node();
        create_general_dealloc_stmt(self.assigner, call_expr, ident)
    }

    fn create_array_dealloc_stmt(&mut self, ident: &IdentTemplate) -> Stmt {
        let ns = self.get_qir_runtime_namespace();
        let mut call_expr =
            create_gen_core_ref(self.core, ns, "ReleaseQubitArray", Vec::new(), ident.span);
        call_expr.id = self.assigner.next_node();
        create_general_dealloc_stmt(self.assigner, call_expr, ident)
    }

    fn get_qir_runtime_namespace(&self) -> NamespaceId {
        self.core
            .find_namespace(QIR_RUNTIME_NAMESPACE.iter().copied())
            .expect("prelude namespaces should exist")
    }
}

impl MutVisitor for ReplaceQubitAllocation<'_> {
    fn visit_block(&mut self, block: &mut Block) {
        let qubits_super_block = take(&mut self.qubits_curr_block);
        self.qubits_curr_callable.push(qubits_super_block);
        self.qubits_curr_block = take(&mut self.prefix_qubits);

        // walk block
        let old_stmts = take(&mut block.stmts);
        for mut stmt in old_stmts {
            if let StmtKind::Qubit(qubit_source, pat, init, qubit_scope) = stmt.kind {
                block.stmts.extend(self.process_qubit_stmt(
                    stmt.span,
                    pat,
                    init,
                    qubit_scope,
                    qubit_source,
                ));
            } else {
                walk_stmt(self, &mut stmt);
                block.stmts.push(stmt);
            }
        }

        if !self.qubits_curr_block.is_empty() {
            let new_end_stmt: Option<Stmt> = match block.stmts.last_mut() {
                Some(s) => {
                    if let StmtKind::Expr(end) = &mut s.kind {
                        let end_capture = self.gen_ident(end.ty.clone(), end.span);
                        *s = end_capture.gen_steppable_id_init(
                            Mutability::Immutable,
                            take(end),
                            self.assigner,
                        );
                        Some(Stmt {
                            id: self.assigner.next_node(),
                            span: Span::default(),
                            kind: StmtKind::Expr(end_capture.gen_local_ref(self.assigner)),
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            };

            block.stmts.extend(self.get_dealloc_stmts_for_block());
            if let Some(end) = new_end_stmt {
                block.stmts.push(end);
            }
        }

        self.qubits_curr_block = self
            .qubits_curr_callable
            .pop()
            .expect("missing expected vector of qubits identifiers");
    }

    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Return(e) => {
                if self.is_qubits_empty() {
                    self.visit_expr(e);
                } else {
                    let rtrn_capture = self.gen_ident(e.ty.clone(), e.span);
                    self.visit_expr(e);
                    let mut stmts: Vec<Stmt> = vec![];
                    stmts.push(rtrn_capture.gen_id_init(
                        Mutability::Immutable,
                        take(e),
                        self.assigner,
                    ));
                    stmts.extend(self.get_dealloc_stmts_for_callable());
                    stmts.push(Stmt {
                        id: self.assigner.next_node(),
                        span: expr.span,
                        kind: StmtKind::Semi(Expr {
                            id: self.assigner.next_node(),
                            span: expr.span,
                            ty: expr.ty.clone(),
                            kind: ExprKind::Return(Box::new(
                                rtrn_capture.gen_local_ref(self.assigner),
                            )),
                        }),
                    });
                    let new_expr = Expr {
                        id: self.assigner.next_node(),
                        span: expr.span,
                        ty: expr.ty.clone(),
                        kind: ExprKind::Block(Block {
                            id: self.assigner.next_node(),
                            span: expr.span,
                            ty: expr.ty.clone(),
                            stmts,
                        }),
                    };
                    *expr = new_expr;
                }
            }
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        // This function is not called by visit_block above, so the only time it will be used is for
        // top-level statements. Given that, the qubits allocated will always be live for
        // the entirety of a global scope, so only qubit allocations need to be generated.
        match stmt.kind.clone() {
            StmtKind::Qubit(_, pat, qubit_init, None) => {
                stmt.kind = create_qubit_global_alloc(self.assigner, self.core, pat, qubit_init);
            }
            StmtKind::Qubit(qubit_source, pat, qubit_init, Some(block)) => {
                let (new_ids, new_stmts) =
                    self.generate_qubit_alloc_stmts(stmt.span, pat, qubit_init, qubit_source);
                *stmt = self.generate_block_stmt(stmt.span, new_ids, block, new_stmts);
            }
            kind => {
                stmt.kind = kind;
                walk_stmt(self, stmt);
            }
        }
    }
}

fn create_qubit_global_alloc(
    assigner: &mut Assigner,
    core: &Table,
    pat: Pat,
    qubit_init: QubitInit,
) -> StmtKind {
    fn qubit_alloc_expr(assigner: &mut Assigner, core: &Table, qubit_init: QubitInit) -> Expr {
        let ns = core
            .find_namespace(QIR_RUNTIME_NAMESPACE.iter().copied())
            .expect("prelude namespaces should exist");
        match qubit_init.kind {
            QubitInitKind::Array(mut expr) => {
                let mut call_expr = create_gen_core_ref(
                    core,
                    ns,
                    "AllocateQubitArray",
                    Vec::new(),
                    qubit_init.span,
                );
                call_expr.id = assigner.next_node();
                create_qubit_alloc_call_expr(
                    assigner,
                    qubit_init.span,
                    call_expr,
                    Some(take(&mut expr)),
                )
            }
            QubitInitKind::Single => {
                let mut call_expr = create_gen_core_ref(
                    core,
                    ns,
                    "__quantum__rt__qubit_allocate",
                    Vec::new(),
                    qubit_init.span,
                );
                call_expr.id = assigner.next_node();
                create_qubit_alloc_call_expr(assigner, qubit_init.span, call_expr, None)
            }
            QubitInitKind::Tuple(tup) => Expr {
                id: assigner.next_node(),
                span: qubit_init.span,
                ty: qubit_init.ty,
                kind: ExprKind::Tuple(
                    tup.into_iter()
                        .map(|init| qubit_alloc_expr(assigner, core, init))
                        .collect(),
                ),
            },
            QubitInitKind::Err => panic!("QubitInitKind::Err"),
        }
    }

    StmtKind::Local(
        Mutability::Immutable,
        pat,
        qubit_alloc_expr(assigner, core, qubit_init),
    )
}

fn create_general_alloc_stmt(
    assigner: &mut Assigner,
    ident: &IdentTemplate,
    call_expr: Expr,
    array_size: Option<Expr>,
) -> Stmt {
    ident.gen_steppable_id_init(
        Mutability::Immutable,
        create_qubit_alloc_call_expr(assigner, ident.span, call_expr, array_size),
        assigner,
    )
}

fn create_qubit_alloc_call_expr(
    assigner: &mut Assigner,
    span: Span,
    call_expr: Expr,
    array_size: Option<Expr>,
) -> Expr {
    let ty = if array_size.is_none() {
        Ty::Prim(Prim::Qubit)
    } else {
        Ty::Array(Box::new(Ty::Prim(Prim::Qubit)))
    };
    Expr {
        id: assigner.next_node(),
        span,
        ty,
        kind: ExprKind::Call(
            Box::new(call_expr),
            Box::new(array_size.unwrap_or(Expr {
                id: assigner.next_node(),
                span,
                ty: Ty::UNIT,
                kind: ExprKind::Tuple(vec![]),
            })),
        ),
    }
}

fn create_general_dealloc_stmt(
    assigner: &mut Assigner,
    call_expr: Expr,
    ident: &IdentTemplate,
) -> Stmt {
    Stmt {
        id: assigner.next_node(),
        span: Span::default(),
        kind: StmtKind::Semi(Expr {
            id: assigner.next_node(),
            span: ident.span,
            ty: Ty::UNIT,
            kind: ExprKind::Call(Box::new(call_expr), Box::new(ident.gen_local_ref(assigner))),
        }),
    }
}
