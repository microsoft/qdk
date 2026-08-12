// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    lex::TokenKind,
    parser::{
        ast::StmtKind, scan::ParserContext, stmt::parse, stmt::parse_annotation, tests::check,
    },
};
use expect_test::expect;

#[test]
fn annotation() {
    check(
        parse_annotation,
        "@a.b.d 23",
        &expect![[r#"
            Annotation [0-9]:
                identifier: a.b.d
                value: "23"
                value_span: [7-9]"#]],
    );
}

#[test]
fn annotation_ident_only() {
    check(
        parse_annotation,
        "@a.b.d",
        &expect![[r#"
            Annotation [0-6]:
                identifier: a.b.d
                value: <none>
                value_span: <none>"#]],
    );
}

#[test]
fn annotation_missing_ident() {
    check(
        parse_annotation,
        "@",
        &expect![[r#"
            Annotation [0-1]:
                identifier: Err
                value: <none>
                value_span: <none>

            [
                Error(
                    Rule(
                        "identifier",
                        DirectiveEnd,
                        Span {
                            lo: 1,
                            hi: 1,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn annotation_payload_and_leading_trivia_matrix() {
    for (input, expected_value) in [
        ("  @vendor.note //payload  \nbit flag;", "//payload  "),
        ("\t@vendor.note πλ  \r\nbit flag;", "πλ  "),
        (
            "// heading\n@vendor.note /*opaque*/ value\rbit flag;",
            "/*opaque*/ value",
        ),
        ("/* heading */ @vendor.note value\nbit flag;", "value"),
    ] {
        let mut context = ParserContext::new(input);
        let statement = parse(&mut context).expect("annotated statement should parse");
        assert!(context.into_errors().is_empty(), "source: {input:?}");
        assert_eq!(statement.annotations.len(), 1, "source: {input:?}");
        assert!(matches!(
            statement.kind.as_ref(),
            StmtKind::ClassicalDecl(_)
        ));
        let annotation = &statement.annotations[0];
        assert_eq!(annotation.identifier.as_string(), "vendor.note");
        assert_eq!(
            annotation.value.as_deref(),
            Some(expected_value),
            "source: {input:?}"
        );
    }
}

#[test]
fn malformed_annotation_path_is_diagnosed_within_physical_line() {
    let mut context = ParserContext::new("@vendor.\nbit flag;");
    let statement = parse(&mut context).expect("malformed annotation should recover");
    let errors = context.into_errors();

    assert_eq!(statement.annotations.len(), 1);
    assert_eq!(errors.len(), 1);
    let crate::parser::ErrorKind::Rule("identifier", TokenKind::DirectiveEnd, span) = &errors[0].0
    else {
        panic!("expected line-bounded malformed-path diagnostic");
    };
    assert_eq!(*span, crate::span::Span { lo: 8, hi: 8 });
}
