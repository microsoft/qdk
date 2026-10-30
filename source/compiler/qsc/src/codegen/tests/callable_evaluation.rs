// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Profile, Value, eval_fragments, format_interpret_errors, interpreter_with_capabilities,
    qir_entry_body,
};
use expect_test::expect;

#[test]
fn loop_condition_tuple_shift_preserves_measurement_selected_output() {
    let source = r#"
        operation Main() : Int {
            use q = Qubit();
            X(q);
            let measured = MResetZ(q) == One;
            mutable (i, a, b, c, d) = (0, false, false, false, false);
            while {
                set (i, a, b, c, d) = (i + 1, b, c, d, measured);
                i < 5
            } {}
            mutable output = 0;
            if a { set output = 5; }
            output
        }
    "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        assert_eq!(eval_fragments(&mut interpreter, "Main()"), Value::Int(5));
        let source_qir = interpreter.qirgen("Main()").expect("source QIR");
        let target = eval_fragments(&mut interpreter, "Main");
        let runtime_qir = interpreter
            .qirgen_from_callable(&target, Value::unit())
            .expect("runtime QIR");
        assert_eq!(source_qir, runtime_qir, "{profile:?}");
        if profile != Profile::Adaptive {
            expect![[r#"
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
                  br i1 %var_0, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  %var_24 = phi i64 [0, %block_0], [5, %block_1]
                  call void @__quantum__rt__int_record_output(i64 %var_24, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0"#]]
            .assert_eq(qir_entry_body(&source_qir));
        }
    }
}

#[test]
fn loop_carried_result_selection_requires_dynamic_result_support() {
    let source = r#"
        namespace Test {
            operation Main() : (Result, Int) {
                use (q1, q2) = (Qubit(), Qubit());
                X(q2);
                let m1 = M(q1);
                let m2 = M(q2);
                mutable (r0, r1, r2, r3, r4, r5) = (Zero, Zero, Zero, Zero, Zero, Zero);
                mutable (i, b, s) = (0, false, Zero);
                while i < 7 {
                    set s = b ? m1 | m2;
                    set (r0, r1, r2, r3, r4, r5) = (r1, r2, r3, r4, r5, s);
                    set b = r0 == One;
                    set i += 1;
                }
                mutable output = 0;
                if r0 == One { set output = 5; }
                ResetAll([q1, q2]);
                (r0, output)
            }
        }
    "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF] {
        let errors = super::compile_source_to_qir_result(source, profile.into())
            .expect_err("measurement-dependent Result selection requires dynamic Result support");
        assert!(!errors.is_empty());
        for error in errors {
            assert!(
                format!("{error:?}").contains("UseOfDynamicResult"),
                "{error:?}"
            );
        }
    }
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, source);
    assert_eq!(
        eval_fragments(&mut interpreter, "Test.Main()"),
        Value::Tuple(
            vec![
                Value::Result(qsc_eval::val::Result::Val(true)),
                Value::Int(5)
            ]
            .into(),
            None
        )
    );
    interpreter
        .qirgen("Test.Main()")
        .expect("Adaptive source QIR");
    let target = eval_fragments(&mut interpreter, "Test.Main");
    interpreter
        .qirgen_from_callable(&target, Value::unit())
        .expect("Adaptive runtime QIR");
}

#[test]
fn unwrapped_callable_preserves_its_operation() {
    let source = r#"
        newtype Fn = (Op : Qubit => Unit);
        operation Main() : Result {
            use q = Qubit();
            let f = Fn(X);
            (f!)(q);
            MResetZ(q)
        }
        operation Expected() : Result {
            use q = Qubit();
            X(q);
            MResetZ(q)
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Result(qsc_eval::val::Result::Val(true))
        );
        let expected = interpreter.qirgen("Expected()").expect("direct X QIR");
        assert_eq!(
            interpreter.qirgen("Main()").expect("unwrapped X QIR"),
            expected
        );
        let target = eval_fragments(&mut interpreter, "Main");
        assert_eq!(
            interpreter
                .qirgen_from_callable(&target, Value::unit())
                .expect("runtime QIR"),
            expected
        );
    }
}

#[test]
fn skipped_short_circuit_return_compiles_to_true() {
    let source = r#"
        namespace Test {
            operation Main() : Bool {
                use q = Qubit();
                let r = MResetZ(q) == One;
                mutable value = true;
                set value or= { Message("rhs"); r and (return true) };
                value
            }
        }
    "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let qir = super::compile_source_to_qir(source, profile.into());
        assert!(
            qir.contains("@__quantum__rt__bool_record_output(i1 zeroext true,"),
            "{profile:?}: {qir}"
        );
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Test.Main()"),
            Value::Bool(true)
        );
        let target = eval_fragments(&mut interpreter, "Test.Main");
        interpreter
            .qirgen_from_callable(&target, Value::unit())
            .expect("runtime QIR for skipped return");
        interpreter
            .qirgen("Test.Main()")
            .expect("source QIR for skipped return");
    }
}

#[test]
fn measured_short_circuit_callable_reassignment_compiles_on_restricted_profiles() {
    let source = r#"
        function Inc(n : Int) : Int { n + 1 }
        function Twice(n : Int) : Int { 2 * n }
        operation Main() : Int {
            use q = Qubit();
            X(q);
            mutable f = Inc;
            let unused = (MResetZ(q) == One) and { set f = Twice; true };
            f(3)
        }
    "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        assert_eq!(eval_fragments(&mut interpreter, "Main()"), Value::Int(6));
        let source_qir = interpreter.qirgen("Main()").expect("source QIR");
        let target = eval_fragments(&mut interpreter, "Main");
        let runtime_qir = interpreter
            .qirgen_from_callable(&target, Value::unit())
            .expect("runtime QIR");
        assert_eq!(source_qir, runtime_qir, "{profile:?}");
    }
}

#[test]
fn runtime_saved_generic_closure_instantiates_capture_types() {
    let source = r#"
        operation Phase(n : Int, q : Qubit) : Unit is Adj + Ctl {
            for _ in 1..n { S(q); }
        }
        function Save<'T>(read : ('T, Qubit) => Unit is Adj + Ctl, value : 'T)
            : Qubit => Unit is Adj + Ctl { read(value, _) }
        function SaveInt(read : (Int, Qubit) => Unit is Adj + Ctl, value : Int)
            : Qubit => Unit is Adj + Ctl { read(value, _) }
        function SaveSecond<'Unused, 'T>(
            unused : 'Unused, read : ('T, Qubit) => Unit is Adj + Ctl, value : 'T
        ) : Qubit => Unit is Adj + Ctl { read(value, _) }
        function Missing<'T>(unused : 'T) : Qubit => Unit is Adj + Ctl {
            q => { let values : 'T[] = []; if Length(values) == 0 { S(q); } }
        }
        operation Run(f : Qubit => Unit is Adj + Ctl) : Result {
            use q = Qubit();
            H(q);
            S(q);
            Adjoint f(q);
            H(q);
            MResetZ(q)
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let expected = interpreter
            .qirgen("Run(SaveInt(Phase, 1))")
            .expect("concrete source QIR");
        let generic_source = interpreter
            .qirgen("Run(Save(Phase, 1))")
            .expect("generic source QIR");
        assert_eq!(qir_entry_body(&generic_source), qir_entry_body(&expected));
        for argument in [
            "SaveInt(Phase, 1)",
            "Save(Phase, 1)",
            "SaveSecond((), Phase, 1)",
        ] {
            let target = eval_fragments(&mut interpreter, "Run");
            let value = eval_fragments(&mut interpreter, argument);
            let runtime_qir = interpreter
                .qirgen_from_callable(&target, value)
                .expect("saved closure QIR");
            assert_eq!(
                qir_entry_body(&runtime_qir),
                qir_entry_body(&expected),
                "{profile:?}: {argument}"
            );
        }
        let target = eval_fragments(&mut interpreter, "Run");
        let missing = eval_fragments(&mut interpreter, "Missing(1)");
        let errors = interpreter
            .qirgen_from_callable(&target, missing)
            .expect_err("a body-only type parameter has no runtime type evidence");
        assert!(
            format_interpret_errors(errors).contains("MissingTypeArguments"),
            "{profile:?}: report missing evidence instead of reaching monomorphization"
        );
    }
}

#[test]
fn recursive_projected_factory_preserves_closure_input() {
    let source = r#"
        struct C { F : Int -> Bool }
        function Pack(c : C, n : Int) : (C, Int) { (c, n) }
        function UseS(c : C, n : Int) : Bool {
            if n > 0 { UseS(Pack(c, n - 1)) } else { c.F(1) }
        }

        operation Run() : Result {
            use q = Qubit();
            if UseS(new C { F = x -> x == 1 }, 2) { X(q); }
            MResetZ(q)
        }
        operation Expected() : Result {
            use q = Qubit();
            X(q);
            MResetZ(q)
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let expected = interpreter.qirgen("Expected()").expect("reference QIR");
        let source_qir = interpreter.qirgen("Run()").expect("source QIR");
        let target = eval_fragments(&mut interpreter, "Run");
        let runtime_qir = interpreter
            .qirgen_from_callable(&target, Value::Tuple(Vec::new().into(), None))
            .expect("runtime entry QIR");
        assert_eq!(source_qir, expected, "{profile:?}: source closure input");
        assert_eq!(runtime_qir, expected, "{profile:?}: runtime closure input");
    }
}

#[test]
fn nested_callable_arrays_preserve_different_length_environments() {
    let source = r#"
        operation ApplyAll(ops : (Qubit => Unit)[], q : Qubit) : Unit {
            for op in ops { op(q); }
        }
        function MakeAll(ops : (Qubit => Unit)[]) : Qubit => Unit { ApplyAll(ops, _) }
        operation Run(f : Qubit => Unit) : Result {
            use q = Qubit();
            f(q);
            MResetZ(q)
        }
        operation Forward() : Result {
            use q = Qubit();
            let first : (Qubit => Unit)[] = [X];
            let second : (Qubit => Unit)[] = [H, H];
            ApplyAll(first, q); ApplyAll(second, q);
            MResetZ(q)
        }
        operation Reverse() : Result {
            use q = Qubit();
            let first : (Qubit => Unit)[] = [H, H];
            let second : (Qubit => Unit)[] = [X];
            ApplyAll(first, q); ApplyAll(second, q);
            MResetZ(q)
        }
        operation Flip() : Result {
            use q = Qubit();
            let first : (Qubit => Unit)[] = [I, I, X];
            let second : (Qubit => Unit)[] = [I];
            ApplyAll(first, q); ApplyAll(second, q);
            MResetZ(q)
        }
    "#;
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let target = eval_fragments(&mut interpreter, "Run");
        for (argument, reference) in [
            ("MakeAll([MakeAll([X]), MakeAll([H, H])])", "Forward()"),
            ("MakeAll([MakeAll([H, H]), MakeAll([X])])", "Reverse()"),
            ("MakeAll([MakeAll([I, I, X]), MakeAll([I])])", "Flip()"),
        ] {
            let expected = interpreter.qirgen(reference).expect("reference QIR");
            let source_qir = interpreter
                .qirgen(&format!("Run({argument})"))
                .expect("source QIR");
            let value = eval_fragments(&mut interpreter, argument);
            let runtime_qir = interpreter
                .qirgen_from_callable(&target, value)
                .expect("runtime argument QIR");
            // Adaptive helper parameter IDs can differ; entry calls must preserve group order.
            assert_eq!(
                qir_entry_body(&source_qir),
                qir_entry_body(&expected),
                "{profile:?}, {argument}: source"
            );
            assert_eq!(
                qir_entry_body(&runtime_qir),
                qir_entry_body(&expected),
                "{profile:?}, {argument}: runtime"
            );
        }
    }
}

#[test]
fn nested_function_arrays_preserve_different_length_environments() {
    let source = r#"
        function Inc(x : Int) : Int { x + 1 }
        function Double(x : Int) : Int { 2 * x }
        function ApplyAll(fs : (Int -> Int)[], x : Int) : Int {
            mutable result = x;
            for f in fs { set result = f(result); }
            result
        }
        function MakeAll(fs : (Int -> Int)[]) : Int -> Int { ApplyAll(fs, _) }
        operation Run(f : Int -> Int) : Int { f(1) }
    "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let target = eval_fragments(&mut interpreter, "Run");
        for (argument, expected) in [
            ("MakeAll([MakeAll([Inc]), MakeAll([Double, Double])])", 8),
            ("MakeAll([MakeAll([Double, Double]), MakeAll([Inc])])", 5),
            ("MakeAll([MakeAll([Inc, Inc, Inc]), MakeAll([Double])])", 8),
        ] {
            let source_qir = interpreter
                .qirgen(&format!("Run({argument})"))
                .expect("source QIR");
            let value = eval_fragments(&mut interpreter, argument);
            let runtime_qir = interpreter
                .qirgen_from_callable(&target, value)
                .expect("runtime argument QIR");
            let output = format!("__quantum__rt__int_record_output(i64 {expected},");
            assert!(
                source_qir.contains(&output),
                "{profile:?}, {argument}: source"
            );
            assert!(
                runtime_qir.contains(&output),
                "{profile:?}, {argument}: runtime"
            );
        }
    }
}

#[test]
fn closure_factory_preserves_mutable_qubit_capture_time() {
    for (factory, capture, invocation, adaptive_only) in [
        ("() => X(q)", "MakeFlip(mq)", "f()", false),
        ("Flip(q, _)", "MakeFlip(mq)", "f()", false),
        ("let qs = [q]; () => X(qs[0])", "MakeFlip(mq)", "f()", false),
        (
            "() => X(q)",
            "new Holder { F = MakeFlip(mq) }",
            "f.F()",
            false,
        ),
        ("() => X(q)", "[MakeFlip(mq)]", "f[0]()", true),
    ] {
        let snapshot = capture.replace("mq", "saved");
        let source = format!(
            r#"
            struct Holder {{ F : Unit => Unit }}
            operation Flip(q : Qubit, value : Unit) : Unit {{ X(q); }}
            function MakeFlip(q : Qubit) : Unit => Unit {{ {factory} }}
            operation Run() : Result[] {{
                use qq = Qubit[2];
                mutable mq = qq[0];
                let f = {capture};
                set mq = qq[1];
                {invocation};
                MResetEachZ(qq)
            }}
            operation Expected() : Result[] {{
                use qq = Qubit[2];
                mutable mq = qq[0];
                let saved = mq;
                let f = {snapshot};
                set mq = qq[1];
                {invocation};
                MResetEachZ(qq)
            }}
            "#
        );
        for profile in [
            Profile::Adaptive,
            Profile::Base,
            Profile::AdaptiveRI,
            Profile::AdaptiveRIF,
        ] {
            if adaptive_only && profile != Profile::Adaptive {
                continue;
            }
            let mut interpreter = interpreter_with_capabilities(profile.into());
            eval_fragments(&mut interpreter, &source);
            let expected = interpreter.qirgen("Expected()").expect("snapshot control");
            match interpreter.qirgen("Run()") {
                Ok(actual) => assert_eq!(
                    qir_entry_body(&actual),
                    qir_entry_body(&expected),
                    "{profile:?}, {factory}, {capture}: capture-time qubit"
                ),
                Err(errors) => {
                    assert_ne!(profile, Profile::Adaptive, "{errors:?}");
                    assert!(
                        format_interpret_errors(errors).contains("UseOfDynamicQubit"),
                        "{profile:?}, {factory}: only the existing qubit capability rejection is allowed"
                    );
                }
            }
        }
    }
}

#[test]
fn runtime_nested_adjoint_closures_preserve_within_apply() {
    let source = r#"
        operation ApplyAll(ops : (Qubit[] => Unit is Adj)[], qs : Qubit[]) : Unit is Adj {
            for op in ops { op(qs); }
        }
        operation Once(op : Qubit[] => Unit is Adj, qs : Qubit[]) : Unit is Adj { op(qs); }
        operation Flip(i : Int, qs : Qubit[]) : Unit is Adj { X(qs[i]); }
        operation FlipFirst(qs : Qubit[]) : Unit is Adj { X(qs[0]); }
        function MakeAll(ops : (Qubit[] => Unit is Adj)[]) : Qubit[] => Unit is Adj {
            ApplyAll(ops, _)
        }
        function MakeOnce(op : Qubit[] => Unit is Adj) : Qubit[] => Unit is Adj { Once(op, _) }
        function MakeFlip(i : Int) : Qubit[] => Unit is Adj { Flip(i, _) }
        operation Run(f : Qubit[] => Unit is Adj) : Result[] {
            use qs = Qubit[2];
            within { Adjoint f(qs); } apply { CNOT(qs[0], qs[1]); }
            MResetEachZ(qs)
        }
    "#;
    let argument = "MakeOnce(MakeAll([MakeAll([FlipFirst]), MakeAll([MakeFlip(1)])]))";
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let expected = interpreter
            .qirgen(&format!("Run({argument})"))
            .expect("source entry QIR");
        let target = eval_fragments(&mut interpreter, "Run");
        let value = eval_fragments(&mut interpreter, argument);
        let actual = interpreter
            .qirgen_from_callable(&target, value)
            .expect("runtime nested closure QIR");
        assert_eq!(
            qir_entry_body(&actual),
            qir_entry_body(&expected),
            "{profile:?}: nested adjoint closure"
        );
    }
}

#[test]
fn runtime_nested_closures_preserve_callable_and_integer_captures() {
    let source = r#"
        struct Wrapper { Op : Qubit[] => Unit is Adj }
        operation ApplyAll(ops : (Qubit[] => Unit is Adj)[], qs : Qubit[]) : Unit is Adj {
            for op in ops { op(qs); }
        }
        operation Flip(i : Int, qs : Qubit[]) : Unit is Adj { X(qs[i]); }
        operation FlipSecond(qs : Qubit[]) : Unit is Adj { X(qs[1]); }
        operation Rep(op : Qubit[] => Unit is Adj, count : Int, qs : Qubit[]) : Unit is Adj {
            for _ in 1..count { op(qs); }
        }
        function MakeAll(ops : (Qubit[] => Unit is Adj)[]) : Qubit[] => Unit is Adj {
            ApplyAll(ops, _)
        }
        function MakeFlip(i : Int) : Qubit[] => Unit is Adj { Flip(i, _) }
        function MakeRep(op : Qubit[] => Unit is Adj, count : Int) : Qubit[] => Unit is Adj {
            Rep(op, count, _)
        }
        function Wrap(op : Qubit[] => Unit is Adj) : Wrapper { new Wrapper { Op = op } }
        operation RunWrapped(w : Wrapper) : Result[] {
            let f = w.Op;
            use qs = Qubit[2];
            f(qs);
            MResetEachZ(qs)
        }
    "#;
    let argument = "Wrap(MakeAll([MakeRep(MakeFlip(0), 1), MakeRep(FlipSecond, 2)]))";
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        let expected = interpreter
            .qirgen(&format!("RunWrapped({argument})"))
            .expect("source entry QIR");
        let target = eval_fragments(&mut interpreter, "RunWrapped");
        let value = eval_fragments(&mut interpreter, argument);
        let actual = interpreter
            .qirgen_from_callable(&target, value)
            .expect("runtime mixed capture QIR");
        assert_eq!(
            qir_entry_body(&actual),
            qir_entry_body(&expected),
            "{profile:?}: callable and integer captures"
        );
    }
}
