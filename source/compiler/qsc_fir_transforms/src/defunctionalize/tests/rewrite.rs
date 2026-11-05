// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{ExprKind, ItemKind, PackageLookup, PatKind, Res};
use rustc_hash::FxHashSet;

use crate::defunctionalize::rewrite::{
    find_local_init_expr_in_callable, is_replayed_callable_selection,
    local_var_is_read_in_callable, local_var_is_used_in_callable, prune_dead_callable_arg_locals,
    remove_write_only_callable_local_from_callable,
};
use crate::defunctionalize::types::UdtMetadata;

/// Call cleanup directly so eligibility checks cannot bypass the guard.
/// The indexed write must reject the entire attempt before the earlier
/// effectful whole-array store is rewritten or its binding is removed.
#[test]
fn partial_callable_array_write_prevents_any_dead_store_cleanup() {
    let source = r#"
            namespace Test {
                function A(value : Int) : Int { value + 1 }
                function B(value : Int) : Int { value + 2 }
                operation Main() : Int {
                    mutable marker = 0;
                    mutable fs = [A, A];
                    set fs = { set marker += 7; [A, B] };
                    set fs w/= 0 <- { set marker += 11; B };
                    marker
                }
            }
        "#;
    let (mut store, package_id) =
        crate::test_utils::compile_to_monomorphized_fir_with_entry(source, "Test.Main()");
    let package = store.get(package_id);
    let callable_id = crate::test_utils::callable_id_by_name(package, "Main");
    let local_var = package
        .pats
        .values()
        .find_map(|pat| match &pat.kind {
            PatKind::Bind(ident) if ident.name.as_ref() == "fs" => Some(ident.id),
            _ => None,
        })
        .expect("the callable array binding should exist");
    let before = format!("{package:?}");
    let mut assigner = Assigner::from_package(package);

    remove_write_only_callable_local_from_callable(
        store.get_mut(package_id),
        package_id,
        callable_id,
        local_var,
        &FxHashSet::default(),
        &mut assigner,
    );
    assert_eq!(
        format!("{:?}", store.get(package_id)),
        before,
        "partial-write rejection must leave the binding, both writes and all FIR nodes unchanged"
    );
    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package_id, &[]);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package_id),
        Ok(qsc_eval::val::Value::Int(18))
    );
}

#[test]
fn callable_local_used_only_by_live_closure_capture_is_retained() {
    let source = r#"
            namespace Test {
                function AddOne(value : Int) : Int { value + 1 }
                function Make() : Int -> Int {
                    let selected = AddOne;
                    value -> selected(value)
                }
            }
        "#;
    let (mut store, package_id) =
        crate::test_utils::compile_to_monomorphized_fir_with_entry(source, "Test.Make()(10)");
    let package = store.get(package_id);
    let callable_id = crate::test_utils::callable_id_by_name(package, "Make");
    let ItemKind::Callable(decl) = &package.get_item(callable_id).kind else {
        panic!("Make should be a callable");
    };
    let mut closures = Vec::new();
    let mut local_reads = FxHashSet::default();
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |expr_id, expr| match &expr.kind {
            ExprKind::Closure(captures, _) => closures.push((expr_id, captures.clone())),
            ExprKind::Var(Res::Local(local), _) => {
                local_reads.insert(*local);
            }
            _ => {}
        },
    );
    assert_eq!(closures.len(), 1, "Make should return one live closure");
    let (closure_id, captures) = &closures[0];
    let [local_var] = captures.as_slice() else {
        panic!("the closure should capture exactly the callable local");
    };
    let local_var = *local_var;
    assert!(
        !local_reads.contains(&local_var),
        "the capture must be the local's only remaining use"
    );
    let initializer = find_local_init_expr_in_callable(package, callable_id, local_var)
        .expect("the captured callable binding should exist");
    assert!(
        is_replayed_callable_selection(package, initializer),
        "the initializer must otherwise qualify for dead-store cleanup"
    );
    assert!(
        local_var_is_used_in_callable(package, callable_id, local_var),
        "a live closure capture must count as a use"
    );
    assert!(
        local_var_is_read_in_callable(package, callable_id, local_var),
        "a live closure capture must count as a read"
    );

    let udts = UdtMetadata::new(&store);
    let mut assigner = Assigner::from_package(store.get(package_id));
    // Model a cleanup candidate whose earlier direct-call read was consumed.
    prune_dead_callable_arg_locals(
        &udts,
        store.get_mut(package_id),
        package_id,
        &FxHashSet::from_iter([(callable_id, local_var)]),
        &FxHashSet::default(),
        &FxHashSet::default(),
        &mut assigner,
    );
    let package = store.get(package_id);
    assert_eq!(
        find_local_init_expr_in_callable(package, callable_id, local_var),
        Some(initializer),
        "cleanup must retain the binding needed by the live capture"
    );
    assert!(
        matches!(&package.get_expr(*closure_id).kind, ExprKind::Closure(retained, _) if retained == captures),
        "cleanup must retain the live closure's capture"
    );
    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package_id, &[]);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package_id),
        Ok(qsc_eval::val::Value::Int(11))
    );
}
