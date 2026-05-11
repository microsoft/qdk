// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{test_expression, test_expression_with_lib};
use indoc::indoc;
use qsc::interpret::Value;

// Tests for Std.Canon namespace

#[test]
fn check_apply_to_each() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[3];
            ApplyToEach(X, register);
            let results = Std.Measurement.MeasureEachZ(register);
            ResetAll(register);
            results
        }"#},
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_apply_to_each_a() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[3];
            ApplyToEach(X, register);
            Adjoint Std.Canon.ApplyToEachA(X, register);
            let results = Std.Measurement.MResetEachZ(register);
            results
        }"#},
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ZERO, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_apply_to_each_c_applied() {
    test_expression(
        indoc! {r#"{
            use control = Qubit();
            use register = Qubit[3];
            Controlled Std.Canon.ApplyToEachC([control], (X, register));
            let results = Std.Measurement.MResetEachZ(register);
            Reset(control);
            results
        }"#},
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ZERO, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_apply_to_each_c_not_applied() {
    test_expression(
        indoc! {r#"{
            use control = Qubit();
            use register = Qubit[3];
            X(control);
            Controlled Std.Canon.ApplyToEachC([control], (X, register));
            let results = Std.Measurement.MResetEachZ(register);
            Reset(control);
            results
        }"#},
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_apply_to_each_ca_applied() {
    test_expression(
        indoc! {r#"{
            use control = Qubit();
            use register = Qubit[3];
            Std.Canon.ApplyToEach(X, register);
            Controlled Adjoint Std.Canon.ApplyToEachCA([control], (X, register));
            let results = Std.Measurement.MResetEachZ(register);
            Reset(control);
            results
        }"#},
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_apply_to_each_ca_not_applied() {
    test_expression(
        indoc! {r#"{
            use control = Qubit();
            use register = Qubit[3];
            X(control);
            Std.Canon.ApplyToEach(X, register);
            Controlled Adjoint Std.Canon.ApplyToEachCA([control], (X, register));
            let results = Std.Measurement.MResetEachZ(register);
            Reset(control);
            results
        }"#},
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ZERO, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_fst_snd() {
    test_expression("Fst(7,6)", &Value::Int(7));
    test_expression("Snd(7,6)", &Value::Int(6));
}

#[test]
fn check_swap_labels() {
    test_expression(
        "{
                use qs = Qubit[2];
                X(qs[0]);
                Relabel(qs, [qs[1], qs[0]]);
                MResetEachZ(qs)
            }",
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_relabel_rotational_permutation() {
    test_expression(
        "{
                use qs = Qubit[3];
                // Prepare |01+⟩
                X(qs[1]);
                H(qs[2]);
                Relabel([qs[0], qs[1], qs[2]], [qs[1], qs[2], qs[0]]);
                // Expected state is |1+0⟩, perform adjoint to get back to ground state.
                X(qs[0]);
                H(qs[1]);
                // Qubit release will fail if the state is not |000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_rotational_permutation_alternate_expression() {
    test_expression(
        "{
                use qs = Qubit[3];
                // Prepare |01+⟩
                X(qs[1]);
                H(qs[2]);
                Relabel([qs[2], qs[0], qs[1]], [qs[0], qs[1], qs[2]]);
                // Expected state is |1+0⟩, perform adjoint to get back to ground state.
                X(qs[0]);
                H(qs[1]);
                // Qubit release will fail if the state is not |000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_rotational_permutation_size_4() {
    test_expression(
        "{
                use qs = Qubit[4];
                // Prepare |01+0⟩
                X(qs[1]);
                H(qs[2]);
                Relabel(qs, qs[2...] + qs[0..1]);
                // Expected state is |+001⟩, perform adjoint to get back to ground state.
                H(qs[0]);
                X(qs[Length(qs)-1]);
                // Qubit release will fail if the state is not |000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_rotational_permutation_size_5() {
    test_expression(
        "{
                use qs = Qubit[5];
                // Prepare |01+00⟩
                X(qs[1]);
                H(qs[2]);
                Relabel(qs, qs[2...] + qs[0..1]);
                // Expected state is |+0001⟩, perform adjoint to get back to ground state.
                H(qs[0]);
                X(qs[Length(qs)-1]);
                // Qubit release will fail if the state is not |000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_rotational_permutation_size_6() {
    test_expression(
        "{
                use qs = Qubit[6];
                // Prepare |01+000⟩
                X(qs[1]);
                H(qs[2]);
                Relabel(qs, qs[2...] + qs[0..1]);
                // Expected state is |+00001⟩, perform adjoint to get back to ground state.
                H(qs[0]);
                X(qs[Length(qs)-1]);
                // Qubit release will fail if the state is not |000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_rotational_permutation_size_7() {
    test_expression(
        "{
                use qs = Qubit[7];
                // Prepare |01+0000⟩
                X(qs[1]);
                H(qs[2]);
                Relabel(qs, qs[2...] + qs[0..1]);
                // Expected state is |+000001⟩, perform adjoint to get back to ground state.
                H(qs[0]);
                X(qs[Length(qs)-1]);
                // Qubit release will fail if the state is not |000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_four_qubit_shuffle_permutation() {
    test_expression(
        "{
                use qs = Qubit[4];
                // Prepare |01+i⟩
                X(qs[1]);
                H(qs[2]);
                H(qs[3]);
                S(qs[3]);
                H(qs[3]);
                Relabel([qs[0], qs[1], qs[2], qs[3]], [qs[1], qs[0], qs[3], qs[2]]);
                // Expected state is |10i+⟩, perform adjoint to get back to ground state.
                X(qs[0]);
                H(qs[2]);
                Adjoint S(qs[2]);
                H(qs[2]);
                H(qs[3]);
                // Qubit release will fail if the state is not |0000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_five_qubit_shuffle_permutation() {
    test_expression(
        "{
                use qs = Qubit[5];
                // Prepare |01+i-⟩
                X(qs[1]);
                H(qs[2]);
                H(qs[3]);
                S(qs[3]);
                H(qs[3]);
                H(qs[4]);
                Z(qs[4]);
                Relabel([qs[0], qs[1], qs[2], qs[3], qs[4]], [qs[1], qs[0], qs[3], qs[4], qs[2]]);
                // Expected state is |10i-+⟩, perform adjoint to get back to ground state.
                X(qs[0]);
                H(qs[2]);
                Adjoint S(qs[2]);
                H(qs[2]);
                Z(qs[3]);
                H(qs[3]);
                H(qs[4]);
                // Qubit release will fail if the state is not |0000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_relabel_adjoint_undoes_permutation() {
    test_expression(
        "{
                use qs = Qubit[3];
                // Prepare |01+⟩
                X(qs[1]);
                H(qs[2]);
                Relabel([qs[0], qs[1], qs[2]], [qs[1], qs[2], qs[0]]);
                // Expected state is |1+0⟩, perform part of the adjoint to correct one of the qubits.
                X(qs[0]);
                Adjoint Relabel([qs[0], qs[1], qs[2]], [qs[1], qs[2], qs[0]]);
                // Expected state is now |00+⟩, perform the rest of the adjoint to get back to ground state,
                // using the original qubit ids.
                H(qs[2]);
                // Qubit release will fail if the state is not |000⟩
            }",
        &Value::unit(),
    );
}

#[test]
fn check_apply_cnot_chain_2() {
    test_expression(
        {
            "{
            use a = Qubit[2];
            mutable result = [];
            within {
                X(a[0]);
                X(a[1]);
                ApplyCNOTChain(a);
            }
            apply {
                set result = [M(a[0]),M(a[1])];
            }
            return result;
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_apply_cnot_chain_3() {
    test_expression(
        {
            "{
            use a = Qubit[3];
            mutable result = [];
            within {
                X(a[0]);
                ApplyCNOTChain(a);
            }
            apply {
                set result = [M(a[0]),M(a[1]),M(a[2])];
            }
            return result;
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_apply_cnot_chain_3a() {
    test_expression(
        {
            "{
            use a = Qubit[3];
            mutable result = [];
            within {
                X(a[0]);
                X(a[2]);
                ApplyCNOTChain(a);
            }
            apply {
                set result = [M(a[0]),M(a[1]),M(a[2])];
            }
            return result;
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_apply_p() {
    test_expression(
        {
            "{
            use q = Qubit[3];
            ApplyP(PauliX, q[0]);
            H(q[1]); ApplyP(PauliY, q[1]);
            H(q[2]); S(q[2]); ApplyP(PauliZ, q[2]);
            return [MResetZ(q[0]),MResetX(q[1]),MResetY(q[2])];
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_apply_pauli() {
    test_expression(
        {
            "{
            use q = Qubit[3];
            H(q[1]);
            H(q[2]); S(q[2]);
            ApplyPauli([PauliX, PauliY, PauliZ], q);
            return [MResetZ(q[0]),MResetX(q[1]),MResetY(q[2])];
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_apply_pauli_from_bit_string() {
    test_expression(
        {
            "{
            use q = Qubit[3];
            ApplyPauliFromBitString(PauliX, false, [true, false, true], q);
            return MResetEachZ(q);
        }"
        },
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_apply_pauli_from_int() {
    test_expression(
        {
            "{
            use q = Qubit[3];
            ApplyPauliFromInt(PauliX, false, 5, q);
            return MResetEachZ(q);
        }"
        },
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_map_pauli_x_x() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => MapPauliAxis(PauliX, PauliX, q[0]),
                    q => I(q[0])
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_y_y() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => MapPauliAxis(PauliY, PauliY, q[0]),
                    q => I(q[0])
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_z_z() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => MapPauliAxis(PauliZ, PauliZ, q[0]),
                    q => I(q[0])
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_x_y() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => { within { MapPauliAxis(PauliX, PauliY, q[0]) } apply { Rx(0.1, q[0]) } },
                    q => { within { Adjoint S(q[0]) } apply { Rx(0.1, q[0]) } }
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_y_x() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => { within { MapPauliAxis(PauliY, PauliX, q[0]) } apply { Ry(0.1, q[0]) } },
                    q => { within { S(q[0]) } apply { Ry(0.1, q[0]) } }
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_x_z() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => { within { MapPauliAxis(PauliX, PauliZ, q[0]) } apply { Rx(0.1, q[0]) } },
                    q => { within { H(q[0]) } apply { Rx(0.1, q[0]) } }
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_z_x() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => { within { MapPauliAxis(PauliZ, PauliX, q[0]) } apply { Rz(0.1, q[0]) } },
                    q => { within { H(q[0]) } apply { Rz(0.1, q[0]) } }
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_y_z() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => { within { MapPauliAxis(PauliY, PauliZ, q[0]) } apply { Ry(0.1, q[0]) } },
                    q => { within { H(q[0]); Adjoint S(q[0]); H(q[0]) } apply { Ry(0.1, q[0]) } }
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_map_pauli_z_y() {
    test_expression(
        {
            "{
                Std.Diagnostics.CheckOperationsAreEqual(1,
                    q => { within { MapPauliAxis(PauliZ, PauliY, q[0]) } apply { Rz(0.1, q[0]) } },
                    q => { within { H(q[0]); S(q[0]); H(q[0]) } apply { Rz(0.1, q[0]) } }
                )
            }"
        },
        &Value::Bool(true),
    );
}

#[test]
fn check_apply_controlled_on_int() {
    test_expression(
        {
            "{
            use c = Qubit[3];
            use t1 = Qubit();
            use t2 = Qubit();
            within {
                X(c[0]);
                X(c[2]);
            } apply {
                ApplyControlledOnInt(5, X, c, t1);
            }
            ApplyControlledOnInt(5, X, c, t2);
            return [MResetZ(t1), M(t2)];
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_apply_controlled_on_bitstring() {
    test_expression(
        {
            "{
            use c = Qubit[4];
            use t1 = Qubit();
            use t2 = Qubit();
            within {
                X(c[0]);
                X(c[2]);
            } apply {
                ApplyControlledOnBitString([true, false, true], X, c, t1);
            }
            ApplyControlledOnBitString([true, false, true], X, c, t2);
            return [MResetZ(t1), M(t2)];
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

const QFT_LE_TEST_LIB: &str = include_str!("resources/src/qft_le.qs");

#[test]
fn check_qft_le_sample_1() {
    test_expression_with_lib(
        "Test.TestQFT(1)",
        QFT_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_qft_le_sample_2() {
    test_expression_with_lib(
        "Test.TestQFT(2)",
        QFT_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}
#[test]
fn check_qft_le_sample_3() {
    test_expression_with_lib(
        "Test.TestQFT(3)",
        QFT_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}
#[test]
fn check_qft_le_sample_4() {
    test_expression_with_lib(
        "Test.TestQFT(4)",
        QFT_LE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

const QPE_TEST_LIB: &str = include_str!("resources/src/qpe.qs");

#[test]
fn check_qpe_z() {
    test_expression_with_lib(
        "Test.TestQPE_Z()",
        QPE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_qpe_s() {
    test_expression_with_lib(
        "Test.TestQPE_S()",
        QPE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_qpe_t() {
    test_expression_with_lib(
        "Test.TestQPE_T()",
        QPE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_qpe_p() {
    test_expression_with_lib(
        "Test.TestQPE_P()",
        QPE_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_swap_reverse_register() {
    test_expression(
        {
            "{
                use q = Qubit[10];
                ApplyXorInPlace(328, q);
                SwapReverseRegister(q);
                let r = MeasureInteger(q);
                ResetAll(q);
                r
        }"
        },
        &Value::Int(74),
    );
}

#[test]
fn check_apply_xor_in_place() {
    test_expression(
        {
            "{
            use a = Qubit[3];
            mutable result = [];
            within {
                ApplyXorInPlace(3, a);
            }
            apply {
                set result = [M(a[0]),M(a[1]),M(a[2])];
            }
            return result;
        }"
        },
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_apply_xor_in_place_l() {
    test_expression(
        {
            "{
            use q = Qubit[100];
            ApplyXorInPlaceL(953L <<< 50, q);
            let result = MeasureInteger(q[50...]);
            ResetAll(q);
            result
        }"
        },
        &Value::Int(953),
    );
}

#[test]
fn check_apply_operation_power_a() {
    test_expression(
        {
            "{
            use q = Qubit();
            ApplyOperationPowerA(12, Rx(Std.Math.PI()/16.0, _), q);
            ApplyOperationPowerA(-3, Rx(Std.Math.PI()/4.0, _), q);
            M(q)
        }"
        },
        &Value::RESULT_ZERO,
    );
}

#[test]
fn check_apply_operation_power_ca() {
    test_expression(
        {
            "{
            use q = Qubit();
            ApplyOperationPowerCA(12, Rx(Std.Math.PI()/16.0, _), q);
            ApplyOperationPowerCA(-3, Rx(Std.Math.PI()/4.0, _), q);
            M(q)
        }"
        },
        &Value::RESULT_ZERO,
    );
}
