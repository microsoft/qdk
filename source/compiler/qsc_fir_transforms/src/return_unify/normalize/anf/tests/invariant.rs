// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The `PostReturnUnify` structural invariant rejects flag writes left in
//! operand position, and the post-return-unify checks tolerate the residual
//! `Return` a deliberately un-rewritten callable retains.
//!
//! After the operand lift runs, no synthesized return-flag write may remain in
//! an operand position: a correctly lifted body moves every flag write to a
//! statement boundary. Two controls pin that contract — a correctly lifted body
//! passes the structural check, and a hand-built body that leaves a flag write
//! in an operand slot trips it.
//!
//! Three further controls pin the structural assumptions the residual-`Return`
//! skip set relies on: the package entry expression is never itself a `Return`,
//! the non-Unit block-tail check is bypassed only for a callable left with a
//! residual `Return` (and would otherwise reject its un-collapsed tail), and a
//! residual-`Return` callable still receives a well-formed, non-empty rebuilt
//! specialization exec graph.

use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BlockId, CallableImpl, ExecGraphConfig, ExprKind, ItemKind, LocalItemId, Mutability, Package,
    PackageId, PackageLookup, PackageStore, SpecDecl, StoreItemId,
};
use qsc_fir::ty::{Prim, Ty};
use rustc_hash::FxHashSet;

use crate::fir_builder::{
    alloc_assign_expr, alloc_bind_pat, alloc_bool_lit, alloc_local_stmt, alloc_local_var_expr,
};
use crate::invariants::{self, InvariantLevel};
use crate::reachability::collect_reachable_from_entry;
use crate::test_utils::{assert_panics_with, compile_and_run_pipeline_to};

use super::*;
use crate::return_unify::symbols;

/// Returns the body specialization block of the callable named `Main`.
fn main_body_block(package: &Package) -> BlockId {
    let decl = package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main" => Some(decl),
            _ => None,
        })
        .expect("Main callable should exist");
    let CallableImpl::Spec(spec_impl) = &decl.implementation else {
        panic!("Main should have a body spec");
    };
    spec_impl.body.block
}

#[test]
fn lifted_operand_return_passes_operand_position_invariant() {
    // A `return` buried in a `1 + { … return … }` operand is lifted to a spine
    // temp, moving every synthesized flag write to a statement boundary. The
    // structural check must accept the lifted body.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                mutable x = 0;
                set x = 1 + { return 5; 2 };
                x
            }
        }
    "#};

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ReturnUnify);

    // The check panics on violation; reaching the end is the positive assertion.
    invariants::check(&store, pkg_id, InvariantLevel::PostReturnUnify);
}

#[test]
fn flag_write_in_operand_position_trips_operand_position_invariant() {
    // Start from a clean, fully transformed body, then splice in a synthesized
    // return-flag write that sits in an operand position (the initializer of a
    // `let`, which the lift treats as an operand slot). The structural check
    // must reject it.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {}
        }
    "#};

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ReturnUnify);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let body_block = main_body_block(store.get(pkg_id));
    let package = store.get_mut(pkg_id);

    // Declare the synthesized return flag at a statement boundary (legitimate).
    let (flag_id, flag_pat) = alloc_bind_pat(
        package,
        &mut assigner,
        symbols::HAS_RETURNED,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let false_lit = alloc_bool_lit(package, &mut assigner, false, Span::default());
    let decl_stmt = alloc_local_stmt(
        package,
        &mut assigner,
        Mutability::Mutable,
        flag_pat,
        false_lit,
        Span::default(),
    );

    // Write the flag from an operand position: the assign is the initializer of
    // a `let`, which the lift classifies as an operand slot. This is exactly the
    // shape a correct lift must never leave behind.
    let flag_ref = alloc_local_var_expr(
        package,
        &mut assigner,
        flag_id,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let true_lit = alloc_bool_lit(package, &mut assigner, true, Span::default());
    let assign = alloc_assign_expr(package, &mut assigner, flag_ref, true_lit, Span::default());
    let (_tmp_id, tmp_pat) = alloc_bind_pat(
        package,
        &mut assigner,
        symbols::OPERAND_TEMP,
        Ty::UNIT,
        Span::default(),
    );
    let violation_stmt = alloc_local_stmt(
        package,
        &mut assigner,
        Mutability::Immutable,
        tmp_pat,
        assign,
        Span::default(),
    );

    package
        .blocks
        .get_mut(body_block)
        .expect("body block should exist")
        .stmts
        .extend([decl_stmt, violation_stmt]);

    assert_panics_with("survives in an operand position", || {
        invariants::check(&store, pkg_id, InvariantLevel::PostReturnUnify);
    });
}

/// Q# source whose entry-point `Main` cannot be single-exit rewritten, so
/// return unification leaves it un-rewritten with a residual `Return`.
///
/// The buried `return` initializes a `let h : Holder` binding whose pattern
/// type `Holder` is non-defaultable (it has a `Qubit` field), which has no
/// classical default for the non-return path. The callable is therefore left
/// un-rewritten — its body keeps both the buried `return` and the trailing
/// `return 7;` — and a non-fatal diagnostic is emitted instead of panicking.
fn residual_return_entry_point_source() -> &'static str {
    indoc! {r#"
        namespace Test {
            struct Holder { Q : Qubit }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                use q2 = Qubit();
                let h = new Holder { Q = { return 5; q2 } };
                return 7;
            }
        }
    "#}
}

/// Returns the body specialization of the callable named `Main`.
fn main_body_spec(package: &Package) -> &SpecDecl {
    let decl = package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main" => Some(decl),
            _ => None,
        })
        .expect("Main callable should exist");
    let CallableImpl::Spec(spec_impl) = &decl.implementation else {
        panic!("Main should have a body spec");
    };
    &spec_impl.body
}

/// Collects the target-package callables whose bodies still contain an
/// `ExprKind::Return` — exactly the callables return unification leaves
/// un-rewritten and records in the pipeline's skip set.
fn callables_with_residual_return(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
) -> FxHashSet<LocalItemId> {
    let mut skip = FxHashSet::default();
    for item_id in reachable {
        if item_id.package != package_id {
            continue;
        }
        let package = store.get(item_id.package);
        let item = package.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let mut has_residual_return = false;
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_expr_id, expr| {
                    if matches!(expr.kind, ExprKind::Return(_)) {
                        has_residual_return = true;
                    }
                },
            );
            if has_residual_return {
                skip.insert(item_id.item);
            }
        }
    }
    skip
}

#[test]
fn entry_expression_never_directly_holds_a_return() {
    // The residual-`Return` checks walk callable bodies, not the package entry
    // expression, so a `Return` parked directly in the entry slot would escape
    // every check. Confirm the entry slot holds the call that drives the
    // program — never a bare `return` — even when a reachable callable is left
    // with a residual `Return`.
    let source = residual_return_entry_point_source();

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ReturnUnify);
    let package = store.get(pkg_id);

    let reachable = collect_reachable_from_entry(&store, pkg_id);
    let residual = callables_with_residual_return(&store, pkg_id, &reachable);
    assert!(
        !residual.is_empty(),
        "fixture should leave a reachable callable with a residual Return",
    );

    let entry_id = package
        .entry
        .expect("package should have an entry expression");
    assert!(
        !matches!(package.get_expr(entry_id).kind, ExprKind::Return(_)),
        "entry expression must not directly hold a Return node",
    );
}

#[test]
fn skip_set_bypasses_non_unit_block_tail_check_for_residual_return_callable() {
    // A callable left with a residual `Return` keeps its un-collapsed,
    // non-single-exit body. The non-Unit block-tail check skips exactly that
    // callable's per-callable fan-out, so checking at `PostReturnUnify` must not
    // panic.
    let source = residual_return_entry_point_source();

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ReturnUnify);
    let reachable = collect_reachable_from_entry(&store, pkg_id);
    let skip = callables_with_residual_return(&store, pkg_id, &reachable);
    assert!(
        !skip.is_empty(),
        "fixture should leave a reachable callable with a residual Return",
    );

    // The check panics on violation; returning normally is the positive
    // assertion that the skip set covers the residual-`Return` callable.
    invariants::check_non_unit_block_tails(&store, pkg_id, &reachable, &skip);
}

#[test]
fn residual_return_tail_trips_non_unit_block_tail_check_without_skip() {
    // The negative control for the skip-set gating: with an empty skip set, the
    // un-rewritten callable's residual `Return` body is checked and its
    // non-single-exit, non-Unit block tail is rejected. This proves the skip in
    // the positive control is load-bearing rather than vacuous.
    let source = residual_return_entry_point_source();

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ReturnUnify);
    let reachable = collect_reachable_from_entry(&store, pkg_id);

    assert_panics_with("Non-Unit block-tail invariant violation", || {
        invariants::check_non_unit_block_tails(&store, pkg_id, &reachable, &FxHashSet::default());
    });
}

#[test]
fn residual_return_callable_gets_well_formed_rebuilt_exec_graph() {
    // Running the full pipeline rebuilds exec graphs and runs the `PostAll`
    // specialization exec-graph check, which is not gated by the skip set. The
    // residual-`Return` callable therefore has its specialization exec graph
    // rebuilt and validated; completing without panic confirms it is
    // structurally well-formed, and the rebuilt graph is non-empty.
    let source = residual_return_entry_point_source();

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    let package = store.get(pkg_id);

    let reachable = collect_reachable_from_entry(&store, pkg_id);
    let residual = callables_with_residual_return(&store, pkg_id, &reachable);
    assert!(
        !residual.is_empty(),
        "fixture should carry the residual Return through to the end of the pipeline",
    );

    let spec = main_body_spec(package);
    let node_count = spec.exec_graph.select_ref(ExecGraphConfig::NoDebug).len();
    assert!(
        node_count > 0,
        "rebuilt specialization exec graph for the residual-Return callable should be non-empty",
    );
}
