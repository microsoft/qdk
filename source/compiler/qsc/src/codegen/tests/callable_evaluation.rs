// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Profile, Value, eval_fragments, format_interpret_errors, interpreter_with_capabilities,
    qir_entry_body,
};
use expect_test::expect;
use qsc_eval::output::CursorReceiver;
use std::fmt::Write;
use std::io::Cursor;

#[test]
fn local_result_array_with_zero_literal_initializes_constant_storage() {
    let source = r#"
        operation Apply(r : Result, q : Qubit) : Unit {
            for value in [r, Zero] {
                if value == One { X(q); }
            }
        }
        operation Main() : Result {
            use (flag, q) = (Qubit(), Qubit());
            X(flag);
            Apply(MResetZ(flag), q);
            MResetZ(q)
        }
    "#;
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, source);
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()"),
        Value::Result(qsc_eval::val::Result::Val(true)),
    );
    let qir = interpreter.qirgen("Main()").expect("mixed Zero array QIR");
    assert!(
        qir.contains(
            "call void @__quantum__rt__write_result(i1 zeroext false, ptr inttoptr (i64 2 to ptr))"
        ),
        "{qir}"
    );
    assert!(
        qir.contains("store ptr inttoptr (i64 2 to ptr), ptr"),
        "{qir}"
    );
}

#[test]
fn local_result_array_with_one_literal_initializes_constant_storage() {
    let source = r#"
        operation Apply(r : Result, q : Qubit) : Unit {
            for value in [One, r] {
                if value == One { X(q); }
            }
        }
        operation Main() : Result {
            use (flag, q) = (Qubit(), Qubit());
            Apply(MResetZ(flag), q);
            MResetZ(q)
        }
    "#;
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, source);
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()"),
        Value::Result(qsc_eval::val::Result::Val(true)),
    );
    let qir = interpreter.qirgen("Main()").expect("mixed One array QIR");
    assert!(
        qir.contains(
            "call void @__quantum__rt__write_result(i1 zeroext true, ptr inttoptr (i64 3 to ptr))"
        ),
        "{qir}"
    );
    assert!(
        qir.contains("store ptr inttoptr (i64 3 to ptr), ptr"),
        "{qir}"
    );
}

#[test]
fn helper_local_qubit_array_loop_emits_initialized_index_to_gate_qir() {
    let source = r#"
        operation Apply(q : Qubit) : Unit {
            for target in [q] { X(target); }
        }
        operation Main() : Result {
            use q = Qubit();
            Apply(q);
            MResetZ(q)
        }
    "#;
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, source);
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()"),
        Value::Result(qsc_eval::val::Result::Val(true)),
    );
    let qir = interpreter.qirgen("Main()").expect("local qubit array QIR");
    expect![[r#"
        block_1:
          %var_1 = alloca ptr
          %var_2 = alloca i64
          %var_5 = alloca [1 x ptr]
          store ptr %var_0, ptr %var_1
          store i64 0, ptr %var_2
          br label %block_2
        block_2:
          %var_3 = load i64, ptr %var_2
          %var_4 = icmp slt i64 %var_3, 1
          br i1 %var_4, label %block_3, label %block_4
        block_3:
          %var_13 = load ptr, ptr %var_1
          %var_5_0 = getelementptr [1 x ptr], ptr %var_5, i64 0, i64 0
          store ptr %var_13, ptr %var_5_0
          %var_15 = load i64, ptr %var_2
          %var_16_offset_chk = icmp slt i64 %var_15, 0
          %var_16_offset = select i1 %var_16_offset_chk, i64 1, i64 0
          %var_16 = getelementptr [1 x ptr], ptr %var_5, i64 %var_16_offset, i64 %var_15
          %var_6 = load ptr, ptr %var_16
          call void @X(ptr %var_6)
          %var_9 = load i64, ptr %var_2
          %var_10 = add i64 %var_9, 1
          store i64 %var_10, ptr %var_2
          br label %block_2
        block_4:
          ret void"#]]
    .assert_eq(qir_internal_void_body(&qir, "Apply"));
}

#[test]
fn local_array_access_after_skipped_branch_initializes_its_index_storage() {
    let source = r#"
        operation Apply(first : Qubit, second : Qubit, index : Int, firstPass : Bool) : Unit {
            let targets = [first, second];
            if firstPass { X(targets[index]); }
            X(targets[index]);
        }
        operation Main() : (Result, Result) {
            use first = Qubit();
            use second = Qubit();
            Apply(first, second, 1, false);
            (MResetZ(first), MResetZ(second))
        }
    "#;
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, source);
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()"),
        Value::Tuple(
            vec![
                Value::Result(qsc_eval::val::Result::Val(false)),
                Value::Result(qsc_eval::val::Result::Val(true)),
            ]
            .into(),
            None,
        ),
    );
    let qir = interpreter
        .qirgen("Main()")
        .expect("branch-local array QIR");
    expect![[r#"
        block_1:
          %var_4 = alloca ptr
          %var_5 = alloca ptr
          %var_6 = alloca [2 x ptr]
          %var_9 = alloca [2 x ptr]
          store ptr %var_0, ptr %var_4
          store ptr %var_1, ptr %var_5
          br i1 %var_3, label %block_2, label %block_3
        block_2:
          %var_17 = load ptr, ptr %var_4
          %var_18 = load ptr, ptr %var_5
          %var_6_0 = getelementptr [2 x ptr], ptr %var_6, i64 0, i64 0
          store ptr %var_17, ptr %var_6_0
          %var_6_1 = getelementptr [2 x ptr], ptr %var_6, i64 0, i64 1
          store ptr %var_18, ptr %var_6_1
          %var_20_offset_chk = icmp slt i64 %var_2, 0
          %var_20_offset = select i1 %var_20_offset_chk, i64 1, i64 0
          %var_20 = getelementptr [2 x ptr], ptr %var_6, i64 %var_20_offset, i64 %var_2
          %var_7 = load ptr, ptr %var_20
          call void @X(ptr %var_7)
          br label %block_3
        block_3:
          %var_13 = load ptr, ptr %var_4
          %var_14 = load ptr, ptr %var_5
          %var_9_0 = getelementptr [2 x ptr], ptr %var_9, i64 0, i64 0
          store ptr %var_13, ptr %var_9_0
          %var_9_1 = getelementptr [2 x ptr], ptr %var_9, i64 0, i64 1
          store ptr %var_14, ptr %var_9_1
          %var_16_offset_chk = icmp slt i64 %var_2, 0
          %var_16_offset = select i1 %var_16_offset_chk, i64 1, i64 0
          %var_16 = getelementptr [2 x ptr], ptr %var_9, i64 %var_16_offset, i64 %var_2
          %var_10 = load ptr, ptr %var_16
          call void @X(ptr %var_10)
          ret void"#]]
    .assert_eq(qir_internal_void_body(&qir, "Apply"));
}

fn qir_internal_void_body<'a>(qir: &'a str, name: &str) -> &'a str {
    let signature = format!("define internal void @{name}(");
    qir.split_once(signature.as_str())
        .expect("internal void callable should exist")
        .1
        .split_once("{\n")
        .expect("callable body should exist")
        .1
        .split_once("\n}")
        .expect("callable body should terminate")
        .0
}

fn check_controlled_phase_capture(body: &str) {
    let source = format!(
        r#"
        operation Apply(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {{
            op(q);
        }}
        function Make(op : Qubit => Unit is Adj + Ctl) : Qubit => Unit is Adj + Ctl {{
            Apply(op, _)
        }}
        {body}
    "#
    );
    for profile in [
        Profile::Base,
        Profile::AdaptiveRI,
        Profile::AdaptiveRIF,
        Profile::Adaptive,
    ] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, &source);
        let target = eval_fragments(&mut interpreter, "Run");
        let captured = eval_fragments(&mut interpreter, "Make(Z)");
        assert!(matches!(&captured, Value::Closure(closure) if closure.fixed_args.len() == 1));
        let mut output = Cursor::new(Vec::new());
        assert_eq!(
            interpreter
                .invoke(
                    &mut CursorReceiver::new(&mut output),
                    target.clone(),
                    captured.clone()
                )
                .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors))),
            Value::Result(qsc_eval::val::Result::Val(false)),
            "{profile:?}",
        );
        let actual = interpreter
            .qirgen_from_callable(&target, captured)
            .expect("runtime captured phase QIR");
        let expected = interpreter
            .qirgen("Expected()")
            .expect("direct controlled-Z QIR");
        assert_eq!(actual, expected, "{profile:?}");
    }
}

#[test]
fn inactive_control_does_not_apply_captured_phase_operation() {
    check_controlled_phase_capture(
        r#"
        operation Run(f : Qubit => Unit is Adj + Ctl) : Result {
            use (control, target) = (Qubit(), Qubit());
            H(target);
            Controlled f([control], target);
            H(target);
            MResetZ(target)
        }
        operation Expected() : Result {
            use (control, target) = (Qubit(), Qubit());
            H(target);
            Controlled Z([control], target);
            H(target);
            MResetZ(target)
        }
    "#,
    );
}

#[test]
fn coherent_control_preserves_captured_phase_against_independent_uncompute() {
    check_controlled_phase_capture(
        r#"
        operation Run(f : Qubit => Unit is Adj + Ctl) : Result {
            use (control, target) = (Qubit(), Qubit());
            H(control);
            X(target);
            Controlled f([control], target);
            Controlled Z([control], target);
            H(control);
            X(target);
            MResetZ(control)
        }
        operation Expected() : Result {
            use (control, target) = (Qubit(), Qubit());
            H(control);
            X(target);
            Controlled Z([control], target);
            Controlled Z([control], target);
            H(control);
            X(target);
            MResetZ(control)
        }
    "#,
    );
}

#[test]
fn effectful_call_preserves_constant_allocation_size_and_runs_once() {
    let source = r#"
        operation Prep(q : Qubit) : Int {
            X(q);
            3
        }
        operation Main() : (Int, Result[]) {
            use q = Qubit();
            let n = Prep(q);
            use qs = Qubit[n];
            for x in qs { X(x); }
            let rs = MResetEachZ(qs);
            (n + (MResetZ(q) == One ? 10 | 0), rs)
        }
        operation Expected() : (Int, Result[]) {
            use q = Qubit();
            Prep(q);
            use qs = Qubit[3];
            for x in qs { X(x); }
            let rs = MResetEachZ(qs);
            (3 + (MResetZ(q) == One ? 10 | 0), rs)
        }
    "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let mut interpreter = interpreter_with_capabilities(profile.into());
        eval_fragments(&mut interpreter, source);
        assert_eq!(
            eval_fragments(&mut interpreter, "Main()"),
            Value::Tuple(
                vec![
                    Value::Int(13),
                    Value::Array(vec![Value::Result(qsc_eval::val::Result::Val(true)); 3].into()),
                ]
                .into(),
                None,
            ),
        );
        let expected = interpreter.qirgen("Expected()").expect("literal-size QIR");
        let qir = interpreter.qirgen("Main()").expect("call-result-size QIR");
        assert_eq!(qir, expected, "{profile:?}");
        if profile == Profile::Adaptive {
            assert_eq!(qir.matches("call i64 @Prep(").count(), 1, "{qir}");
            assert!(qir.contains("ret i64 3"), "{qir}");
        }
        let target = eval_fragments(&mut interpreter, "Main");
        assert_eq!(
            interpreter
                .qirgen_from_callable(&target, Value::unit())
                .expect("runtime call-result-size QIR"),
            expected,
            "{profile:?}",
        );
    }
}

#[test]
fn effectful_bool_call_keeps_shared_body_and_known_true_output() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(
        &mut interpreter,
        r#"
        operation Flag(q : Qubit) : Bool { X(q); true }
        operation Main() : (Bool, Result) {
            use q = Qubit();
            let flag = Flag(q);
            (flag, MResetZ(q))
        }
        "#,
    );
    let qir = interpreter.qirgen("Main()").expect("known Bool QIR");
    assert_eq!(qir.matches("call zeroext i1 @Flag(").count(), 1, "{qir}");
    assert_eq!(qir.matches("call void @X(").count(), 1, "{qir}");
    assert!(qir.contains("ret i1 true"), "{qir}");
    assert!(qir.contains("bool_record_output(i1 zeroext true,"), "{qir}");
    let target = eval_fragments(&mut interpreter, "Main");
    assert_eq!(
        interpreter
            .qirgen_from_callable(&target, Value::unit())
            .expect("runtime QIR"),
        qir
    );
}

#[test]
fn effectful_double_call_keeps_shared_body_and_known_output() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(
        &mut interpreter,
        r#"
        operation Angle(q : Qubit) : Double { X(q); 1.5 }
        operation Main() : (Double, Result) {
            use q = Qubit();
            let angle = Angle(q);
            (angle, MResetZ(q))
        }
        "#,
    );
    let qir = interpreter.qirgen("Main()").expect("known Double QIR");
    assert_eq!(qir.matches("call double @Angle(").count(), 1, "{qir}");
    assert_eq!(qir.matches("call void @X(").count(), 1, "{qir}");
    assert!(qir.contains("ret double 1.5"), "{qir}");
    assert!(qir.contains("double_record_output(double 1.5,"), "{qir}");
    let target = eval_fragments(&mut interpreter, "Main");
    assert_eq!(
        interpreter
            .qirgen_from_callable(&target, Value::unit())
            .expect("runtime QIR"),
        qir
    );
}

#[test]
fn repeated_constant_returning_calls_share_effects_without_expanding_the_call_graph() {
    let mut source = String::from("operation Level0(q : Qubit) : Int { X(q); 3 }\n");
    for level in 1..=8 {
        let previous = level - 1;
        writeln!(
            source,
            "operation Level{level}(q : Qubit) : Int {{ Level{previous}(q); Level{previous}(q); 3 }}"
        )
        .expect("writing to a String succeeds");
    }
    source.push_str(
        "operation Main() : (Int, Result) { use q = Qubit(); let n = Level8(q); (n, MResetZ(q)) }",
    );
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, &source);
    let qir = interpreter.qirgen("Main()").expect("shared call graph QIR");
    assert_eq!(qir.matches("call void @X(").count(), 1, "{qir}");
    for level in 0..8 {
        assert_eq!(
            qir.matches(&format!("call i64 @Level{level}(")).count(),
            2,
            "{qir}",
        );
    }
    assert!(qir.contains("int_record_output(i64 3,"), "{qir}");
    let target = eval_fragments(&mut interpreter, "Main");
    assert_eq!(
        interpreter
            .qirgen_from_callable(&target, Value::unit())
            .expect("runtime QIR"),
        qir
    );
}

#[test]
fn argument_dependent_results_remain_distinct_across_effectful_calls() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(
        &mut interpreter,
        r#"
        operation Echo(value : Int, q : Qubit) : Int { X(q); value }
        operation Main() : (Int, Int, Result) {
            use q = Qubit();
            let first = Echo(2, q);
            let second = Echo(5, q);
            (first, second, MResetZ(q))
        }
        "#,
    );
    let qir = interpreter
        .qirgen("Main()")
        .expect("argument-dependent QIR");
    assert!(qir.contains("int_record_output(i64 2,"), "{qir}");
    assert!(qir.contains("int_record_output(i64 5,"), "{qir}");
    assert_eq!(qir.matches("call void @X(").count(), 2, "{qir}");
    assert!(!qir.contains("@Echo("), "{qir}");
    let target = eval_fragments(&mut interpreter, "Main");
    assert_eq!(
        interpreter
            .qirgen_from_callable(&target, Value::unit())
            .expect("runtime QIR"),
        qir
    );
}

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
        let expected_entry = if profile == Profile::Adaptive {
            expect![[r#"
                block_0:
                  %var_0 = alloca i1
                  %var_5 = alloca i1
                  %var_7 = alloca i64
                  call void @__quantum__rt__initialize(ptr null)
                  store i1 false, ptr %var_0
                  call void @X(ptr inttoptr (i64 0 to ptr))
                  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                  %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
                  store i1 %var_2, ptr %var_0
                  %var_4 = load i1, ptr %var_0
                  store i1 false, ptr %var_5
                  br i1 %var_4, label %block_1, label %block_2
                block_1:
                  store i1 true, ptr %var_5
                  br label %block_2
                block_2:
                  %var_12 = load i1, ptr %var_0
                  br i1 %var_12, label %block_3, label %block_4
                block_3:
                  store i64 6, ptr %var_7
                  br label %block_5
                block_4:
                  store i64 4, ptr %var_7
                  br label %block_5
                block_5:
                  %var_8 = load i64, ptr %var_7
                  call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
                  ret i64 0"#]]
        } else {
            expect![[r#"
                block_0:
                  call void @__quantum__rt__initialize(i8* null)
                  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
                  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
                  %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
                  br i1 %var_1, label %block_1, label %block_2
                block_1:
                  br label %block_2
                block_2:
                  %var_8 = phi i1 [false, %block_0], [true, %block_1]
                  br i1 %var_1, label %block_3, label %block_4
                block_3:
                  br label %block_5
                block_4:
                  br label %block_5
                block_5:
                  %var_9 = phi i64 [6, %block_3], [4, %block_4]
                  call void @__quantum__rt__int_record_output(i64 %var_9, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
                  ret i64 0"#]]
        };
        expected_entry.assert_eq(qir_entry_body(&source_qir));
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
        let runtime_qir = interpreter
            .qirgen_from_callable(&target, missing)
            .expect("body-only type arguments are retained by the saved closure");
        let body_only_source = interpreter
            .qirgen("Run(Missing(1))")
            .expect("body-only source instantiation");
        assert_eq!(runtime_qir, body_only_source);
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
