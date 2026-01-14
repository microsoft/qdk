// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod array;
mod bit;
mod bool;
mod complex;
mod def;
mod float;
mod gate;
mod integer;
mod io;
mod qubit;
mod unsigned_integer;

use crate::{
    CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
    tests::{compile_fragments, compile_with_config, fail_on_compilation_errors},
};
use miette::Report;

use super::compile_qasm_best_effort;

#[test]
fn classical() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int[10] a;
        int[10] b;
        uint[32] c = 0xFa_1F;
        uint[32] d = 0XFa_1F;
        uint[16] e = 0o12_34;
        uint[16] f = 0b1001_1001;
        uint[16] g = 0B1001_1001;
        uint h;
        qubit[6] q1;
        qubit q2;
        bit[4] b1 = "0100";
        bit[8] b2 = "1001_0100";
        bool i = true;
        bool j = false;
        const float[64] k = 5.5e3;
        const float[64] l = 5;
        float[32] m = .1e+3;
    "#;

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
fn duration_literal() -> miette::Result<(), Vec<Report>> {
    let source = "
        duration dur0;
        duration dur1 = 1000dt;
        duration dur2 = 10 ms;
        duration dur3 = 8	us;
        duration dur4 = 1s;
    ";

    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::Fragments,
        None,
        None,
    );
    let unit = compile_with_config(source, config).expect("parse failed");
    for error in &unit.errors {
        println!("{error}");
    }
    assert_eq!(unit.errors.len(), 10);
    for error in &unit.errors {
        assert!(
            [
                "duration type values are not supported",
                "timing literals are not supported",
            ]
            .contains(&error.to_string().as_str())
        );
    }

    Ok(())
}

#[test]
fn stretch() {
    let source = "
        stretch s;
    ";

    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::Fragments,
        None,
        None,
    );
    let unit = compile_with_config(source, config).expect("parse failed");
    assert!(unit.has_errors());
    for error in &unit.errors {
        println!("{error}");
    }
    assert_eq!(unit.errors.len(), 2);
    for error in &unit.errors {
        assert!(
            [
                "stretch type values are not supported",
                "timing literals are not supported",
            ]
            .contains(&error.to_string().as_str())
        );
    }
}

#[test]
fn gate_decl_with_missing_seq_item_doesnt_panic() {
    let source = r#"gate g1 x,,y {}"#;
    compile_qasm_best_effort(source);
}
