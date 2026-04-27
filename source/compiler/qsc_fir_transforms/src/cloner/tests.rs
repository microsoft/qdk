// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use qsc_data_structures::{index_map::IndexMap, span::Span};
use qsc_fir::fir::{
    Block, BlockId, CallableDecl, CallableImpl, CallableKind, ExecGraph, Expr, ExprId, ExprKind,
    Item, LocalItemId, Mutability, NodeId, Pat, PatId, PatKind, SpecDecl, SpecImpl, Stmt, StmtId,
    StmtKind, Visibility,
};
use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, Prim, Ty};
use std::rc::Rc;

fn empty_exec_graph_range() -> std::ops::Range<qsc_fir::fir::ExecGraphIdx> {
    let zero = qsc_fir::fir::ExecGraphIdx {
        no_debug_idx: 0,
        debug_idx: 0,
    };
    zero..zero
}

/// Creates a minimal package with a single callable body for testing.
#[allow(clippy::similar_names)]
fn make_test_package() -> Package {
    let mut blocks: IndexMap<BlockId, Block> = IndexMap::new();
    let mut exprs: IndexMap<ExprId, Expr> = IndexMap::new();
    let mut pats: IndexMap<PatId, Pat> = IndexMap::new();
    let mut stmts: IndexMap<StmtId, Stmt> = IndexMap::new();

    // Pat 0: Bind(x) with LocalVarId 0
    let pat0 = Pat {
        id: PatId::from(0u32),
        span: Span::default(),
        ty: Ty::Prim(qsc_fir::ty::Prim::Int),
        kind: PatKind::Bind(Ident {
            id: LocalVarId::from(0u32),
            span: Span::default(),
            name: "x".into(),
        }),
    };
    pats.insert(PatId::from(0u32), pat0);

    // Expr 0: Var(Local(0)) — reference to x
    let expr0 = Expr {
        id: ExprId::from(0u32),
        span: Span::default(),
        ty: Ty::Prim(qsc_fir::ty::Prim::Int),
        kind: ExprKind::Var(Res::Local(LocalVarId::from(0u32)), vec![]),
        exec_graph_range: empty_exec_graph_range(),
    };
    exprs.insert(ExprId::from(0u32), expr0);

    // Expr 1: Lit(Int(42))
    let expr1 = Expr {
        id: ExprId::from(1u32),
        span: Span::default(),
        ty: Ty::Prim(qsc_fir::ty::Prim::Int),
        kind: ExprKind::Lit(qsc_fir::fir::Lit::Int(42)),
        exec_graph_range: empty_exec_graph_range(),
    };
    exprs.insert(ExprId::from(1u32), expr1);

    // Stmt 0: Local(Immutable, Pat 0, Expr 1) — let x = 42;
    let stmt0 = Stmt {
        id: StmtId::from(0u32),
        span: Span::default(),
        kind: StmtKind::Local(Mutability::Immutable, PatId::from(0u32), ExprId::from(1u32)),
        exec_graph_range: empty_exec_graph_range(),
    };
    stmts.insert(StmtId::from(0u32), stmt0);

    // Stmt 1: Expr(Expr 0) — x (tail expression)
    let stmt1 = Stmt {
        id: StmtId::from(1u32),
        span: Span::default(),
        kind: StmtKind::Expr(ExprId::from(0u32)),
        exec_graph_range: empty_exec_graph_range(),
    };
    stmts.insert(StmtId::from(1u32), stmt1);

    // Block 0: [Stmt 0, Stmt 1]
    let block0 = Block {
        id: BlockId::from(0u32),
        span: Span::default(),
        ty: Ty::Prim(qsc_fir::ty::Prim::Int),
        stmts: vec![StmtId::from(0u32), StmtId::from(1u32)],
    };
    blocks.insert(BlockId::from(0u32), block0);

    Package {
        items: IndexMap::new(),
        entry: None,
        entry_exec_graph: ExecGraph::default(),
        blocks,
        exprs,
        pats,
        stmts,
    }
}

#[test]
fn clone_block_produces_fresh_ids() {
    let source = make_test_package();
    let mut target = make_test_package();
    let mut cloner = FirCloner::new(&target);

    let new_block_id = cloner.clone_block(&source, BlockId::from(0u32), &mut target);

    // New block ID must differ from original.
    assert_ne!(u32::from(new_block_id), 0);

    // Target must contain the new block.
    assert!(target.blocks.get(new_block_id).is_some());

    // New block should have the same number of stmts.
    let new_block = target.blocks.get(new_block_id).expect("block not found");
    assert_eq!(new_block.stmts.len(), 2);

    // All new stmt IDs should be > the original max (1).
    for &stmt_id in &new_block.stmts {
        assert!(u32::from(stmt_id) > 1);
    }
}

#[test]
fn clone_pat_remaps_local_var_id() {
    let source = make_test_package();
    let mut target = make_test_package();
    // Use local_offset > 0 to simulate inlining into a caller that
    // already uses locals 0..N.
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));

    let new_pat_id = cloner.clone_pat(&source, PatId::from(0u32), &mut target);
    let new_pat = target.pats.get(new_pat_id).expect("pat not found");

    // The cloned pattern's Bind should have a fresh LocalVarId starting at 10.
    if let PatKind::Bind(ident) = &new_pat.kind {
        assert_eq!(ident.id, LocalVarId::from(10u32));
    } else {
        panic!("expected PatKind::Bind");
    }
}

#[test]
fn clone_pat_mono_local_starts_at_zero() {
    let source = make_test_package();
    let mut target = make_test_package();
    let mut cloner = FirCloner::new(&target);

    let new_pat_id = cloner.clone_pat(&source, PatId::from(0u32), &mut target);
    let new_pat = target.pats.get(new_pat_id).expect("pat not found");

    // For monomorphization, locals start at 0 (new callable scope).
    if let PatKind::Bind(ident) = &new_pat.kind {
        assert_eq!(ident.id, LocalVarId::from(0u32));
        // But the local_map should have recorded the mapping.
        assert!(cloner.local_map().contains_key(&LocalVarId::from(0u32)));
    } else {
        panic!("expected PatKind::Bind");
    }
}

#[test]
fn clone_expr_remaps_local_res() {
    let source = make_test_package();
    let mut target = make_test_package();
    // Use offset to ensure locals are remapped to distinct values.
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));

    // Clone the pat first so that the local mapping is established.
    let _new_pat = cloner.clone_pat(&source, PatId::from(0u32), &mut target);
    let new_expr_id = cloner.clone_expr(&source, ExprId::from(0u32), &mut target);
    let new_expr = target.exprs.get(new_expr_id).expect("expr not found");

    if let ExprKind::Var(Res::Local(var), _) = &new_expr.kind {
        // The local ref should be remapped to the offset value.
        assert_eq!(*var, LocalVarId::from(10u32));
    } else {
        panic!("expected ExprKind::Var(Res::Local(_))");
    }
}

#[test]
fn clone_preserves_cross_package_res() {
    let target = make_test_package();
    let cloner = FirCloner::new(&target);

    // Manually insert an expr that references a cross-package item.
    let cross_pkg_item = ItemId {
        package: qsc_fir::fir::PackageId::CORE,
        item: LocalItemId::from(5usize),
    };
    let cross_res = Res::Item(cross_pkg_item);
    let remapped = cloner.remap_res(&cross_res);
    assert_eq!(remapped, cross_res);
}

#[test]
fn self_item_remap_rewrites_item_resource() {
    let target = make_test_package();
    let mut cloner = FirCloner::new(&target);

    let old_item = ItemId {
        package: qsc_fir::fir::PackageId::from(2usize),
        item: LocalItemId::from(10usize),
    };
    let new_item = ItemId {
        package: qsc_fir::fir::PackageId::from(2usize),
        item: LocalItemId::from(20usize),
    };
    cloner.set_self_item_remap(old_item, new_item);

    let remapped = cloner.remap_res(&Res::Item(old_item));
    assert_eq!(remapped, Res::Item(new_item));

    // Other items should not be affected.
    let other_item = ItemId {
        package: qsc_fir::fir::PackageId::from(2usize),
        item: LocalItemId::from(11usize),
    };
    let remapped_other = cloner.remap_res(&Res::Item(other_item));
    assert_eq!(remapped_other, Res::Item(other_item));
}

#[test]
fn clone_closure_with_captures_remaps_local_ids() {
    let mut source = make_test_package();
    let mut target = make_test_package();

    // Add a second local binding: Pat 1: Bind(y) with LocalVarId 1
    let pat1 = Pat {
        id: PatId::from(1u32),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: PatKind::Bind(Ident {
            id: LocalVarId::from(1u32),
            span: Span::default(),
            name: "y".into(),
        }),
    };
    source.pats.insert(PatId::from(1u32), pat1.clone());
    target.pats.insert(PatId::from(1u32), pat1);

    // Expr 2: Lit(Int(10)) — initializer for y
    let expr2 = Expr {
        id: ExprId::from(2u32),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::Lit(qsc_fir::fir::Lit::Int(10)),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.exprs.insert(ExprId::from(2u32), expr2.clone());
    target.exprs.insert(ExprId::from(2u32), expr2);

    // Stmt 2: Local(Immutable, Pat 1, Expr 2) — let y = 10;
    let stmt2 = Stmt {
        id: StmtId::from(2u32),
        span: Span::default(),
        kind: StmtKind::Local(Mutability::Immutable, PatId::from(1u32), ExprId::from(2u32)),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.stmts.insert(StmtId::from(2u32), stmt2.clone());
    target.stmts.insert(StmtId::from(2u32), stmt2);

    // Expr 3: Closure capturing [LocalVarId(0), LocalVarId(1)], targeting LocalItemId(0)
    let expr3 = Expr {
        id: ExprId::from(3u32),
        span: Span::default(),
        ty: Ty::Arrow(Box::new(Arrow {
            kind: CallableKind::Function,
            input: Box::new(Ty::Prim(Prim::Int)),
            output: Box::new(Ty::Prim(Prim::Int)),
            functors: FunctorSet::Value(FunctorSetValue::Empty),
        })),
        kind: ExprKind::Closure(
            vec![LocalVarId::from(0u32), LocalVarId::from(1u32)],
            LocalItemId::from(0usize),
        ),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.exprs.insert(ExprId::from(3u32), expr3.clone());
    target.exprs.insert(ExprId::from(3u32), expr3);

    // Use offset so locals are remapped to distinct values.
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));

    // Clone the patterns first to establish the local mappings.
    let _new_pat0 = cloner.clone_pat(&source, PatId::from(0u32), &mut target);
    let _new_pat1 = cloner.clone_pat(&source, PatId::from(1u32), &mut target);

    // Clone the closure expression.
    let new_expr_id = cloner.clone_expr(&source, ExprId::from(3u32), &mut target);
    let new_expr = target.exprs.get(new_expr_id).expect("expr not found");

    // Verify captures are remapped.
    if let ExprKind::Closure(captures, _item_id) = &new_expr.kind {
        assert_eq!(captures.len(), 2);
        assert_eq!(captures[0], LocalVarId::from(10u32));
        assert_eq!(captures[1], LocalVarId::from(11u32));
    } else {
        panic!("expected ExprKind::Closure");
    }

    // Verify the expression type is preserved as Arrow.
    assert!(matches!(&new_expr.ty, Ty::Arrow(_)));
}

#[test]
#[allow(clippy::similar_names)]
#[allow(clippy::too_many_lines)]
fn clone_nested_item_isolates_local_scope() {
    let mut source = make_test_package();
    let mut target = make_test_package();

    // Build a nested callable item (inner function) with its own local binding.
    // Inner function body: let z = 99; z

    // Pat 2: Bind(z) with LocalVarId 0 (same as outer — scoped per-callable)
    let inner_pat = Pat {
        id: PatId::from(2u32),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: PatKind::Bind(Ident {
            id: LocalVarId::from(0u32),
            span: Span::default(),
            name: "z".into(),
        }),
    };
    source.pats.insert(PatId::from(2u32), inner_pat.clone());
    target.pats.insert(PatId::from(2u32), inner_pat);

    // Pat 3: Discard — inner function input pattern (no parameters)
    let inner_input_pat = Pat {
        id: PatId::from(3u32),
        span: Span::default(),
        ty: Ty::UNIT,
        kind: PatKind::Discard,
    };
    source
        .pats
        .insert(PatId::from(3u32), inner_input_pat.clone());
    target.pats.insert(PatId::from(3u32), inner_input_pat);

    // Expr 2: Lit(Int(99))
    let inner_init = Expr {
        id: ExprId::from(2u32),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::Lit(qsc_fir::fir::Lit::Int(99)),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.exprs.insert(ExprId::from(2u32), inner_init.clone());
    target.exprs.insert(ExprId::from(2u32), inner_init);

    // Expr 3: Var(Local(0)) — reference to z
    let inner_var = Expr {
        id: ExprId::from(3u32),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::Var(Res::Local(LocalVarId::from(0u32)), vec![]),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.exprs.insert(ExprId::from(3u32), inner_var.clone());
    target.exprs.insert(ExprId::from(3u32), inner_var);

    // Stmt 2: Local(Immutable, Pat 2, Expr 2)
    let inner_let = Stmt {
        id: StmtId::from(2u32),
        span: Span::default(),
        kind: StmtKind::Local(Mutability::Immutable, PatId::from(2u32), ExprId::from(2u32)),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.stmts.insert(StmtId::from(2u32), inner_let.clone());
    target.stmts.insert(StmtId::from(2u32), inner_let);

    // Stmt 3: Expr(Expr 3)
    let inner_tail = Stmt {
        id: StmtId::from(3u32),
        span: Span::default(),
        kind: StmtKind::Expr(ExprId::from(3u32)),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.stmts.insert(StmtId::from(3u32), inner_tail.clone());
    target.stmts.insert(StmtId::from(3u32), inner_tail);

    // Block 1: inner function body [Stmt 2, Stmt 3]
    let inner_block = Block {
        id: BlockId::from(1u32),
        span: Span::default(),
        ty: Ty::Prim(Prim::Int),
        stmts: vec![StmtId::from(2u32), StmtId::from(3u32)],
    };
    source
        .blocks
        .insert(BlockId::from(1u32), inner_block.clone());
    target.blocks.insert(BlockId::from(1u32), inner_block);

    // Item 0: Callable (inner function)
    let inner_callable = Item {
        id: LocalItemId::from(0usize),
        span: Span::default(),
        parent: None,
        doc: Rc::from(""),
        attrs: vec![],
        visibility: Visibility::Public,
        kind: ItemKind::Callable(Box::new(CallableDecl {
            id: NodeId::from(0u32),
            span: Span::default(),
            kind: CallableKind::Function,
            name: Ident {
                id: LocalVarId::default(),
                span: Span::default(),
                name: "inner".into(),
            },
            generics: vec![],
            input: PatId::from(3u32),
            output: Ty::Prim(Prim::Int),
            functors: FunctorSetValue::Empty,
            implementation: CallableImpl::Spec(SpecImpl {
                body: SpecDecl {
                    id: NodeId::from(1u32),
                    span: Span::default(),
                    block: BlockId::from(1u32),
                    input: None,
                    exec_graph: ExecGraph::default(),
                },
                adj: None,
                ctl: None,
                ctl_adj: None,
            }),
            attrs: vec![],
        })),
    };
    source
        .items
        .insert(LocalItemId::from(0usize), inner_callable.clone());
    target
        .items
        .insert(LocalItemId::from(0usize), inner_callable);

    // Add StmtKind::Item to the outer block so the nested item is reachable.
    let stmt_item = Stmt {
        id: StmtId::from(4u32),
        span: Span::default(),
        kind: StmtKind::Item(LocalItemId::from(0usize)),
        exec_graph_range: empty_exec_graph_range(),
    };
    source.stmts.insert(StmtId::from(4u32), stmt_item.clone());
    target.stmts.insert(StmtId::from(4u32), stmt_item);

    // Add Stmt 4 to Block 0 (the outer block)
    source
        .blocks
        .get_mut(BlockId::from(0u32))
        .expect("block 0")
        .stmts
        .push(StmtId::from(4u32));
    target
        .blocks
        .get_mut(BlockId::from(0u32))
        .expect("block 0")
        .stmts
        .push(StmtId::from(4u32));

    // Clone with an offset so outer locals are remapped to 10+.
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));

    // Clone the outer block (which includes the nested item via StmtKind::Item).
    let _outer_pat = cloner.clone_pat(&source, PatId::from(0u32), &mut target);
    let new_block_id = cloner.clone_block(&source, BlockId::from(0u32), &mut target);

    // Verify the outer local was remapped to 10.
    let outer_new_local = cloner.local_map().get(&LocalVarId::from(0u32)).copied();
    assert_eq!(
        outer_new_local,
        Some(LocalVarId::from(10u32)),
        "outer local should be remapped to offset 10"
    );

    // Verify that the nested item was cloned (should appear in item_map).
    assert!(
        !cloner.item_map().is_empty(),
        "nested item should have been cloned"
    );

    // Verify the cloned nested item's inner local bindings are independent:
    // The inner callable resets its locals to 0, so the inner Pat Bind(z)
    // should have LocalVarId(0), not 10+.
    let new_inner_item_id = cloner
        .item_map()
        .get(&LocalItemId::from(0usize))
        .expect("inner item should be in item_map");
    let new_inner_item = target
        .items
        .get(*new_inner_item_id)
        .expect("cloned inner item should exist");
    if let ItemKind::Callable(decl) = &new_inner_item.kind {
        if let CallableImpl::Spec(spec_impl) = &decl.implementation {
            let inner_block = target
                .blocks
                .get(spec_impl.body.block)
                .expect("inner block");
            // The first statement is the local binding with the inner Pat.
            let first_stmt = target.stmts.get(inner_block.stmts[0]).expect("inner stmt");
            if let StmtKind::Local(_, pat_id, _) = &first_stmt.kind {
                let inner_pat = target.pats.get(*pat_id).expect("inner pat");
                if let PatKind::Bind(ident) = &inner_pat.kind {
                    // Inner callable's locals start fresh at 0.
                    assert_eq!(
                        ident.id,
                        LocalVarId::from(0u32),
                        "inner callable's local should start at 0, not inherit outer offset"
                    );
                } else {
                    panic!("expected PatKind::Bind on inner local");
                }
            } else {
                panic!("expected StmtKind::Local as first inner stmt");
            }
        } else {
            panic!("expected CallableImpl::Spec");
        }
    } else {
        panic!("expected ItemKind::Callable");
    }

    // Verify the outer block's cloned stmts exist.
    let new_block = target.blocks.get(new_block_id).expect("new outer block");
    assert_eq!(new_block.stmts.len(), 3, "outer block should have 3 stmts");
}

/// Creates an empty package for use as a clone target.
fn empty_package() -> Package {
    Package {
        items: IndexMap::new(),
        entry: None,
        entry_exec_graph: ExecGraph::default(),
        blocks: IndexMap::new(),
        exprs: IndexMap::new(),
        pats: IndexMap::new(),
        stmts: IndexMap::new(),
    }
}

// ── Idempotency tests ──

#[test]
fn clone_block_is_idempotent() {
    let source = make_test_package();

    // First clone: source → target1.
    let mut target1 = empty_package();
    let mut cloner1 = FirCloner::new(&target1);
    let block1_id = cloner1.clone_block(&source, BlockId::from(0u32), &mut target1);

    // Second clone: target1 → target2.
    let mut target2 = empty_package();
    let mut cloner2 = FirCloner::new(&target2);
    let block2_id = cloner2.clone_block(&target1, block1_id, &mut target2);

    let block1 = target1.blocks.get(block1_id).expect("block1");
    let block2 = target2.blocks.get(block2_id).expect("block2");

    assert_eq!(block1.stmts.len(), block2.stmts.len());
    assert_eq!(block1.ty, block2.ty);

    // Statement kind discriminants must match.
    for (&s1, &s2) in block1.stmts.iter().zip(block2.stmts.iter()) {
        let stmt1 = target1.stmts.get(s1).expect("stmt1");
        let stmt2 = target2.stmts.get(s2).expect("stmt2");
        assert_eq!(
            std::mem::discriminant(&stmt1.kind),
            std::mem::discriminant(&stmt2.kind),
        );
    }

    // Element counts must match across both clones.
    assert_eq!(target1.exprs.iter().count(), target2.exprs.iter().count());
    assert_eq!(target1.pats.iter().count(), target2.pats.iter().count());
    assert_eq!(target1.stmts.iter().count(), target2.stmts.iter().count());
}

#[test]
fn clone_expr_is_idempotent_for_literal() {
    let source = make_test_package();

    // First clone of Expr 1: Lit(Int(42)).
    let mut target1 = empty_package();
    let mut cloner1 = FirCloner::new(&target1);
    let expr1_id = cloner1.clone_expr(&source, ExprId::from(1u32), &mut target1);

    // Second clone from target1.
    let mut target2 = empty_package();
    let mut cloner2 = FirCloner::new(&target2);
    let expr2_id = cloner2.clone_expr(&target1, expr1_id, &mut target2);

    let expr1 = target1.exprs.get(expr1_id).expect("expr1");
    let expr2 = target2.exprs.get(expr2_id).expect("expr2");

    assert_eq!(expr1.ty, expr2.ty);
    match (&expr1.kind, &expr2.kind) {
        (ExprKind::Lit(qsc_fir::fir::Lit::Int(v1)), ExprKind::Lit(qsc_fir::fir::Lit::Int(v2))) => {
            assert_eq!(v1, v2, "literal value must survive double-clone");
        }
        _ => panic!("expected Lit(Int) on both clones"),
    }
}

// ── Type preservation and structural assertion tests ──

#[test]
fn clone_preserves_expression_types() {
    let source = make_test_package();
    let mut target = empty_package();
    let mut cloner = FirCloner::new(&target);
    cloner.clone_block(&source, BlockId::from(0u32), &mut target);

    assert_eq!(
        target.exprs.iter().count(),
        source.exprs.iter().count(),
        "expression count must match"
    );

    let mut source_types: Vec<String> = source
        .exprs
        .iter()
        .map(|(_, e)| format!("{:?}", e.ty))
        .collect();
    let mut target_types: Vec<String> = target
        .exprs
        .iter()
        .map(|(_, e)| format!("{:?}", e.ty))
        .collect();
    source_types.sort();
    target_types.sort();
    assert_eq!(source_types, target_types, "expression types must match");
}

#[test]
fn clone_preserves_pattern_types_and_kinds() {
    let source = make_test_package();
    let mut target = empty_package();
    let mut cloner = FirCloner::new(&target);
    cloner.clone_block(&source, BlockId::from(0u32), &mut target);

    assert_eq!(
        target.pats.iter().count(),
        source.pats.iter().count(),
        "pattern count must match"
    );

    let mut source_types: Vec<String> = source
        .pats
        .iter()
        .map(|(_, p)| format!("{:?}", p.ty))
        .collect();
    let mut target_types: Vec<String> = target
        .pats
        .iter()
        .map(|(_, p)| format!("{:?}", p.ty))
        .collect();
    source_types.sort();
    target_types.sort();
    assert_eq!(source_types, target_types, "pattern types must match");

    let source_bind_count = source
        .pats
        .iter()
        .filter(|(_, p)| matches!(p.kind, PatKind::Bind(_)))
        .count();
    let target_bind_count = target
        .pats
        .iter()
        .filter(|(_, p)| matches!(p.kind, PatKind::Bind(_)))
        .count();
    assert_eq!(
        source_bind_count, target_bind_count,
        "bind pattern count must match"
    );
}

#[test]
#[allow(clippy::similar_names)]
fn clone_nested_item_preserves_callable_signature() {
    let mut source = make_test_package();

    // Add a Discard input pattern for the callable.
    let input_pat = Pat {
        id: PatId::from(2u32),
        span: Span::default(),
        ty: Ty::UNIT,
        kind: PatKind::Discard,
    };
    source.pats.insert(PatId::from(2u32), input_pat);

    // Add a callable item using block 0 as its body.
    let item = Item {
        id: LocalItemId::from(0usize),
        span: Span::default(),
        parent: None,
        doc: Rc::from(""),
        attrs: vec![],
        visibility: Visibility::Public,
        kind: ItemKind::Callable(Box::new(CallableDecl {
            id: NodeId::from(10u32),
            span: Span::default(),
            kind: CallableKind::Function,
            name: Ident {
                id: LocalVarId::default(),
                span: Span::default(),
                name: "test_fn".into(),
            },
            generics: vec![],
            input: PatId::from(2u32),
            output: Ty::Prim(Prim::Int),
            functors: FunctorSetValue::Empty,
            implementation: CallableImpl::Spec(SpecImpl {
                body: SpecDecl {
                    id: NodeId::from(11u32),
                    span: Span::default(),
                    block: BlockId::from(0u32),
                    input: None,
                    exec_graph: ExecGraph::default(),
                },
                adj: None,
                ctl: None,
                ctl_adj: None,
            }),
            attrs: vec![],
        })),
    };
    source.items.insert(LocalItemId::from(0usize), item);

    let mut target = empty_package();
    let mut cloner = FirCloner::new(&target);
    let new_item_id = cloner.clone_nested_item(&source, LocalItemId::from(0usize), &mut target);

    assert_eq!(target.items.iter().count(), 1, "cloned item count");

    let orig = source
        .items
        .get(LocalItemId::from(0usize))
        .expect("source item");
    let cloned = target.items.get(new_item_id).expect("cloned item");

    if let (ItemKind::Callable(orig_decl), ItemKind::Callable(new_decl)) =
        (&orig.kind, &cloned.kind)
    {
        assert_eq!(orig_decl.kind, new_decl.kind, "callable kind");
        assert_eq!(orig_decl.output, new_decl.output, "return type");
        assert_eq!(orig_decl.functors, new_decl.functors, "functors");
        assert_eq!(
            orig_decl.generics.len(),
            new_decl.generics.len(),
            "generics count"
        );

        // Verify the body block was cloned with matching stmt count.
        if let (CallableImpl::Spec(orig_spec), CallableImpl::Spec(new_spec)) =
            (&orig_decl.implementation, &new_decl.implementation)
        {
            let orig_block = source.blocks.get(orig_spec.body.block).expect("orig block");
            let new_block = target.blocks.get(new_spec.body.block).expect("new block");
            assert_eq!(
                orig_block.stmts.len(),
                new_block.stmts.len(),
                "body stmt count"
            );
        } else {
            panic!("expected CallableImpl::Spec on both");
        }
    } else {
        panic!("expected ItemKind::Callable on both");
    }
}
