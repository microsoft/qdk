// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use indoc::indoc;
use miette::Result;
use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};
use qsc_eval::{output::CursorReceiver, val::Value};
use qsc_frontend::compile::SourceMap;
use qsc_passes::PackageType;
use std::io::Cursor;

use crate::interpret::{Error, InterpretResult, Interpreter};

fn line(interpreter: &mut Interpreter, line: impl AsRef<str>) -> (InterpretResult, String) {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    (
        interpreter.eval_fragments(&mut receiver, line.as_ref()),
        receiver.dump(),
    )
}

fn eval(interpreter: &mut Interpreter) -> (Result<Value, Vec<Error>>, String) {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    (interpreter.eval_entry(&mut receiver), receiver.dump())
}

#[test]
fn stack_traces_can_cross_eval_session_and_file_boundaries() {
    let source1 = indoc! { r#"
        namespace Test {
            operation B(input : Int) : Unit is Adj {
                body ... {
                    C(input)
                }
                adjoint invert;
            }

            operation C(input : Int) : Unit is Adj {
                body ... {
                    1 / input;
                }
                adjoint self;
            }
        }
        "#};
    let source2 = indoc! { r#"
        namespace Test2 {
            open Test;
            operation A(input : Int) : Unit is Adj {
                body ... {
                    B(input)
                }
                adjoint invert;
            }
        }
        "#};

    let source_map = SourceMap::new(
        [
            ("1.qs".into(), source1.into()),
            ("2.qs".into(), source2.into()),
        ],
        None,
    );

    let (std_id, store) = crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
    let mut interpreter = Interpreter::new(
        source_map,
        PackageType::Lib,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )
    .expect("Failed to compile base environment.");

    let (result, _) = line(
        &mut interpreter,
        "operation Z(input : Int) : Unit { Adjoint Test2.A(input); }",
    );
    result.expect("code should compile");

    let (result, _output) = line(&mut interpreter, "Z(0)");

    match result {
        Ok(_) => panic!("Expected error"),
        Err(e) => {
            let stack_trace = e[0]
                .stack_trace()
                .expect("code should have a valid stack trace");
            expect![[r#"
                Error: division by zero
                Call stack:
                    at Adjoint Test.C in 1.qs:11:13
                    at Adjoint Test.B in 1.qs:4:13
                    at Adjoint Test2.A in 2.qs:5:13
                    at Z in line_0:1:35
            "#]]
            .assert_eq(stack_trace);
        }
    }
}

#[test]
fn stack_traces_can_cross_file_and_entry_boundaries() {
    let source1 = indoc! { r#"
        namespace Test {
            operation B(input : Int) : Unit is Adj {
                body ... {
                    C(input)
                }
                adjoint invert;
            }

            operation C(input : Int) : Unit is Adj {
                body ... {
                    1 / input;
                }
                adjoint self;
            }
        }
        "#};
    let source2 = indoc! { r#"
        namespace Test2 {
            open Test;
            operation A(input : Int) : Unit is Adj {
                body ... {
                    B(input)
                }
                adjoint invert;
            }
        }
        "#};

    let source_map = SourceMap::new(
        [
            ("1.qs".into(), source1.into()),
            ("2.qs".into(), source2.into()),
        ],
        Some("Adjoint Test2.A(0)".into()),
    );

    let (std_id, store) = crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
    let mut interpreter = Interpreter::new(
        source_map,
        PackageType::Exe,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )
    .expect("Failed to compile base environment.");

    let (result, _) = eval(&mut interpreter);

    match result {
        Ok(_) => panic!("Expected error"),
        Err(e) => {
            let stack_trace = e[0]
                .stack_trace()
                .expect("code should have a valid stack trace");
            expect![[r#"
                Error: division by zero
                Call stack:
                    at Adjoint Test.C in 1.qs:11:13
                    at Adjoint Test.B in 1.qs:4:13
                    at Adjoint Test2.A in 2.qs:5:13
            "#]]
            .assert_eq(stack_trace);
        }
    }
}
