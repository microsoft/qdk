// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the wrapper refactor code action logic.

use crate::tests::test_fs::{FsNode, TestProjectHost, dir, file};
use crate::{
    Encoding, LanguageService, UpdateWorker,
    protocol::{DiagnosticUpdate, ErrorKind, TestCallables},
};
use std::{cell::RefCell, rc::Rc};

// Local copy of helper to create update worker (cannot import private symbol from parent tests module directly).
fn create_update_worker<'a>(
    ls: &mut LanguageService,
    received_errors: &'a RefCell<
        Vec<(
            String,
            Option<u32>,
            Vec<qsc::compile::ErrorKind>,
            Vec<qsc::project::Error>,
        )>,
    >,
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

// Helper to fetch a specific wrapper action and return its inserted text
async fn get_wrapper_text(source: &str, op_name: &str) -> String {
    let received_errors = RefCell::new(Vec::new());
    let test_cases = RefCell::new(Vec::new());
    let mut ls = LanguageService::new(Encoding::Utf8);
    let mut worker = create_update_worker(&mut ls, &received_errors, &test_cases);

    ls.update_document("test.qs", 1, source, "qsharp");
    worker.apply_pending().await;

    let newline_count = u32::try_from(source.matches('\n').count()).expect("count fits");
    let end_pos = if newline_count == 0 {
        qsc::line_column::Position {
            line: 0,
            column: u32::try_from(source.len()).expect("length fits"),
        }
    } else {
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
    let source =
        "namespace Test {\n    // Some preceding code\n    operation Ind(a : Int) : Unit { }\n}";
    let wrapper_text = get_wrapper_text(source, "Ind").await;
    assert!(wrapper_text.starts_with("operation Ind_Wrapper()"));
    assert!(wrapper_text.contains("        // Call original operation"));
}

#[tokio::test]
async fn code_action_wrapper_indentation_tabs() {
    let source = "namespace Test {\n\toperation Tabbed(a : Int) : Unit { }\n}";
    let wrapper_text = get_wrapper_text(source, "Tabbed").await;
    assert!(wrapper_text.starts_with("operation Tabbed_Wrapper()"));
    assert!(wrapper_text.contains("\t\t// Call original operation"));
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
    let source =
        "namespace Test { operation Tup(param : (Int, Bool, (Double, Qubit))) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Tup").await;
    assert!(wrapper.contains("let param = ("));
    assert!(wrapper.contains("0, false, (0.0, "));
    assert!(wrapper.contains("use param_q"));
    assert!(wrapper.contains("Tup(param);"));
}

#[tokio::test]
async fn code_action_wrapper_default_tuple_bound() {
    let source = "namespace Test { operation Tup2(t : (Qubit, Int, (Bool, Qubit[]))) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Tup2").await;
    assert!(wrapper.contains("use t_q0 = Qubit();"));
    assert!(wrapper.contains("let t = (t_q0, 0, (false,"));
    assert!(wrapper.contains("use t_qs"));
}

#[tokio::test]
async fn code_action_wrapper_qubit_tuple_counter_persistence() {
    let source = "namespace Test { operation Deep(t : (Qubit, (Qubit, Qubit), Qubit, (Qubit, Qubit), (Qubit, (Qubit, Qubit)))) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Deep").await;
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
    let source = "namespace Test { operation Mix(t : (Qubit, Qubit[], (Qubit, Qubit[], Qubit[]), Qubit, Qubit[])) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Mix").await;
    assert!(wrapper.contains("use t_q0 = Qubit();"));
    assert!(wrapper.contains("use t_q1 = Qubit();"));
    assert!(wrapper.contains("use t_q2 = Qubit();"));
    assert!(wrapper.contains("use t_qs0 = Qubit[1];"));
    assert!(wrapper.contains("use t_qs1 = Qubit[1];"));
    assert!(wrapper.contains("use t_qs2 = Qubit[1];"));
}

#[tokio::test]
async fn code_action_wrapper_tuple_todo_positioning() {
    let source = "namespace Test { newtype MyT = Int; operation Mixed<'T>(t : (Qubit, MyT, 'T)) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Mixed").await;
    let alloc_index = wrapper
        .find("use t_q0 = Qubit();")
        .expect("allocation present");
    let binding_index = wrapper.find("let t = (").expect("binding present");
    assert!(alloc_index < binding_index);
    let todo_udt_index = wrapper
        .find("TODO: provide value for tuple component of t (UDT MyT)")
        .expect("UDT TODO");
    let generic_index = wrapper
        .find("TODO: provide value for tuple component of t (Generic parameter")
        .expect("Generic TODO");
    assert!(alloc_index < todo_udt_index && todo_udt_index < binding_index);
    assert!(alloc_index < generic_index && generic_index < binding_index);
}

#[tokio::test]
async fn code_action_wrapper_default_single_element_tuple() {
    let source = "namespace Test { operation Single(t : (Double,)) : Unit { } }";
    let wrapper = get_wrapper_text(source, "Single").await;
    assert!(wrapper.contains("let t = (0.0,);"));
}
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
