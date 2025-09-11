// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::needless_raw_string_hashes, clippy::too_many_lines)]

use expect_test::expect;
use indoc::indoc;
use qsc::{SparseSim, interpret::Value, target::Profile};

use super::{test_expression, test_expression_fails, test_expression_with_lib_and_profile_and_sim};

// These tests verify multi-controlled decomposition logic for gate operations. Each test
// manually allocates 2N qubits, performs the decomposed operation from the library on the first N,
// verifies the resulting state via dump, and then uncomputes the operation via simulator-native
// multi-controlled operations to verify via Choi-Jamiolkowski isomorphism that the decomposition
// was correct.

#[test]
fn test_mch_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled H(qs[0..0], qs[1]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.3536+0.0000𝑖
        |1011⟩: 0.3536+0.0000𝑖
        |1110⟩: 0.3536+0.0000𝑖
        |1111⟩: −0.3536+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0], 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mch_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled H(qs[0..1], qs[2]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.2500+0.0000𝑖
        |110111⟩: 0.2500+0.0000𝑖
        |111110⟩: 0.2500+0.0000𝑖
        |111111⟩: −0.2500+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1], 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mch_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled H(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.1768+0.0000𝑖
        |11101111⟩: 0.1768+0.0000𝑖
        |11111110⟩: 0.1768+0.0000𝑖
        |11111111⟩: −0.1768+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mch_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled H(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.1768+0.0000𝑖
        |11101111⟩: 0.1768+0.0000𝑖
        |11111110⟩: 0.1768+0.0000𝑖
        |11111111⟩: −0.1768+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mch_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled H(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1250+0.0000𝑖
        |1111011111⟩: 0.1250+0.0000𝑖
        |1111111110⟩: 0.1250+0.0000𝑖
        |1111111111⟩: −0.1250+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mch_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled H(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1250+0.0000𝑖
        |1111011111⟩: 0.1250+0.0000𝑖
        |1111111110⟩: 0.1250+0.0000𝑖
        |1111111111⟩: −0.1250+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcrz_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Rz(qs[0..0], (Std.Math.PI() / 7.0, qs[1]));
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.4875−0.1113𝑖
        |1111⟩: 0.4875+0.1113𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcrz(&[0], std::f64::consts::PI / -7.0, 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcrz_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Rz(qs[0..1], (Std.Math.PI() / 7.0, qs[2]));
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.3447−0.0787𝑖
        |111111⟩: 0.3447+0.0787𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcrz(&[0, 1], std::f64::consts::PI / -7.0, 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcrz_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Rz(qs[0..1], (Std.Math.PI() / 7.0, qs[2]));
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.3447−0.0787𝑖
        |111111⟩: 0.3447+0.0787𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcrz(&[0, 1], std::f64::consts::PI / -7.0, 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcrz_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Rz(qs[0..2], (Std.Math.PI() / 7.0, qs[3]));
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2437−0.0556𝑖
        |11111111⟩: 0.2437+0.0556𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcrz(&[0, 1, 2], std::f64::consts::PI / -7.0, 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcrz_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Rz(qs[0..2], (Std.Math.PI() / 7.0, qs[3]));
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2437−0.0556𝑖
        |11111111⟩: 0.2437+0.0556𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcrz(&[0, 1, 2], std::f64::consts::PI / -7.0, 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcrx_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Rx(qs[0..0], (Std.Math.PI() / 7.0, qs[1]));
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.4875+0.0000𝑖
        |1011⟩: 0.0000−0.1113𝑖
        |1110⟩: 0.0000−0.1113𝑖
        |1111⟩: 0.4875+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcrx(&[0], std::f64::consts::PI / -7.0, 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcry_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Ry(qs[0..0], (Std.Math.PI() / 7.0, qs[1]));
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.4875+0.0000𝑖
        |1011⟩: −0.1113+0.0000𝑖
        |1110⟩: 0.1113+0.0000𝑖
        |1111⟩: 0.4875+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcry(&[0], std::f64::consts::PI / -7.0, 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcs_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled S(qs[0..0], qs[1]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.5000+0.0000𝑖
        |1111⟩: 0.0000+0.5000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcsadj(&[0], 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcs_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled S(qs[0..1], qs[2]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.3536+0.0000𝑖
        |111111⟩: 0.0000+0.3536𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcsadj(&[0, 1], 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcs_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled S(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.0000+0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcsadj(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcs_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled S(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.0000+0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcsadj(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcs_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled S(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.0000+0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcsadj(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcs_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled S(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.0000+0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcsadj(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcsadj_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint S(qs[0..0], qs[1]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.5000+0.0000𝑖
        |1111⟩: 0.0000−0.5000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcs(&[0], 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcsadj_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint S(qs[0..1], qs[2]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.3536+0.0000𝑖
        |111111⟩: 0.0000−0.3536𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcs(&[0, 1], 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcsadj_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint S(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.0000−0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcs(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcsadj_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint S(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.0000−0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcs(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcsadj_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint S(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.0000−0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcs(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcsadj_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint S(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.0000−0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcs(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcsx_0_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(1);
            let aux = QIR.Runtime.AllocateQubitArray(1);
            H(aux[0]);
            CNOT(aux[0], qs[0]);
            Controlled SX([], qs[0]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00⟩: 0.3536+0.3536𝑖
        |01⟩: 0.3536−0.3536𝑖
        |10⟩: 0.3536−0.3536𝑖
        |11⟩: 0.3536+0.3536𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.h(0);
    sim.sim.sadj(0);
    sim.sim.h(0);
    sim.sim.mcx(&[1], 0);
    sim.sim.h(1);
    assert!(sim.sim.qubit_is_zero(1), "qubit 1 is not zero");
    assert!(sim.sim.qubit_is_zero(0), "qubit 0 is not zero");
}

#[test]
fn test_mcsx_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled SX(qs[0..0], qs[1]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.2500+0.2500𝑖
        |1011⟩: 0.2500−0.2500𝑖
        |1110⟩: 0.2500−0.2500𝑖
        |1111⟩: 0.2500+0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0], 1);
    sim.sim.mcsadj(&[0], 1);
    sim.sim.mch(&[0], 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcsx_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled SX(qs[0..1], qs[2]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.1768+0.1768𝑖
        |110111⟩: 0.1768−0.1768𝑖
        |111110⟩: 0.1768−0.1768𝑖
        |111111⟩: 0.1768+0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1], 2);
    sim.sim.mcsadj(&[0, 1], 2);
    sim.sim.mch(&[0, 1], 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcsx_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled SX(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.1250+0.1250𝑖
        |11101111⟩: 0.1250−0.1250𝑖
        |11111110⟩: 0.1250−0.1250𝑖
        |11111111⟩: 0.1250+0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2], 3);
    sim.sim.mcsadj(&[0, 1, 2], 3);
    sim.sim.mch(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcsx_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled SX(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.1250+0.1250𝑖
        |11101111⟩: 0.1250−0.1250𝑖
        |11111110⟩: 0.1250−0.1250𝑖
        |11111111⟩: 0.1250+0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2], 3);
    sim.sim.mcsadj(&[0, 1, 2], 3);
    sim.sim.mch(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcsx_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled SX(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.0884+0.0884𝑖
        |1111011111⟩: 0.0884−0.0884𝑖
        |1111111110⟩: 0.0884−0.0884𝑖
        |1111111111⟩: 0.0884+0.0884𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2, 3], 4);
    sim.sim.mcsadj(&[0, 1, 2, 3], 4);
    sim.sim.mch(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcsx_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled SX(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.0884+0.0884𝑖
        |1111011111⟩: 0.0884−0.0884𝑖
        |1111111110⟩: 0.0884−0.0884𝑖
        |1111111111⟩: 0.0884+0.0884𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2, 3], 4);
    sim.sim.mcsadj(&[0, 1, 2, 3], 4);
    sim.sim.mch(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcsxadj_0_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(1);
            let aux = QIR.Runtime.AllocateQubitArray(1);
            H(aux[0]);
            CNOT(aux[0], qs[0]);
            Adjoint Controlled SX([], qs[0]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00⟩: 0.3536−0.3536𝑖
        |01⟩: 0.3536+0.3536𝑖
        |10⟩: 0.3536+0.3536𝑖
        |11⟩: 0.3536−0.3536𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.h(0);
    sim.sim.s(0);
    sim.sim.h(0);
    sim.sim.mcx(&[1], 0);
    sim.sim.h(1);
    assert!(sim.sim.qubit_is_zero(1), "qubit 1 is not zero");
    assert!(sim.sim.qubit_is_zero(0), "qubit 0 is not zero");
}

#[test]
fn test_mcsxadj_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint SX(qs[0..0], qs[1]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.2500−0.2500𝑖
        |1011⟩: 0.2500+0.2500𝑖
        |1110⟩: 0.2500+0.2500𝑖
        |1111⟩: 0.2500−0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0], 1);
    sim.sim.mcs(&[0], 1);
    sim.sim.mch(&[0], 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mcsxadj_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint SX(qs[0..1], qs[2]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.1768−0.1768𝑖
        |110111⟩: 0.1768+0.1768𝑖
        |111110⟩: 0.1768+0.1768𝑖
        |111111⟩: 0.1768−0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1], 2);
    sim.sim.mcs(&[0, 1], 2);
    sim.sim.mch(&[0, 1], 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcsxadj_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint SX(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.1250−0.1250𝑖
        |11101111⟩: 0.1250+0.1250𝑖
        |11111110⟩: 0.1250+0.1250𝑖
        |11111111⟩: 0.1250−0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2], 3);
    sim.sim.mcs(&[0, 1, 2], 3);
    sim.sim.mch(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcsxadj_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint SX(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.1250−0.1250𝑖
        |11101111⟩: 0.1250+0.1250𝑖
        |11111110⟩: 0.1250+0.1250𝑖
        |11111111⟩: 0.1250−0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2], 3);
    sim.sim.mcs(&[0, 1, 2], 3);
    sim.sim.mch(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcsxadj_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint SX(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.0884−0.0884𝑖
        |1111011111⟩: 0.0884+0.0884𝑖
        |1111111110⟩: 0.0884+0.0884𝑖
        |1111111111⟩: 0.0884−0.0884𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2, 3], 4);
    sim.sim.mcs(&[0, 1, 2, 3], 4);
    sim.sim.mch(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcsxadj_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint SX(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.0884−0.0884𝑖
        |1111011111⟩: 0.0884+0.0884𝑖
        |1111111110⟩: 0.0884+0.0884𝑖
        |1111111111⟩: 0.0884−0.0884𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mch(&[0, 1, 2, 3], 4);
    sim.sim.mcs(&[0, 1, 2, 3], 4);
    sim.sim.mch(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mct_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled T(qs[0..0], qs[1]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.5000+0.0000𝑖
        |1111⟩: 0.3536+0.3536𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mctadj(&[0], 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mct_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled T(qs[0..1], qs[2]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.3536+0.0000𝑖
        |111111⟩: 0.2500+0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mctadj(&[0, 1], 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mct_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled T(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.1768+0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mctadj(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mct_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled T(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.1768+0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mctadj(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mct_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled T(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.1250+0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mctadj(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mct_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled T(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.1250+0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mctadj(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mctadj_1_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(2);
            let aux = QIR.Runtime.AllocateQubitArray(2);
            for i in 0..1 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint T(qs[0..0], qs[1]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000⟩: 0.5000+0.0000𝑖
        |0101⟩: 0.5000+0.0000𝑖
        |1010⟩: 0.5000+0.0000𝑖
        |1111⟩: 0.3536−0.3536𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mct(&[0], 1);
    for i in 0..2 {
        sim.sim.mcx(&[i + 2], i);
        sim.sim.h(i + 2);
        assert!(sim.sim.qubit_is_zero(i + 2), "qubit {} is not zero", i + 2);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_mctadj_2_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(3);
            let aux = QIR.Runtime.AllocateQubitArray(3);
            for i in 0..2 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint T(qs[0..1], qs[2]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |000000⟩: 0.3536+0.0000𝑖
        |001001⟩: 0.3536+0.0000𝑖
        |010010⟩: 0.3536+0.0000𝑖
        |011011⟩: 0.3536+0.0000𝑖
        |100100⟩: 0.3536+0.0000𝑖
        |101101⟩: 0.3536+0.0000𝑖
        |110110⟩: 0.3536+0.0000𝑖
        |111111⟩: 0.2500−0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mct(&[0, 1], 2);
    for i in 0..3 {
        sim.sim.mcx(&[i + 3], i);
        sim.sim.h(i + 3);
        assert!(sim.sim.qubit_is_zero(i + 3), "qubit {} is not zero", i + 3);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mctadj_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint T(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.1768−0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mct(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mctadj_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint T(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: 0.1768−0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mct(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mctadj_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint T(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.1250−0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mct(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mctadj_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Adjoint T(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: 0.1250−0.1250𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mct(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcx_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled X(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101111⟩: 0.2500+0.0000𝑖
        |11111110⟩: 0.2500+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcx(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcx_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled X(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101111⟩: 0.2500+0.0000𝑖
        |11111110⟩: 0.2500+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcx(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcx_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled X(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011111⟩: 0.1768+0.0000𝑖
        |1111111110⟩: 0.1768+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcx(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcx_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled X(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011111⟩: 0.1768+0.0000𝑖
        |1111111110⟩: 0.1768+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcx(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcy_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Y(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101111⟩: 0.0000−0.2500𝑖
        |11111110⟩: 0.0000+0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcy(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcy_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Y(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101111⟩: 0.0000−0.2500𝑖
        |11111110⟩: 0.0000+0.2500𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcy(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcy_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Y(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011111⟩: 0.0000−0.1768𝑖
        |1111111110⟩: 0.0000+0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcy(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcy_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Y(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011111⟩: 0.0000−0.1768𝑖
        |1111111110⟩: 0.0000+0.1768𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcy(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcz_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Z(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: −0.2500+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcz(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcz_3_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(4);
            let aux = QIR.Runtime.AllocateQubitArray(4);
            for i in 0..3 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Z(qs[0..2], qs[3]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |00000000⟩: 0.2500+0.0000𝑖
        |00010001⟩: 0.2500+0.0000𝑖
        |00100010⟩: 0.2500+0.0000𝑖
        |00110011⟩: 0.2500+0.0000𝑖
        |01000100⟩: 0.2500+0.0000𝑖
        |01010101⟩: 0.2500+0.0000𝑖
        |01100110⟩: 0.2500+0.0000𝑖
        |01110111⟩: 0.2500+0.0000𝑖
        |10001000⟩: 0.2500+0.0000𝑖
        |10011001⟩: 0.2500+0.0000𝑖
        |10101010⟩: 0.2500+0.0000𝑖
        |10111011⟩: 0.2500+0.0000𝑖
        |11001100⟩: 0.2500+0.0000𝑖
        |11011101⟩: 0.2500+0.0000𝑖
        |11101110⟩: 0.2500+0.0000𝑖
        |11111111⟩: −0.2500+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcz(&[0, 1, 2], 3);
    for i in 0..4 {
        sim.sim.mcx(&[i + 4], i);
        sim.sim.h(i + 4);
        assert!(sim.sim.qubit_is_zero(i + 4), "qubit {} is not zero", i + 4);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_unrestricted_mcz_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Z(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
        }"},
        "",
        Profile::Unrestricted,
        &mut sim,
        &Value::unit(),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: −0.1768+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcz(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn test_base_mcz_4_control() {
    let mut sim = SparseSim::default();
    let dump = test_expression_with_lib_and_profile_and_sim(
        indoc! {"{
            let qs = QIR.Runtime.AllocateQubitArray(5);
            let aux = QIR.Runtime.AllocateQubitArray(5);
            for i in 0..4 {
                H(aux[i]);
                CNOT(aux[i], qs[i]);
            }
            Controlled Z(qs[0..3], qs[4]);
            Std.Diagnostics.DumpMachine();
            let result : Result[] = [];
            result
        }"},
        "",
        Profile::Base,
        &mut sim,
        &Value::Array(Vec::new().into()),
    );
    expect![[r#"
        STATE:
        |0000000000⟩: 0.1768+0.0000𝑖
        |0000100001⟩: 0.1768+0.0000𝑖
        |0001000010⟩: 0.1768+0.0000𝑖
        |0001100011⟩: 0.1768+0.0000𝑖
        |0010000100⟩: 0.1768+0.0000𝑖
        |0010100101⟩: 0.1768+0.0000𝑖
        |0011000110⟩: 0.1768+0.0000𝑖
        |0011100111⟩: 0.1768+0.0000𝑖
        |0100001000⟩: 0.1768+0.0000𝑖
        |0100101001⟩: 0.1768+0.0000𝑖
        |0101001010⟩: 0.1768+0.0000𝑖
        |0101101011⟩: 0.1768+0.0000𝑖
        |0110001100⟩: 0.1768+0.0000𝑖
        |0110101101⟩: 0.1768+0.0000𝑖
        |0111001110⟩: 0.1768+0.0000𝑖
        |0111101111⟩: 0.1768+0.0000𝑖
        |1000010000⟩: 0.1768+0.0000𝑖
        |1000110001⟩: 0.1768+0.0000𝑖
        |1001010010⟩: 0.1768+0.0000𝑖
        |1001110011⟩: 0.1768+0.0000𝑖
        |1010010100⟩: 0.1768+0.0000𝑖
        |1010110101⟩: 0.1768+0.0000𝑖
        |1011010110⟩: 0.1768+0.0000𝑖
        |1011110111⟩: 0.1768+0.0000𝑖
        |1100011000⟩: 0.1768+0.0000𝑖
        |1100111001⟩: 0.1768+0.0000𝑖
        |1101011010⟩: 0.1768+0.0000𝑖
        |1101111011⟩: 0.1768+0.0000𝑖
        |1110011100⟩: 0.1768+0.0000𝑖
        |1110111101⟩: 0.1768+0.0000𝑖
        |1111011110⟩: 0.1768+0.0000𝑖
        |1111111111⟩: −0.1768+0.0000𝑖
    "#]]
    .assert_eq(&dump);

    sim.sim.mcz(&[0, 1, 2, 3], 4);
    for i in 0..5 {
        sim.sim.mcx(&[i + 5], i);
        sim.sim.h(i + 5);
        assert!(sim.sim.qubit_is_zero(i + 5), "qubit {} is not zero", i + 5);
        assert!(sim.sim.qubit_is_zero(i), "qubit {i} is not zero");
    }
}

#[test]
fn global_phase_correct_for_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use q = Qubit();
            H(q);
            R1(PI() / 2.0, q);
            Adjoint S(q);
            H(q);
            DumpMachine();
            Reset(q);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |0⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn global_phase_correct_for_adjoint_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use q = Qubit();
            H(q);
            Adjoint R1(PI() / 2.0, q);
            S(q);
            H(q);
            DumpMachine();
            Reset(q);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |0⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn global_phase_correct_for_singly_controlled_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use ctls = Qubit[1];
            use q = Qubit();
            for c in ctls {
                H(c);
            }
            H(q);
            Controlled R1(ctls, (PI() / 2.0, q));
            Controlled Adjoint S(ctls, q);
            H(q);
            for c in ctls {
                H(c);
            }
            DumpMachine();
            Reset(q);
            ResetAll(ctls);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |00⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn global_phase_correct_for_singly_controlled_adjoint_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use ctls = Qubit[1];
            use q = Qubit();
            for c in ctls {
                H(c);
            }
            H(q);
            Adjoint Controlled R1(ctls, (PI() / 2.0, q));
            Controlled S(ctls, q);
            H(q);
            for c in ctls {
                H(c);
            }
            DumpMachine();
            Reset(q);
            ResetAll(ctls);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |00⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn global_phase_correct_for_doubly_controlled_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use ctls = Qubit[2];
            use q = Qubit();
            for c in ctls {
                H(c);
            }
            H(q);
            Controlled R1(ctls, (PI() / 2.0, q));
            Controlled Adjoint S(ctls, q);
            H(q);
            for c in ctls {
                H(c);
            }
            DumpMachine();
            Reset(q);
            ResetAll(ctls);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |000⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn global_phase_correct_for_doubly_controlled_adjoint_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use ctls = Qubit[2];
            use q = Qubit();
            for c in ctls {
                H(c);
            }
            H(q);
            Adjoint Controlled R1(ctls, (PI() / 2.0, q));
            Controlled S(ctls, q);
            H(q);
            for c in ctls {
                H(c);
            }
            DumpMachine();
            Reset(q);
            ResetAll(ctls);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |000⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn global_phase_correct_for_triply_controlled_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use ctls = Qubit[3];
            use q = Qubit();
            for c in ctls {
                H(c);
            }
            H(q);
            Controlled R1(ctls, (PI() / 2.0, q));
            Controlled Adjoint S(ctls, q);
            H(q);
            for c in ctls {
                H(c);
            }
            DumpMachine();
            Reset(q);
            ResetAll(ctls);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |0000⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn global_phase_correct_for_triply_controlled_adjoint_r1() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use ctls = Qubit[3];
            use q = Qubit();
            for c in ctls {
                H(c);
            }
            H(q);
            Adjoint Controlled R1(ctls, (PI() / 2.0, q));
            Controlled S(ctls, q);
            H(q);
            for c in ctls {
                H(c);
            }
            DumpMachine();
            Reset(q);
            ResetAll(ctls);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |0000⟩: 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_exp() {
    let dump = test_expression(
        indoc! {r#"
        {
            open Std.Math;
            open Std.Diagnostics;
            for p in [PauliX, PauliY, PauliZ, PauliI] {
                for i in 1 .. 4 {
                    Message($"Exp with {p} on {i} qubits:");
                    use qs = Qubit[i];
                    for q in qs {
                        H(q);
                    }
                    Exp(Repeated(p, i), PI() / 7.0, qs);
                    DumpMachine();
                    ResetAll(qs);
                }
            }
        }
        "#},
        &Value::unit(),
    );

    expect![[r#"
        Exp with PauliX on 1 qubits:
        STATE:
        |0⟩: 0.6371+0.3068𝑖
        |1⟩: 0.6371+0.3068𝑖
        Exp with PauliX on 2 qubits:
        STATE:
        |00⟩: 0.4505+0.2169𝑖
        |01⟩: 0.4505+0.2169𝑖
        |10⟩: 0.4505+0.2169𝑖
        |11⟩: 0.4505+0.2169𝑖
        Exp with PauliX on 3 qubits:
        STATE:
        |000⟩: 0.3185+0.1534𝑖
        |001⟩: 0.3185+0.1534𝑖
        |010⟩: 0.3185+0.1534𝑖
        |011⟩: 0.3185+0.1534𝑖
        |100⟩: 0.3185+0.1534𝑖
        |101⟩: 0.3185+0.1534𝑖
        |110⟩: 0.3185+0.1534𝑖
        |111⟩: 0.3185+0.1534𝑖
        Exp with PauliX on 4 qubits:
        STATE:
        |0000⟩: 0.2252+0.1085𝑖
        |0001⟩: 0.2252+0.1085𝑖
        |0010⟩: 0.2252+0.1085𝑖
        |0011⟩: 0.2252+0.1085𝑖
        |0100⟩: 0.2252+0.1085𝑖
        |0101⟩: 0.2252+0.1085𝑖
        |0110⟩: 0.2252+0.1085𝑖
        |0111⟩: 0.2252+0.1085𝑖
        |1000⟩: 0.2252+0.1085𝑖
        |1001⟩: 0.2252+0.1085𝑖
        |1010⟩: 0.2252+0.1085𝑖
        |1011⟩: 0.2252+0.1085𝑖
        |1100⟩: 0.2252+0.1085𝑖
        |1101⟩: 0.2252+0.1085𝑖
        |1110⟩: 0.2252+0.1085𝑖
        |1111⟩: 0.2252+0.1085𝑖
        Exp with PauliY on 1 qubits:
        STATE:
        |0⟩: 0.9439+0.0000𝑖
        |1⟩: 0.3303+0.0000𝑖
        Exp with PauliY on 2 qubits:
        STATE:
        |00⟩: 0.4505−0.2169𝑖
        |01⟩: 0.4505+0.2169𝑖
        |10⟩: 0.4505+0.2169𝑖
        |11⟩: 0.4505−0.2169𝑖
        Exp with PauliY on 3 qubits:
        STATE:
        |000⟩: 0.1651+0.0000𝑖
        |001⟩: 0.4719+0.0000𝑖
        |010⟩: 0.4719+0.0000𝑖
        |011⟩: 0.1651+0.0000𝑖
        |100⟩: 0.4719+0.0000𝑖
        |101⟩: 0.1651+0.0000𝑖
        |110⟩: 0.1651+0.0000𝑖
        |111⟩: 0.4719+0.0000𝑖
        Exp with PauliY on 4 qubits:
        STATE:
        |0000⟩: 0.2252+0.1085𝑖
        |0001⟩: 0.2252−0.1085𝑖
        |0010⟩: 0.2252−0.1085𝑖
        |0011⟩: 0.2252+0.1085𝑖
        |0100⟩: 0.2252−0.1085𝑖
        |0101⟩: 0.2252+0.1085𝑖
        |0110⟩: 0.2252+0.1085𝑖
        |0111⟩: 0.2252−0.1085𝑖
        |1000⟩: 0.2252−0.1085𝑖
        |1001⟩: 0.2252+0.1085𝑖
        |1010⟩: 0.2252+0.1085𝑖
        |1011⟩: 0.2252−0.1085𝑖
        |1100⟩: 0.2252+0.1085𝑖
        |1101⟩: 0.2252−0.1085𝑖
        |1110⟩: 0.2252−0.1085𝑖
        |1111⟩: 0.2252+0.1085𝑖
        Exp with PauliZ on 1 qubits:
        STATE:
        |0⟩: 0.6371+0.3068𝑖
        |1⟩: 0.6371−0.3068𝑖
        Exp with PauliZ on 2 qubits:
        STATE:
        |00⟩: 0.4505+0.2169𝑖
        |01⟩: 0.4505−0.2169𝑖
        |10⟩: 0.4505−0.2169𝑖
        |11⟩: 0.4505+0.2169𝑖
        Exp with PauliZ on 3 qubits:
        STATE:
        |000⟩: 0.3185+0.1534𝑖
        |001⟩: 0.3185−0.1534𝑖
        |010⟩: 0.3185−0.1534𝑖
        |011⟩: 0.3185+0.1534𝑖
        |100⟩: 0.3185−0.1534𝑖
        |101⟩: 0.3185+0.1534𝑖
        |110⟩: 0.3185+0.1534𝑖
        |111⟩: 0.3185−0.1534𝑖
        Exp with PauliZ on 4 qubits:
        STATE:
        |0000⟩: 0.2252+0.1085𝑖
        |0001⟩: 0.2252−0.1085𝑖
        |0010⟩: 0.2252−0.1085𝑖
        |0011⟩: 0.2252+0.1085𝑖
        |0100⟩: 0.2252−0.1085𝑖
        |0101⟩: 0.2252+0.1085𝑖
        |0110⟩: 0.2252+0.1085𝑖
        |0111⟩: 0.2252−0.1085𝑖
        |1000⟩: 0.2252−0.1085𝑖
        |1001⟩: 0.2252+0.1085𝑖
        |1010⟩: 0.2252+0.1085𝑖
        |1011⟩: 0.2252−0.1085𝑖
        |1100⟩: 0.2252+0.1085𝑖
        |1101⟩: 0.2252−0.1085𝑖
        |1110⟩: 0.2252−0.1085𝑖
        |1111⟩: 0.2252+0.1085𝑖
        Exp with PauliI on 1 qubits:
        STATE:
        |0⟩: 0.6371+0.3068𝑖
        |1⟩: 0.6371+0.3068𝑖
        Exp with PauliI on 2 qubits:
        STATE:
        |00⟩: 0.4505+0.2169𝑖
        |01⟩: 0.4505+0.2169𝑖
        |10⟩: 0.4505+0.2169𝑖
        |11⟩: 0.4505+0.2169𝑖
        Exp with PauliI on 3 qubits:
        STATE:
        |000⟩: 0.3185+0.1534𝑖
        |001⟩: 0.3185+0.1534𝑖
        |010⟩: 0.3185+0.1534𝑖
        |011⟩: 0.3185+0.1534𝑖
        |100⟩: 0.3185+0.1534𝑖
        |101⟩: 0.3185+0.1534𝑖
        |110⟩: 0.3185+0.1534𝑖
        |111⟩: 0.3185+0.1534𝑖
        Exp with PauliI on 4 qubits:
        STATE:
        |0000⟩: 0.2252+0.1085𝑖
        |0001⟩: 0.2252+0.1085𝑖
        |0010⟩: 0.2252+0.1085𝑖
        |0011⟩: 0.2252+0.1085𝑖
        |0100⟩: 0.2252+0.1085𝑖
        |0101⟩: 0.2252+0.1085𝑖
        |0110⟩: 0.2252+0.1085𝑖
        |0111⟩: 0.2252+0.1085𝑖
        |1000⟩: 0.2252+0.1085𝑖
        |1001⟩: 0.2252+0.1085𝑖
        |1010⟩: 0.2252+0.1085𝑖
        |1011⟩: 0.2252+0.1085𝑖
        |1100⟩: 0.2252+0.1085𝑖
        |1101⟩: 0.2252+0.1085𝑖
        |1110⟩: 0.2252+0.1085𝑖
        |1111⟩: 0.2252+0.1085𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_exp_mixed_paulis() {
    let dump = test_expression(
        indoc! {r#"
        {
            open Std.Math;
            open Std.Diagnostics;
            use qs = Qubit[3];
            for q in qs {
                H(q);
            }
            Exp([PauliX, PauliI, PauliY], PI() / 7.0, qs);
            DumpMachine();
            ResetAll(qs);
        }
        "#},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |000⟩: 0.4719+0.0000𝑖
        |001⟩: 0.1651+0.0000𝑖
        |010⟩: 0.4719+0.0000𝑖
        |011⟩: 0.1651+0.0000𝑖
        |100⟩: 0.4719+0.0000𝑖
        |101⟩: 0.1651+0.0000𝑖
        |110⟩: 0.4719+0.0000𝑖
        |111⟩: 0.1651+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_rxx() {
    let dump = test_expression(
        indoc! {"
        Std.Diagnostics.DumpOperation(
            2,
            qs => Rxx(Std.Math.PI() / 3.0, qs[0], qs[1])
        )
        "},
        &Value::unit(),
    );

    expect![[r#"
        MATRIX:
        0.8660+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000−0.5000𝑖
        0.0000+0.0000𝑖 0.8660+0.0000𝑖 0.0000−0.5000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000−0.5000𝑖 0.8660+0.0000𝑖 0.0000+0.0000𝑖
        0.0000−0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_rxx_one_control() {
    let dump = test_expression(
        indoc! {"
        Std.Diagnostics.DumpOperation(
            3,
            qs => Controlled Rxx([qs[0]], (Std.Math.PI() / 3.0, qs[1], qs[2]))
        )
        "},
        &Value::unit(),
    );

    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000−0.5000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖 0.0000−0.5000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000−0.5000𝑖 0.8660+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000−0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_ryy() {
    let dump = test_expression(
        indoc! {"
        Std.Diagnostics.DumpOperation(
            2,
            qs => Ryy(Std.Math.PI() / 3.0, qs[0], qs[1])
        )
        "},
        &Value::unit(),
    );

    expect![[r#"
        MATRIX:
        0.8660+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.5000𝑖
        0.0000+0.0000𝑖 0.8660+0.0000𝑖 0.0000−0.5000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000−0.5000𝑖 0.8660+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_ryy_one_control() {
    let dump = test_expression(
        indoc! {"
        Std.Diagnostics.DumpOperation(
            3,
            qs => Controlled Ryy([qs[0]], (Std.Math.PI() / 3.0, qs[1], qs[2]))
        )
        "},
        &Value::unit(),
    );

    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.5000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖 0.0000−0.5000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000−0.5000𝑖 0.8660+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_rzz() {
    let dump = test_expression(
        indoc! {"
        Std.Diagnostics.DumpOperation(
            2,
            qs => Rzz(Std.Math.PI() / 3.0, qs[0], qs[1])
        )
        "},
        &Value::unit(),
    );

    expect![[r#"
        MATRIX:
        0.8660−0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.8660+0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.5000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660−0.5000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_rzz_one_control() {
    let dump = test_expression(
        indoc! {"
        Std.Diagnostics.DumpOperation(
            3,
            qs => Controlled Rzz([qs[0]], (Std.Math.PI() / 3.0, qs[1], qs[2]))
        )
        "},
        &Value::unit(),
    );

    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660−0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.5000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660+0.5000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.8660−0.5000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_apply_unitary_with_h_matrix() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use q = Qubit();
            let one_sqrt_2 = new Complex { Real = 1.0 / Sqrt(2.0), Imag = 0.0 };
            ApplyUnitary(
                [
                    [one_sqrt_2, one_sqrt_2],
                    [one_sqrt_2, NegationC(one_sqrt_2)]
                ],
                [q]
            );
            DumpMachine();
            Reset(q);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |0⟩: 0.7071+0.0000𝑖
        |1⟩: 0.7071+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_apply_unitary_with_swap_matrix() {
    let dump = test_expression(
        indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use qs = Qubit[2];
            H(qs[0]);
            DumpMachine();
            let one = new Complex { Real = 1.0, Imag = 0.0 };
            let zero = new Complex { Real = 0.0, Imag = 0.0 };
            ApplyUnitary(
                [
                    [one, zero, zero, zero],
                    [zero, zero, one, zero],
                    [zero, one, zero, zero],
                    [zero, zero, zero, one]
                ],
                qs
            );
            DumpMachine();
            ResetAll(qs);
        }
        "},
        &Value::unit(),
    );

    expect![[r#"
        STATE:
        |00⟩: 0.7071+0.0000𝑖
        |10⟩: 0.7071+0.0000𝑖
        STATE:
        |00⟩: 0.7071+0.0000𝑖
        |01⟩: 0.7071+0.0000𝑖
    "#]]
    .assert_eq(&dump);
}

#[test]
fn test_apply_unitary_fails_when_matrix_not_square() {
    let err = test_expression_fails(indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use q = Qubit();
            ApplyUnitary(
                [
                    [new Complex { Real = 1.0, Imag = 0.0 }],
                    [new Complex { Real = 0.0, Imag = 0.0 }]
                ],
                [q]
            );
            DumpMachine();
            Reset(q);
        }
        "});

    expect!["program failed: matrix passed to ApplyUnitary must be square."].assert_eq(&err);
}

#[test]
fn test_apply_unitary_fails_when_matrix_wrong_size() {
    let err = test_expression_fails(indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use qs = Qubit[2];
            let one_sqrt_2 = new Complex { Real = 1.0 / Sqrt(2.0), Imag = 0.0 };
            ApplyUnitary(
                [
                    [one_sqrt_2, one_sqrt_2],
                    [one_sqrt_2, NegationC(one_sqrt_2)]
                ],
                qs
            );
            DumpMachine();
            ResetAll(qs);
        }
        "});

    expect!["program failed: matrix passed to ApplyUnitary must have dimensions 2^Length(qubits)."]
        .assert_eq(&err);
}

#[test]
fn test_apply_unitary_fails_when_matrix_not_unitary() {
    let err = test_expression_fails(indoc! {"
        {
            open Std.Math;
            open Std.Diagnostics;
            use q = Qubit();
            let zero = new Complex { Real = 0.0, Imag = 0.0 };
            ApplyUnitary(
                [
                    [zero, zero],
                    [zero, zero]
                ],
                [q]
            );
            DumpMachine();
            Reset(q);
        }
        "});

    expect!["intrinsic callable `Apply` failed: matrix is not unitary"].assert_eq(&err);
}
