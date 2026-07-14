// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::{io::InMemorySourceResolver, semantic::parse_source};
use expect_test::expect;

use super::check_all;

#[test]
fn entry_versions_are_normalized_and_supported_deterministically() {
    for (source, expected, has_errors) in [
        ("", None, false),
        ("OPENQASM 2;", Some((2, Some(0))), true),
        ("OPENQASM 2.0;", Some((2, Some(0))), false),
        ("OPENQASM 3;", Some((3, Some(0))), false),
        ("OPENQASM 3.0;", Some((3, Some(0))), false),
        ("OPENQASM 3.1;", Some((3, Some(1))), false),
    ] {
        let source: Arc<str> = source.into();
        let mut resolver = InMemorySourceResolver::from_iter([("test".into(), source.clone())]);
        let result = parse_source(source, "test", &mut resolver);
        let actual = result
            .program
            .version
            .map(|version| (version.major, version.minor));

        assert_eq!(actual, expected);
        assert_eq!(
            result.has_errors(),
            has_errors,
            "errors: {:?}",
            result.all_errors()
        );
    }
}

#[test]
fn qasm2_version_selects_qasm2_include_policy() {
    let source: Arc<str> = "OPENQASM 2.0; include \"qelib1.inc\";".into();
    let mut resolver = InMemorySourceResolver::from_iter([("test".into(), source.clone())]);
    let result = parse_source(source, "test", &mut resolver);

    assert!(!result.has_errors(), "errors: {:?}", result.all_errors());
}

#[test]
fn included_version_declarations_are_always_rejected() {
    check_all(
        "main.qasm",
        [
            (
                "main.qasm".into(),
                "OPENQASM 3; include \"matching.inc\";".into(),
            ),
            (
                "matching.inc".into(),
                "OPENQASM 3.0; include \"nested.inc\";".into(),
            ),
            ("nested.inc".into(), "OPENQASM 2.0; int value = 1;".into()),
        ],
        &expect![[r#"
            Program:
                version: 3.0
                pragmas: <empty>
                statements:
                    Stmt [86-100]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [86-100]:
                            symbol_id: 8
                            ty_span: [86-89]
                            ty_exprs: <empty>
                            init_expr: Expr [98-99]:
                                ty: int
                                kind: Lit: Int(1)

            [Qdk.Qasm.Lowerer.VersionInIncludedSource

              x included source must not declare OPENQASM version 3.0
               ,-[matching.inc:1:10]
             1 | OPENQASM 3.0; include "nested.inc";
               :          ^^^
               `----
            , Qdk.Qasm.Lowerer.VersionInIncludedSource

              x included source must not declare OPENQASM version 2.0
               ,-[nested.inc:1:10]
             1 | OPENQASM 2.0; int value = 1;
               :          ^^^
               `----
            ]"#]],
    );
}
