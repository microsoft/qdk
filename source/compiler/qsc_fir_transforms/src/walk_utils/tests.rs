// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::fir_builder::{
    alloc_assign_expr, alloc_block, alloc_bool_lit, alloc_expr, alloc_expr_stmt, alloc_if_expr,
    alloc_local_var_expr,
};
use crate::test_utils::compile_to_fir;
use crate::test_utils::find_callable_body_block as find_callable_block;
use expect_test::expect;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{CallableDecl, CallableImpl, CallableKind, ItemKind, Lit, PatKind};
use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, Prim, Ty};
use std::rc::Rc;

/// Finds the `LocalVarId` for the first pattern binding with the given name.
fn find_local_var(package: &Package, name: &str) -> LocalVarId {
    for pat in package.pats.values() {
        if let PatKind::Bind(ident) = &pat.kind
            && ident.name.as_ref() == name
        {
            return ident.id;
        }
    }
    panic!("local var '{name}' not found");
}

/// Finds the [`CallableDecl`] of the named callable in the user package.
fn find_callable_decl<'a>(package: &'a Package, name: &str) -> &'a CallableDecl {
    for item in package.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == name
        {
            return decl;
        }
    }
    panic!("callable '{name}' not found");
}

#[test]
fn field_only_access_classified_as_field_use() {
    let (store, pkg_id) = compile_to_fir(
        "struct Pair { X : Int, Y : Int }
             function Main() : Int {
                 let p = new Pair { X = 1, Y = 2 };
                 p.X + p.Y
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "p");
    let class = classify_block_use(package, block_id, local_id);
    // Both p.X and p.Y are field-only accesses; the aggregate is field-only.
    assert_eq!(class, UseClass::FieldOnly);
}

#[test]
fn whole_value_use_as_function_argument() {
    let (store, pkg_id) = compile_to_fir(
        "function Consume(t : (Int, Int)) : Int {
                 let (a, b) = t;
                 a + b
             }
             function Main() : Int {
                 let t = (1, 2);
                 Consume(t)
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "t");
    let class = classify_block_use(package, block_id, local_id);
    // t is passed directly to Consume — whole-value use, so the aggregate is general.
    assert_eq!(class, UseClass::GeneralUse);
}

#[test]
fn decomposable_assign_tuple_literal_rhs() {
    let (store, pkg_id) = compile_to_fir(
        "function Main() : (Int, Int) {
                 mutable t = (1, 2);
                 t = (3, 4);
                 t
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "t");
    let class = classify_block_use(package, block_id, local_id);
    // set t = (3, 4) is decomposable (field-only) and the final `t` is a
    // whole-value read; the aggregate collapses to general use.
    assert_eq!(class, UseClass::GeneralUse);
}

#[test]
fn mutable_reassignment_non_tuple_rhs_classified_as_general_use() {
    // Reassigning a mutable local from a *non*-tuple-literal right-hand side
    // (here another whole value) is a promotion-blocking whole-value write,
    // unlike the decomposable tuple-literal reassignment above.
    let (store, pkg_id) = compile_to_fir(
        "function Main() : (Int, Int) {
                 let src = (5, 6);
                 mutable t = (1, 2);
                 t = src;
                 t
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "t");

    // Aggregate: the blocking write forces the whole aggregate to general use.
    assert_eq!(
        classify_block_use(package, block_id, local_id),
        UseClass::GeneralUse
    );

    // Per-site: the reassignment is a `HardBlock`, the trailing read a
    // `WholeValueRead`.
    let mut uses = Vec::new();
    classify_uses_in_block(package, block_id, local_id, &mut uses);
    assert_eq!(variant_names(&uses), ["HardBlock", "WholeValueRead"]);
}

#[test]
fn closure_capture_classified_as_whole_use() {
    let (store, pkg_id) = compile_to_fir(
        "function Apply(f : Int -> Int, x : Int) : Int { f(x) }
             function Main() : Int {
                 let y = 5;
                 let f = x -> x + y;
                 Apply(f, 10)
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "y");
    let class = classify_block_use(package, block_id, local_id);
    // y is captured by the closure — whole-value use, so the aggregate is general.
    assert_eq!(class, UseClass::GeneralUse);
}

#[test]
fn nested_field_access_classified_as_field_use() {
    let (store, pkg_id) = compile_to_fir(
        "struct Inner { X : Int }
             struct Outer { I : Inner }
             function Main() : Int {
                 let o = new Outer { I = new Inner { X = 42 } };
                 o.I.X
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "o");
    let class = classify_block_use(package, block_id, local_id);
    // o.I.X is a nested field access — still field-only.
    assert_eq!(class, UseClass::FieldOnly);
}

#[test]
fn deeply_nested_field_access_classified_as_field_use() {
    // A three-level field projection (`c.B.A.V`) must remain field-only: each
    // intermediate projection keeps the access on the field path rather than
    // escalating to a whole-value read.
    let (store, pkg_id) = compile_to_fir(
        "struct Leaf { V : Int }
             struct Mid { L : Leaf }
             struct Top { M : Mid }
             function Main() : Int {
                 let c = new Top { M = new Mid { L = new Leaf { V = 7 } } };
                 c.M.L.V
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "c");
    let class = classify_block_use(package, block_id, local_id);
    assert_eq!(class, UseClass::FieldOnly);
}

#[test]
fn walker_visits_nested_expression_kinds_in_program() {
    let (store, pkg_id) = compile_to_fir(
        "function Main() : Int {
                 let x = 1 + 2;
                 let t = (x, 3);
                 if x > 0 { 10 } else { 20 }
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");

    let mut kinds: Vec<String> = Vec::new();
    for_each_expr_in_block(package, block_id, &mut |_id, expr| {
        let kind_str = match &expr.kind {
            ExprKind::Array(_) => "Array",
            ExprKind::ArrayLit(_) => "ArrayLit",
            ExprKind::ArrayRepeat(_, _) => "ArrayRepeat",
            ExprKind::Assign(_, _) => "Assign",
            ExprKind::AssignOp(_, _, _) => "AssignOp",
            ExprKind::AssignField(_, _, _) => "AssignField",
            ExprKind::AssignIndex(_, _, _) => "AssignIndex",
            ExprKind::BinOp(_, _, _) => "BinOp",
            ExprKind::Block(_) => "Block",
            ExprKind::Call(_, _) => "Call",
            ExprKind::Closure(_, _) => "Closure",
            ExprKind::Fail(_) => "Fail",
            ExprKind::Field(_, _) => "Field",
            ExprKind::Hole => "Hole",
            ExprKind::If(_, _, _) => "If",
            ExprKind::Index(_, _) => "Index",
            ExprKind::Lit(_) => "Lit",
            ExprKind::Range(_, _, _) => "Range",
            ExprKind::Return(_) => "Return",
            ExprKind::Struct(_, _, _) => "Struct",
            ExprKind::String(_) => "String",
            ExprKind::UpdateIndex(_, _, _) => "UpdateIndex",
            ExprKind::Tuple(_) => "Tuple",
            ExprKind::UnOp(_, _) => "UnOp",
            ExprKind::UpdateField(_, _, _) => "UpdateField",
            ExprKind::Var(_, _) => "Var",
            ExprKind::While(_, _) => "While",
        };
        kinds.push(kind_str.to_string());
    });
    kinds.sort();
    expect![[r#"
            [
                "BinOp",
                "BinOp",
                "Block",
                "Block",
                "If",
                "Lit",
                "Lit",
                "Lit",
                "Lit",
                "Lit",
                "Lit",
                "Tuple",
                "Var",
                "Var",
            ]
        "#]]
    .assert_debug_eq(&kinds);
}

#[test]
fn assigner_ids_do_not_collide_with_existing_package_ids() {
    let (store, pkg_id) = compile_to_fir("function Main() : Int { 1 + 2 }");
    let package = store.get(pkg_id);
    let mut assigner = Assigner::from_package(package);

    // Assigner::from_package tracks expr, stmt, pat, and local IDs.
    let new_expr = assigner.next_expr();
    let new_stmt = assigner.next_stmt();
    let new_pat = assigner.next_pat();
    let new_local = assigner.next_local();

    // Verify allocated IDs are strictly beyond all existing IDs.
    let max_expr = package
        .exprs
        .iter()
        .map(|(id, _)| u32::from(id))
        .max()
        .unwrap_or(0);
    let max_stmt = package
        .stmts
        .iter()
        .map(|(id, _)| u32::from(id))
        .max()
        .unwrap_or(0);
    let max_pat = package
        .pats
        .iter()
        .map(|(id, _)| u32::from(id))
        .max()
        .unwrap_or(0);

    let mut max_local: u32 = 0;
    for pat in package.pats.values() {
        if let PatKind::Bind(ident) = &pat.kind {
            max_local = max_local.max(u32::from(ident.id));
        }
    }

    assert!(
        u32::from(new_expr) > max_expr,
        "new expr {new_expr} should be > max existing {max_expr}"
    );
    assert!(
        u32::from(new_stmt) > max_stmt,
        "new stmt {new_stmt} should be > max existing {max_stmt}"
    );
    assert!(
        u32::from(new_pat) > max_pat,
        "new pat {new_pat} should be > max existing {max_pat}"
    );
    assert!(
        u32::from(new_local) > max_local,
        "new local {new_local} should be > max existing {max_local}"
    );
}

#[test]
fn collect_entry_expr_ids_returns_all_entry_descendants() {
    let (store, pkg_id) = compile_to_fir(
        "function Main() : Int {
             let x = 1 + 2;
             x
         }",
    );
    let package = store.get(pkg_id);
    let ids = collect_expr_ids_in_entry(package);
    // The entry expression wraps the call to Main. The collected set must
    // contain the entry expression itself plus the specific descendant kinds:
    // the `Call` node and the callee `Var` that resolves to the `Main` item.
    let entry_id = package
        .entry
        .expect("program should have an entry expression");
    assert!(
        ids.contains(&entry_id),
        "collected entry IDs should include the entry expression itself"
    );
    // All returned IDs should be valid expression IDs in the package.
    for &id in &ids {
        let _ = package.get_expr(id);
    }
    let has_call = ids
        .iter()
        .any(|&id| matches!(&package.get_expr(id).kind, ExprKind::Call(_, _)));
    assert!(
        has_call,
        "entry descendants should include the Call to Main"
    );
    let calls_main = ids.iter().any(|&id| {
        let ExprKind::Var(Res::Item(item_id), _) = &package.get_expr(id).kind else {
            return false;
        };
        matches!(
            &package.get_item(item_id.item).kind,
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main"
        )
    });
    assert!(
        calls_main,
        "entry descendants should include the callee Var resolving to Main"
    );
}

#[test]
fn collect_callable_expr_ids_covers_all_specs() {
    let (store, pkg_id) = compile_to_fir(
        "operation Op() : Unit is Adj + Ctl {
             body ... { Message(\"body\"); }
             adjoint ... { Message(\"adj\"); }
             controlled (cs, ...) { Message(\"ctl\"); }
         }
         operation Main() : Unit { Op(); }",
    );
    let package = store.get(pkg_id);

    // Find Op's LocalItemId.
    let op_local_id = package
        .items
        .iter()
        .find_map(|(id, item)| {
            if let ItemKind::Callable(decl) = &item.kind
                && decl.name.name.as_ref() == "Op"
            {
                return Some(id);
            }
            None
        })
        .expect("Op callable not found");

    let ids = collect_expr_ids_in_local_callables(package, &[op_local_id]);
    // No duplicates.
    let unique: FxHashSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "expression IDs should be unique");

    // Each of the three specs (body, adj, ctl) contains a distinct
    // `Message("...")` call. Collecting the string-literal payloads of every
    // visited expression must therefore surface all three markers, proving the
    // walk actually descended into each specialization rather than merely
    // counting three expressions from one spec.
    let mut markers: Vec<String> = Vec::new();
    for &id in &ids {
        if let ExprKind::String(components) = &package.get_expr(id).kind
            && let [StringComponent::Lit(text)] = components.as_slice()
        {
            markers.push(text.to_string());
        }
    }
    markers.sort();
    // The auto-generated controlled-adjoint spec re-derives the adjoint body, so
    // the "adj" marker can appear more than once; dedup to assert that each
    // user-written spec's marker was visited at least once.
    markers.dedup();
    assert_eq!(
        markers,
        ["adj", "body", "ctl"],
        "every specialization's Message marker should be visited"
    );
}

#[test]
fn extend_does_not_duplicate_seen_ids() {
    let (store, pkg_id) = compile_to_fir(
        "function Helper() : Int { 42 }
         function Main() : Int { Helper() }",
    );
    let package = store.get(pkg_id);

    // Collect all local callable IDs.
    let local_ids: Vec<_> = package
        .items
        .iter()
        .filter_map(|(id, item)| {
            if let ItemKind::Callable(_) = &item.kind {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    // First collection.
    let mut ids = Vec::new();
    let mut seen = FxHashSet::default();
    extend_expr_ids_in_local_callables(package, &local_ids, &mut ids, &mut seen);
    let first_count = ids.len();
    assert!(first_count > 0, "should collect some expression IDs");

    // Second extension with same callables — should add nothing.
    extend_expr_ids_in_local_callables(package, &local_ids, &mut ids, &mut seen);
    assert_eq!(
        ids.len(),
        first_count,
        "second extension should not add duplicates"
    );
}

#[test]
fn empty_local_items_returns_empty() {
    let (store, pkg_id) = compile_to_fir("function Main() : Int { 1 }");
    let package = store.get(pkg_id);
    let ids = collect_expr_ids_in_local_callables(package, &[]);
    assert!(ids.is_empty(), "empty item list should yield empty result");
}

/// Maps each [`ParamUse`] to a stable variant name, discarding the
/// non-deterministic [`ExprId`] inside [`ParamUse::WholeValueRead`] so the
/// classification order can be asserted without snapshot brittleness.
fn variant_names(uses: &[ParamUse]) -> Vec<&'static str> {
    uses.iter()
        .map(|u| match u {
            ParamUse::FieldAccess => "FieldAccess",
            ParamUse::WholeValueRead(_) => "WholeValueRead",
            ParamUse::HardBlock => "HardBlock",
        })
        .collect()
}

#[test]
fn classify_field_projection_is_field_access() {
    let (store, pkg_id) = compile_to_fir(
        "struct Pair { X : Int, Y : Int }
             function Main() : Int {
                 let p = new Pair { X = 1, Y = 2 };
                 p.X + p.Y
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "p");
    let mut uses = Vec::new();
    classify_uses_in_block(package, block_id, local_id, &mut uses);
    // p.X and p.Y are both field projections.
    assert_eq!(variant_names(&uses), ["FieldAccess", "FieldAccess"]);
}

#[test]
fn classify_whole_value_read_is_whole_value_read() {
    let (store, pkg_id) = compile_to_fir(
        "function Consume(t : (Int, Int)) : Int {
                 let (a, b) = t;
                 a + b
             }
             function Main() : Int {
                 let t = (1, 2);
                 Consume(t)
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "t");
    let mut uses = Vec::new();
    classify_uses_in_block(package, block_id, local_id, &mut uses);
    // t is passed by value as a call argument — a bare whole-value read.
    assert_eq!(variant_names(&uses), ["WholeValueRead"]);
    // The recorded ExprId must resolve to the `t` Var read in the package.
    let ParamUse::WholeValueRead(expr_id) = uses[0] else {
        panic!("expected WholeValueRead, got {:?}", uses[0]);
    };
    assert!(
        matches!(
            &package.get_expr(expr_id).kind,
            ExprKind::Var(Res::Local(v), _) if *v == local_id
        ),
        "WholeValueRead must point at the local's Var read"
    );
}

#[test]
fn classify_closure_capture_is_hard_block() {
    let (store, pkg_id) = compile_to_fir(
        "function Apply(f : Int -> Int, x : Int) : Int { f(x) }
             function Main() : Int {
                 let y = 5;
                 let f = x -> x + y;
                 Apply(f, 10)
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "y");
    let mut uses = Vec::new();
    classify_uses_in_block(package, block_id, local_id, &mut uses);
    // y is captured by the closure — a hard block on promotion.
    assert_eq!(variant_names(&uses), ["HardBlock"]);
}

#[test]
fn classify_local_used_only_in_struct_field_is_recorded() {
    // Regression guard: a local used only inside a struct-literal field value must
    // be classified (as a whole-value read), not silently dropped. Before the
    // fix, `ExprKind::Struct` was not recursed and this produced an empty list.
    let (store, pkg_id) = compile_to_fir(
        "struct Wrapper { V : Int }
             function Main() : Wrapper {
                 let n = 42;
                 new Wrapper { V = n }
             }",
    );
    let package = store.get(pkg_id);
    let block_id = find_callable_block(package, "Main");
    let local_id = find_local_var(package, "n");
    let mut uses = Vec::new();
    classify_uses_in_block(package, block_id, local_id, &mut uses);
    // n flows into the struct field by value — recorded as a whole-value read.
    assert_eq!(variant_names(&uses), ["WholeValueRead"]);
}

// ---------------------------------------------------------------------------
// Direct unit tests for `expr_is_side_effect_free` and
// `expr_is_safe_to_discard`.
//
// At least five positive and five negative shapes are covered to pin both
// the duplicate-safe and discard-safe purity contracts.
// ---------------------------------------------------------------------------

/// Allocate an `Int` literal `ExprId`.
fn int_lit(package: &mut Package, assigner: &mut Assigner, value: i64) -> ExprId {
    alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::Int),
        ExprKind::Lit(Lit::Int(value)),
        Span::default(),
    )
}

#[test]
fn given_lit_is_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let e = int_lit(&mut package, &mut assigner, 1);
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_var_is_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let some_local = assigner.next_local();
    let e = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        some_local,
        Ty::Prim(Prim::Int),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_tuple_of_lits_is_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let a = int_lit(&mut package, &mut assigner, 1);
    let b = int_lit(&mut package, &mut assigner, 2);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::Int)]),
        ExprKind::Tuple(vec![a, b]),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_array_of_lits_is_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let a = int_lit(&mut package, &mut assigner, 1);
    let b = int_lit(&mut package, &mut assigner, 2);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Array(Box::new(Ty::Prim(Prim::Int))),
        ExprKind::Array(vec![a, b]),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_block_with_single_lit_is_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let lit = int_lit(&mut package, &mut assigner, 7);
    let stmt = alloc_expr_stmt(&mut package, &mut assigner, lit, Span::default());
    let bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![stmt],
        Ty::Prim(Prim::Int),
        Span::default(),
    );
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Int),
        ExprKind::Block(bid),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_closure_is_side_effect_free() {
    // Closure construction itself is pure: capturing a local does not
    // invoke the closure body.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let some_local = assigner.next_local();
    let closure_ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(Ty::UNIT),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        closure_ty,
        ExprKind::Closure(vec![some_local], LocalItemId::from(0)),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_call_is_not_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let arrow_ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(Ty::Prim(Prim::Int)),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    let callee = alloc_expr(
        &mut package,
        &mut assigner,
        arrow_ty,
        ExprKind::Hole,
        Span::default(),
    );
    let arg = int_lit(&mut package, &mut assigner, 0);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Int),
        ExprKind::Call(callee, arg),
        Span::default(),
    );
    assert!(!expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

/// Finds the first call expression whose direct callee resolves to the named
/// callable in the same package.
fn find_call_to_callable(package: &Package, pkg_id: PackageId, name: &str) -> ExprId {
    package
        .exprs
        .iter()
        .find_map(|(expr_id, expr)| {
            let ExprKind::Call(callee_id, _) = expr.kind else {
                return None;
            };
            let ExprKind::Var(Res::Item(item_id), _) = &package.get_expr(callee_id).kind else {
                return None;
            };
            if item_id.package != pkg_id {
                return None;
            }
            let ItemKind::Callable(decl) = &package.get_item(item_id.item).kind else {
                return None;
            };
            (decl.name.name.as_ref() == name).then_some(expr_id)
        })
        .unwrap_or_else(|| panic!("call to callable '{name}' not found"))
}

#[test]
fn given_known_pure_function_call_is_side_effect_free_and_safe_to_discard() {
    let (store, pkg_id) = compile_to_fir(
        "function Pure(x : Int) : Int { x + 1 }
         function Main() : Int { Pure(41) }",
    );
    let package = store.get(pkg_id);
    let call = find_call_to_callable(package, pkg_id, "Pure");

    assert!(expr_is_side_effect_free(package, pkg_id, call));
    assert!(expr_is_safe_to_discard(package, pkg_id, call));
}

#[test]
fn given_function_call_with_local_mutation_is_side_effect_free() {
    let (store, pkg_id) = compile_to_fir(
        "function PureWithLocalMutation(x : Int) : Int {
             mutable y = x;
             set y += 1;
             y
         }
         function Main() : Int { PureWithLocalMutation(41) }",
    );
    let package = store.get(pkg_id);
    let call = find_call_to_callable(package, pkg_id, "PureWithLocalMutation");

    assert!(expr_is_side_effect_free(package, pkg_id, call));
    assert!(expr_is_safe_to_discard(package, pkg_id, call));
}

#[test]
fn given_fallible_pure_function_call_is_side_effect_free_but_not_safe_to_discard() {
    let (store, pkg_id) = compile_to_fir(
        "function DivideBy(x : Int) : Int { 1 / x }
         function Main() : Int { DivideBy(0) }",
    );
    let package = store.get(pkg_id);
    let call = find_call_to_callable(package, pkg_id, "DivideBy");

    assert!(expr_is_side_effect_free(package, pkg_id, call));
    assert!(!expr_is_safe_to_discard(package, pkg_id, call));
}

#[test]
fn given_function_call_that_can_fail_is_side_effect_free_but_not_safe_to_discard() {
    let (store, pkg_id) = compile_to_fir(
        "function FailOnZero(x : Int) : Int {
             if x == 0 { fail \"zero\" } else { x }
         }
         function Main() : Int { FailOnZero(0) }",
    );
    let package = store.get(pkg_id);
    let call = find_call_to_callable(package, pkg_id, "FailOnZero");

    assert!(expr_is_side_effect_free(package, pkg_id, call));
    assert!(!expr_is_safe_to_discard(package, pkg_id, call));
}

#[test]
fn given_function_call_that_calls_message_is_not_side_effect_free() {
    let (store, pkg_id) = compile_to_fir(
        "function Noisy(x : Int) : Int {
             Message(\"x\");
             x
         }
         function Main() : Int { Noisy(41) }",
    );
    let package = store.get(pkg_id);
    let call = find_call_to_callable(package, pkg_id, "Noisy");

    assert!(!expr_is_side_effect_free(package, pkg_id, call));
    assert!(!expr_is_safe_to_discard(package, pkg_id, call));
}

#[test]
fn given_assign_is_not_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let some_local = assigner.next_local();
    let lhs = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        some_local,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let rhs = alloc_bool_lit(&mut package, &mut assigner, true, Span::default());
    let e = alloc_assign_expr(&mut package, &mut assigner, lhs, rhs, Span::default());
    assert!(!expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_return_is_not_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let inner = int_lit(&mut package, &mut assigner, 1);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::Return(inner),
        Span::default(),
    );
    assert!(!expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_fail_is_not_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    // Construct a String("boom") literal to feed Fail.
    let msg = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::String),
        ExprKind::String(vec![StringComponent::Lit(Rc::from("boom"))]),
        Span::default(),
    );
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::Fail(msg),
        Span::default(),
    );
    assert!(!expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_while_is_not_side_effect_free() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let cond = alloc_bool_lit(&mut package, &mut assigner, false, Span::default());
    let body = alloc_block(
        &mut package,
        &mut assigner,
        Vec::new(),
        Ty::UNIT,
        Span::default(),
    );
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::While(cond, body),
        Span::default(),
    );
    assert!(!expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_total_binop_is_side_effect_free_and_safe_to_discard() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let a = int_lit(&mut package, &mut assigner, 1);
    let b = int_lit(&mut package, &mut assigner, 2);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Int),
        ExprKind::BinOp(BinOp::Add, a, b),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_fallible_binop_is_side_effect_free_but_not_safe_to_discard() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let a = int_lit(&mut package, &mut assigner, 1);
    let b = int_lit(&mut package, &mut assigner, 0);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Int),
        ExprKind::BinOp(BinOp::Div, a, b),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_array_index_is_side_effect_free_but_not_safe_to_discard() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let value = int_lit(&mut package, &mut assigner, 1);
    let array = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Array(Box::new(Ty::Prim(Prim::Int))),
        ExprKind::Array(vec![value]),
        Span::default(),
    );
    let index = int_lit(&mut package, &mut assigner, 2);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Int),
        ExprKind::Index(array, index),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_array_repeat_is_side_effect_free_but_not_safe_to_discard() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let value = int_lit(&mut package, &mut assigner, 1);
    let count = int_lit(&mut package, &mut assigner, -1);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Array(Box::new(Ty::Prim(Prim::Int))),
        ExprKind::ArrayRepeat(value, count),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_result_equality_is_side_effect_free_but_not_safe_to_discard() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let some_local = assigner.next_local();
    let lhs = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        some_local,
        Ty::Prim(Prim::Result),
        Span::default(),
    );
    let rhs = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Result),
        ExprKind::Lit(Lit::Result(qsc_fir::fir::Result::Zero)),
        Span::default(),
    );
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Bool),
        ExprKind::BinOp(BinOp::Eq, lhs, rhs),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_negation_is_side_effect_free_and_safe_to_discard() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let operand = int_lit(&mut package, &mut assigner, 1);
    let e = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Prim(Prim::Int),
        ExprKind::UnOp(UnOp::Neg, operand),
        Span::default(),
    );
    assert!(expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn given_if_then_only_is_not_side_effect_free() {
    // The predicate only accepts `If` with both arms present; the absent-else
    // case can run its `then` branch for effect.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let cond = alloc_bool_lit(&mut package, &mut assigner, true, Span::default());
    let then_expr = int_lit(&mut package, &mut assigner, 1);
    let e = alloc_if_expr(
        &mut package,
        &mut assigner,
        cond,
        then_expr,
        None,
        Ty::UNIT,
        Span::default(),
    );
    assert!(!expr_is_side_effect_free(&package, PackageId::CORE, e));
    assert!(!expr_is_safe_to_discard(&package, PackageId::CORE, e));
}

#[test]
fn structural_walker_covers_blocks_locals_and_spec_inputs() {
    // A controlled operation with a nested block (the `if` body) and tuple-pattern
    // `let` bindings in both the body and the explicit controlled specialization,
    // so each present `SpecDecl.input` (including the control-register input) is
    // exercised alongside nested-block and Local-pat coverage.
    let (store, pkg_id) = compile_to_fir(
        "operation Op(q : Qubit) : Unit is Ctl {
             body ... {
                 let (a, b) = (1, 2);
                 if a > 0 {
                     let c = a + b;
                 }
             }
             controlled (cs, ...) {
                 let (d, e) = (3, 4);
             }
         }
         operation Main() : Unit { use q = Qubit(); Op(q); }",
    );
    let package = store.get(pkg_id);
    let decl = find_callable_decl(package, "Op");

    let mut blocks: FxHashSet<BlockId> = FxHashSet::default();
    let mut stmts: FxHashSet<StmtId> = FxHashSet::default();
    let mut exprs: FxHashSet<ExprId> = FxHashSet::default();
    let mut pats: FxHashSet<PatId> = FxHashSet::default();
    for_each_node_in_callable(package, decl, &mut |node| match node {
        CallableNode::Block(b) => {
            blocks.insert(b);
        }
        CallableNode::Stmt(s) => {
            stmts.insert(s);
        }
        CallableNode::Expr(ex) => {
            exprs.insert(ex);
        }
        CallableNode::Pat(p) => {
            pats.insert(p);
        }
    });

    // The callable input pattern is covered.
    assert!(
        pats.contains(&decl.input),
        "decl.input pat {} should be visited",
        decl.input
    );

    // Every present `SpecDecl.input` pat (body + functored specs) is covered.
    let CallableImpl::Spec(spec_impl) = &decl.implementation else {
        panic!("Op should have a Spec implementation");
    };
    let mut spec_inputs = Vec::new();
    if let Some(input) = spec_impl.body.input {
        spec_inputs.push(input);
    }
    for spec in crate::fir_builder::functored_specs(spec_impl) {
        if let Some(input) = spec.input {
            spec_inputs.push(input);
        }
    }
    // The explicit `controlled (cs, ...)` spec carries a control-register input.
    assert!(
        !spec_inputs.is_empty(),
        "the controlled spec should carry an input pattern"
    );
    for input in spec_inputs {
        assert!(
            pats.contains(&input),
            "SpecDecl.input pat {input} should be visited"
        );
    }

    // Nested-block coverage: body block + nested `if` block + controlled block.
    assert!(
        blocks.len() >= 3,
        "expected at least body, nested-if, and ctl blocks, got {}",
        blocks.len()
    );

    // Every Local binding is visited as a Pat, including those nested in the
    // tuple `let` patterns of both specializations.
    for name in ["a", "b", "c", "d", "e"] {
        let found = pats.iter().any(|&p| {
            matches!(&package.get_pat(p).kind, PatKind::Bind(ident) if ident.name.as_ref() == name)
        });
        assert!(found, "local binding '{name}' should be visited as a Pat");
    }

    // The tuple Local patterns themselves are visited (the `(a, b)` and `(d, e)`
    // `let` patterns), confirming recursive tuple-pat descent.
    let tuple_pats = pats
        .iter()
        .filter(|&&p| matches!(&package.get_pat(p).kind, PatKind::Tuple(_)))
        .count();
    assert!(
        tuple_pats >= 2,
        "expected at least two tuple Local pats, got {tuple_pats}"
    );

    // Statements and expressions are collected too.
    assert!(!stmts.is_empty(), "expected statements to be visited");
    assert!(!exprs.is_empty(), "expected expressions to be visited");
}

#[test]
fn structural_walker_from_expr_root_covers_nested_blocks() {
    // Driving the walker from a bare `ExprId` (here, the `if` expression) must
    // descend into the nested block, yielding its statements, the tuple Local
    // pat, and the inner expressions — exercising the entry-root reuse path.
    let (store, pkg_id) = compile_to_fir(
        "function Main() : Int {
             if true {
                 let (a, b) = (1, 2);
                 a + b
             } else {
                 0
             }
         }",
    );
    let package = store.get(pkg_id);
    let if_expr = package
        .exprs
        .iter()
        .find_map(|(id, e)| matches!(&e.kind, ExprKind::If(..)).then_some(id))
        .expect("program should contain an `if` expression");

    let mut blocks: FxHashSet<BlockId> = FxHashSet::default();
    let mut pats: FxHashSet<PatId> = FxHashSet::default();
    let mut saw_expr = false;
    for_each_node_from_expr_root(package, if_expr, &mut |node| match node {
        CallableNode::Block(b) => {
            blocks.insert(b);
        }
        CallableNode::Pat(p) => {
            pats.insert(p);
        }
        CallableNode::Expr(_) => saw_expr = true,
        CallableNode::Stmt(_) => {}
    });

    assert!(saw_expr, "expected expressions to be visited from the root");
    // The then/else blocks nested in the `if` are reached from the bare root.
    assert!(
        blocks.len() >= 2,
        "expected the then and else blocks to be visited, got {}",
        blocks.len()
    );
    // A tuple Local pat reachable through the nested then-block is visited.
    let tuple_pats = pats
        .iter()
        .filter(|&&p| matches!(&package.get_pat(p).kind, PatKind::Tuple(_)))
        .count();
    assert!(
        tuple_pats >= 1,
        "expected the nested tuple Local pat to be visited, got {tuple_pats}"
    );
}
