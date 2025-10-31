// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use indoc::indoc;
use qsc::{
    LanguageFeatures, PackageStore, PauliNoise, SourceMap, TargetCapabilityFlags, interpret,
};

use crate::get_qir_from_qsharp;

use super::run_internal_with_features;

fn run_internal<F>(sources: SourceMap, event_cb: F, shots: u32) -> Result<(), Box<interpret::Error>>
where
    F: FnMut(&str),
{
    let (std_id, store) = crate::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
    run_internal_with_features(
        sources,
        event_cb,
        shots,
        LanguageFeatures::default(),
        TargetCapabilityFlags::all(),
        store,
        &[(std_id, None)],
        &PauliNoise::default(),
        0.0, // qubit loss
    )
}

#[test]
fn test_missing_type() {
    let code = "namespace input { operation Foo(a) : Unit {} }";
    let expr = "";
    let count = std::cell::Cell::new(0);

    let _ = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            expect![[r#"{"result":{"code":"Qsc.TypeCk.MissingTy","errors":[{"diagnostic":{"code":"Qsc.TypeCk.MissingTy","message":"type error: missing type in item signature\n\nhelp: a type must be provided for this item","range":{"end":{"character":33,"line":0},"start":{"character":32,"line":0}},"severity":"error"},"document":"test.qs","stack":null},{"diagnostic":{"code":"Qsc.TypeCk.AmbiguousTy","message":"type error: insufficient type information to infer type\n\nhelp: provide a type annotation","range":{"end":{"character":33,"line":0},"start":{"character":32,"line":0}},"severity":"error"},"document":"test.qs","stack":null}],"message":"type error: missing type in item signature\n\nhelp: a type must be provided for this item","range":{"end":{"character":33,"line":0},"start":{"character":32,"line":0}},"severity":"error"},"success":false,"type":"Result"}"#]].assert_eq(msg);
            count.set(count.get() + 1);
        },
        1,
    );
    assert_eq!(count.get(), 1);
}

#[test]
fn test_compile() {
    let code = "namespace test { @EntryPoint() operation Foo(): Result {
    use q = Qubit();
    H(q);
    M(q)
    }}";

    let (std_id, store) = crate::compile::package_store_with_stdlib(TargetCapabilityFlags::empty());

    let result = qsc::codegen::qir::get_qir(
        SourceMap::new([("test.qs".into(), code.into())], None),
        LanguageFeatures::default(),
        TargetCapabilityFlags::empty(),
        store,
        &[(std_id, None)],
    );
    assert!(result.is_ok());
}

#[test]
fn test_run_two_shots() {
    let code = "
        namespace Test {
            function Answer() : Int {
                return 42;
            }
        }
    ";
    let expr = "Test.Answer()";
    let count = std::cell::Cell::new(0);

    let _result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            assert!(msg.contains("42"));
            count.set(count.get() + 1);
        },
        2,
    );
    assert_eq!(count.get(), 2);
}

#[test]
fn fail_ry() {
    let code = "namespace Sample {
        operation main() : Result[] {
            use q1 = Qubit();
            Ry(q1);
            let m1 = M(q1);
            return [m1];
        }
    }";
    let expr = "";
    let count = std::cell::Cell::new(0);

    let _result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            expect![[r#"{"result":{"code":"Qsc.TypeCk.TyMismatch","errors":[{"diagnostic":{"code":"Qsc.TypeCk.TyMismatch","message":"type error: expected (Double, Qubit), found Qubit","range":{"end":{"character":18,"line":3},"start":{"character":12,"line":3}},"severity":"error"},"document":"test.qs","stack":null}],"message":"type error: expected (Double, Qubit), found Qubit","range":{"end":{"character":18,"line":3},"start":{"character":12,"line":3}},"severity":"error"},"success":false,"type":"Result"}"#]].assert_eq(msg);
            count.set(count.get() + 1);
        },
        1,
    );
    assert_eq!(count.get(), 1);
}

#[test]
fn test_message() {
    let code = r#"namespace Sample {
        import Std.Diagnostics.*;

        operation main() : Unit {
            Message("hi");
            return ();
        }
    }"#;
    let expr = "Sample.main()";
    let result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            assert!(msg.contains("hi") || msg.contains("result"));
        },
        1,
    );
    assert!(result.is_ok());
}

#[test]
fn test_matrix() {
    let code = r"namespace Test {
        import Microsoft.Quantum.Diagnostics.DumpOperation;
        operation Main() : Unit {
            DumpOperation(2, Bell);
        }

        operation Bell(q: Qubit[]) : Unit is Adj {
            H(q[0]);
            CX(q[0], q[1]);
        }
    }";
    let expr = "Test.Main()";
    let count = std::cell::Cell::new(0);
    let result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            if msg.contains("Matrix") {
                count.set(count.get() + 1);
                // Check the start and end of the matrix LaTeX is formatted as expected
                assert!(msg.contains(
                    r"$ \\begin{bmatrix} \\frac{1}{\\sqrt{2}} & 0 & \\frac{1}{\\sqrt{2}} & 0 \\\\"
                ));
                assert!(msg.contains(
                    r"\\frac{1}{\\sqrt{2}} & 0 & -\\frac{1}{\\sqrt{2}} & 0 \\\\ \\end{bmatrix} $"
                ));
            }
        },
        1,
    );
    assert!(result.is_ok());
    assert!(count.get() == 1);
}

#[test]
fn message_with_escape_sequences() {
    let code = r#"namespace Sample {
        import Std.Diagnostics.*;

        operation main() : Unit {
            Message("\ta\n\t");

            return ();
        }
    }"#;
    let expr = "Sample.main()";
    let result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            assert!(msg.contains(r"\ta\n\t") || msg.contains("result"));
        },
        1,
    );
    assert!(result.is_ok());
}
#[test]
fn message_with_backslashes() {
    let code = r#"namespace Sample {
        import Std.Diagnostics.*;

        operation main() : Unit {
            Message("hi \\World");
            Message("hello { \\World [");

            return ();
        }
    }"#;
    let expr = "Sample.main()";
    let result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            assert!(
                msg.contains("hello { \\\\World [")
                    || msg.contains("hi \\\\World")
                    || msg.contains("result")
            );
        },
        1,
    );
    assert!(result.is_ok());
}

#[test]
fn test_entrypoint() {
    let code = r#"namespace Sample {
        @EntryPoint()
        operation main() : Unit {
            Message("hi");
            return ();
        }
    }"#;
    let expr = "";
    let result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            assert!(msg.contains("hi") || msg.contains("result"));
        },
        1,
    );
    assert!(result.is_ok());
}

#[test]
fn test_missing_entrypoint() {
    let code = "namespace Sample {
        operation test() : Result[] {
            use q1 = Qubit();
            let m1 = M(q1);
            return [m1];
        }
    }";
    let expr = "";
    let result = run_internal(
        SourceMap::new([("test.qs".into(), code.into())], Some(expr.into())),
        |msg| {
            expect![[r#"{"result":{"code":"Qsc.EntryPoint.NotFound","errors":[{"diagnostic":{"code":"Qsc.EntryPoint.NotFound","message":"entry point not found\n\nhelp: a single callable with the `@EntryPoint()` attribute must be present if no entry expression is provided and no callable named `Main` is present","range":{"end":{"character":1,"line":0},"start":{"character":0,"line":0}},"severity":"error"},"document":"<project>","stack":null}],"message":"entry point not found\n\nhelp: a single callable with the `@EntryPoint()` attribute must be present if no entry expression is provided and no callable named `Main` is present","range":{"end":{"character":1,"line":0},"start":{"character":0,"line":0}},"severity":"error"},"success":false,"type":"Result"}"#]].assert_eq(msg);
        },
        1,
    );
    assert!(result.is_err());
}

#[test]
fn test_run_simple_program_multiple_shots() {
    let mut output = Vec::new();
    let code = indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int { 4 }
            }"
    };
    run_internal(
        SourceMap::new([("code".into(), code.into())], None),
        |s| output.push(s.to_string()),
        3,
    )
    .expect("code should compile and run");
    expect![[r#"
        {"result":"4","success":true,"type":"Result"}
        {"result":"4","success":true,"type":"Result"}
        {"result":"4","success":true,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

#[test]
fn test_run_error_program_multiple_shots() {
    let mut output = Vec::new();
    let code = indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    CNOT(q, q)
                }
            }"
    };
    run_internal(
        SourceMap::new([("test.qs".into(), code.into())], None),
        |s| output.push(s.to_string()),
        3,
    )
    .expect("code should compile and run");
    expect![[r#"
        {"result":{"code":"Qsc.Eval.QubitUniqueness","errors":[{"diagnostic":{"code":"Qsc.Eval.QubitUniqueness","message":"runtime error: qubits in invocation are not unique","range":{"end":{"character":49,"line":113},"start":{"character":32,"line":113}},"severity":"error"},"document":"qsharp-library-source:Std/Intrinsic.qs","stack":"Error: qubits in invocation are not unique\nCall stack:\n    at QIR.Intrinsic.__quantum__qis__cx__body in qsharp-library-source:QIR/Intrinsic.qs:12:1\n    at Std.Intrinsic.CNOT in qsharp-library-source:Std/Intrinsic.qs:114:9\n    at Test.Main in test.qs:5:9\n"}],"message":"runtime error: qubits in invocation are not unique","range":{"end":{"character":49,"line":113},"start":{"character":32,"line":113}},"severity":"error"},"success":false,"type":"Result"}
        {"result":{"code":"Qsc.Eval.QubitUniqueness","errors":[{"diagnostic":{"code":"Qsc.Eval.QubitUniqueness","message":"runtime error: qubits in invocation are not unique","range":{"end":{"character":49,"line":113},"start":{"character":32,"line":113}},"severity":"error"},"document":"qsharp-library-source:Std/Intrinsic.qs","stack":"Error: qubits in invocation are not unique\nCall stack:\n    at QIR.Intrinsic.__quantum__qis__cx__body in qsharp-library-source:QIR/Intrinsic.qs:12:1\n    at Std.Intrinsic.CNOT in qsharp-library-source:Std/Intrinsic.qs:114:9\n    at Test.Main in test.qs:5:9\n"}],"message":"runtime error: qubits in invocation are not unique","range":{"end":{"character":49,"line":113},"start":{"character":32,"line":113}},"severity":"error"},"success":false,"type":"Result"}
        {"result":{"code":"Qsc.Eval.QubitUniqueness","errors":[{"diagnostic":{"code":"Qsc.Eval.QubitUniqueness","message":"runtime error: qubits in invocation are not unique","range":{"end":{"character":49,"line":113},"start":{"character":32,"line":113}},"severity":"error"},"document":"qsharp-library-source:Std/Intrinsic.qs","stack":"Error: qubits in invocation are not unique\nCall stack:\n    at QIR.Intrinsic.__quantum__qis__cx__body in qsharp-library-source:QIR/Intrinsic.qs:12:1\n    at Std.Intrinsic.CNOT in qsharp-library-source:Std/Intrinsic.qs:114:9\n    at Test.Main in test.qs:5:9\n"}],"message":"runtime error: qubits in invocation are not unique","range":{"end":{"character":49,"line":113},"start":{"character":32,"line":113}},"severity":"error"},"success":false,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

#[test]
fn test_run_error_program_multiple_shots_qubit_leak() {
    // If qubits are leaked from execution, the runtime will fail with an out of memory
    // error pretty quickly.
    let mut output = Vec::new();
    let code = indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    H(q);
                }
            }"
    };
    run_internal(
        SourceMap::new([("code".into(), code.into())], None),
        |s| output.push(s.to_string()),
        100,
    )
    .expect("code should compile and run");

    // Spot check the results to make sure we're getting the right error.
    expect![[r#"{"result":{"code":"Qsc.Eval.ReleasedQubitNotZero","errors":[{"diagnostic":{"code":"Qsc.Eval.ReleasedQubitNotZero","message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"code","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"document":"code","stack":"Error: Qubit0 released while not in |0⟩ state\nCall stack:\n    at QIR.Runtime.__quantum__rt__qubit_release in qsharp-library-source:core/qir.qs:9:5\n    at Test.Main in code:4:9\n"}],"message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"code","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"success":false,"type":"Result"}"#]]
        .assert_eq(&output[0]);
    expect![[r#"{"result":{"code":"Qsc.Eval.ReleasedQubitNotZero","errors":[{"diagnostic":{"code":"Qsc.Eval.ReleasedQubitNotZero","message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"code","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"document":"code","stack":"Error: Qubit0 released while not in |0⟩ state\nCall stack:\n    at QIR.Runtime.__quantum__rt__qubit_release in qsharp-library-source:core/qir.qs:9:5\n    at Test.Main in code:4:9\n"}],"message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"code","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"success":false,"type":"Result"}"#]]
        .assert_eq(&output[50]);
    expect![[r#"{"result":{"code":"Qsc.Eval.ReleasedQubitNotZero","errors":[{"diagnostic":{"code":"Qsc.Eval.ReleasedQubitNotZero","message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"code","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"document":"code","stack":"Error: Qubit0 released while not in |0⟩ state\nCall stack:\n    at QIR.Runtime.__quantum__rt__qubit_release in qsharp-library-source:core/qir.qs:9:5\n    at Test.Main in code:4:9\n"}],"message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"code","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"success":false,"type":"Result"}"#]]
        .assert_eq(&output[99]);
}

#[test]
fn test_runtime_error_with_span() {
    let mut output = Vec::new();
    let code = indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    fail "hello"
                }
            }"#
    };
    run_internal(
        SourceMap::new([("test.qs".into(), code.into())], None),
        |s| output.push(s.to_string()),
        3,
    )
    .expect("code should compile and run");
    expect![[r#"
        {"result":{"code":"Qsc.Eval.UserFail","errors":[{"diagnostic":{"code":"Qsc.Eval.UserFail","message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}}},"message":"explicit fail"}],"severity":"error"},"document":"test.qs","stack":"Error: program failed: hello\nCall stack:\n    at Test.Main in test.qs:4:9\n"}],"message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}}},"message":"explicit fail"}],"severity":"error"},"success":false,"type":"Result"}
        {"result":{"code":"Qsc.Eval.UserFail","errors":[{"diagnostic":{"code":"Qsc.Eval.UserFail","message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}}},"message":"explicit fail"}],"severity":"error"},"document":"test.qs","stack":"Error: program failed: hello\nCall stack:\n    at Test.Main in test.qs:4:9\n"}],"message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}}},"message":"explicit fail"}],"severity":"error"},"success":false,"type":"Result"}
        {"result":{"code":"Qsc.Eval.UserFail","errors":[{"diagnostic":{"code":"Qsc.Eval.UserFail","message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}}},"message":"explicit fail"}],"severity":"error"},"document":"test.qs","stack":"Error: program failed: hello\nCall stack:\n    at Test.Main in test.qs:4:9\n"}],"message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":20,"line":3},"start":{"character":8,"line":3}}},"message":"explicit fail"}],"severity":"error"},"success":false,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

// Need to revisit spans in output: https://github.com/microsoft/qsharp/issues/944
#[test]
fn test_runtime_error_in_another_file_with_project() {
    let mut output = Vec::new();
    let first = indoc! {r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    Test.other()
                }
            }"
    };
    let second = indoc! {r#"
            namespace Test {
                operation other() : Unit {
                    fail "hello"
                }
            }"#
    };
    run_internal(
        SourceMap::new(
            [
                ("test1.qs".into(), first.into()),
                ("test2.qs".into(), second.into()),
            ],
            None,
        ),
        |s| output.push(s.to_string()),
        1,
    )
    .expect("code should compile and run");
    expect![[r#"{"result":{"code":"Qsc.Eval.UserFail","errors":[{"diagnostic":{"code":"Qsc.Eval.UserFail","message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":2},"start":{"character":8,"line":2}},"related":[{"location":{"source":"test2.qs","span":{"end":{"character":20,"line":2},"start":{"character":8,"line":2}}},"message":"explicit fail"}],"severity":"error"},"document":"test2.qs","stack":"Error: program failed: hello\nCall stack:\n    at Test.other in test2.qs:3:9\n    at Test.Main in test1.qs:4:9\n"}],"message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":2},"start":{"character":8,"line":2}},"related":[{"location":{"source":"test2.qs","span":{"end":{"character":20,"line":2},"start":{"character":8,"line":2}}},"message":"explicit fail"}],"severity":"error"},"success":false,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

#[test]
fn test_runtime_error_with_failure_in_main_file_project() {
    let mut output = Vec::new();
    let first = indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    Test.other()
                }
                operation failing_call() : Unit {
                    fail "hello"
                }
            }"#
    };
    let second = indoc! {r"
            namespace Test {
                operation other() : Unit {
                    Test.failing_call()
                }
            }"
    };
    run_internal(
        SourceMap::new(
            [
                ("test1.qs".into(), first.into()),
                ("test2.qs".into(), second.into()),
            ],
            None,
        ),
        |s| output.push(s.to_string()),
        1,
    )
    .expect("code should compile and run");
    expect![[r#"{"result":{"code":"Qsc.Eval.UserFail","errors":[{"diagnostic":{"code":"Qsc.Eval.UserFail","message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":6},"start":{"character":8,"line":6}},"related":[{"location":{"source":"test1.qs","span":{"end":{"character":20,"line":6},"start":{"character":8,"line":6}}},"message":"explicit fail"}],"severity":"error"},"document":"test1.qs","stack":"Error: program failed: hello\nCall stack:\n    at Test.failing_call in test1.qs:7:9\n    at Test.other in test2.qs:3:9\n    at Test.Main in test1.qs:4:9\n"}],"message":"runtime error: program failed: hello","range":{"end":{"character":20,"line":6},"start":{"character":8,"line":6}},"related":[{"location":{"source":"test1.qs","span":{"end":{"character":20,"line":6},"start":{"character":8,"line":6}}},"message":"explicit fail"}],"severity":"error"},"success":false,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

#[test]
fn test_compile_error_related_spans() {
    let mut output = Vec::new();
    let code = indoc! {r"
            namespace Other { operation DumpMachine() : Unit { } }
            namespace Test {
                open Other;
                import Std.Diagnostics.*;
                @EntryPoint()
                operation Main() : Unit {
                    DumpMachine()
                }
            }
        "
    };
    run_internal(
        SourceMap::new([("test.qs".into(), code.into())], None),
        |s| output.push(s.to_string()),
        1,
    )
    .expect_err("code should fail to compile");
    expect![[r#"{"result":{"code":"Qsc.Resolve.Ambiguous","errors":[{"diagnostic":{"code":"Qsc.Resolve.Ambiguous","message":"name error: `DumpMachine` could refer to the item in `Other` or `Std.Diagnostics`","range":{"end":{"character":19,"line":6},"start":{"character":8,"line":6}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":19,"line":6},"start":{"character":8,"line":6}}},"message":"ambiguous name"},{"location":{"source":"test.qs","span":{"end":{"character":14,"line":2},"start":{"character":9,"line":2}}},"message":"found in this namespace"},{"location":{"source":"test.qs","span":{"end":{"character":26,"line":3},"start":{"character":11,"line":3}}},"message":"and also in this namespace"}],"severity":"error"},"document":"test.qs","stack":null}],"message":"name error: `DumpMachine` could refer to the item in `Other` or `Std.Diagnostics`","range":{"end":{"character":19,"line":6},"start":{"character":8,"line":6}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":19,"line":6},"start":{"character":8,"line":6}}},"message":"ambiguous name"},{"location":{"source":"test.qs","span":{"end":{"character":14,"line":2},"start":{"character":9,"line":2}}},"message":"found in this namespace"},{"location":{"source":"test.qs","span":{"end":{"character":26,"line":3},"start":{"character":11,"line":3}}},"message":"and also in this namespace"}],"severity":"error"},"success":false,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

#[test]
fn test_runtime_error_related_spans() {
    let mut output = Vec::new();
    let code = indoc! {r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    X(q);
                }
            }
        "
    };
    run_internal(
        SourceMap::new([("test.qs".into(), code.into())], None),
        |s| output.push(s.to_string()),
        1,
    )
    .expect("code should compile and run");
    expect![[r#"{"result":{"code":"Qsc.Eval.ReleasedQubitNotZero","errors":[{"diagnostic":{"code":"Qsc.Eval.ReleasedQubitNotZero","message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"document":"test.qs","stack":"Error: Qubit0 released while not in |0⟩ state\nCall stack:\n    at QIR.Runtime.__quantum__rt__qubit_release in qsharp-library-source:core/qir.qs:9:5\n    at Test.Main in test.qs:4:9\n"}],"message":"runtime error: Qubit0 released while not in |0⟩ state\n\nhelp: qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state","range":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}},"related":[{"location":{"source":"test.qs","span":{"end":{"character":24,"line":3},"start":{"character":8,"line":3}}},"message":"Qubit0"}],"severity":"error"},"success":false,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

#[test]
fn test_runtime_error_default_span() {
    let mut output = Vec::new();
    let code = indoc! {r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use qs = Qubit[-1];
                }
            }
        "
    };
    run_internal(
        SourceMap::new([("test.qs".into(), code.into())], None),
        |s| output.push(s.to_string()),
        1,
    )
    .expect("code should compile and run");
    expect![[r#"{"result":{"code":"Qsc.Eval.UserFail","errors":[{"diagnostic":{"code":"Qsc.Eval.UserFail","message":"runtime error: program failed: Cannot allocate qubit array with a negative length","range":{"end":{"character":69,"line":14},"start":{"character":12,"line":14}},"related":[{"location":{"source":"qsharp-library-source:core/qir.qs","span":{"end":{"character":69,"line":14},"start":{"character":12,"line":14}}},"message":"explicit fail"}],"severity":"error"},"document":"qsharp-library-source:core/qir.qs","stack":"Error: program failed: Cannot allocate qubit array with a negative length\nCall stack:\n    at QIR.Runtime.AllocateQubitArray in qsharp-library-source:core/qir.qs:15:13\n    at Test.Main in test.qs:4:9\n"}],"message":"runtime error: program failed: Cannot allocate qubit array with a negative length","range":{"end":{"character":69,"line":14},"start":{"character":12,"line":14}},"related":[{"location":{"source":"qsharp-library-source:core/qir.qs","span":{"end":{"character":69,"line":14},"start":{"character":12,"line":14}}},"message":"explicit fail"}],"severity":"error"},"success":false,"type":"Result"}"#]]
    .assert_eq(&output.join("\n"));
}

#[test]
fn test_doc_gen() {
    let docs = qsc_doc_gen::generate_docs::generate_docs(None, None, None);
    assert!(docs.len() > 100);
    for (name, metadata, contents) in docs {
        // filename will be something like "Std.Canon/ApplyToEachC.md"
        let filename = name.to_string();
        // Text is the full markdown including initial metadata inside '---' blocks
        let text = format!("{metadata}\n\n{contents}");
        if filename.eq("toc.yml") {
            assert!(text.contains("uid: Qdk.Std.Core"));
        } else if !filename.eq("index.md") {
            assert!(
                std::path::Path::new(&filename)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            );
            assert!(
                text.starts_with("---\n"),
                "file {name} does not start with metadata\ncontents: {text}"
            );
        }
    }
}

#[test]
fn code_with_errors_returns_errors() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit()
                let pi_over_two = 4.0 / 2.0;
            }
        }";
    let sources = SourceMap::new([("test.qs".into(), source.into())], None);
    let language_features = LanguageFeatures::default();
    let capabilities = TargetCapabilityFlags::empty();

    let store = PackageStore::new(qsc::compile::core());

    expect![[r#"
        Err(
            "[{\"document\":\"test.qs\",\"diagnostic\":{\"range\":{\"start\":{\"line\":4,\"character\":16},\"end\":{\"line\":4,\"character\":19}},\"message\":\"syntax error: expected `;`, found keyword `let`\",\"severity\":\"error\",\"code\":\"Qsc.Parse.Token\"},\"stack\":null}]",
        )
    "#]]
    .assert_debug_eq(&get_qir_from_qsharp(sources, language_features, capabilities, store, &[]));
}
