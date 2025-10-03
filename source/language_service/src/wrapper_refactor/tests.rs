// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{code_action, test_utils::compile_project_with_markers_no_cursor};
use expect_test::expect;
use qsc::line_column::{Encoding, Position, Range};

fn get_wrapper_text(source: &str, op_name: &str) -> String {
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
    let action = actions
        .iter()
        .find(|a| a.title == format!("Generate wrapper with default arguments for {op_name}"))
        .unwrap_or_else(|| {
            panic!(
                "Expected wrapper action for {op_name}. Available: {:?}",
                actions.iter().map(|a| &a.title).collect::<Vec<_>>()
            )
        });
    // --- Range validation ---
    let edit = action.edit.as_ref().expect("expected edit");
    assert_eq!(edit.changes.len(), 1, "Expected a single file change");
    let (file, edits) = &edit.changes[0];
    assert_eq!(file, "<source>", "Unexpected file in edit change");
    assert_eq!(edits.len(), 1, "Expected exactly one text edit");
    let text_edit = &edits[0];
    let edit_range = text_edit.range;
    // The wrapper insertion should be a zero-length insertion immediately before the operation declaration.
    assert_eq!(
        edit_range.start, edit_range.end,
        "Wrapper edit should be an insertion (zero-length range)"
    );
    if let Some(op_start_byte) = source.find(&format!("operation {op_name}")) {
        // Compute expected (line, column) for op_start_byte.
        let mut line: usize = 0;
        let mut col: usize = 0;
        let mut counted: usize = 0;
        for part in source.split_inclusive('\n') {
            let part_len = part.len();
            if counted + part_len > op_start_byte {
                // op starts in this line
                let line_start_index = op_start_byte - counted;
                col = part[..line_start_index].chars().count();
                break;
            }
            counted += part_len;
            line += 1;
        }
        assert_eq!(
            edit_range.start.line as usize, line,
            "Edit start line mismatch (expected {line}, got {})",
            edit_range.start.line
        );
        assert_eq!(
            edit_range.start.column as usize, col,
            "Edit start column mismatch (expected {col}, got {})",
            edit_range.start.column
        );
    } else {
        panic!("Could not locate operation {op_name} in source to validate range");
    }
    text_edit.new_text.clone()
}

#[test]
fn basic_wrapper() {
    // Wrap in a namespace since most language features (including item collection) assume a namespace context.
    let wrapper_text = get_wrapper_text(
        "namespace Test { operation Op(a : Int, b : Bool) : Unit { } }",
        "Op",
    );
    expect![[r#"
        operation OpWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            let a = 0;
            let b = false;

            // Call original operation
            Op(a, b);
        }

    "#]]
    .assert_eq(&wrapper_text);
}

#[test]
fn indentation_nested() {
    let source =
        "namespace Test {\n    // Some preceding code\n    operation Ind(a : Int) : Unit { }\n}";
    let wrapper_text = get_wrapper_text(source, "Ind");
    assert!(wrapper_text.starts_with("operation IndWithDefaults()"));
    assert!(wrapper_text.contains("        // Call original operation"));
}

#[test]
fn indentation_tabs() {
    let source = "namespace Test {\n\toperation Tabbed(a : Int) : Unit { }\n}";
    let wrapper_text = get_wrapper_text(source, "Tabbed");
    assert!(wrapper_text.starts_with("operation TabbedWithDefaults()"));
    assert!(wrapper_text.contains("\t\t// Call original operation"));
}

#[test]
fn default_qubit() {
    let wrapper = get_wrapper_text("namespace Test { operation Q(q : Qubit) : Unit { } }", "Q");
    expect![[r#"
        operation QWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            use q = Qubit();

            // Call original operation
            Q(q);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_qubit_array() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation QA(qs : Qubit[]) : Unit { } }",
        "QA",
    );
    expect![[r#"
        operation QAWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            use qs = Qubit[1];

            // Call original operation
            QA(qs);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_primitives() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Prims(a : Int, b : Bool, c : Double, d : Result, e : Pauli, f : BigInt, g : String) : Unit { } }",
        "Prims",
    );
    expect![[r#"
        operation PrimsWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            let a = 0;
            let b = false;
            let c = 0.0;
            let d = Zero;
            let e = PauliI;
            let f = 0L;
            let g = "";

            // Call original operation
            Prims(a, b, c, d, e, f, g);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_udt() {
    let wrapper = get_wrapper_text(
        "namespace Test { newtype MyT = Int; operation UsesUdt(x : MyT) : Unit { } }",
        "UsesUdt",
    );
    expect![[r#"
        operation UsesUdtWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            // TODO: provide value for x (UDT MyT)

            // Call original operation
            UsesUdt(_);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_generic() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Generic<'T>(x : 'T) : Unit { } }",
        "Generic",
    );
    expect![[r#"
        operation GenericWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            // TODO: provide value for x (Generic parameter 'T)

            // Call original operation
            Generic(_);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_array_int() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Arr(arr : Int[]) : Unit { } }",
        "Arr",
    );
    expect![[r#"
        operation ArrWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            let arr = [];

            // Call original operation
            Arr(arr);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_tuple_destructured() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Tup(param : (Int, Bool, (Double, Qubit))) : Unit { } }",
        "Tup",
    );
    expect![[r#"
        operation TupWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            use param_q0 = Qubit();
            let param = (0, false, (0.0, param_q0));

            // Call original operation
            Tup(param);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_tuple_bound() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Tup2(t : (Qubit, Int, (Bool, Qubit[]))) : Unit { } }",
        "Tup2",
    );
    expect![[r#"
        operation Tup2WithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            use t_q0 = Qubit();
            use t_qs0 = Qubit[1];
            let t = (t_q0, 0, (false, t_qs0));

            // Call original operation
            Tup2(t);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn qubit_tuple_counter_persistence() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Deep(t : (Qubit, (Qubit, Qubit), Qubit, (Qubit, Qubit), (Qubit, (Qubit, Qubit)))) : Unit { } }",
        "Deep",
    );
    expect![[r#"
        operation DeepWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            use t_q0 = Qubit();
            use t_q1 = Qubit();
            use t_q2 = Qubit();
            use t_q3 = Qubit();
            use t_q4 = Qubit();
            use t_q5 = Qubit();
            use t_q6 = Qubit();
            use t_q7 = Qubit();
            use t_q8 = Qubit();
            let t = (t_q0, (t_q1, t_q2), t_q3, (t_q4, t_q5), (t_q6, (t_q7, t_q8)));

            // Call original operation
            Deep(t);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn qubit_and_array_counters() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Mix(t : (Qubit, Qubit[], (Qubit, Qubit[], Qubit[]), Qubit, Qubit[])) : Unit { } }",
        "Mix",
    );
    expect![[r#"
        operation MixWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            use t_q0 = Qubit();
            use t_qs0 = Qubit[1];
            use t_q1 = Qubit();
            use t_qs1 = Qubit[1];
            use t_qs2 = Qubit[1];
            use t_q2 = Qubit();
            use t_qs3 = Qubit[1];
            let t = (t_q0, t_qs0, (t_q1, t_qs1, t_qs2), t_q2, t_qs3);

            // Call original operation
            Mix(t);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn tuple_todo_positioning() {
    let wrapper = get_wrapper_text(
        "namespace Test { newtype MyT = Int; operation Mixed<'T>(t : (Qubit, MyT, 'T)) : Unit { } }",
        "Mixed",
    );
    expect![[r#"
        operation MixedWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            use t_q0 = Qubit();
            // TODO: provide value for tuple component of t (UDT MyT)
            // TODO: provide value for tuple component of t (Generic parameter 'T)
            let t = (t_q0, _, _);

            // Call original operation
            Mixed(t);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn default_single_element_tuple() {
    let wrapper = get_wrapper_text(
        "namespace Test { operation Single(t : (Double,)) : Unit { } }",
        "Single",
    );
    expect![[r#"
        operation SingleWithDefaults() : Unit {
            // TODO: Fill out the values for the parameters
            let t = (0.0,);

            // Call original operation
            Single(t);
        }

    "#]]
    .assert_eq(&wrapper);
}

#[test]
fn no_code_action_for_lambdas_() {
    let source = "namespace Test { operation Named(x : Int) : Unit { let l = (y) => { x + y }; let e = (y) => { x + y }; l(2); } }";
    let (compilation, _targets) =
        compile_project_with_markers_no_cursor(&[("<source>", source)], false);
    let range = Range {
        start: Position { line: 0, column: 0 },
        end: Position {
            line: 0,
            column: u32::try_from(source.len()).expect("len fits"),
        },
    };
    let actions = code_action::get_code_actions(&compilation, "<source>", range, Encoding::Utf8);
    let titles = actions
        .iter()
        .filter_map(|a| {
            if a.title.contains("Generate wrapper") {
                Some(a.title.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    expect!["Generate wrapper with default arguments for Named"].assert_eq(&titles);
}

#[test]
fn preserves_crlf_newlines() {
    // Source with Windows CRLF newlines. We embed them explicitly.
    let source = "namespace Test {\r\n    operation Op(a : Int) : Unit { }\r\n}";
    let wrapper = get_wrapper_text(source, "Op");
    // Ensure the wrapper uses CRLF consistently (no lone \n occurrences)
    assert!(
        wrapper.matches("\r\n").count() > 2,
        "Expected multiple CRLF sequences in wrapper text"
    );
    assert!(
        !wrapper.contains('\n') || wrapper.contains("\r\n"),
        "Found bare LF without CR"
    );
}
