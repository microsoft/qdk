// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{compile_to_fir, find_callable};
use qsc_data_structures::{index_map::IndexMap, span::Span};
use qsc_fir::fir::{
    Block, BlockId, CallableDecl, CallableImpl, ExecGraph, Expr, ExprId, ExprKind, LocalItemId,
    Mutability, Pat, PatId, PatKind, Stmt, StmtId, StmtKind,
};
use qsc_fir::ty::Ty;

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
    let (store, pkg_id) = compile_to_fir(
        "function Main() : Int { let a = 1; let b = 2; let f = (x) -> a + b + x; f(0) }",
    );
    let source = store.get(pkg_id);
    let main_block = body_block(find_callable(source, "Main"));

    let mut target = empty_package();
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));
    cloner.clone_block(source, main_block, &mut target);

    // Find the closure expression in the cloned output.
    let (new_captures, new_ty) = target
        .exprs
        .values()
        .find_map(|expr| match &expr.kind {
            ExprKind::Closure(caps, _) => Some((caps.clone(), expr.ty.clone())),
            _ => None,
        })
        .expect("no closure in cloned output");

    // Captures should be remapped starting at offset 10.
    assert_eq!(new_captures.len(), 2);
    assert_eq!(new_captures[0], LocalVarId::from(10u32));
    assert_eq!(new_captures[1], LocalVarId::from(11u32));

    // Arrow type is preserved.
    assert!(matches!(&new_ty, Ty::Arrow(_)));
}

#[test]
fn clone_nested_item_isolates_local_scope() {
    let (store, pkg_id) = compile_to_fir(
        "function Main() : Int {\
         let x = 42;\
         function Inner() : Int { let z = 99; z }\
         Inner()\
         }",
    );
    let source = store.get(pkg_id);
    let main_block = body_block(find_callable(source, "Main"));
    let inner_item_id = find_callable_item_id(source, "Inner");

    let mut target = empty_package();
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));
    let new_block_id = cloner.clone_block(source, main_block, &mut target);

    // Outer local "x" was remapped starting at offset 10.
    let x_local = source
        .pats
        .values()
        .find_map(|p| match &p.kind {
            PatKind::Bind(id) if id.name.as_ref() == "x" => Some(id.id),
            _ => None,
        })
        .expect("pat 'x' not found");
    assert_eq!(
        cloner.local_map()[&x_local],
        LocalVarId::from(10u32),
        "outer local should be remapped to offset 10"
    );

    // Nested item was cloned.
    assert!(
        cloner.item_map().contains_key(&inner_item_id),
        "nested item should have been cloned"
    );

    // Inner callable's locals start fresh at 0 (not inheriting outer offset).
    let new_inner_id = cloner.item_map()[&inner_item_id];
    let ItemKind::Callable(inner_decl) = &target
        .items
        .get(new_inner_id)
        .expect("expected cloned item")
        .kind
    else {
        panic!("expected callable")
    };
    let inner_block = target
        .blocks
        .get(body_block(inner_decl))
        .expect("expected body block");
    let first_stmt = target
        .stmts
        .get(inner_block.stmts[0])
        .expect("expected first stmt");
    if let StmtKind::Local(_, pat_id, _) = &first_stmt.kind {
        if let PatKind::Bind(ident) = &target.pats.get(*pat_id).expect("expected pattern").kind {
            assert_eq!(
                ident.id,
                LocalVarId::from(0u32),
                "inner callable's local should start at 0"
            );
        } else {
            panic!("expected PatKind::Bind on inner local");
        }
    } else {
        panic!("expected StmtKind::Local as first inner stmt");
    }

    // Outer block stmts were cloned.
    let new_block = target.blocks.get(new_block_id).expect("expected new block");
    assert!(
        new_block.stmts.len() >= 3,
        "outer block should have at least 3 stmts"
    );
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

/// Returns the `LocalItemId` for a callable with the given name.
fn find_callable_item_id(pkg: &Package, name: &str) -> LocalItemId {
    pkg.items
        .iter()
        .find_map(|(_, item)| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == name => Some(item.id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable '{name}' not found"))
}

/// Extracts the body block ID from a `CallableDecl` with a `Spec` implementation.
fn body_block(decl: &CallableDecl) -> BlockId {
    match &decl.implementation {
        CallableImpl::Spec(spec) => spec.body.block,
        _ => panic!("expected Spec implementation"),
    }
}

// ── Idempotency tests ──

#[test]
fn clone_block_is_idempotent() {
    fn bind_and_use_local(pkg: &Package) -> (LocalVarId, LocalVarId) {
        let bind = pkg
            .pats
            .values()
            .find_map(|p| match &p.kind {
                PatKind::Bind(ident) => Some(ident.id),
                _ => None,
            })
            .expect("cloned bind pattern should exist");
        let used = pkg
            .exprs
            .values()
            .find_map(|e| match &e.kind {
                ExprKind::Var(Res::Local(id), _) => Some(*id),
                _ => None,
            })
            .expect("cloned local use should exist");
        (bind, used)
    }

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

    // ID-remap integrity: after cloning, the `Var(Local)` use must resolve to
    // the *same* LocalVarId as the cloned `Bind` pattern — i.e. the reference
    // was remapped to the freshly cloned binding, not left pointing at a stale
    // source id. This consistency must hold identically across both clone
    // generations.
    let (bind1, use1) = bind_and_use_local(&target1);
    let (bind2, use2) = bind_and_use_local(&target2);
    assert_eq!(
        bind1, use1,
        "first clone must remap the local use to its freshly cloned binding"
    );
    assert_eq!(
        bind2, use2,
        "second clone must remap the local use to its freshly cloned binding"
    );

    // Element counts must match across both clones.
    assert_eq!(target1.exprs.iter().count(), target2.exprs.iter().count());
    assert_eq!(target1.pats.iter().count(), target2.pats.iter().count());
    assert_eq!(target1.stmts.iter().count(), target2.stmts.iter().count());
}

// ── Type preservation and structural assertion tests ──

#[test]
fn clone_preserves_expression_and_pattern_types() {
    let source = make_test_package();
    let mut target = empty_package();
    let mut cloner = FirCloner::new(&target);
    cloner.clone_block(&source, BlockId::from(0u32), &mut target);

    // Expression count and the multiset of expression types are preserved.
    assert_eq!(
        target.exprs.iter().count(),
        source.exprs.iter().count(),
        "expression count must match"
    );
    let mut source_expr_types: Vec<String> = source
        .exprs
        .iter()
        .map(|(_, e)| format!("{:?}", e.ty))
        .collect();
    let mut target_expr_types: Vec<String> = target
        .exprs
        .iter()
        .map(|(_, e)| format!("{:?}", e.ty))
        .collect();
    source_expr_types.sort();
    target_expr_types.sort();
    assert_eq!(
        source_expr_types, target_expr_types,
        "expression types must match"
    );

    // Pattern count and the multiset of pattern types are preserved.
    assert_eq!(
        target.pats.iter().count(),
        source.pats.iter().count(),
        "pattern count must match"
    );
    let mut source_pat_types: Vec<String> = source
        .pats
        .iter()
        .map(|(_, p)| format!("{:?}", p.ty))
        .collect();
    let mut target_pat_types: Vec<String> = target
        .pats
        .iter()
        .map(|(_, p)| format!("{:?}", p.ty))
        .collect();
    source_pat_types.sort();
    target_pat_types.sort();
    assert_eq!(
        source_pat_types, target_pat_types,
        "pattern types must match"
    );

    // Bind-pattern kind counts are preserved.
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
fn clone_nested_item_preserves_callable_signature() {
    let (store, pkg_id) = compile_to_fir(
        "function Main() : Int { function Inner() : Int { let x = 42; x } Inner() }",
    );
    let source = store.get(pkg_id);
    let inner_id = find_callable_item_id(source, "Inner");
    let orig = find_callable(source, "Inner");

    let mut target = empty_package();
    let mut cloner = FirCloner::new(&target);
    let new_item_id = cloner.clone_nested_item(source, inner_id, &mut target);

    let ItemKind::Callable(cloned_target) = &target
        .items
        .get(new_item_id)
        .expect("expected cloned item")
        .kind
    else {
        panic!("expected callable")
    };

    assert_eq!(orig.kind, cloned_target.kind, "callable kind");
    assert_eq!(orig.output, cloned_target.output, "return type");
    assert_eq!(orig.functors, cloned_target.functors, "functors");
    assert_eq!(
        orig.generics.len(),
        cloned_target.generics.len(),
        "generics count"
    );
    assert_eq!(
        source
            .blocks
            .get(body_block(orig))
            .expect("expected body block")
            .stmts
            .len(),
        target
            .blocks
            .get(body_block(cloned_target))
            .expect("expected body block")
            .stmts
            .len(),
        "body stmt count"
    );
}
