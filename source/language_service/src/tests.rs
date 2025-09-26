// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Encoding, LanguageService, UpdateWorker,
    protocol::{DiagnosticUpdate, ErrorKind, TestCallables},
};
use expect_test::{Expect, expect};
use qsc::{compile, line_column::Position, project};
use std::{cell::RefCell, rc::Rc};
use test_fs::{FsNode, TestProjectHost, dir, file};

pub(crate) mod test_fs;

#[tokio::test]
async fn code_action_operation_refactor() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);

    let contents = "namespace Foo { operation Op(a : Int, b : Bool) : Unit { } }";
    ls.update_document("foo.qs", 1, contents, "qsharp");
    worker.apply_pending().await;

    // Position range covering the word 'operation'
    let range = qsc::line_column::Range {
        start: qsc::line_column::Position {
            line: 0,
            column: 16,
        },
        end: qsc::line_column::Position {
            line: 0,
            column: 25,
        },
    };

    let actions = ls.get_code_actions("foo.qs", range);
    let wrapper_action = actions
        .iter()
        .find(|a| a.title.contains("Generate wrapper for Op"))
        .expect("wrapper refactor should be present");
    let edit = wrapper_action
        .edit
        .as_ref()
        .expect("wrapper refactor should produce an edit");
    assert!(edit.changes.iter().any(|(_, edits)| {
        edits
            .iter()
            .any(|e| e.new_text.contains("operation Op_Wrapper()"))
    }));
}

// Helper to fetch a specific wrapper action and return its inserted text
fn get_wrapper_text(ls: &LanguageService, uri: &str, source: &str, op_name: &str) -> String {
    let range = qsc::line_column::Range {
        start: qsc::line_column::Position { line: 0, column: 0 },
        end: qsc::line_column::Position {
            line: 0,
            column: u32::try_from(source.len()).unwrap(),
        },
    };
    let actions = ls.get_code_actions(uri, range);
    let action = actions
        .iter()
        .find(|a| a.title == format!("Generate wrapper for {op_name}"))
        .unwrap_or_else(|| panic!("Expected wrapper action for {op_name}"));
    action.edit.as_ref().expect("expected edit").changes[0].1[0]
        .new_text
        .clone()
}

#[tokio::test]
async fn code_action_wrapper_default_qubit() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    let source = "namespace Test { operation Q(q : Qubit) : Unit {} }";
    ls.update_document("q.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "q.qs", source, "Q");
    assert!(wrapper.contains("use q = Qubit();"));
}

#[tokio::test]
async fn code_action_wrapper_default_qubit_array() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    let source = "namespace Test { operation QA(qs : Qubit[]) : Unit {} }";
    ls.update_document("qa.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "qa.qs", source, "QA");
    assert!(wrapper.contains("use qs = Qubit[1];"));
}

#[tokio::test]
async fn code_action_wrapper_default_primitives() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    let source = "namespace Test { operation Prims(a : Int, b : Bool, c : Double, d : Result, e : Pauli, f : BigInt, g : String) : Unit {} }";
    ls.update_document("prims.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "prims.qs", source, "Prims");
    for expected in [
        "let a = 0;",
        "let b = false;",
        "let c = 0.0;",
        "let d = Zero;",
        "let e = PauliI;",
        "let f = 0L;",
        "let g = \"\";",
    ] {
        assert!(
            wrapper.contains(expected),
            "missing primitive default line: {expected}\nFull text: {wrapper}"
        );
    }
}

#[tokio::test]
async fn code_action_wrapper_default_udt() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    let source = "namespace Test { newtype MyT = Int; operation UsesUdt(x : MyT) : Unit {} }";
    ls.update_document("udt.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "udt.qs", source, "UsesUdt");
    assert!(wrapper.contains("// TODO: provide value for x (UDT MyT)"));
}

#[tokio::test]
async fn code_action_wrapper_default_generic() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    let source = "namespace Test { operation Generic<'T>(x : 'T) : Unit {} }";
    ls.update_document("generic.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "generic.qs", source, "Generic");
    assert!(wrapper.contains("// TODO: provide value for x (Generic parameter"));
}

#[tokio::test]
async fn code_action_wrapper_default_array_int() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    let source = "namespace Test { operation Arr(arr : Int[]) : Unit {} }";
    ls.update_document("arr.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "arr.qs", source, "Arr");
    assert!(wrapper.contains("let arr = [];"));
}

#[tokio::test]
async fn code_action_wrapper_default_tuple_destructured() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    // Destructured tuple parameter pattern
    // Q#: a single tuple parameter is written as one parameter whose pattern is a tuple without an explicit type annotation per component.
    // We'll supply a typed outer tuple type as the parameter type to keep it simple.
    let source =
        "namespace Test { operation Tup(param : (Int, Bool, (Double, Qubit))) : Unit { } }";
    ls.update_document("tup.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "tup.qs", source, "Tup");
    // Expect allocations and primitive defaults
    // Expect synthesized bound variable 'param' with tuple literal and its inner declarations
    assert!(wrapper.contains("let param = ("));
    assert!(wrapper.contains("0, false, (0.0, "));
    assert!(wrapper.contains("use param_q"));
    // Call should pass 'param'
    assert!(wrapper.contains("Tup(param);"));
}

#[tokio::test]
async fn code_action_wrapper_default_tuple_bound() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    // Single binding to tuple type
    let source = "namespace Test { operation Tup2(t : (Qubit, Int, (Bool, Qubit[]))) : Unit { } }";
    ls.update_document("tup2.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "tup2.qs", source, "Tup2");
    // Expect synthesized component declarations
    assert!(wrapper.contains("use t_q0 = Qubit();"));
    assert!(wrapper.contains("let t = (t_q0, 0, (false,"));
    // Allocation for qubit array element (name may be t_qs0 or t_qs1 depending on ordering)
    assert!(wrapper.contains("use t_qs"));
}

#[tokio::test]
async fn code_action_wrapper_default_single_element_tuple() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    // Single element tuple parameter (type) bound to a name
    let source = "namespace Test { operation Single(t : (Double,)) : Unit { } }";
    ls.update_document("single.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    let wrapper = get_wrapper_text(&ls, "single.qs", source, "Single");
    // Expect trailing comma in tuple literal
    assert!(
        wrapper.contains("let t = (0.0,);"),
        "wrapper text did not contain single element tuple with trailing comma.\n{wrapper}"
    );
}

#[tokio::test]
async fn code_action_wrapper_indentation_nested() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);
    // Simulate an operation appearing with extra indentation (e.g., inside a cell or generated block)
    let source =
        "namespace Test {\n    // Some preceding code\n    operation Ind(a : Int) : Unit { }\n}"; // 4-space indent level before 'operation'
    ls.update_document("indent.qs", 1, source, "qsharp");
    worker.apply_pending().await;
    // Manually fetch actions to aid debugging if it fails
    let line_count = u32::try_from(source.matches('\n').count()).unwrap();
    let range = qsc::line_column::Range {
        start: qsc::line_column::Position { line: 0, column: 0 },
        end: qsc::line_column::Position {
            line: line_count,
            column: 0,
        },
    };
    let actions = ls.get_code_actions("indent.qs", range);
    let wrapper_action = actions
        .iter()
        .find(|a| a.title == "Generate wrapper for Ind")
        .unwrap_or_else(|| {
            panic!(
                "Expected wrapper action for Ind. Titles: {:?}",
                actions.iter().map(|a| &a.title).collect::<Vec<_>>()
            )
        });
    let wrapper = wrapper_action.edit.as_ref().unwrap().changes[0].1[0]
        .new_text
        .clone();
    // Expect wrapper to start with same 4-space indent before 'operation'
    assert!(
        wrapper.starts_with("    operation Ind_Wrapper()"),
        "Wrapper did not preserve base indentation.\n{wrapper}"
    );
    // Body lines should have one more indent level (8 spaces) for inner content after adjustment
    assert!(wrapper.contains("        // Call original operation"));
}

#[tokio::test]
async fn single_document() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 1, "namespace Foo { }", "qsharp");

    worker.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "foo.qs",
        &(expect![[r#"
            []
        "#]]),
        &(expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "foo.qs",
                        contents: "namespace Foo { }",
                        offset: 0,
                    },
                ],
                common_prefix: None,
                entry: None,
            }
        "#]]),
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn single_document_update() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);

    ls.update_document("foo.qs", 1, "namespace Foo { }", "qsharp");

    worker.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "foo.qs",
        &(expect![[r#"
            []
        "#]]),
        &(expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "foo.qs",
                        contents: "namespace Foo { }",
                        offset: 0,
                    },
                ],
                common_prefix: None,
                entry: None,
            }
        "#]]),
    );

    // UPDATE 2
    ls.update_document(
        "foo.qs",
        1,
        "namespace Foo { @EntryPoint() operation Bar() : Unit {} }",
        "qsharp",
    );

    worker.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "foo.qs",
        &(expect![[r#"
            []
        "#]]),
        &(expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "foo.qs",
                        contents: "namespace Foo { @EntryPoint() operation Bar() : Unit {} }",
                        offset: 0,
                    },
                ],
                common_prefix: None,
                entry: None,
            }
        "#]]),
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn document_in_project() {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);

    ls.update_document("project/src/this_file.qs", 1, "namespace Foo { }", "qsharp");

    check_errors_and_no_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "project/src/this_file.qs",
        &(expect![[r#"
            []
        "#]]),
    );

    // now process background work
    worker.apply_pending().await;

    check_errors_and_compilation(
        &ls,
        &mut received_errors.borrow_mut(),
        "project/src/this_file.qs",
        &expect![[r#"
            []
        "#]],
        &expect![[r#"
            SourceMap {
                sources: [
                    Source {
                        name: "project/src/other_file.qs",
                        contents: "namespace OtherFile { operation Other() : Unit {} }",
                        offset: 0,
                    },
                    Source {
                        name: "project/src/this_file.qs",
                        contents: "namespace Foo { }",
                        offset: 52,
                    },
                ],
                common_prefix: Some(
                    "project/src/",
                ),
                entry: None,
            }
        "#]],
    );
}

// the below tests test the asynchronous behavior of the language service.
// we use `get_completions` as a rough analog for all document operations, as
// they all go through the same `document_op` infrastructure.
#[tokio::test]
async fn completions_requested_before_document_load() {
    let errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let _worker = create_update_worker(&mut ls, &errors, &test_cases);

    ls.update_document(
        "foo.qs",
        1,
        "namespace Foo { open Microsoft.Quantum.Diagnostics; @EntryPoint() operation Main() : Unit { DumpMachine() } }",
        "qsharp"
    );

    // we intentionally don't await work to test how LSP features function when
    // a document hasn't fully loaded

    // this should be empty, because the doc hasn't loaded
    assert!(
        ls.get_completions(
            "foo.qs",
            Position {
                line: 0,
                column: 76
            }
        )
        .items
        .is_empty()
    );
}

#[tokio::test]
async fn completions_requested_after_document_load() {
    let errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &errors, &test_cases);

    // this test is a contrast to `completions_requested_before_document_load`
    // we want to ensure that completions load when the update_document call has been awaited
    ls.update_document(
        "foo.qs",
        1,
        "namespace Foo { open Microsoft.Quantum.Diagnostics; @EntryPoint() operation Main() : Unit { DumpMachine() } }",
        "qsharp"
    );

    worker.apply_pending().await;

    assert!(
        &ls.get_completions(
            "foo.qs",
            Position {
                line: 0,
                column: 92,
            },
        )
        .items
        .iter()
        .any(|item| item.label == "DumpMachine")
    );
}

fn check_errors_and_compilation(
    ls: &LanguageService,
    received_errors: &mut Vec<ErrorInfo>,
    uri: &str,
    expected_errors: &Expect,
    expected_compilation: &Expect,
) {
    expected_errors.assert_debug_eq(received_errors);
    assert_compilation(ls, uri, expected_compilation);
    received_errors.clear();
}

fn check_errors_and_no_compilation(
    ls: &LanguageService,
    received_errors: &mut Vec<ErrorInfo>,
    uri: &str,
    expected_errors: &Expect,
) {
    expected_errors.assert_debug_eq(received_errors);
    received_errors.clear();

    let state = ls.state.try_borrow().expect("borrow should succeed");
    assert!(state.get_compilation(uri).is_none());
}

fn assert_compilation(ls: &LanguageService, uri: &str, expected: &Expect) {
    let state = ls.state.try_borrow().expect("borrow should succeed");
    let compilation = state
        .get_compilation(uri)
        .expect("compilation should exist");
    expected.assert_debug_eq(&compilation.user_unit().sources);
}

type ErrorInfo = (
    String,
    Option<u32>,
    Vec<compile::ErrorKind>,
    Vec<project::Error>,
);

fn create_update_worker<'a>(
    ls: &mut LanguageService,
    received_errors: &'a RefCell<Vec<ErrorInfo>>,
    received_test_cases: &'a RefCell<Vec<TestCallables>>,
) -> UpdateWorker<'a> {
    ls.create_update_worker(
        |update: DiagnosticUpdate| {
            let project_errors = update.errors.iter().filter_map(|error| match error {
                ErrorKind::Project(error) => Some(error.clone()),
                ErrorKind::Compile(_) | ErrorKind::DocumentStatus { .. } => None,
            });
            let compile_errors = update.errors.iter().filter_map(|error| match error {
                ErrorKind::Compile(error) => Some(error.error().clone()),
                ErrorKind::Project(_) | ErrorKind::DocumentStatus { .. } => None,
            });

            let mut v = received_errors.borrow_mut();

            v.push((
                update.uri,
                update.version,
                compile_errors.collect(),
                project_errors.collect(),
            ));
        },
        move |update: TestCallables| {
            let mut v = received_test_cases.borrow_mut();
            v.push(update);
        },
        TestProjectHost {
            fs: TEST_FS.with(Clone::clone),
        },
    )
}

thread_local! { static TEST_FS: Rc<RefCell<FsNode>> = Rc::new(RefCell::new(test_fs())) }

fn test_fs() -> FsNode {
    FsNode::Dir(
        [dir(
            "project",
            [
                file("qsharp.json", "{}"),
                dir(
                    "src",
                    [
                        file(
                            "other_file.qs",
                            "namespace OtherFile { operation Other() : Unit {} }",
                        ),
                        file("this_file.qs", "namespace Foo { }"),
                    ],
                ),
            ],
        )]
        .into_iter()
        .collect(),
    )
}
