// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
    tests::{compile_all_with_config, qsharp_from_qasm_compilation},
};
use expect_test::expect;
use miette::Report;

#[test]
fn programs_with_includes_can_be_parsed() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        include "custom_intrinsics.inc";
        bit[1] c;
        qubit[1] q;
        my_gate q[0];
        c[0] = measure q[0];
    "#;
    let custom_intrinsics = r#"
        @SimulatableIntrinsic
        gate my_gate q {
            x q;
        }
    "#;
    let all_sources = [
        ("source0.qasm".into(), source.into()),
        ("custom_intrinsics.inc".into(), custom_intrinsics.into()),
    ];
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let r = compile_all_with_config("source0.qasm", all_sources, config)?;
    let qsharp = qsharp_from_qasm_compilation(r)?;
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            @EntryPoint()
            operation Test() : Result[] {
                @SimulatableIntrinsic()
                operation my_gate(q : Qubit) : Unit {
                    x(q);
                }
                mutable c = [Zero];
                let q = QIR.Runtime.AllocateQubitArray(1);
                my_gate(q[0]);
                set c[0] = Std.Intrinsic.M(q[0]);
                Std.Arrays.Reversed(c)
            }
        }"#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn programs_with_includes_with_includes_can_be_compiled() -> miette::Result<(), Vec<Report>> {
    let source0 = r#"
        include "stdgates.inc";
        include "source1.qasm";
    "#;
    let source1 = r#"include "source2.qasm";"#;
    let source2 = "bit a;";
    let all_sources = [
        ("source0.qasm".into(), source0.into()),
        ("source1.qasm".into(), source1.into()),
        ("source2.qasm".into(), source2.into()),
    ];

    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let r = compile_all_with_config("source0.qasm", all_sources, config)?;
    let qsharp = qsharp_from_qasm_compilation(r)?;
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            @EntryPoint()
            operation Test() : Unit {
                mutable a = Zero;
                ()
            }
        }"#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn including_stdgates_multiple_times_causes_symbol_redifintion_errors() {
    let source0 = r#"
        include "stdgates.inc";
        include "source1.qasm";
    "#;
    let source1 = r#"include "source2.qasm";"#;
    let source2 = r#"include "stdgates.inc";"#;
    let all_sources = [
        ("source0.qasm".into(), source0.into()),
        ("source1.qasm".into(), source1.into()),
        ("source2.qasm".into(), source2.into()),
    ];

    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );

    let Err(errors) = compile_all_with_config("main.qasm", all_sources, config) else {
        panic!("expected errors")
    };

    let errors: Vec<_> = errors.iter().map(|e| format!("{e}")).collect();
    let errors_string = errors.join("\n");
    expect!["Not Found: Could not resolve include file: main.qasm"].assert_eq(&errors_string);
}

#[test]
fn multiple_include_in_same_file_errors() {
    let main = r#"
        include "source1.inc";
        include "source1.inc";
    "#;
    let source1 = r#"
        bit[1] c;
    "#;
    let all_sources = [
        ("main.qasm".into(), main.into()),
        ("source1.inc".into(), source1.into()),
    ];
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );

    let Err(errors) = compile_all_with_config("main.qasm", all_sources, config) else {
        panic!("expected errors")
    };

    let errors: Vec<_> = errors.iter().map(|e| format!("{e}")).collect();
    let errors_string = errors.join("\n");
    expect!["source1.inc was already included in: main.qasm"].assert_eq(&errors_string);
}

#[test]
fn multiple_include_in_different_files_errors() {
    let main = r#"
        include "source1.inc";
        include "source2.inc";
    "#;
    let source1 = r#"
        include "source3.inc";
    "#;
    let source2 = r#"
        include "source3.inc";
    "#;
    let source3 = r#"
        bit[1] c;
    "#;
    let all_sources = [
        ("main.qasm".into(), main.into()),
        ("source1.inc".into(), source1.into()),
        ("source2.inc".into(), source2.into()),
        ("source3.inc".into(), source3.into()),
    ];
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );

    let Err(errors) = compile_all_with_config("main.qasm", all_sources, config) else {
        panic!("expected errors")
    };

    let errors: Vec<_> = errors.iter().map(|e| format!("{e}")).collect();
    let errors_string = errors.join("\n");
    expect!["source3.inc was already included in: source1.inc"].assert_eq(&errors_string);
}

#[test]
fn self_include_errors() {
    let main = r#"
        include "source1.inc";
    "#;
    let source1 = r#"
        include "source1.inc";
    "#;
    let all_sources = [
        ("main.qasm".into(), main.into()),
        ("source1.inc".into(), source1.into()),
    ];
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );

    let Err(errors) = compile_all_with_config("main.qasm", all_sources, config) else {
        panic!("expected errors")
    };

    let errors: Vec<_> = errors.iter().map(|e| format!("{e}")).collect();
    let errors_string = errors.join("\n");
    expect![[r#"
        Cyclic include:
          source1.inc includes source1.inc"#]]
    .assert_eq(&errors_string);
}

#[test]
fn mutual_include_errors() {
    let main = r#"
        include "source1.inc";
    "#;
    let source1 = r#"
        include "source2.inc";
    "#;
    let source2 = r#"
        include "source1.inc";
    "#;
    let all_sources = [
        ("main.qasm".into(), main.into()),
        ("source1.inc".into(), source1.into()),
        ("source2.inc".into(), source2.into()),
    ];
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );

    let Err(errors) = compile_all_with_config("main.qasm", all_sources, config) else {
        panic!("expected errors")
    };

    let errors: Vec<_> = errors.iter().map(|e| format!("{e}")).collect();
    let errors_string = errors.join("\n");
    expect![[r#"
        Cyclic include:
          source1.inc includes source2.inc
          source2.inc includes source1.inc"#]]
    .assert_eq(&errors_string);
}

#[test]
fn cyclic_include_errors() {
    let main = r#"
        include "source1.inc";
    "#;
    let source1 = r#"
        include "source2.inc";
    "#;
    let source2 = r#"
        include "source3.inc";
    "#;
    let source3 = r#"
        include "source1.inc";
    "#;
    let all_sources = [
        ("main.qasm".into(), main.into()),
        ("source1.inc".into(), source1.into()),
        ("source2.inc".into(), source2.into()),
        ("source3.inc".into(), source3.into()),
    ];
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );

    let Err(errors) = compile_all_with_config("main.qasm", all_sources, config) else {
        panic!("expected errors")
    };

    let errors: Vec<_> = errors.iter().map(|e| format!("{e}")).collect();
    let errors_string = errors.join("\n");

    expect![[r#"
        Cyclic include:
          source1.inc includes source2.inc
          source2.inc includes source3.inc
          source3.inc includes source1.inc"#]]
    .assert_eq(&errors_string);
}
