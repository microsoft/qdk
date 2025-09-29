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

// Helper to fetch a specific wrapper action and return its inserted text
async fn get_wrapper_text(source: &str, op_name: &str) -> String {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);

    ls.update_document("test.qs", 1, source, "qsharp");
    worker.apply_pending().await;

    // Select the entire document range. We compute the number of newline characters to get the last line index.
    // End position uses the line just after the last content line (column 0) to safely encompass multi-line sources.
    let newline_count = u32::try_from(source.matches('\n').count()).expect("count fits");
    let end_pos = if newline_count == 0 {
        qsc::line_column::Position {
            line: 0,
            column: u32::try_from(source.len()).expect("length fits"),
        }
    } else {
        // Use the line just after the last content line (column 0) to cover entire file.
        qsc::line_column::Position {
            line: newline_count,
            column: 0,
        }
    };
    let range = qsc::line_column::Range {
        start: qsc::line_column::Position { line: 0, column: 0 },
        end: end_pos,
    };
    let actions = ls.get_code_actions("test.qs", range);
    let action = actions
        .iter()
        .find(|a| a.title == format!("Generate wrapper for {op_name}"))
        .unwrap_or_else(|| panic!("Expected wrapper action for {op_name}"));
    action.edit.as_ref().expect("expected edit").changes[0].1[0]
        .new_text
        .clone()
}

#[tokio::test]
async fn code_action_operation_refactor() {
    let wrapper_text = get_wrapper_text("operation Op(a : Int, b : Bool) : Unit { }", "Op").await;
    assert!(wrapper_text.contains("operation Op_Wrapper() : Unit {"));
}

#[tokio::test]
async fn code_action_wrapper_indentation_nested() {
    // Simulate an operation appearing with extra indentation (e.g., inside a cell or generated block)
    let source =
        "namespace Test {\n    // Some preceding code\n    operation Ind(a : Int) : Unit { }\n}"; // 4-space indent level before 'operation'

    let wrapper_text = get_wrapper_text(source, "Ind").await;
    // Wrapper intentionally starts flush-left (insertion point supplies indentation in file), so no leading spaces expected.
    assert!(
        wrapper_text.starts_with("operation Ind_Wrapper()"),
        "Wrapper does not start flush-left.\n{wrapper_text}"
    );
    // Body lines should still be indented relative to base (which will be applied on insertion). We just ensure internal indentation tokens exist.
    assert!(
        wrapper_text.contains("        // Call original operation"),
        "Did not find internally indented call comment.\n{wrapper_text}"
    );
}

#[tokio::test]
async fn code_action_wrapper_indentation_tabs() {
    // Tab-indented operation
    let source = "namespace Test {\n\toperation Tabbed(a : Int) : Unit { }\n}"; // leading tab before operation

    let wrapper_text = get_wrapper_text(source, "Tabbed").await;
    // Wrapper intentionally starts flush-left (insertion point supplies indentation in file), so no leading spaces expected.
    assert!(
        wrapper_text.starts_with("operation Tabbed_Wrapper()"),
        "Wrapper does not start flush-left.\n{wrapper_text}"
    );
    // Body lines should still be indented relative to base (which will be applied on insertion). We just ensure internal indentation tokens exist.
    assert!(
        wrapper_text.contains("\t\t// Call original operation"),
        "Did not find internally indented call comment.\n{wrapper_text}"
    );
}

#[tokio::test]
async fn code_action_wrapper_default_qubit() {
    let source = "namespace Test { operation Q(q : Qubit) : Unit {} }";
    let wrapper = get_wrapper_text(source, "Q").await;
    assert!(wrapper.contains("use q = Qubit();"));
}

#[tokio::test]
async fn code_action_wrapper_default_qubit_array() {
    let source = "namespace Test { operation QA(qs : Qubit[]) : Unit {} }";
    let wrapper = get_wrapper_text(source, "QA").await;
    assert!(wrapper.contains("use qs = Qubit[1];"));
}

#[tokio::test]
async fn code_action_wrapper_default_primitives() {
    let source = "namespace Test { operation Prims(a : Int, b : Bool, c : Double, d : Result, e : Pauli, f : BigInt, g : String) : Unit {} }";
    let wrapper = get_wrapper_text(source, "Prims").await;
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
    let source = "namespace Test { newtype MyT = Int; operation UsesUdt(x : MyT) : Unit {} }";
    let wrapper = get_wrapper_text(source, "UsesUdt").await;
    assert!(wrapper.contains("// TODO: provide value for x (UDT MyT)"));
}

#[tokio::test]
async fn code_action_wrapper_default_generic() {
    let source = "namespace Test { operation Generic<'T>(x : 'T) : Unit {} }";
    let wrapper = get_wrapper_text(source, "Generic").await;
    assert!(wrapper.contains("// TODO: provide value for x (Generic parameter"));
}

#[tokio::test]
async fn code_action_wrapper_default_array_int() {
    let source = "namespace Test { operation Arr(arr : Int[]) : Unit {} }";
    let wrapper = get_wrapper_text(source, "Arr").await;
    assert!(wrapper.contains("let arr = [];"));
}

#[tokio::test]
async fn code_action_wrapper_default_tuple_destructured() {
    // Destructured tuple parameter pattern
    // Q#: a single tuple parameter is written as one parameter whose pattern is a tuple without an explicit type annotation per component.
    // We'll supply a typed outer tuple type as the parameter type to keep it simple.
    let source =
        "namespace Test { operation Tup(param : (Int, Bool, (Double, Qubit))) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Tup").await;
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
    // Single binding to tuple type
    let source = "namespace Test { operation Tup2(t : (Qubit, Int, (Bool, Qubit[]))) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Tup2").await;
    // Expect synthesized component declarations
    assert!(wrapper.contains("use t_q0 = Qubit();"));
    assert!(wrapper.contains("let t = (t_q0, 0, (false,"));
    // Allocation for qubit array element (name may be t_qs0 or t_qs1 depending on ordering)
    assert!(wrapper.contains("use t_qs"));
}

#[tokio::test]
async fn code_action_wrapper_qubit_tuple_counter_persistence() {
    // Complex nested tuple of qubits to ensure counter continuity.
    let source = "namespace Test { operation Deep(t : (Qubit, (Qubit, Qubit), Qubit, (Qubit, Qubit), (Qubit, (Qubit, Qubit)))) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Deep").await;
    // Verify sequential allocation names appear without resets.
    for name in [
        "t_q0", "t_q1", "t_q2", "t_q3", "t_q4", "t_q5", "t_q6", "t_q7",
    ] {
        assert!(
            wrapper.contains(&format!("use {name} = Qubit();")),
            "Missing expected allocation {name}. Wrapper:\n{wrapper}"
        );
    }
}

#[tokio::test]
async fn code_action_wrapper_qubit_and_array_counters() {
    // Mix single qubits and qubit arrays to ensure independent counters.
    let source = "namespace Test { operation Mix(t : (Qubit, Qubit[], (Qubit, Qubit[], Qubit[]), Qubit, Qubit[])) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Mix").await;
    // Expect sequential q variables: t_q0, t_q1 ... (only for single qubits: first, inside tuple, then another)
    assert!(wrapper.contains("use t_q0 = Qubit();"));
    assert!(wrapper.contains("use t_q1 = Qubit();"));
    assert!(wrapper.contains("use t_q2 = Qubit();"));
    // Expect sequential qs variables independent: t_qs0, t_qs1, t_qs2 (outer, inner tuple, inner tuple second array, final)
    assert!(wrapper.contains("use t_qs0 = Qubit[1];"));
    assert!(wrapper.contains("use t_qs1 = Qubit[1];"));
    assert!(wrapper.contains("use t_qs2 = Qubit[1];"));
}

#[tokio::test]
async fn code_action_wrapper_default_single_element_tuple() {
    // Single element tuple parameter (type) bound to a name
    let source = "namespace Test { operation Single(t : (Double,)) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Single").await;
    // Expect trailing comma in tuple literal
    assert!(
        wrapper.contains("let t = (0.0,);"),
        "wrapper text did not contain single element tuple with trailing comma.\n{wrapper}"
    );
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
