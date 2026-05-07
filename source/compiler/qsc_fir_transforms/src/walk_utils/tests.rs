// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::compile_to_fir;
use expect_test::expect;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{CallableImpl, ItemKind, PatKind};

/// Finds the body block of the named callable in the user package.
fn find_callable_block(package: &Package, name: &str) -> BlockId {
    for item in package.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == name
            && let CallableImpl::Spec(spec) = &decl.implementation
        {
            return spec.body.block;
        }
    }
    panic!("callable '{name}' not found");
}

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
    let mut uses = Vec::new();
    collect_uses_in_block(package, block_id, local_id, &mut uses);
    // Both p.X and p.Y are field-only accesses.
    expect![[r#"
            [
                true,
                true,
            ]
        "#]]
    .assert_debug_eq(&uses);
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
    let mut uses = Vec::new();
    collect_uses_in_block(package, block_id, local_id, &mut uses);
    // t is passed directly to Consume — whole-value use.
    expect![[r#"
            [
                false,
            ]
        "#]]
    .assert_debug_eq(&uses);
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
    let mut uses = Vec::new();
    collect_uses_in_block(package, block_id, local_id, &mut uses);
    // set t = (3, 4) is decomposable (true), final `t` is whole-value (false).
    expect![[r#"
            [
                true,
                false,
            ]
        "#]]
    .assert_debug_eq(&uses);
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
    let mut uses = Vec::new();
    collect_uses_in_block(package, block_id, local_id, &mut uses);
    // y is captured by the closure — whole-value use.
    expect![[r#"
            [
                false,
            ]
        "#]]
    .assert_debug_eq(&uses);
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
    let mut uses = Vec::new();
    collect_uses_in_block(package, block_id, local_id, &mut uses);
    // o.I.X is a nested field access — still field-only.
    expect![[r#"
            [
                true,
            ]
        "#]]
    .assert_debug_eq(&uses);
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
    // The entry expression wraps the call to Main. It should contain at least
    // the call expression and the callee/args sub-expressions.
    assert!(
        !ids.is_empty(),
        "entry expression IDs should be non-empty for a program with an entry point"
    );
    // All returned IDs should be valid expression IDs in the package.
    for &id in &ids {
        let _ = package.get_expr(id);
    }
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
            if let ItemKind::Callable(decl) = &item.kind {
                if decl.name.name.as_ref() == "Op" {
                    return Some(id);
                }
            }
            None
        })
        .expect("Op callable not found");

    let ids = collect_expr_ids_in_local_callables(package, &[op_local_id]);
    // Op has body, adj, and ctl specs — each contains at least a Call expression.
    assert!(
        ids.len() >= 3,
        "expected at least 3 expression IDs covering multiple specs, got {}",
        ids.len()
    );
    // No duplicates.
    let unique: FxHashSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "expression IDs should be unique");
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
