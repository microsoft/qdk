// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Verifies that an effect-preserving immutable arrow pin does not change target
//! capability classification. The check runs after FIR transformation because
//! the pin does not exist in source FIR.

use crate::PipelineStage;
use crate::test_utils::compile_and_run_pipeline_to;
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_passes::PassContext;

/// Base-profile source that requires an effectful arrow operand to be pinned.
const EFFECTFUL_ARROW_OPERAND_PIN: &str = r#"
    namespace Test {
        function Inc(x : Int) : Int { x + 1 }
        operation GetOp(q : Qubit) : (Int -> Int) {
            X(q);
            Inc
        }
        operation Main() : Result {
            use q = Qubit();
            let go = false;
            let x = GetOp(q)({
                if go { return Zero; }
                5
            });
            if x > 5 {
                X(q);
            }
            M(q)
        }
    }"#;

/// Control source proving the post-transform capability check still rejects
/// measurement-dependent Boolean output.
const EFFECTFUL_ARROW_OPERAND_PIN_WITH_BOOL_OUTPUT: &str = r#"
    namespace Test {
        function Inc(x : Int) : Int { x + 1 }
        operation GetOp(q : Qubit) : (Int -> Int) {
            X(q);
            Inc
        }
        operation Main() : Bool {
            use q = Qubit();
            let go = false;
            let x = GetOp(q)({
                if go { return false; }
                5
            });
            M(q) == One and x > 5
        }
    }"#;

#[test]
fn effectful_arrow_pin_stays_base_profile_legal() {
    // The immutable pin must preserve the source program's base-profile verdict.
    assert_eq!(
        base_profile_capability_errors(EFFECTFUL_ARROW_OPERAND_PIN),
        Vec::<String>::new(),
        "the immutable arrow pin must not push a base-profile-legal program over the profile"
    );
}

#[test]
fn base_profile_check_still_fires_on_a_pinned_program() {
    // Sensitivity control for the post-transform check.
    assert_eq!(
        base_profile_capability_errors(EFFECTFUL_ARROW_OPERAND_PIN_WITH_BOOL_OUTPUT),
        vec!["CapabilitiesCk(UseOfBoolOutput(Span { lo: 182, hi: 186 }))".to_string()],
        "the post-transform capability check should still reject a bool output"
    );
}

/// Runs codegen's capability check on fully transformed FIR.
fn base_profile_capability_errors(source: &str) -> Vec<String> {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    match PassContext::run_fir_passes_on_fir(&store, pkg_id, TargetCapabilityFlags::empty()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.iter().map(|error| format!("{error:?}")).collect(),
    }
}
