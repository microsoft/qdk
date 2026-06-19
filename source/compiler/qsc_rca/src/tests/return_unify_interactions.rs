// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests that exercise RCA behavior in the presence of FIR transforms that
//! desugar `return` (arity-consistency / return-unify interaction coverage). The `return_unify` pass introduces a
//! synthetic flag-based early-return when a `return` appears inside a dynamic
//! scope (e.g. `if M(q) == One { return ... }`). Historically this interacted
//! badly with RCA's `dynamic_param_applications` arity invariants; the
//! `assert_arity_consistency` post-walker (see
//! `source/compiler/qsc_rca/src/invariants.rs`) now runs in debug builds at
//! the end of `Analyzer::analyze_all` / `Analyzer::analyze_package` to catch
//! skew regressions.

use expect_test::expect;
use qsc_data_structures::target::Profile;

use super::{PipelineContext, check_callable_compute_properties};

/// Return-unify regression: after the return-unification pass rewrites a dynamic-scope
/// `return` into a flag-based fallback, RCA must produce a coherent
/// `ApplicationGeneratorSet` for the enclosing callable's body spec. The
/// measurement-driven dynamism guarantees the value kind is `Variable`.
///
/// Regression note: the implicit arity-consistency invariant is enforced by
/// `PipelineContext::new`, which invokes `Analyzer::analyze_all` and therefore
/// runs `assert_arity_consistency` on the user package. Reverting the
/// arity-consistency invariant (or regressing the return-unify pass so arities diverge from
/// `CallableImpl` input counts) would flip that implicit assertion into a skew
/// panic before the explicit snapshot assertion below is reached.
#[test]
fn flag_fallback_value_kind_after_dynamic_scope_return() {
    let source = r#"
        namespace Test {
            operation DynReturn(qs : Qubit[]) : Result[] {
                mutable results = [Zero, size = Length(qs)];
                mutable i = 0;
                while i < Length(qs) {
                    if M(qs[i]) == One {
                        return results;
                    }
                    set i += 1;
                }
                results
            }
        }
    "#;
    let entry = "{ use qs = Qubit[2]; Test.DynReturn(qs) }";

    let context = PipelineContext::new(source, entry, Profile::AdaptiveRIF.into());

    check_callable_compute_properties(
        &context.fir_store,
        context.get_compute_properties(),
        "DynReturn",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicQubit | UseOfDynamicallySizedArray | MeasurementWithinDynamicScope | UseOfDynamicIndex | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                        value_kind: Variable
                    dynamic_param_applications:
                        [0]: [Parameter Type Array] ArrayParamApplication:
                            constant_content: Dynamic:
                                runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicQubit | UseOfDynamicallySizedArray | MeasurementWithinDynamicScope | UseOfDynamicIndex | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                                value_kind: Variable
                            static_size: Dynamic:
                                runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicQubit | UseOfDynamicArray | UseOfDynamicallySizedArray | MeasurementWithinDynamicScope | UseOfDynamicIndex | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                                value_kind: Variable
                            dynamic_size: Dynamic:
                                runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicQubit | UseOfDynamicArray | UseOfDynamicallySizedArray | MeasurementWithinDynamicScope | UseOfDynamicIndex | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                                value_kind: Variable
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}
