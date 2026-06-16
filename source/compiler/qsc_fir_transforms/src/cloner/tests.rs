// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{compile_to_fir, find_callable};
use qsc_fir::fir::{
    BlockId, CallableDecl, CallableImpl, ExprId, ExprKind, LocalItemId, PatId, PatKind, StmtKind,
};
use qsc_fir::ty::Ty;

/// Q# source whose `Main` body is `{ let x = 42; x }` — a `let`-binding plus a
/// tail expression that reads the bound local.
const LET_X_SOURCE: &str = "function Main() : Int { let x = 42; x }";

#[test]
fn clone_block_produces_fresh_ids() {
    let (store, pkg_id) = compile_to_fir(LET_X_SOURCE);
    let source = store.get(pkg_id);
    let main_block = body_block(find_callable(source, "Main"));

    let mut target = Package::default();
    let mut cloner = FirCloner::new(&target);

    let new_block_id = cloner.clone_block(source, main_block, &mut target);

    // The cloned block is inserted into the target package.
    let new_block = target.blocks.get(new_block_id).expect("block not found");

    // `let x = 42;` plus the tail expression `x`.
    assert_eq!(new_block.stmts.len(), 2);

    // Every cloned statement ID resolves to a freshly inserted statement
    // in the target package.
    for &stmt_id in &new_block.stmts {
        assert!(
            target.stmts.get(stmt_id).is_some(),
            "cloned stmt should exist in target"
        );
    }
}

#[test]
fn clone_pat_remaps_local_var_id() {
    let (store, pkg_id) = compile_to_fir(LET_X_SOURCE);
    let source = store.get(pkg_id);
    let (x_pat, _x_local) = find_local_bind(source, "x");

    let mut target = Package::default();
    // Use a local offset > 0 to simulate inlining into a caller that
    // already uses locals 0..N.
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));

    let new_pat_id = cloner.clone_pat(source, x_pat, &mut target);
    let new_pat = target.pats.get(new_pat_id).expect("pat not found");

    // The cloned pattern's Bind should have a fresh LocalVarId starting at the offset.
    if let PatKind::Bind(ident) = &new_pat.kind {
        assert_eq!(ident.id, LocalVarId::from(10u32));
    } else {
        panic!("expected PatKind::Bind");
    }
}

#[test]
fn clone_pat_mono_local_starts_at_zero() {
    let (store, pkg_id) = compile_to_fir(LET_X_SOURCE);
    let source = store.get(pkg_id);
    let (x_pat, x_local) = find_local_bind(source, "x");

    let mut target = Package::default();
    let mut cloner = FirCloner::new(&target);

    let new_pat_id = cloner.clone_pat(source, x_pat, &mut target);
    let new_pat = target.pats.get(new_pat_id).expect("pat not found");

    // For monomorphization, locals start at 0 (new callable scope).
    if let PatKind::Bind(ident) = &new_pat.kind {
        assert_eq!(ident.id, LocalVarId::from(0u32));
        // But the local_map should have recorded the source -> fresh mapping.
        assert!(cloner.local_map().contains_key(&x_local));
    } else {
        panic!("expected PatKind::Bind");
    }
}

#[test]
fn clone_expr_remaps_local_res() {
    let (store, pkg_id) = compile_to_fir(LET_X_SOURCE);
    let source = store.get(pkg_id);
    let (x_pat, x_local) = find_local_bind(source, "x");
    let x_expr = find_local_var_expr(source, x_local);

    let mut target = Package::default();
    // Use an offset to ensure locals are remapped to distinct values.
    let mut cloner = FirCloner::with_local_offset(&target, LocalVarId::from(10u32));

    // Clone the pat first so that the local mapping is established.
    let _new_pat = cloner.clone_pat(source, x_pat, &mut target);
    let new_expr_id = cloner.clone_expr(source, x_expr, &mut target);
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
    let target = Package::default();
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
    let target = Package::default();
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

    let mut target = Package::default();
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

    let mut target = Package::default();
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

/// Finds the `let`-binding pattern for a local with the given name, returning
/// its `PatId` and the bound `LocalVarId`.
fn find_local_bind(pkg: &Package, name: &str) -> (PatId, LocalVarId) {
    pkg.pats
        .iter()
        .find_map(|(pat_id, pat)| match &pat.kind {
            PatKind::Bind(ident) if ident.name.as_ref() == name => Some((pat_id, ident.id)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("local binding '{name}' not found"))
}

/// Finds the expression that reads the given local variable.
fn find_local_var_expr(pkg: &Package, local: LocalVarId) -> ExprId {
    pkg.exprs
        .iter()
        .find_map(|(expr_id, expr)| match &expr.kind {
            ExprKind::Var(Res::Local(id), _) if *id == local => Some(expr_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("var expression for local not found"))
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

    let (store, pkg_id) = compile_to_fir(LET_X_SOURCE);
    let source = store.get(pkg_id);
    let source_block = body_block(find_callable(source, "Main"));

    // First clone: source → target1.
    let mut target1 = Package::default();
    let mut cloner1 = FirCloner::new(&target1);
    let block1_id = cloner1.clone_block(source, source_block, &mut target1);

    // Second clone: target1 → target2.
    let mut target2 = Package::default();
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
    let (store, pkg_id) = compile_to_fir(LET_X_SOURCE);
    let compiled = store.get(pkg_id);
    let main_block = body_block(find_callable(compiled, "Main"));

    // Seed a minimal `source` package containing exactly the block's reachable
    // nodes, so the whole-package multiset comparisons below stay meaningful.
    let mut source = Package::default();
    let mut seed = FirCloner::new(&source);
    let source_block = seed.clone_block(compiled, main_block, &mut source);

    let mut target = Package::default();
    let mut cloner = FirCloner::new(&target);
    cloner.clone_block(&source, source_block, &mut target);

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

    let mut target = Package::default();
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
