// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::completion::{
    CompletionContext, CompletionDirective, completion_at_offset_in_source,
    possible_words_at_offset_in_source,
};
use expect_test::expect;

fn get_source_and_cursor(input: &str) -> (String, u32) {
    let mut cursor = -1;
    let mut source = String::new();
    for c in input.chars() {
        if c == '|' {
            cursor = i32::try_from(source.len()).expect("input length should fit into u32");
        } else {
            source.push(c);
        }
    }
    let cursor = u32::try_from(cursor).expect("missing cursor marker in input");
    (source, cursor)
}

fn check_valid_words(input: &str, expect: &expect_test::Expect) {
    let (input, cursor) = get_source_and_cursor(input);
    let w = possible_words_at_offset_in_source(&input, cursor);
    expect.assert_debug_eq(&w);
}

fn check_context(input: &str, expected: CompletionContext) {
    let (input, cursor) = get_source_and_cursor(input);
    let completion = completion_at_offset_in_source(&input, cursor);
    assert_eq!(completion.context, Some(expected), "source: {input:?}");
}

fn check_directive(input: &str, expected: CompletionDirective) {
    let (input, cursor) = get_source_and_cursor(input);
    let completion = completion_at_offset_in_source(&input, cursor);
    assert_eq!(
        completion.context,
        Some(CompletionContext::DirectiveValue),
        "source: {input:?}"
    );
    assert_eq!(completion.directive, Some(expected), "source: {input:?}");
}

#[test]
fn begin_document() {
    check_valid_words(
        "|OPENQASM 3;",
        &expect![[r#"
            WordKinds(
                PathExpr | Annotation | Durationof | Barrier | Box | Break | Cal | Const | Continue | CReg | Ctrl | Def | DefCal | DefCalGrammar | Delay | End | Extern | False | For | Gate | If | Include | Input | Inv | Let | Measure | NegCtrl | OpenQASM | Output | Pow | Pragma | QReg | Qubit | Reset | True | Return | Switch | While,
            )
        "#]],
    );
}

#[test]
fn end_of_version() {
    check_valid_words(
        "OPENQASM 3;|",
        &expect![[r#"
            WordKinds(
                PathExpr | Annotation | Durationof | Barrier | Box | Break | Cal | Const | Continue | CReg | Ctrl | Def | DefCal | DefCalGrammar | Delay | End | Extern | False | For | Gate | If | Include | Input | Inv | Let | Measure | NegCtrl | Output | Pow | Pragma | QReg | Qubit | Reset | True | Return | Switch | While,
            )
        "#]],
    );
}

#[test]
fn annotation_completion_context_tracks_name_and_value_boundaries() {
    for input in [
        "@|",
        "@|qdk.qir.profile",
        "@qdk.|qir.profile",
        "@qdk.qir.profile|",
        "@!malformed|",
    ] {
        check_context(input, CompletionContext::AnnotationName);
    }

    for input in ["@qdk.qir.profile |Base", "@qdk.qir.profile Ba|se"] {
        check_context(input, CompletionContext::DirectiveValue);
    }
}

#[test]
fn pragma_completion_context_tracks_name_and_value_boundaries() {
    for input in [
        "#pragma |",
        "#pragma |qdk.qir.profile",
        "#pragma qdk.qir.profile|",
        "#pragma qdk.|",
        "#pragma !malformed|",
    ] {
        check_context(input, CompletionContext::PragmaName);
    }

    for input in [
        "#pragma qdk.qir.profile |Base",
        "#pragma qdk.qir.profile Ba|se",
    ] {
        check_context(input, CompletionContext::DirectiveValue);
    }
}

#[test]
fn directive_value_completion_includes_directive_identity() {
    for input in ["@qdk.qir.profile |", "@qdk.qir.profile Ba|se"] {
        check_directive(
            input,
            CompletionDirective::Annotation("qdk.qir.profile".to_string()),
        );
    }

    for input in ["#pragma qdk.qir.profile |", "#pragma qdk.qir.profile Ba|se"] {
        check_directive(
            input,
            CompletionDirective::Pragma("qdk.qir.profile".to_string()),
        );
    }
}

#[test]
fn directive_context_does_not_cross_physical_line_boundary() {
    let (input, cursor) = get_source_and_cursor("@qdk.qir.profile Base\n|");
    let completion = completion_at_offset_in_source(&input, cursor);
    assert_eq!(completion.context, None);
}
