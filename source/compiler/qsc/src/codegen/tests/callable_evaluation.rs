// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Profile, Value, eval_fragments, format_interpret_errors, interpreter_with_capabilities,
    qir_entry_body,
};

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
            use q = Qubit(); X(q); H(q); H(q); MResetZ(q)
        }
        operation Reverse() : Result {
            use q = Qubit(); H(q); H(q); X(q); MResetZ(q)
        }
        operation Flip() : Result {
            use q = Qubit(); I(q); I(q); X(q); I(q); MResetZ(q)
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
            // Adaptive helper parameter IDs can differ; entry calls must preserve gate order.
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
