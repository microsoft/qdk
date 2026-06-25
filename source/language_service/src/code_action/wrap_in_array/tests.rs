// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    code_action,
    test_utils::{compile_project_with_markers_no_cursor, whole_document_range},
};
use qsc::{line_column::Encoding, location::Location};

fn get_wrap_in_array_actions(source: &str) -> (Vec<Location>, Vec<crate::protocol::CodeAction>) {
    let (compilation, targets) =
        compile_project_with_markers_no_cursor(&[("<source>", source)], false);
    let range = whole_document_range(source);
    let actions = code_action::get_code_actions(&compilation, "<source>", range, Encoding::Utf8);
    (
        targets,
        actions
            .into_iter()
            .filter(|a| a.title == "Convert to single-element array")
            .collect(),
    )
}

#[test]
fn single_arg_qubit_to_qubit_array_fix_offered() {
    let source = "namespace A {
    operation Foo(qs: Qubit[]) : Unit is Adj {
        use q = Qubit();
        Foo(◉◉q◉◉);
    }
}
";
    let (locations, actions) = get_wrap_in_array_actions(source);
    assert_eq!(actions.len(), 1, "Expected 1 action, got: {actions:?}");
    let action = &actions[0];
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = &edit.changes[0];
    assert_eq!(text_edits.len(), locations.len());
    assert_eq!(text_edits[0].new_text, "[");
    assert_eq!(text_edits[0].range, locations[0].range);
    assert_eq!(text_edits[1].new_text, "]");
    assert_eq!(text_edits[1].range, locations[1].range);
}

#[test]
fn multi_arg_second_param_is_array_fix_offered() {
    let source = "namespace A {
    operation Bar(x: Int, qs: Qubit[]) : Unit {
        use q = Qubit();
        Bar(1, ◉◉q◉◉);
    }
}
";
    let (locations, actions) = get_wrap_in_array_actions(source);
    assert_eq!(actions.len(), 1, "Expected 1 action, got: {actions:?}");
    let action = &actions[0];
    let edit = action.edit.as_ref().expect("expected edit");
    let (_, text_edits) = &edit.changes[0];
    assert_eq!(text_edits.len(), locations.len());
    assert_eq!(text_edits[0].new_text, "[");
    assert_eq!(text_edits[0].range, locations[0].range);
    assert_eq!(text_edits[1].new_text, "]");
    assert_eq!(text_edits[1].range, locations[1].range);
}

#[test]
fn types_already_match_fix_not_offered() {
    let source = "namespace A {
    operation Foo(qs: Qubit[]) : Unit is Adj {
        use q = Qubit();
        Foo([q]);
    }
}
";
    let (_, actions) = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}

#[test]
fn unrelated_mismatch_fix_not_offered() {
    // Int passed where String expected - should NOT offer wrap in array.
    let source = "namespace A {
    function Foo(s: String) : Unit {}
    function Bar() : Unit {
        Foo(42);
    }
}
";
    let (_, actions) = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}

#[test]
fn tuple_to_tuple_array_fix_not_offered() {
    // (Qubit, Qubit) passed where (Qubit, Qubit)[] expected - not a primitive type.
    let source = "namespace A {
    operation Foo(qs: (Qubit, Qubit)[]) : Unit {}
    operation Bar() : Unit {
        use (q1, q2) = (Qubit(), Qubit());
        Foo((q1, q2));
    }
}
";
    let (_, actions) = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}

#[test]
fn array_to_nested_array_fix_not_offered() {
    // Qubit[] passed where Qubit[][] expected - the expression type is Qubit[] (not
    // a primitive), so the code action should not be offered.
    let source = "namespace A {
    operation Foo(qs: Qubit[][]) : Unit {}
    operation Bar(qs: Qubit[]) : Unit {
        Foo(qs);
    }
}
";
    let (_, actions) = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}

#[test]
fn arrow_to_arrow_array_fix_not_offered() {
    // An operation value passed where ((Qubit) => Unit)[] expected - not a primitive type.
    let source = "namespace A {
    operation MyOp(q: Qubit) : Unit {}
    operation Foo(ops: ((Qubit) => Unit)[]) : Unit {}
    operation Bar() : Unit {
        Foo(MyOp);
    }
}
";
    let (_, actions) = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}

#[test]
fn param_to_param_array_fix_not_offered() {
    // A generic type parameter passed where 'T[] expected - not a primitive type.
    let source = "namespace A {
    operation Foo<'T>(ts: 'T[]) : Unit {}
    operation Bar<'T>(x: 'T) : Unit {
        Foo(x);
    }
}
";
    let (_, actions) = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}

#[test]
fn udt_to_udt_array_fix_not_offered() {
    // A UDT value passed where MyType[] expected - not a primitive type.
    let source = "namespace A {
    newtype MyType = Int;
    function Foo(xs: MyType[]) : Unit {}
    function Bar(x: MyType) : Unit {
        Foo(x);
    }
}
";
    let (_, actions) = get_wrap_in_array_actions(source);
    assert!(actions.is_empty(), "Expected no actions, got: {actions:?}");
}
