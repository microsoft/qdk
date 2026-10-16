// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    parser,
    semantic::{self, Circuit, Item},
};

fn lower_without_spans(source: &str) -> Circuit {
    let (parsed, parser_errors) = parser::parse(source);
    assert!(parser_errors.is_empty(), "{parser_errors:#?}");

    let (mut circuit, semantic_errors) = semantic::lower(parsed);
    assert!(semantic_errors.is_empty(), "{semantic_errors:#?}");

    circuit.span = Default::default();
    for item in &mut circuit.items {
        let Item::Instruction(instruction) = item else {
            panic!("{source} should produce instructions");
        };
        instruction.span = Default::default();
    }
    circuit
}

fn assert_alias(canonical: &str, alias: &str) {
    assert_eq!(lower_without_spans(canonical), lower_without_spans(alias));
}

#[test]
fn single_qubit_gate_aliases_produce_same_circuit() {
    for (canonical, alias) in [
        ("H 2 7", "H_XZ 2 7"),
        ("S 2 7", "SQRT_Z 2 7"),
        ("S_DAG 2 7", "SQRT_Z_DAG 2 7"),
    ] {
        assert_alias(canonical, alias);
    }
}

#[test]
fn two_qubit_gate_aliases_produce_same_circuit() {
    for (canonical, alias) in [
        ("CX 2 7", "CNOT 2 7"),
        ("CX 2 7", "ZCX 2 7"),
        ("CY 2 7", "ZCY 2 7"),
        ("CZ 2 7", "ZCZ 2 7"),
        ("CZSWAP 2 7", "SWAPCZ 2 7"),
    ] {
        assert_alias(canonical, alias);
    }
}

#[test]
fn correlated_error_alias_produces_same_circuit() {
    assert_alias("CORRELATED_ERROR(0.1) X2 Y7", "E(0.1) X2 Y7");
}

#[test]
fn measurement_aliases_produce_same_circuit() {
    for (canonical, alias) in [("M 2 7", "MZ 2 7"), ("MR 2 7", "MRZ 2 7")] {
        assert_alias(canonical, alias);
    }
}

#[test]
fn reset_alias_produces_same_circuit() {
    assert_alias("R 2 7", "RZ 2 7");
}

#[test]
fn u3_alias_produces_same_circuit() {
    assert_alias("U3(0.1,0.2,0.3) 2 7", "U(0.1,0.2,0.3) 2 7");
}
