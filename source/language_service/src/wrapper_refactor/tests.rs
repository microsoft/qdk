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
        .find(|a| a.title == format!("Generate wrapper for {op_name}"))
        .unwrap_or_else(|| {
            panic!(
                "Expected wrapper action for {op_name}. Available: {:?}",
                actions.iter().map(|a| &a.title).collect::<Vec<_>>()
            )
        });
    action.edit.as_ref().expect("expected edit").changes[0].1[0]
        .new_text
        .clone()
}

#[test]
fn basic_wrapper() {
    // Wrap in a namespace since most language features (including item collection) assume a namespace context.
    let wrapper_text = get_wrapper_text(
        "namespace Test { operation Op(a : Int, b : Bool) : Unit { } }",
        "Op",
    );
    expect![[r#"
        operation Op_Wrapper() : Unit {
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
    assert!(wrapper_text.starts_with("operation Ind_Wrapper()"));
    assert!(wrapper_text.contains("        // Call original operation"));
}

#[test]
fn indentation_tabs() {
    let source = "namespace Test {\n\toperation Tabbed(a : Int) : Unit { }\n}";
    let wrapper_text = get_wrapper_text(source, "Tabbed");
    assert!(wrapper_text.starts_with("operation Tabbed_Wrapper()"));
    assert!(wrapper_text.contains("\t\t// Call original operation"));
}

#[test]
fn default_qubit() {
    let wrapper = get_wrapper_text("namespace Test { operation Q(q : Qubit) : Unit { } }", "Q");
    expect![[r#"
        operation Q_Wrapper() : Unit {
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
        operation QA_Wrapper() : Unit {
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
        operation Prims_Wrapper() : Unit {
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
        operation UsesUdt_Wrapper() : Unit {
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
        operation Generic_Wrapper() : Unit {
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
        operation Arr_Wrapper() : Unit {
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
        operation Tup_Wrapper() : Unit {
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
        operation Tup2_Wrapper() : Unit {
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
        operation Deep_Wrapper() : Unit {
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
        operation Mix_Wrapper() : Unit {
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
        operation Mixed_Wrapper() : Unit {
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
        operation Single_Wrapper() : Unit {
            // TODO: Fill out the values for the parameters
            let t = (0.0,);

            // Call original operation
            Single(t);
        }

    "#]]
    .assert_eq(&wrapper);
}
