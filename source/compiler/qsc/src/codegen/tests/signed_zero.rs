// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::target::Profile;

use super::{compile_source_to_qir, qir_entry_body};

/// Regression: the compiler dropped the mutable update from +0.0 to -0.0 because
/// those values compare equal. A reciprocal makes their different signs observable.
///
/// Division by zero is intentional: the classical evaluator follows IEEE Double
/// arithmetic, so 1.0 / +0.0 is positive infinity and 1.0 / -0.0 is negative infinity.
/// The condition must therefore fold to true, and X must flip the fresh qubit so
/// the recorded measurement is One. Losing the update would instead produce Zero.
///
/// Each snapshot checks the complete entry body: exactly one X before measuring
/// the same qubit and recording its result, with no division or branch remaining.
fn assert_negating_positive_zero_applies_x_before_measurement(profile: Profile, expected: &Expect) {
    let qir = compile_source_to_qir(
        indoc! {r#"
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable x = 0.0;
                set x = -x;
                if 1.0 / x < 0.0 {
                    X(q);
                }
                MResetZ(q)
            }
        "#},
        profile.into(),
    );
    expected.assert_eq(qir_entry_body(&qir));
}

#[test]
fn negating_positive_zero_applies_x_before_measurement_in_base_profile() {
    // Base lowers the terminal MResetZ to measurement without a reset.
    assert_negating_positive_zero_applies_x_before_measurement(
        Profile::Base,
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
    );
}

#[test]
fn negating_positive_zero_applies_x_before_measurement_in_adaptive_ri_profile() {
    assert_negating_positive_zero_applies_x_before_measurement(
        Profile::AdaptiveRI,
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
    );
}

#[test]
fn negating_positive_zero_applies_x_before_measurement_in_adaptive_rif_profile() {
    assert_negating_positive_zero_applies_x_before_measurement(
        Profile::AdaptiveRIF,
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0"#]],
    );
}

#[test]
fn negating_positive_zero_applies_x_before_measurement_in_adaptive_profile() {
    // Full Adaptive calls the emitted X operation; lower profiles inline its intrinsic.
    assert_negating_positive_zero_applies_x_before_measurement(
        Profile::Adaptive,
        &expect![[r#"
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              call void @X(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
              ret i64 0"#]],
    );
}
