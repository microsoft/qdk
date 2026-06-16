// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::code_action;
use crate::test_utils::{
    compile_notebook_with_fake_stdlib, compile_project_with_markers_no_cursor,
};
use expect_test::{Expect, expect};
use qsc::{Span, line_column::{Encoding, Range}};



/// Collects the titles of the auto-import code actions offered for `source`.
fn import_action_titles(source: &str) -> Vec<String> {
    let (compilation, _targets) =
        compile_project_with_markers_no_cursor(&[("<source>", source)], true);
    let len = u32::try_from(source.len()).expect("source length fits in u32");
    let range = Range::from_span(Encoding::Utf8, source, &Span { lo: 0, hi: len });
    let actions = code_action::get_code_actions(&compilation, "<source>", range, Encoding::Utf8);
    actions
        .into_iter()
        .filter(|a| a.title.starts_with("Import "))
        .map(|a| a.title)
        .collect()
}

fn check_import_titles(source: &str, expect: &Expect) {
    expect.assert_eq(&format!("{:#?}", import_action_titles(source)));
}

#[test]
fn unresolved_term_offers_import() {
    check_import_titles(
        "namespace Test {
            operation Main() : Unit {
                Fake();
            }
        }",
        &expect![[r#"
            [
                "Import FakeStdLib.Fake",
            ]"#]],
    );
}

#[test]
fn unresolved_type_offers_import() {
    check_import_titles(
        "namespace Test {
            operation Main(x : Udt) : Unit {}
        }",
        &expect![[r#"
            [
                "Import FakeStdLib.Udt",
            ]"#]],
    );
}

#[test]
fn resolved_name_offers_no_import() {
    check_import_titles(
        "namespace Test {
            open FakeStdLib;
            operation Main() : Unit {
                Fake();
            }
        }",
        &expect![[r#"
            []"#]],
    );
}

#[test]
fn qualified_unresolved_name_is_skipped() {
    // v1 only handles unqualified names; a partial path like `Wrong.Fake` should not
    // produce an auto-import quick fix.
    check_import_titles(
        "namespace Test {
            operation Main() : Unit {
                Wrong.Fake();
            }
        }",
        &expect![[r#"
            []"#]],
    );
}

#[test]
fn name_in_multiple_namespaces_offers_one_import_each() {
    // The same unqualified name exists in two namespaces, neither of which is open,
    // so a separate import action is offered for each (sorted by namespace name).
    check_import_titles(
        "namespace NsA {
            operation Collide() : Unit {}
            export Collide;
        }
        namespace NsB {
            operation Collide() : Unit {}
            export Collide;
        }
        namespace Test {
            operation Main() : Unit {
                Collide();
            }
        }",
        &expect![[r#"
            [
                "Import NsA.Collide",
                "Import NsB.Collide",
            ]"#]],
    );
}

#[test]
fn import_edit_inserts_at_namespace_start() {
    let source = "namespace Test {
            operation Main() : Unit {
                Fake();
            }
        }";
    let (compilation, _targets) =
        compile_project_with_markers_no_cursor(&[("<source>", source)], true);
    let len = u32::try_from(source.len()).expect("source length fits in u32");
    let range = Range::from_span(Encoding::Utf8, source, &Span { lo: 0, hi: len });
    let actions = code_action::get_code_actions(&compilation, "<source>", range, Encoding::Utf8);
    let action = actions
        .iter()
        .find(|a| a.title == "Import FakeStdLib.Fake")
        .expect("expected an import action for Fake");

    let edit = action.edit.as_ref().expect("expected an edit");
    assert_eq!(edit.changes.len(), 1);
    let (file, edits) = &edit.changes[0];
    assert_eq!(file, "<source>");
    assert_eq!(edits.len(), 1);
    let text_edit = &edits[0];
    // Insertion (zero-length range) before the first item in the namespace.
    assert_eq!(text_edit.range.start, text_edit.range.end);
    assert!(
        text_edit.new_text.contains("import FakeStdLib.Fake;"),
        "unexpected edit text: {:?}",
        text_edit.new_text
    );
}

#[test]
fn notebook_unresolved_term_offers_import() {
    let compilation = compile_notebook_with_fake_stdlib([("cell1", "Fake();")].into_iter());
    let source = "Fake();";
    let len = u32::try_from(source.len()).expect("source length fits in u32");
    let range = Range::from_span(Encoding::Utf8, source, &Span { lo: 0, hi: len });
    let actions = code_action::get_code_actions(&compilation, "cell1", range, Encoding::Utf8);
    let titles: Vec<String> = actions
        .into_iter()
        .filter(|a| a.title.starts_with("Import "))
        .map(|a| a.title)
        .collect();
    expect![[r#"
        [
            "Import FakeStdLib.Fake",
        ]"#]]
    .assert_eq(&format!("{titles:#?}"));
}
