// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Behavior tests for [`super`]'s administrative-normal-form operand lift.
//!
//! Each file groups the assertions for one observable behavior of the lift so
//! a failure localizes to the property that broke:
//!
//! * [`lift_shapes`] — the spine `let __operand_tmp` bindings the lift emits
//!   for a `return` buried in each operand-slot shape.
//! * [`multi_lift_convergence`] — the fixpoint draining several operand
//!   returns from one statement, innermost-first.
//! * [`short_circuit_semantics`] — a buried `return` short-circuits before the
//!   surrounding operator, access, or call (and its sibling effects) runs.
//! * [`buried_loop_lowering`] — an operand-position loop carrying a `return`
//!   is flag-lowered with no raw `Return` surviving.
//! * [`rejection`] — a non-defaultable operand temp now lifts via an
//!   array-backed spine `let`; the residual warning is for a non-defaultable
//!   *binding* whose initializer buries a `return`, left un-rewritten rather
//!   than panicking.
//! * [`slot_defaultable`] — the slot machinery's `is_type_defaultable` type
//!   predicate, exercised in isolation: the array backing of a non-defaultable
//!   operand (`Qubit`, a qubit-bearing tuple or UDT) is always defaultable, so
//!   the operand-lift rejection guard stays dead.
//! * [`invariant`] — the `PostReturnUnify` structural check rejects a flag write
//!   left in operand position and accepts a correctly lifted body.
//! * [`semantic`] — the lifted spine returns the same value (or fails the same
//!   way) as the untransformed program for a `return` buried in each
//!   operand-slot shape.
//! * [`trace`] — the lifted spine performs the same ordered quantum effects as
//!   the untransformed program, skipping sibling and branch effects a buried
//!   `return` short-circuits.
//! * [`snapshot`] — the spine `let __operand_tmp` bindings the lift emits for a
//!   `return` buried in additional operand-slot shapes.
//! * [`isolation`] — a before/after delta snapshot taken across only the ANF
//!   phase, attributing every change to the operand lift in isolation.
//! * [`boundary`] — the Normalize→Transform hand-off for a `return` buried in a
//!   `while` condition: ANF leaves it as a leaf, Transform guards it.
//! * [`convergence`] — the operand-lift driver's convergence: the measure
//!   strictly decreases per changed iteration, and a hand-built diverging block
//!   surfaces `FixpointNotReached("anf", _)` instead of looping or panicking.

pub(super) use crate::PipelineStage;
pub(super) use crate::return_unify::normalize::tests::check_no_returns_q_roundtrip;
pub(super) use crate::return_unify::tests::{
    assert_no_reachable_returns, assert_while_condition_guarded_by_not_flag, check_anf_isolated_q,
    check_no_returns_q, check_pre_fir_transforms_to_return_unify_q, compile_return_unified,
    find_body_block_id, find_local_init, local_var_id_from_named_pat,
};
pub(super) use crate::test_utils::{
    check_semantic_equivalence, compile_and_run_pipeline_to_with_errors,
};
pub(super) use expect_test::expect;
pub(super) use indoc::indoc;

mod boundary;
mod buried_loop_lowering;
mod convergence;
mod invariant;
mod isolation;
mod lift_shapes;
mod multi_lift_convergence;
mod rejection;
mod semantic;
mod short_circuit_semantics;
mod slot_defaultable;
mod snapshot;
mod trace;
