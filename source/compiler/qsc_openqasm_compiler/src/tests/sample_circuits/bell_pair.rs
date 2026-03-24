// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
    tests::{compile_with_config, gen_qsharp, print_compilation_errors},
};

const SOURCE: &str = r#"
OPENQASM 3.0;
include "stdgates.inc";
bit[2] c;
qubit[2] q;
h q[0];
cx q[0], q[1];
barrier q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
"#;

#[test]
fn it_compiles() {
    let source = SOURCE;

    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config).expect("parse failed");

    print_compilation_errors(&unit);
    assert!(!unit.has_errors());
    let qsharp = gen_qsharp(&unit.package);
    println!("{qsharp}");
}
