// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp;
use expect_test::expect;

#[test]
fn missing_version_number() {
    let source = "OPENQASM;";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Parser.Literal

          x invalid version literal
           ,-[Test.qasm:1:9]
         1 | OPENQASM;
           :         ^
           `----

        Qasm.Parser.EmptyStatement

          x Empty statements are not supported
           ,-[Test.qasm:1:9]
         1 | OPENQASM;
           :         ^
           `----
    "#]],
    );
}

#[test]
fn major() {
    let source = "OPENQASM 3;";

    check_qasm_to_qsharp(source, &expect!["import Std.OpenQASM.Intrinsic.*;"]);
}

#[test]
fn major_minor() {
    let source = "OPENQASM 3.1;";

    check_qasm_to_qsharp(source, &expect!["import Std.OpenQASM.Intrinsic.*;"]);
}

#[test]
fn non_existing_version_errors() {
    let source = "OPENQASM 1.5;";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.UnsupportedVersion

          x unsupported version: '1.5'
           ,-[Test.qasm:1:10]
         1 | OPENQASM 1.5;
           :          ^^^
           `----
    "#]],
    );
}

#[test]
fn other_expressions_error() {
    let source = "OPENQASM (2 + 3);";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Parser.Literal

              x invalid version literal
               ,-[Test.qasm:1:10]
             1 | OPENQASM (2 + 3);
               :          ^
               `----
        "#]],
    );
}
