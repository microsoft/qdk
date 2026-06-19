// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The operand-lift driver's convergence: the measure strictly decreases per
//! changed iteration, and the divergence guard degrades gracefully.
//!
//! [`super::super::run_to_fixpoint`] proves termination with the monotone
//! measure [`super::super::count_operand_position_returns`], which counts every
//! `Return` sitting in an operand position. Each operand lift the normal
//! pipeline produces retires exactly one such `Return`, so the measure strictly
//! decreases on every changed iteration and the driver always reaches a fixed
//! point. The guard that compares successive measures is therefore defensive:
//! for pipeline-derived input it never fires.
//!
//! The positive test
//! [`anf_operand_position_return_measure_strictly_decreases_each_iteration`]
//! pins the termination property directly: it steps the ANF sweep by hand over
//! pipeline-derived FIR and samples the measure between iterations to witness
//! the strict decrease that guarantees convergence.
//!
//! The negative test exercises the defensive path. It compiles Q# whose two
//! operand-position conditional expressions have *bare* `Return` conditions
//! (`(return 5) ? 1 | 2`) and feeds the `Mono`-stage FIR straight to
//! [`super::super::run_to_fixpoint`], bypassing the statement-boundary hoist the
//! full pipeline runs first. That hoist rewrites such condition `Return`s before
//! the ANF phase ever sees them, so feeding the un-hoisted FIR directly is what
//! surfaces the shape to the driver. It is deliberately pathological for the
//! measure: an `if` condition's `Return` is always counted in operand position
//! regardless of where the `if` itself sits, yet the lift moves the *whole* `if`
//! to a temp without removing the condition's `Return`. So each changed
//! iteration lifts one `if` whole (reporting progress) while the measure stays
//! flat. After two such iterations the guard observes the stalled measure and
//! pushes `Error::FixpointNotReached("anf", _)`, returning instead of looping
//! forever or panicking.

use super::*;
use qsc_fir::assigner::Assigner;

use crate::return_unify::Error;

#[test]
fn nonconverging_operand_lift_pushes_fixpoint_not_reached_without_aborting() {
    // Two operand-position conditional expressions whose conditions are *bare*
    // `Return`s:
    //   `let x = ((return 5) ? 1 | 2) + ((return 6) ? 3 | 4);`
    // The full pipeline would hoist the condition `Return`s to the statement
    // boundary before the ANF phase runs, so this test stops at `Mono` and
    // feeds the un-hoisted FIR straight to the fixpoint driver to exercise the
    // divergence guard on the shape the hoist would otherwise eliminate.
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
        matches!(errors[0], Error::FixpointNotReached("anf", reported) if reported == block_id),
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
) -> bool {
    let mut changed = false;
    for reachable in crate::return_unify::normalize::collect_reachable_blocks(package, block_id) {
        if super::super::anf_block_once(
            package,
            assigner,
            package_id,
            reachable,
            operand_temp_counter,
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

    let changed_1 = anf_step_once(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut counter,
    );
    let measure_1 = super::super::count_operand_position_returns(store.get(pkg_id), block_id);

    let changed_2 = anf_step_once(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut counter,
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
    ) {}
    let measure_final = super::super::count_operand_position_returns(store.get(pkg_id), block_id);
    assert_eq!(
        measure_final, 0,
        "every buried operand return should be drained at the fixed point"
    );
}
