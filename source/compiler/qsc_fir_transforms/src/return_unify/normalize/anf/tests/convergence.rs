// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests ANF fixpoint convergence and its defensive failure paths.
//!
//! Normal pipeline input lowers the operand-return measure until stable. Tests
//! also cover malformed pre-ANF shapes where the measure stalls or an unowned
//! `AssignOp` would otherwise leave a return buried.

use super::*;
use qsc_fir::assigner::Assigner;
// The assertion test is debug-only, matching its debug-only imports.
#[cfg(debug_assertions)]
use qsc_fir::fir::{ExprId, ExprKind, PackageLookup, StmtKind};

use crate::return_unify::Error;

#[test]
fn anf_fixpoint_is_structurally_idempotent_after_nested_ancestor_lift() {
    let source = indoc! {r#"
        namespace Test {
            operation Apply(pair : (Int, Int)) : Unit {}

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                ({ X(q); Reset(q); Apply })((0, { return 5; 1 }));
                0
            }
        }
    "#};

    let (mut store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let block_id = find_body_block_id(store.get(pkg_id), "Main");
    let mut errors = Vec::new();
    crate::return_unify::normalize::hoist_returns_to_statement_boundary(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    assert!(errors.is_empty(), "hoist produced errors: {errors:?}");

    let first_changed = super::super::run_to_fixpoint(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    assert!(
        first_changed,
        "the first ANF run should lift nested operands"
    );
    assert!(
        errors.is_empty(),
        "first ANF run produced errors: {errors:?}"
    );
    let after_first = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);

    let second_changed = super::super::run_to_fixpoint(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    let after_second = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);

    assert!(!second_changed, "the second ANF run should be a no-op");
    assert!(
        errors.is_empty(),
        "second ANF run produced errors: {errors:?}"
    );
    assert_eq!(after_first, after_second, "the second ANF run changed FIR");
}

#[test]
fn nonconverging_operand_lift_pushes_fixpoint_not_reached_without_aborting() {
    // Bypass the prior hoist to feed the driver's stalled-measure guard.
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = ((return 5) ? 1 | 2) + ((return 6) ? 3 | 4);
                x
            }
        }
    "#};

    let (mut store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let block_id = find_body_block_id(store.get(pkg_id), "Main");

    let mut errors = Vec::new();
    let changed = super::super::run_to_fixpoint(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );

    assert!(
        changed,
        "the driver should report it rewrote operands before detecting divergence"
    );
    assert_eq!(
        errors.len(),
        1,
        "the stalled measure should surface exactly one guard error, got {errors:?}"
    );
    assert!(
        matches!(errors[0], Error::FixpointNotReached("anf", reported)
            if reported == (pkg_id, block_id).into()),
        "expected FixpointNotReached(\"anf\", {block_id:?}), got {:?}",
        errors[0]
    );
}

/// Run a single ANF operand-lift sweep over every reachable block, mirroring
/// one iteration of the standalone fixpoint driver: each reachable block gets
/// one [`anf_block_once`](super::super::anf_block_once) pass (which performs at
/// most one operand lift per direct statement), and the sweep reports whether
/// any block changed. Tests drive this by hand so they can sample the
/// convergence measure between iterations.
fn anf_step_once(
    package: &mut qsc_fir::fir::Package,
    assigner: &mut qsc_fir::assigner::Assigner,
    package_id: qsc_fir::fir::PackageId,
    block_id: qsc_fir::fir::BlockId,
    operand_temp_counter: &mut u32,
    generated_operand_reads: &mut rustc_hash::FxHashSet<qsc_fir::fir::ExprId>,
) -> bool {
    let mut changed = false;
    for reachable in crate::return_unify::normalize::collect_reachable_blocks(package, block_id) {
        if super::super::anf_block_once(
            package,
            assigner,
            package_id,
            reachable,
            operand_temp_counter,
            generated_operand_reads,
        ) {
            changed = true;
        }
    }
    changed
}

#[test]
fn anf_operand_position_return_measure_strictly_decreases_each_iteration() {
    // `1 + { return 2; 3 } + { return 4; 5 }` holds two operand-position
    // returns, so the convergence measure starts at 2. Stepping the ANF sweep
    // by hand and sampling `count_operand_position_returns` between iterations
    // proves the measure the fixpoint driver relies on *strictly* decreases on
    // every changed iteration — the property that guarantees termination. The
    // assertions deliberately check strict monotonic decrease (`>`), not that
    // each step drops by exactly one, so a future lift that retires more than
    // one buried return per pass would still satisfy the contract.
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1 + { return 2; 3 } + { return 4; 5 };
                x
            }
        }
    "#};

    let (mut store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let block_id = find_body_block_id(store.get(pkg_id), "Main");

    // Run the statement-boundary hoist first, exactly as the isolation seam
    // does, so only operand-position returns remain for the ANF sweep to drain.
    let mut errors = Vec::new();
    crate::return_unify::normalize::hoist_returns_to_statement_boundary(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    assert!(errors.is_empty(), "hoist produced errors: {errors:?}");

    let measure_0 = super::super::count_operand_position_returns(store.get(pkg_id), block_id);
    assert_eq!(
        measure_0, 2,
        "two buried operand returns should give an initial measure of 2"
    );

    let mut counter = 0u32;
    let mut generated_operand_reads = rustc_hash::FxHashSet::default();

    let changed_1 = anf_step_once(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut counter,
        &mut generated_operand_reads,
    );
    let measure_1 = super::super::count_operand_position_returns(store.get(pkg_id), block_id);

    let changed_2 = anf_step_once(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut counter,
        &mut generated_operand_reads,
    );
    let measure_2 = super::super::count_operand_position_returns(store.get(pkg_id), block_id);

    assert!(
        changed_1 && changed_2,
        "both iterations should still be retiring buried operand returns \
         (changed_1={changed_1}, changed_2={changed_2})"
    );
    assert!(
        measure_0 > measure_1,
        "measure must strictly decrease on the first changed iteration \
         ({measure_0} -> {measure_1})"
    );
    assert!(
        measure_1 > measure_2,
        "measure must strictly decrease on the second changed iteration \
         ({measure_1} -> {measure_2})"
    );

    // Drain to the fixed point and confirm the measure bottoms out at zero.
    while anf_step_once(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut counter,
        &mut generated_operand_reads,
    ) {}
    let measure_final = super::super::count_operand_position_returns(store.get(pkg_id), block_id);
    assert_eq!(
        measure_final, 0,
        "every buried operand return should be drained at the fixed point"
    );
}

#[test]
fn scalar_assignop_staging_reduces_measure_in_same_sweep() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 10;
                let go = false;
                set x += {
                    set x = 20;
                    if go { return 7; }
                    5
                };
                x
            }
        }
    "#};

    let (mut store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let block_id = find_body_block_id(store.get(pkg_id), "Main");
    let mut errors = Vec::new();
    crate::return_unify::normalize::hoist_returns_to_statement_boundary(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    assert!(errors.is_empty(), "hoist produced errors: {errors:?}");

    let measure_before = super::super::count_operand_position_returns(store.get(pkg_id), block_id);
    let mut counter = 0u32;
    let mut generated_operand_reads = rustc_hash::FxHashSet::default();
    let changed = anf_step_once(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut counter,
        &mut generated_operand_reads,
    );
    let measure_after = super::super::count_operand_position_returns(store.get(pkg_id), block_id);

    assert!(
        changed,
        "the scalar AssignOp should be rewritten in one sweep"
    );
    assert_eq!(
        measure_before, 1,
        "the RHS should contain one buried return"
    );
    assert_eq!(
        measure_after, 0,
        "old-value staging and RHS lifting should retire the return together"
    );
}

/// Returns the surface expression of the first statement in `block_id` whose
/// expression kind satisfies `predicate`.
///
/// Gated with its sole caller, which is a debug-only test: `build.py` runs
/// `cargo test --release`, where an ungated helper would be dead code.
#[cfg(debug_assertions)]
fn find_surface_expr(
    package: &qsc_fir::fir::Package,
    block_id: qsc_fir::fir::BlockId,
    predicate: impl Fn(&ExprKind) -> bool,
) -> ExprId {
    package
        .get_block(block_id)
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let expr_id = match &package.get_stmt(stmt_id).kind {
                StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
                StmtKind::Item(_) => return None,
            };
            predicate(&package.get_expr(expr_id).kind).then_some(expr_id)
        })
        .expect("expected the body to contain a statement matching the predicate")
}

/// A compound assignment whose place is a `Field`/`Index` projection is the one
/// `AssignOp` shape [`super::super::anf_lift_in_expr`] cannot lower: it is
/// neither the scalar `Var` place nor the array place the two guarded arms
/// accept. No front end emits it — Q# spells indexed update as
/// `a w/= i <- v` (an `UpdateIndex`/`AssignIndex`) and the OpenQASM
/// compiler never passes `is_assignment = true` — so the shape cannot be
/// reached from a Q# source string.
///
/// The package is therefore assembled from real compiler output rather than
/// faked: a scalar `x += { return 5; 1 };` supplies the compound assignment
/// and a separate `arr[idx]` binding supplies a genuine `ExprKind::Index`
/// projection, whose kind and type are copied into a fresh expression that
/// replaces the assignment's place slot. Every id and type comes from the
/// compiler; only the place slot is repointed.
///
/// Before the dedicated `AssignOp` arm existed this shape fell through to the
/// shared leaf/no-op arm, so the lift returned `None` and the right-hand side
/// `Return` stayed buried in an operand position with neither a lift nor a
/// diagnostic. `run_to_fixpoint` cannot catch that by inspecting its own
/// measure, because `count_operand_position_returns` is legitimately nonzero at
/// convergence for the `while`-condition shapes ANF treats as leaves.
///
/// Gated on debug builds because `debug_assert!` is elided in release.
#[cfg(debug_assertions)]
#[test]
fn projected_assign_op_place_trips_the_unhandled_shape_assertion() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 0;
                let arr = [1, 2, 3];
                mutable idx = 0;
                let projected = arr[idx];
                x += { return 5; 1 };
                x + projected
            }
        }
    "#};

    let (mut store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let block_id = find_body_block_id(store.get(pkg_id), "Main");

    // Run the statement-boundary hoist first so the ANF sweep sees exactly the
    // FIR the pipeline would hand it. The hoist leaves the compound assignment
    // alone: its RHS is a statement-carrying `Block`.
    let mut errors = Vec::new();
    crate::return_unify::normalize::hoist_returns_to_statement_boundary(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    assert!(errors.is_empty(), "hoist produced errors: {errors:?}");

    // Repoint the compound assignment's place at a copy of the compiler-built
    // `arr[idx]` projection.
    let package = store.get_mut(pkg_id);
    let projection_id = find_surface_expr(package, block_id, |kind| {
        matches!(kind, ExprKind::Index(_, _))
    });
    let assign_op_id = find_surface_expr(package, block_id, |kind| {
        matches!(kind, ExprKind::AssignOp(_, _, _))
    });

    let projection = package.get_expr(projection_id).clone();
    let place_id = crate::fir_builder::alloc_expr(
        package,
        &mut assigner,
        projection.ty,
        projection.kind,
        qsc_data_structures::span::Span::default(),
    );

    let ExprKind::AssignOp(op, _, rhs) = package.get_expr(assign_op_id).kind.clone() else {
        unreachable!("the located expression is an AssignOp")
    };
    package
        .exprs
        .get_mut(assign_op_id)
        .expect("assignment expression should exist")
        .kind = ExprKind::AssignOp(op, place_id, rhs);

    crate::test_utils::assert_panics_with(
        "a scalar `AssignOp` place is expected to be `ExprKind::Var`",
        move || {
            let mut errors = Vec::new();
            super::super::run_to_fixpoint(
                store.get_mut(pkg_id),
                &mut assigner,
                pkg_id,
                block_id,
                &mut errors,
            );
        },
    );
}
