// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    QubitSemantics,
    tests::{
        compile_qasm_stmt_to_qsharp, compile_qasm_stmt_to_qsharp_with_semantics,
        compile_qasm_to_qsharp,
    },
};
use expect_test::expect;
use miette::Report;

#[test]
fn qubit_array_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[5] my_qubit;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![
        r#"
        let my_qubit = QIR.Runtime.AllocateQubitArray(5);
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qubit_array_decl_with_qsharp_semantics() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[5] my_qubits;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp_with_semantics(source, QubitSemantics::QSharp)?;
    expect![
        r#"
        use my_qubits = Qubit[5];
    "#
    ]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn declaring_a_qubit_array_of_zero_size_fails() {
    let source = "
        qubit[0] qs;
    ";

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.ExprMustBePositiveInt

          x quantum register size must be a positive integer
           ,-[Test.qasm:2:15]
         1 | 
         2 |         qubit[0] qs;
           :               ^
         3 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}
