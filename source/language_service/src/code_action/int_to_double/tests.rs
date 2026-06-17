// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    code_action,
    test_utils::{compile_project_with_markers_no_cursor, whole_document_range},
};
use qsc::{line_column::Encoding, location::Location};

fn expect_single<T: std::fmt::Debug>(items: &[T]) -> &T {
    let [item] = items else {
        panic!("expected a single item, got: {items:?}");
    };
    item
}

fn get_int_to_double_actions(source: &str) -> (Vec<Location>, Vec<crate::protocol::CodeAction>) {
    let (compilation, targets) =
        compile_project_with_markers_no_cursor(&[("<source>", source)], false);
    let range = whole_document_range(source);
    let actions = code_action::get_code_actions(&compilation, "<source>", range, Encoding::Utf8);
    (
        targets,
        actions
            .into_iter()
            .filter(|a| a.title == "Convert to double literal")
            .collect(),
    )
}

#[test]
fn int_literal_to_double() {
    let source = "namespace A {
    function Foo(d: Double) : Unit {
        Foo(1◉◉);
    }
}
";
    let (locations, actions) = get_int_to_double_actions(source);
    let action = expect_single(&actions);
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = expect_single(&edit.changes);
    let text_edit = expect_single(text_edits);
    let location = expect_single(&locations);
    assert_eq!(text_edit.range, location.range);
    assert_eq!(text_edit.new_text, ".");
}

#[test]
fn int_literal_to_double_with_parens() {
    let source = "namespace A {
    function Foo(d: Double) : Unit {
        Foo((1◉◉));
    }
}
";
    let (locations, actions) = get_int_to_double_actions(source);
    let action = expect_single(&actions);
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = expect_single(&edit.changes);
    let text_edit = expect_single(text_edits);
    let location = expect_single(&locations);
    assert_eq!(text_edit.range, location.range);
    assert_eq!(text_edit.new_text, ".");
}

#[test]
fn int_literal_to_double_with_pos() {
    let source = "namespace A {
    function Foo(d: Double) : Unit {
        Foo((+1◉◉));
    }
}
";
    let (locations, actions) = get_int_to_double_actions(source);
    let action = expect_single(&actions);
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = expect_single(&edit.changes);
    let text_edit = expect_single(text_edits);
    let location = expect_single(&locations);
    assert_eq!(text_edit.range, location.range);
    assert_eq!(text_edit.new_text, ".");
}

#[test]
fn int_literal_to_double_with_neg() {
    let source = "namespace A {
    function Foo(d: Double) : Unit {
        Foo((-1◉◉));
    }
}
";
    let (locations, actions) = get_int_to_double_actions(source);
    let action = expect_single(&actions);
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = expect_single(&edit.changes);
    let text_edit = expect_single(text_edits);
    let location = expect_single(&locations);
    assert_eq!(text_edit.range, location.range);
    assert_eq!(text_edit.new_text, ".");
}

#[test]
fn int_literal_to_double_with_notb() {
    let source = "namespace A {
    function Foo(d: Double) : Unit {
        Foo((~~~1◉◉));
    }
}
";
    let (_, actions) = get_int_to_double_actions(source);
    assert_eq!(actions.len(), 0, "Expected 0 actions, got: {actions:?}");
}

#[test]
fn int_local_to_double() {
    let source = "namespace A {
    function Foo(d: Double) : Unit {
        let x = 1;
        Foo((x◉◉));
    }
}
";
    let (_, actions) = get_int_to_double_actions(source);
    assert_eq!(actions.len(), 0, "Expected 0 actions, got: {actions:?}");
}
