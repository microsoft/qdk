// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::test_expression;
use super::test_expression_fails;
use super::test_expression_with_lib;
use expect_test::expect;
use qsc::interpret::Value;

// Tests for Std.StatePreparation namespace

const STATE_PREPARATION_TEST_LIB: &str = include_str!("resources/src/state_preparation.qs");

#[test]
fn check_plus_state_preparation() {
    let out = test_expression_with_lib(
        "Test.TestPlusState()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |0⟩: 0.7071+0.0000𝑖
        |1⟩: 0.7071+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_minus_state_preparation() {
    let out = test_expression_with_lib(
        "Test.TestMinusState()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |0⟩: 0.7071+0.0000𝑖
        |1⟩: −0.7071+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_bell_state_preparation() {
    let out = test_expression_with_lib(
        "Test.TestBellState()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |00⟩: 0.7071+0.0000𝑖
        |11⟩: 0.7071+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_cat_state_preparation() {
    let out = test_expression_with_lib(
        "Test.TestCat3State()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |000⟩: 0.7071+0.0000𝑖
        |111⟩: 0.7071+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_complex_preparation() {
    let out = test_expression_with_lib(
        "Test.TestPrepareComplex()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |00⟩: 0.5000+0.0000𝑖
        |01⟩: 0.3536+0.3536𝑖
        |10⟩: 0.0000+0.5000𝑖
        |11⟩: −0.3536+0.3536𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_preparation_completion() {
    let out = test_expression_with_lib(
        "Test.TestPreparationCompletion()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |0⟩: 0.7738+0.0000𝑖
        |1⟩: 0.6335+0.0000𝑖
        STATE:
        |00⟩: 0.1830+0.0000𝑖
        |01⟩: 0.4070+0.0000𝑖
        |10⟩: 0.6049+0.0000𝑖
        |11⟩: 0.6595+0.0000𝑖
        STATE:
        |000⟩: 0.0987+0.0000𝑖
        |001⟩: 0.3590+0.0000𝑖
        |010⟩: 0.4657+0.0000𝑖
        |011⟩: 0.4674+0.0000𝑖
        |100⟩: 0.4199+0.0000𝑖
        |101⟩: 0.1184+0.0000𝑖
        |110⟩: 0.4619+0.0000𝑖
        |111⟩: 0.1496+0.0000𝑖
        STATE:
        |0000⟩: 0.2715+0.0000𝑖
        |0001⟩: 0.0584+0.0000𝑖
        |0010⟩: 0.1164+0.0000𝑖
        |0011⟩: 0.3611+0.0000𝑖
        |0100⟩: 0.3074+0.0000𝑖
        |0101⟩: 0.1934+0.0000𝑖
        |0110⟩: 0.2742+0.0000𝑖
        |0111⟩: 0.3325+0.0000𝑖
        |1000⟩: 0.1302+0.0000𝑖
        |1001⟩: 0.2225+0.0000𝑖
        |1010⟩: 0.3149+0.0000𝑖
        |1011⟩: 0.2107+0.0000𝑖
        |1100⟩: 0.2124+0.0000𝑖
        |1101⟩: 0.2455+0.0000𝑖
        |1110⟩: 0.3067+0.0000𝑖
        |1111⟩: 0.2277+0.0000𝑖
        STATE:
        |0⟩: −0.7738+0.0000𝑖
        |1⟩: 0.6335+0.0000𝑖
        STATE:
        |00⟩: 0.1830+0.0000𝑖
        |01⟩: −0.4070+0.0000𝑖
        |10⟩: 0.6049+0.0000𝑖
        |11⟩: 0.6595+0.0000𝑖
        STATE:
        |000⟩: 0.0987+0.0000𝑖
        |001⟩: −0.3590+0.0000𝑖
        |010⟩: 0.4657+0.0000𝑖
        |011⟩: −0.4674+0.0000𝑖
        |100⟩: 0.4199+0.0000𝑖
        |101⟩: 0.1184+0.0000𝑖
        |110⟩: −0.4619+0.0000𝑖
        |111⟩: 0.1496+0.0000𝑖
        STATE:
        |0000⟩: −0.2715+0.0000𝑖
        |0001⟩: 0.0584+0.0000𝑖
        |0010⟩: 0.1164+0.0000𝑖
        |0011⟩: 0.3611+0.0000𝑖
        |0100⟩: −0.3074+0.0000𝑖
        |0101⟩: 0.1934+0.0000𝑖
        |0110⟩: −0.2742+0.0000𝑖
        |0111⟩: 0.3325+0.0000𝑖
        |1000⟩: 0.1302+0.0000𝑖
        |1001⟩: 0.2225+0.0000𝑖
        |1010⟩: 0.3149+0.0000𝑖
        |1011⟩: −0.2107+0.0000𝑖
        |1100⟩: 0.2124+0.0000𝑖
        |1101⟩: 0.2455+0.0000𝑖
        |1110⟩: −0.3067+0.0000𝑖
        |1111⟩: −0.2277+0.0000𝑖
        STATE:
        |000⟩: 0.7412+0.0000𝑖
        |001⟩: 0.2422+0.0000𝑖
        |010⟩: 0.3142+0.0000𝑖
        |011⟩: −0.3153+0.0000𝑖
        |100⟩: 0.2833+0.0000𝑖
        |101⟩: 0.0799+0.0000𝑖
        |110⟩: 0.3116+0.0000𝑖
        |111⟩: 0.1009+0.0000𝑖
        STATE:
        |000⟩: 0.7844+0.0000𝑖
        |001⟩: 0.2563+0.0000𝑖
        |010⟩: 0.3325+0.0000𝑖
        |011⟩: −0.3337+0.0000𝑖
        |100⟩: 0.2998+0.0000𝑖
        |101⟩: 0.0846+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_preparation_endianness() {
    let out = test_expression_with_lib(
        "Test.TestEndianness()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |0000⟩: 1.0000+0.0000𝑖
        STATE:
        |0001⟩: 1.0000+0.0000𝑖
        STATE:
        |0010⟩: 1.0000+0.0000𝑖
        STATE:
        |0011⟩: 1.0000+0.0000𝑖
        STATE:
        |0100⟩: 1.0000+0.0000𝑖
        STATE:
        |0101⟩: 1.0000+0.0000𝑖
        STATE:
        |0110⟩: 1.0000+0.0000𝑖
        STATE:
        |0111⟩: 1.0000+0.0000𝑖
        STATE:
        |1000⟩: 1.0000+0.0000𝑖
        STATE:
        |1001⟩: 1.0000+0.0000𝑖
        STATE:
        |1010⟩: 1.0000+0.0000𝑖
        STATE:
        |1011⟩: 1.0000+0.0000𝑖
        STATE:
        |1100⟩: 1.0000+0.0000𝑖
        STATE:
        |1101⟩: 1.0000+0.0000𝑖
        STATE:
        |1110⟩: 1.0000+0.0000𝑖
        STATE:
        |1111⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_preparation_doc_sample() {
    let out = test_expression(
        "{
        import Std.Math.*;
        let amplitudes = [Sqrt(0.125), 0.0, Sqrt(0.875), 0.0];
        use qubits = Qubit[2];
        Std.StatePreparation.PreparePureStateD(amplitudes, qubits);
        Std.Diagnostics.DumpMachine();
        ResetAll(qubits); }",
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |00⟩: 0.3536+0.0000𝑖
        |10⟩: 0.9354+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_uniform_superposition_preparation() {
    let out = test_expression_with_lib(
        "Test.TestPrepareUniformSuperposition(5)",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );

    expect![[r#"
        STATE:
        |0000000000⟩: 0.4472+0.0000𝑖
        |0010000000⟩: 0.4472+0.0000𝑖
        |0100000000⟩: 0.4472+0.0000𝑖
        |1000000000⟩: 0.4472+0.0000𝑖
        |1100000000⟩: 0.4472+0.0000𝑖
    "#]]
    .assert_eq(&out);
}

#[test]
fn check_uniform_superposition_preparation_exhaustive() {
    let _ = test_expression_with_lib(
        "Test.TestPrepareUniformSuperpositionExhaustive()",
        STATE_PREPARATION_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_uniform_superposition_short_array() {
    let out = test_expression_fails(
        "{
            use qs=Qubit[2];
            Std.StatePreparation.PrepareUniformSuperposition(5, qs);
        }",
    );

    expect!["program failed: Qubit register is too short to prepare 5 states."].assert_eq(&out);
}

#[test]
fn check_uniform_superposition_invalid_state_count() {
    let out = test_expression_fails(
        "{
            use qs=Qubit[2];
            Std.StatePreparation.PrepareUniformSuperposition(0, qs);
        }",
    );

    expect!["program failed: Number of basis states must be positive."].assert_eq(&out);
}
