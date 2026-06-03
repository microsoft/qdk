// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{code_action, test_utils::compile_project_with_markers_no_cursor};
use qsc::line_column::{Encoding, Position, Range};

fn get_wrap_in_array_actions(source: &str) -> Vec<crate::protocol::CodeAction> {
    let (compilation, _targets) =
        compile_project_with_markers_no_cursor(&[("<source>", source)], false);
    let newline_count = u32::try_from(source.matches('\n').count()).expect("count fits");
    let end = if newline_count == 0 {
        Position {
            line: 0,
            column: u32::try_from(source.len()).expect("len fits"),
        }
    } else {
        Position {
            line: newline_count,
            column: 0,
        }
    };
    let range = Range {
        start: Position { line: 0, column: 0 },
        end,
    };
    let actions = code_action::get_code_actions(&compilation, "<source>", range, Encoding::Utf8);
    actions
        .into_iter()
        .filter(|a| a.title == "Convert to single-element array")
        .collect()
}

#[test]
fn single_arg_qubit_to_qubit_array() {
    let source = "namespace A {
    operation Foo(qs: Qubit[]) : Unit is Adj {
        use q = Qubit();
        Foo(q);
    }
}
";
    let actions = get_wrap_in_array_actions(source);
    assert_eq!(actions.len(), 1, "Expected 1 action, got: {actions:?}");
    let action = &actions[0];
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = &edit.changes[0];
    assert_eq!(text_edits.len(), 1);
    assert_eq!(text_edits[0].new_text, "[q]");
}

#[test]
fn multi_arg_second_param_is_array() {
    let source = "namespace A {
    operation Bar(x: Int, qs: Qubit[]) : Unit {
        use q = Qubit();
        Bar(1, q);
    }
}
";
    let actions = get_wrap_in_array_actions(source);
    assert_eq!(actions.len(), 1, "Expected 1 action, got: {actions:?}");
    let action = &actions[0];
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = &edit.changes[0];
    assert_eq!(text_edits.len(), 1);
    assert_eq!(text_edits[0].new_text, "[q]");
}

#[test]
fn no_action_when_types_already_match() {
    let source = "namespace A {
    operation Foo(qs: Qubit[]) : Unit is Adj {
        use q = Qubit();
        Foo([q]);
    }
}
";
    let actions = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}

#[test]
fn no_action_for_unrelated_mismatch() {
    // Int passed where String expected - should NOT offer wrap in array.
    let source = "namespace A {
    function Foo(s: String) : Unit {}
    function Bar() : Unit {
        Foo(42);
    }
}
";
    let actions = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}
