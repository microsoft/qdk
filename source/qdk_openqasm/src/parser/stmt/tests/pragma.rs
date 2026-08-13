// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::{
    parser::{ast::StmtKind, scan::ParserContext, tests::check},
    span::Span,
};

use crate::parser::stmt::parse;

#[test]
fn pragma_decl() {
    check(
        parse,
        "pragma a.b.d 23",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: Pragma [0-15]:
                    identifier: a.b.d
                    value: "23"
                    value_span: [13-15]"#]],
    );
}

#[test]
fn pragma_decl_complex_value_stops_at_newline() {
    check(
        parse,
        "pragma a.b.d 23 or \"value\" or 'other' or // comment\n 42",
        &expect![[r#"
            Stmt [0-51]:
                annotations: <empty>
                kind: Pragma [0-51]:
                    identifier: a.b.d
                    value: "23 or "value" or 'other' or // comment"
                    value_span: [13-51]"#]],
    );
}

#[test]
fn pragma_decl_ident_only() {
    check(
        parse,
        "pragma a.b.d",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: Pragma [0-12]:
                    identifier: a.b.d
                    value: <none>
                    value_span: <none>"#]],
    );
}

#[test]
fn pragma_decl_missing_ident() {
    check(
        parse,
        "pragma ",
        &expect![[r#"
            Stmt [0-7]:
                annotations: <empty>
                kind: Pragma [0-7]:
                    identifier: <none>
                    value: <none>
                    value_span: <none>

            [
                Error(
                    EmptyPragma(
                        Span {
                            lo: 7,
                            hi: 7,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn pragma_decl_incomplete_ident_is_value_only() {
    check(
        parse,
        "pragma name rest of line content",
        &expect![[r#"
            Stmt [0-32]:
                annotations: <empty>
                kind: Pragma [0-32]:
                    identifier: <none>
                    value: "name rest of line content"
                    value_span: [7-32]"#]],
    );
}

#[test]
fn pragma_decl_value_only_at_nonzero_offset() {
    check(
        parse,
        "    pragma 42",
        &expect![[r#"
            Stmt [4-13]:
                annotations: <empty>
                kind: Pragma [4-13]:
                    identifier: <none>
                    value: "42"
                    value_span: [11-13]"#]],
    );
}

#[test]
fn pragma_decl_incomplete_ident_is_value_only_at_nonzero_offset() {
    check(
        parse,
        "    pragma name rest of line content",
        &expect![[r#"
            Stmt [4-36]:
                annotations: <empty>
                kind: Pragma [4-36]:
                    identifier: <none>
                    value: "name rest of line content"
                    value_span: [11-36]"#]],
    );
}

#[test]
fn legacy_pragma_decl() {
    check(
        parse,
        "#pragma a.b 23",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: Pragma [0-14]:
                    identifier: a.b
                    value: "23"
                    value_span: [12-14]"#]],
    );
}

#[test]
fn legacy_pragma_decl_ident_only() {
    check(
        parse,
        "#pragma a.b.d",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: Pragma [0-13]:
                    identifier: a.b.d
                    value: <none>
                    value_span: <none>"#]],
    );
}

#[test]
fn legacy_pragma_ws_after_hash() {
    check(
        parse,
        "# pragma a.b.d",
        &expect![[r#"
            Stmt [2-14]:
                annotations: <empty>
                kind: Pragma [2-14]:
                    identifier: a.b.d
                    value: <none>
                    value_span: <none>

            [
                Error(
                    Lex(
                        Incomplete(
                            Ident,
                            Identifier,
                            Whitespace,
                            Span {
                                lo: 1,
                                hi: 2,
                            },
                        ),
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn legacy_pragma_decl_missing_ident() {
    check(
        parse,
        "#pragma ",
        &expect![[r#"
            Stmt [0-8]:
                annotations: <empty>
                kind: Pragma [0-8]:
                    identifier: <none>
                    value: <none>
                    value_span: <none>

            [
                Error(
                    EmptyPragma(
                        Span {
                            lo: 8,
                            hi: 8,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn spec_example_1() {
    check(
        parse,
        r#"pragma qiskit.simulator noise model "qpu1.noise""#,
        &expect![[r#"
            Stmt [0-48]:
                annotations: <empty>
                kind: Pragma [0-48]:
                    identifier: qiskit.simulator
                    value: "noise model "qpu1.noise""
                    value_span: [24-48]"#]],
    );
}

#[test]
fn spec_example_2() {
    check(
        parse,
        r#"pragma ibm.user alice account 12345678"#,
        &expect![[r#"
            Stmt [0-38]:
                annotations: <empty>
                kind: Pragma [0-38]:
                    identifier: ibm.user
                    value: "alice account 12345678"
                    value_span: [16-38]"#]],
    );
}

#[test]
fn spec_example_3() {
    check(
        parse,
        r#"pragma ibm.max_temp qpu 0.4"#,
        &expect![[r#"
            Stmt [0-27]:
                annotations: <empty>
                kind: Pragma [0-27]:
                    identifier: ibm.max_temp
                    value: "qpu 0.4"
                    value_span: [20-27]"#]],
    );
}

#[test]
fn pragma_command_is_authoritative_and_lossless() {
    let input = "#pragma qdk.box.open target/*opaque*/  ";
    let mut context = ParserContext::new(input);
    let statement = parse(&mut context).expect("pragma should parse");
    assert!(context.into_errors().is_empty());
    let StmtKind::Pragma(pragma) = statement.kind.as_ref() else {
        panic!("expected pragma statement");
    };

    assert_eq!(pragma.command.as_ref(), "qdk.box.open target/*opaque*/  ");
    assert_eq!(pragma.command_span, Span { lo: 8, hi: 39 });
    assert_eq!(pragma.command().name, Some("qdk.box.open"));
    assert_eq!(pragma.command().value, Some("target/*opaque*/  "));
}

#[test]
fn pragma_physical_line_and_payload_matrix() {
    for (input, command, command_span, statement_hi) in [
        (
            "pragma vendor.cmd payload\nbit x;",
            "vendor.cmd payload",
            Span { lo: 7, hi: 25 },
            25,
        ),
        (
            "pragma vendor.cmd payload\rbit x;",
            "vendor.cmd payload",
            Span { lo: 7, hi: 25 },
            25,
        ),
        (
            "pragma vendor.cmd payload\r\nbit x;",
            "vendor.cmd payload",
            Span { lo: 7, hi: 25 },
            25,
        ),
        (
            "pragma vendor.cmd //comment  ",
            "vendor.cmd //comment  ",
            Span { lo: 7, hi: 29 },
            29,
        ),
        (
            "pragma vendor.cmd πλ  ",
            "vendor.cmd πλ  ",
            Span { lo: 7, hi: 24 },
            24,
        ),
    ] {
        let mut context = ParserContext::new(input);
        let statement = parse(&mut context).expect("pragma should parse");
        assert!(context.into_errors().is_empty(), "source: {input:?}");
        let StmtKind::Pragma(pragma) = statement.kind.as_ref() else {
            panic!("expected pragma statement");
        };
        assert_eq!(pragma.command.as_ref(), command, "source: {input:?}");
        assert_eq!(pragma.command_span, command_span, "source: {input:?}");
        assert_eq!(statement.span.hi, statement_hi, "source: {input:?}");
    }
}
