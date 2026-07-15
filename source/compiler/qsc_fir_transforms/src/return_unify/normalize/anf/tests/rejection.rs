// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The operand lift covers non-defaultable temps; the residual rejection is a
//! non-defaultable *binding* whose initializer buries a `return`.
//!
//! An operand whose lifted temp has a type with no synthesizable classical
//! default (realistically `Qubit`, or a tuple/UDT containing it) is backed by a
//! length-1 array, so the temp travels through lowering as `operand.ty[]`
//! (which always has the default `[]`). Such operands therefore lift without a
//! diagnostic; the first two locks pin that policy by asserting the warning
//! channel stays silent and no raw `Return` survives.
//!
//! What still cannot be lowered is unrelated to operand temps: a `let`/`use`
//! binding whose *pattern type* is itself non-defaultable (a user struct with a
//! `Qubit` field) and whose initializer buries a `return`. That binding would
//! need a default for its own non-return path, and none exists, so the callable
//! is left un-rewritten and a non-fatal `UnsupportedHoistContext` diagnostic is
//! emitted on the warning channel. The final lock reads that channel through
//! the shared [`has_unsupported_hoist_context`] probe.

use super::*;

/// Returns `true` when `diagnostics` contains at least one
/// `UnsupportedHoistContext` `return_unify` diagnostic.
///
/// The warn-and-delegate policy routes these into the pipeline's warning
/// channel, so the locks below probe `result.warnings`.
fn has_unsupported_hoist_context(diagnostics: &[crate::OwnedPipelineError]) -> bool {
    diagnostics.iter().any(|err| {
        matches!(
            err.error,
            crate::PipelineError::ReturnUnify(crate::return_unify::Error::UnsupportedHoistContext(
                _,
                _
            ))
        )
    })
}

#[test]
fn operand_return_with_qubit_temp_lifts_without_warning() {
    // A `return` buried in a `{ … return … }` block whose value type is `Qubit`
    // (an array element). The ANF lift backs the temp with a length-1 array, so
    // it binds as `let __operand_tmp : Qubit[] = …;` and lowers cleanly. The
    // enclosing `let arr : Qubit[]` binding has a classical default (`[]`), so
    // nothing on this path is rejected: the callable is rewritten and no warning
    // is emitted.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Qubit[] {
                use q = Qubit();
                use q2 = Qubit();
                let arr = [q, { return [q]; q2 }];
                arr
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !has_unsupported_hoist_context(&result.warnings),
        "operand return whose lifted temp is Qubit should lift without UnsupportedHoistContext, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );

    let (store, pkg_id) = compile_return_unified(source);
    assert_no_reachable_returns(&store, pkg_id);
}

#[test]
fn non_defaultable_binding_with_buried_return_is_left_with_warning() {
    // A `return` buried in the initializer of a `let h : Holder` binding whose
    // *pattern type* `Holder` is non-defaultable (it has a `Qubit` field). This
    // is not an operand temp: the binding itself would need a `Holder` default
    // for its non-return path, and none exists. (The inner field block does lift
    // its own operand temp via array backing, but the surrounding non-defaultable
    // binding still cannot be lowered.) The callable is left un-rewritten and a
    // graceful warning is emitted instead of panicking in the slot machinery.
    let source = indoc! {r#"
        namespace Test {
            struct Holder { Q : Qubit }
            @EntryPoint()
            operation Main() : Qubit {
                use q = Qubit();
                use q2 = Qubit();
                let h = new Holder { Q = { return q; q2 } };
                h.Q
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        has_unsupported_hoist_context(&result.warnings),
        "buried return in a non-defaultable `Holder` binding should warn with UnsupportedHoistContext, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );
}

#[test]
fn operand_return_with_tuple_temp_lifts_without_warning() {
    // A `return` buried in a `{ … return … }` block whose value type is the
    // tuple `(Qubit, Int)` (an array element). The lifted temp is backed by a
    // length-1 array (`(Qubit, Int)[]`), so it lowers cleanly. The enclosing
    // `let arr : (Qubit, Int)[]` binding has a classical default (`[]`), so the
    // callable is rewritten and no warning is emitted.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Qubit, Int) {
                use q = Qubit();
                use q2 = Qubit();
                let arr = [{ return (q, 0); (q2, 1) }];
                arr[0]
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !has_unsupported_hoist_context(&result.warnings),
        "operand return whose lifted temp is (Qubit, Int) should lift without UnsupportedHoistContext, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );

    let (store, pkg_id) = compile_return_unified(source);
    assert_no_reachable_returns(&store, pkg_id);
}

/// Returns the number of `UnsupportedHoistContext` `return_unify` diagnostics
/// in `diagnostics`.
fn count_unsupported_hoist_context(diagnostics: &[crate::OwnedPipelineError]) -> usize {
    diagnostics
        .iter()
        .filter(|err| {
            matches!(
                err.error,
                crate::PipelineError::ReturnUnify(
                    crate::return_unify::Error::UnsupportedHoistContext(_, _)
                )
            )
        })
        .count()
}

/// Returns `true` when `diagnostics` contains at least one
/// `UnsupportedEarlyReturnType` `return_unify` diagnostic.
fn has_unsupported_early_return_type(diagnostics: &[crate::OwnedPipelineError]) -> bool {
    diagnostics.iter().any(|err| {
        matches!(
            err.error,
            crate::PipelineError::ReturnUnify(
                crate::return_unify::Error::UnsupportedEarlyReturnType(_, _)
            )
        )
    })
}

#[test]
fn hoist_minted_short_circuit_context_normalizes_without_spurious_rejection() {
    // The only problematic context here is minted by the hoist pass, not
    // present in the pristine pre-hoist FIR the pre-check inspects: a
    // short-circuit `a or (return …)` whose enclosing binding type is
    // defaultable (`Bool`). The pre-check runs once on pre-hoist FIR and must
    // not reject this, because the `or`'s only unconditional operand is its
    // return-free LHS. Hoist then rewrites the short-circuit into an `If`, and
    // the callable normalizes with no surviving raw `Return`.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                let b = (MResetZ(q) == One) or (return true);
                b
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !has_unsupported_hoist_context(&result.warnings),
        "hoist-minted short-circuit context should not be rejected, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );
    assert!(
        !has_unsupported_early_return_type(&result.errors),
        "a Bool early return is supported and must not be rejected, got errors: {:?}",
        result.errors,
    );

    let (store, pkg_id) = compile_return_unified(source);
    assert_no_reachable_returns(&store, pkg_id);
}

#[test]
fn hoist_minted_condition_block_context_normalizes_without_spurious_rejection() {
    // The only problematic context is again minted by hoist: a `return` buried
    // in an `if` condition whose enclosing `if` is `Unit`-typed (defaultable).
    // The pre-check rejects an `if`-condition return only when the `if` type is
    // non-Unit and non-defaultable, so a `Unit` `if` is left for hoist to lower
    // into a leading `Block`. The callable normalizes with no surviving
    // `Return`.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                if (MResetZ(q) == One and (return true)) {
                    X(q);
                }
                false
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !has_unsupported_hoist_context(&result.warnings),
        "hoist-minted condition-block context should not be rejected, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );
    assert!(
        !has_unsupported_early_return_type(&result.errors),
        "a Bool early return is supported and must not be rejected, got errors: {:?}",
        result.errors,
    );

    let (store, pkg_id) = compile_return_unified(source);
    assert_no_reachable_returns(&store, pkg_id);
}

#[test]
fn genuinely_unsupported_binding_is_rejected_exactly_once() {
    // A genuinely-unsupported context is rejected by the single pre-check pass
    // before any mutation: a `let picked : Qubit` binding (non-defaultable
    // pattern type) whose initializer buries a `return`. There is no `Qubit`
    // default to seed the binding's non-return path, so the callable is left
    // un-rewritten with exactly one `UnsupportedHoistContext` diagnostic — the
    // pre-check runs once over pristine FIR and does not double-report.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Qubit {
                use q = Qubit();
                use q2 = Qubit();
                let picked = if MResetZ(q) == One { return q; q2 } else { q2 };
                picked
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert_eq!(
        count_unsupported_hoist_context(&result.warnings),
        1,
        "a non-defaultable Qubit binding with a buried return should be rejected exactly once, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );
}

#[test]
fn use_allocated_register_operand_return_lifts_without_warning() {
    // A `use`-allocated qubit register followed by an operand-position return
    // whose lifted temp is a `Qubit[]` element. The lift backs the temp with a
    // length-1 array and the enclosing `let arr : Qubit[]` binding has the
    // classical default `[]`, so the callable is rewritten with no warning and
    // no surviving `Return`.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Qubit[] {
                use qs = Qubit[2];
                let arr = [qs[0], { return qs; qs[1] }];
                arr
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !has_unsupported_hoist_context(&result.warnings),
        "use-allocated register operand return should lift without UnsupportedHoistContext, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );

    let (store, pkg_id) = compile_return_unified(source);
    assert_no_reachable_returns(&store, pkg_id);
}

#[test]
fn tuple_pattern_binding_operand_return_normalizes_soundly() {
    // A tuple-pattern local `let (a, b) = (…, …)` whose initializer buries a
    // `return` in operand position. The tuple pattern type `(Int, Int)` is
    // defaultable, so the binding is not rejected, and the buried return lifts
    // to a spine temp. The transformed program returns the same value as the
    // untransformed one.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                let (a, b) = ({ return 1; 2 }, 3);
                a + b
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !has_unsupported_hoist_context(&result.warnings),
        "tuple-pattern binding with a buried operand return should normalize without UnsupportedHoistContext, got warnings: {:?}, errors: {:?}",
        result.warnings,
        result.errors,
    );

    let (store, pkg_id) = compile_return_unified(source);
    assert_no_reachable_returns(&store, pkg_id);

    check_semantic_equivalence(source);
}
