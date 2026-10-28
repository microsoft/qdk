// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for QIR "IR function" emission in the partial evaluator. Eligible
//! specializations returning Unit or a scalar (Int/Double/Bool) are emitted as
//! `Regular` RIR callables with bodies and called via `Instruction::Call` instead of being inlined.
//! Scalar-returning callables bind a fresh call-site output variable. Every ineligible callable
//! (including unresolved bodies, callable-valued inputs and non-scalar returns) continues to inline.

use super::{
    assert_blocks, assert_callable, get_rir_program, get_rir_program_with_adaptive_profile,
    get_rir_program_with_dynamic_qubit_allocation,
};
use expect_test::expect;
use qsc_rir::rir::{CallableId, CallableType, Program};

/// Returns the names of the bodied `Regular` callables that were emitted as IR functions, i.e. all
/// bodied `Regular` callables except the entry point. The result is sorted for stable assertions.
fn ir_function_names(program: &Program) -> Vec<String> {
    let mut names: Vec<String> = program
        .callables
        .iter()
        .filter(|(id, callable)| {
            *id != program.entry
                && matches!(callable.call_type, CallableType::Regular)
                && callable.body.is_some()
        })
        .map(|(_, callable)| callable.name.clone())
        .collect();
    names.sort();
    names
}

fn assert_ir_function_names(program: &Program, expected: &expect_test::Expect) {
    expected.assert_eq(&format!("{:#?}", ir_function_names(program)));
}

#[test]
fn finite_recursion_in_measurement_branch_keeps_repeated_array_length_known() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Rec(n : Int, target : Qubit) : Unit {
                { use q = Qubit(); X(q); X(q); }
                if n > 0 {
                    if MResetZ(target) == Zero { Rec(n - 1, target); }
                }
                let values = [true, size = n];
                if Length(values) == 2 { X(target); }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                Rec(2, target);
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 2);
    assert_eq!(program.num_results, 3);
    assert_eq!(ir_function_names(&program), ["X"]);
}

#[test]
fn finite_recursion_keeps_repeated_array_length_known() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Rec(n : Int, target : Qubit) : Unit {
                { use q = Qubit(); X(q); X(q); }
                let values = [true, size = n];
                if Length(values) == 2 { X(target); }
                if n > 0 { Rec(n - 1, target); }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                Rec(2, target);
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 2);
    assert_eq!(ir_function_names(&program), ["X"]);
}

#[test]
fn finite_recursion_keeps_range_bounds_known() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Rec(n : Int, target : Qubit) : Unit {
                { use q = Qubit(); X(q); X(q); }
                for _ in 1..n { X(target); }
                if n > 0 { Rec(n - 1, target); }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                Rec(2, target);
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 2);
    assert_eq!(ir_function_names(&program), ["X"]);
}

#[test]
fn finite_recursion_does_not_evaluate_unreachable_fail() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Rec(n : Int, target : Qubit) : Unit {
                { use q = Qubit(); X(q); X(q); }
                if n < 0 { fail "negative depth"; }
                X(target);
                if n > 0 { Rec(n - 1, target); }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                Rec(2, target);
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 2);
    assert_eq!(ir_function_names(&program), ["X"]);
}

#[test]
fn finite_recursion_keeps_array_size_with_unused_runtime_integer() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Size(n : Int, unused : Int) : Int {
                { use q = Qubit(); X(q); X(q); }
                if n > 0 { let ignored = Size(n - 1, unused); }
                2
            }
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit(); X(flag);
                let unused = if MResetZ(flag) == One { 1 } else { 0 };
                let size = Size(2, unused);
                use qs = Qubit[size];
                MResetZ(qs[0])
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 3);
    assert_eq!(ir_function_names(&program), ["X"]);
}

#[test]
fn finite_recursion_keeps_array_size_with_unused_runtime_boolean() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Size(n : Int, unused : Bool) : Int {
                { use q = Qubit(); X(q); X(q); }
                if n > 0 { let ignored = Size(n - 1, unused); }
                2
            }
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit(); X(flag);
                let unused = MResetZ(flag) == One;
                let size = Size(2, unused);
                use qs = Qubit[size];
                MResetZ(qs[0])
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 3);
    assert_eq!(ir_function_names(&program), ["X"]);
}

#[test]
fn finite_recursion_with_released_local_qubit_keeps_array_size_constant() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Size(n : Int) : Int {
                { use q = Qubit(); X(q); X(q); }
                if n > 0 { let ignored = Size(n - 1); }
                2
            }
            @EntryPoint()
            operation Main() : Result {
                let size = Size(2);
                use qs = Qubit[size];
                MResetZ(qs[0])
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 2);
    assert!(!program.use_dynamic_qubit_management);
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
}

#[test]
fn finite_recursion_with_released_local_qubit_keeps_computed_integer_constant() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Count(n : Int, target : Qubit) : Int {
                { use q = Qubit(); X(q); X(q); }
                X(target);
                if n > 0 { Count(n - 1, target) + 1 } else { 0 }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let count = Count(2, target);
                if count == 2 { X(target); }
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 2);
    assert!(!program.use_dynamic_qubit_management);
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
}

#[test]
fn runtime_depth_recursion_reuses_released_scratch_isolated_from_entry_qubits() {
    let source = r#"
        namespace Test {
            operation Rec(n : Int) : Unit {
                { use fresh = Qubit(); X(fresh); X(fresh); }
                if n > 0 { Rec(n - 1); }
            }
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit();
                X(flag);
                let n = if MResetZ(flag) == One { 2 } else { 0 };
                Rec(n);
                MResetZ(flag)
            }
        }
    "#;
    let program = get_rir_program_with_adaptive_profile(source);
    assert_eq!(program.num_qubits, 2);
    assert!(!program.use_dynamic_qubit_management);
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Rec",
            "X",
        ]"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(0), )
            Variable(1, Boolean) = Call id(5), args( Result(0), )
            Variable(2, Boolean) = Store Variable(1, Boolean)
            Branch Variable(2, Boolean), 3, 4
        Block 1:Block:
            Call id(3), args( Variable(0, Qubit), )
            Return
        Block 2:Block:
            Variable(4, Integer) = Store Variable(3, Integer)
            Call id(6), args( Variable(4, Integer), )
            Call id(4), args( Qubit(0), Result(1), )
            Call id(7), args( Result(1), Tag(0, 3), )
            Return Integer(0)
        Block 3:Block:
            Variable(3, Integer) = Store Integer(2)
            Jump(2)
        Block 4:Block:
            Variable(3, Integer) = Store Integer(0)
            Jump(2)
        Block 5:Block:
            Call id(2), args( Qubit(1), )
            Call id(2), args( Qubit(1), )
            Variable(6, Integer) = Store Variable(5, Integer)
            Variable(7, Boolean) = Icmp Sgt, Variable(6, Integer), Integer(0)
            Branch Variable(7, Boolean), 7, 6
        Block 6:Block:
            Return
        Block 7:Block:
            Variable(8, Integer) = Store Variable(5, Integer)
            Variable(9, Integer) = Sub Variable(8, Integer), Integer(1)
            Call id(6), args( Variable(9, Integer), )
            Jump(6)"#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn released_fixed_arrays_in_mutual_recursion_have_independent_reservations() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation A(n : Int) : Unit {
                { use qs = Qubit[2]; X(qs[0]); X(qs[0]); }
                if n > 0 { B(n - 1); }
            }
            operation B(n : Int) : Unit {
                { use qs = Qubit[2]; X(qs[1]); X(qs[1]); }
                if n > 0 { A(n - 1); }
            }
            @EntryPoint()
            operation Main() : Result[] {
                use flag = Qubit(); X(flag);
                let n = if MResetZ(flag) == One { 2 } else { 0 };
                use before = Qubit(); X(before);
                A(n);
                { use between = Qubit(); X(between); X(between); A(n); }
                use after = Qubit(); X(after);
                B(n);
                [MResetZ(before), MResetZ(after)]
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 7);
    assert!(!program.use_dynamic_qubit_management);
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "A",
            "B",
            "X",
        ]"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(0), )
            Variable(1, Boolean) = Call id(5), args( Result(0), )
            Variable(2, Boolean) = Store Variable(1, Boolean)
            Branch Variable(2, Boolean), 3, 4
        Block 1:Block:
            Call id(3), args( Variable(0, Qubit), )
            Return
        Block 2:Block:
            Variable(4, Integer) = Store Variable(3, Integer)
            Call id(2), args( Qubit(1), )
            Call id(6), args( Variable(4, Integer), )
            Call id(2), args( Qubit(6), )
            Call id(2), args( Qubit(6), )
            Call id(6), args( Variable(4, Integer), )
            Call id(2), args( Qubit(6), )
            Call id(7), args( Variable(4, Integer), )
            Call id(4), args( Qubit(1), Result(1), )
            Call id(4), args( Qubit(6), Result(2), )
            Call id(8), args( Integer(2), Tag(0, 3), )
            Call id(9), args( Result(1), Tag(1, 5), )
            Call id(9), args( Result(2), Tag(2, 5), )
            Return Integer(0)
        Block 3:Block:
            Variable(3, Integer) = Store Integer(2)
            Jump(2)
        Block 4:Block:
            Variable(3, Integer) = Store Integer(0)
            Jump(2)
        Block 5:Block:
            Variable(6, Integer) = Store Integer(0)
            Variable(6, Integer) = Store Integer(1)
            Variable(6, Integer) = Store Integer(2)
            Call id(2), args( Qubit(2), )
            Call id(2), args( Qubit(2), )
            Variable(7, Integer) = Store Variable(5, Integer)
            Variable(8, Boolean) = Icmp Sgt, Variable(7, Integer), Integer(0)
            Branch Variable(8, Boolean), 7, 6
        Block 6:Block:
            Return
        Block 7:Block:
            Variable(9, Integer) = Store Variable(5, Integer)
            Variable(10, Integer) = Sub Variable(9, Integer), Integer(1)
            Call id(7), args( Variable(10, Integer), )
            Jump(6)
        Block 8:Block:
            Variable(12, Integer) = Store Integer(0)
            Variable(12, Integer) = Store Integer(1)
            Variable(12, Integer) = Store Integer(2)
            Call id(2), args( Qubit(5), )
            Call id(2), args( Qubit(5), )
            Variable(13, Integer) = Store Variable(11, Integer)
            Variable(14, Boolean) = Icmp Sgt, Variable(13, Integer), Integer(0)
            Branch Variable(14, Boolean), 10, 9
        Block 9:Block:
            Return
        Block 10:Block:
            Variable(15, Integer) = Store Variable(11, Integer)
            Variable(16, Integer) = Sub Variable(15, Integer), Integer(1)
            Call id(6), args( Variable(16, Integer), )
            Jump(9)"#]],
    );
}

#[test]
fn inlined_allocating_array_parameter_helper_uses_the_proved_owner_layout() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Scratch(qs : Qubit[]) : Unit {
                use own = Qubit(); X(own); X(own); X(qs[0]);
            }
            operation Rec(n : Int, target : Qubit) : Unit {
                { use qs = Qubit[2]; X(qs[0]); X(qs[0]); }
                Scratch([target]);
                if n > 0 { Rec(n - 1, target); }
            }
            @EntryPoint()
            operation Main() : Result {
                use (flag, target) = (Qubit(), Qubit()); X(flag);
                let n = if MResetZ(flag) == One { 2 } else { 0 };
                Rec(n, target);
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 4);
    assert!(!program.use_dynamic_qubit_management);
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Rec",
            "X",
        ]"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(0), )
            Variable(1, Boolean) = Call id(5), args( Result(0), )
            Variable(2, Boolean) = Store Variable(1, Boolean)
            Branch Variable(2, Boolean), 3, 4
        Block 1:Block:
            Call id(3), args( Variable(0, Qubit), )
            Return
        Block 2:Block:
            Variable(4, Integer) = Store Variable(3, Integer)
            Variable(5, Integer) = Store Variable(4, Integer)
            Call id(6), args( Variable(5, Integer), Qubit(1), )
            Call id(4), args( Qubit(1), Result(1), )
            Call id(7), args( Result(1), Tag(0, 3), )
            Return Integer(0)
        Block 3:Block:
            Variable(3, Integer) = Store Integer(2)
            Jump(2)
        Block 4:Block:
            Variable(3, Integer) = Store Integer(0)
            Jump(2)
        Block 5:Block:
            Variable(8, Integer) = Store Integer(0)
            Variable(8, Integer) = Store Integer(1)
            Variable(8, Integer) = Store Integer(2)
            Call id(2), args( Qubit(2), )
            Call id(2), args( Qubit(2), )
            Variable(9, Qubit) = Store Variable(7, Qubit)
            Variable(10, Array(1, Qubit)) = StoreArray [Variable(9, Qubit)]
            Call id(2), args( Qubit(2), )
            Call id(2), args( Qubit(2), )
            Call id(2), args( Variable(9, Qubit), )
            Variable(11, Integer) = Store Variable(6, Integer)
            Variable(12, Boolean) = Icmp Sgt, Variable(11, Integer), Integer(0)
            Branch Variable(12, Boolean), 7, 6
        Block 6:Block:
            Return
        Block 7:Block:
            Variable(13, Integer) = Store Variable(6, Integer)
            Variable(14, Integer) = Sub Variable(13, Integer), Integer(1)
            Variable(15, Integer) = Store Variable(14, Integer)
            Variable(16, Qubit) = Store Variable(7, Qubit)
            Call id(6), args( Variable(15, Integer), Variable(16, Qubit), )
            Jump(6)"#]],
    );
}

fn assert_runtime_recursion_is_rejected(body: &str, helper: &str) {
    use super::get_partial_evaluation_error_with_capabilities;
    let error = get_partial_evaluation_error_with_capabilities(
        &format!(
            r#"
                namespace Test {{
                    operation Rec(n : Int) : Unit {{ {body} }}
                    {helper}
                    @EntryPoint()
                    operation Main() : Unit {{
                        use flag = Qubit(); X(flag);
                        let n = if MResetZ(flag) == One {{ 2 }} else {{ 0 }};
                        Rec(n);
                    }}
                }}
            "#
        ),
        qsc::target::Profile::Adaptive.into(),
    );
    assert!(
        matches!(error, crate::Error::UnsupportedRecursiveInlining(_)),
        "{body}: {error:?}"
    );
}

#[test]
fn direct_runtime_recursion_with_unreleased_local_qubit_is_rejected() {
    assert_runtime_recursion_is_rejected("use q = Qubit(); if n > 0 { Rec(n - 1); } Reset(q);", "");
}

#[test]
fn direct_runtime_recursion_with_reset_but_unreleased_local_qubit_is_rejected() {
    assert_runtime_recursion_is_rejected("use q = Qubit(); Reset(q); if n > 0 { Rec(n - 1); }", "");
}

#[test]
fn mutual_runtime_recursion_with_unreleased_local_qubit_is_rejected() {
    assert_runtime_recursion_is_rejected(
        "use q = Qubit(); if n > 0 { Again(n - 1); } Reset(q);",
        "operation Again(n : Int) : Unit { Rec(n); }",
    );
}

#[test]
fn runtime_recursion_with_qubit_release_skipped_by_logical_and_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            X(q);
            let ignored = false and { Reset(q); QIR.Runtime.__quantum__rt__qubit_release(q); true };
            if n > 0 { Rec(n - 1); }"#,
        "",
    );
}

#[test]
fn runtime_recursion_with_qubit_release_skipped_by_logical_or_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            X(q);
            let ignored = true or { Reset(q); QIR.Runtime.__quantum__rt__qubit_release(q); true };
            if n > 0 { Rec(n - 1); }"#,
        "",
    );
}

#[test]
fn runtime_recursion_with_qubit_release_skipped_by_logical_and_assignment_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            X(q);
            mutable ignored = false;
            set ignored and= { Reset(q); QIR.Runtime.__quantum__rt__qubit_release(q); true };
            if n > 0 { Rec(n - 1); }"#,
        "",
    );
}

#[test]
fn runtime_recursion_with_qubit_release_skipped_by_logical_or_assignment_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            X(q);
            mutable ignored = true;
            set ignored or= { Reset(q); QIR.Runtime.__quantum__rt__qubit_release(q); true };
            if n > 0 { Rec(n - 1); }"#,
        "",
    );
}

#[test]
fn runtime_recursion_in_array_update_index_before_local_qubit_release_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            let ignored = { QIR.Runtime.__quantum__rt__qubit_release(q); [false] }
                w/ { if n > 0 { Rec(n - 1); } 0 } <- true;"#,
        "",
    );
}

#[test]
fn runtime_recursion_in_array_update_replacement_before_local_qubit_release_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            let ignored = { QIR.Runtime.__quantum__rt__qubit_release(q); [false] }
                w/ 0 <- { if n > 0 { Rec(n - 1); } true };"#,
        "",
    );
}

#[test]
fn runtime_recursion_in_array_update_index_before_local_qubit_array_release_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let qs = QIR.Runtime.AllocateQubitArray(1);
            let ignored = { QIR.Runtime.ReleaseQubitArray(qs); [false] }
                w/ { if n > 0 { Rec(n - 1); } 0 } <- true;"#,
        "",
    );
}

#[test]
fn runtime_recursion_in_array_update_replacement_before_local_qubit_array_release_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"let qs = QIR.Runtime.AllocateQubitArray(1);
            let ignored = { QIR.Runtime.ReleaseQubitArray(qs); [false] }
                w/ 0 <- { if n > 0 { Rec(n - 1); } true };"#,
        "",
    );
}

fn assert_parameterized_local_qubit_layout_is_rejected(body: &str) {
    use super::get_partial_evaluation_error_with_capabilities;
    let error = get_partial_evaluation_error_with_capabilities(
        &format!(
            r#"
                namespace Test {{
                    operation Rec(n : Int, size : Int) : Unit {{
                        {body}
                        if n > 0 {{ Rec(n - 1, size); }}
                    }}
                    @EntryPoint()
                    operation Main() : Unit {{
                        use flag = Qubit(); X(flag);
                        let n = if MResetZ(flag) == One {{ 2 }} else {{ 0 }};
                        Rec(n, 2);
                    }}
                }}
            "#
        ),
        qsc::target::Profile::Adaptive.into(),
    );
    assert!(
        matches!(error, crate::Error::UnsupportedRecursiveInlining(_)),
        "{body}: {error:?}"
    );
}

#[test]
fn runtime_recursion_with_aliased_local_qubit_is_rejected() {
    assert_parameterized_local_qubit_layout_is_rejected(
        "{ use q = Qubit(); let alias = q; X(alias); X(q); }",
    );
}

#[test]
fn runtime_recursion_with_parameter_sized_local_qubit_array_is_rejected() {
    assert_parameterized_local_qubit_layout_is_rejected(
        "{ use qs = Qubit[size]; X(qs[0]); X(qs[0]); }",
    );
}

#[test]
fn parameter_sized_scratch_with_static_depth_preserves_finite_inline_fallback() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Rec(n : Int, size : Int, target : Qubit) : Unit {
                { use qs = Qubit[size]; X(qs[0]); X(qs[0]); }
                X(target);
                if n > 0 { Rec(n - 1, size, target); }
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                Rec(2, 2, target);
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 3);
    assert!(!program.use_dynamic_qubit_management);
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Variable(0, Integer) = Store Integer(0)
            Variable(0, Integer) = Store Integer(1)
            Variable(0, Integer) = Store Integer(2)
            Call id(2), args( Qubit(1), )
            Call id(2), args( Qubit(1), )
            Call id(2), args( Qubit(0), )
            Variable(2, Integer) = Store Integer(0)
            Variable(2, Integer) = Store Integer(1)
            Variable(2, Integer) = Store Integer(2)
            Call id(2), args( Qubit(1), )
            Call id(2), args( Qubit(1), )
            Call id(2), args( Qubit(0), )
            Variable(3, Integer) = Store Integer(0)
            Variable(3, Integer) = Store Integer(1)
            Variable(3, Integer) = Store Integer(2)
            Call id(2), args( Qubit(1), )
            Call id(2), args( Qubit(1), )
            Call id(2), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(0), )
            Call id(5), args( Result(0), Tag(0, 3), )
            Return Integer(0)
        Block 1:Block:
            Call id(3), args( Variable(1, Qubit), )
            Return"#]],
    );
}

#[test]
fn released_scratch_boolean_measurement_is_materialized_before_recursive_reentry() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Rec(n : Int, target : Qubit) : Unit {
                let observed = {
                    use scratch = Qubit();
                    X(scratch);
                    MResetZ(scratch) == One
                };
                if n > 0 { Rec(n - 1, target); }
                if observed { X(target); }
            }
            @EntryPoint()
            operation Main() : Result {
                use (flag, target) = (Qubit(), Qubit()); X(flag);
                let n = if MResetZ(flag) == One { 2 } else { 0 };
                Rec(n, target);
                MResetZ(target)
            }
        }
        "#,
    );
    assert_eq!(program.num_qubits, 3);
    assert_eq!(program.num_results, 3);
    assert!(!program.use_dynamic_qubit_management);
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Rec",
            "X",
        ]"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(0), )
            Variable(1, Boolean) = Call id(5), args( Result(0), )
            Variable(2, Boolean) = Store Variable(1, Boolean)
            Branch Variable(2, Boolean), 3, 4
        Block 1:Block:
            Call id(3), args( Variable(0, Qubit), )
            Return
        Block 2:Block:
            Variable(4, Integer) = Store Variable(3, Integer)
            Variable(5, Integer) = Store Variable(4, Integer)
            Call id(6), args( Variable(5, Integer), Qubit(1), )
            Call id(4), args( Qubit(1), Result(2), )
            Call id(7), args( Result(2), Tag(0, 3), )
            Return Integer(0)
        Block 3:Block:
            Variable(3, Integer) = Store Integer(2)
            Jump(2)
        Block 4:Block:
            Variable(3, Integer) = Store Integer(0)
            Jump(2)
        Block 5:Block:
            Call id(2), args( Qubit(2), )
            Call id(4), args( Qubit(2), Result(1), )
            Variable(8, Boolean) = Call id(5), args( Result(1), )
            Variable(9, Boolean) = Store Variable(8, Boolean)
            Variable(10, Boolean) = Store Variable(9, Boolean)
            Variable(11, Boolean) = Store Variable(10, Boolean)
            Variable(12, Integer) = Store Variable(6, Integer)
            Variable(13, Boolean) = Icmp Sgt, Variable(12, Integer), Integer(0)
            Branch Variable(13, Boolean), 7, 6
        Block 6:Block:
            Branch Variable(11, Boolean), 9, 8
        Block 7:Block:
            Variable(14, Integer) = Store Variable(6, Integer)
            Variable(15, Integer) = Sub Variable(14, Integer), Integer(1)
            Variable(16, Integer) = Store Variable(15, Integer)
            Variable(17, Qubit) = Store Variable(7, Qubit)
            Call id(6), args( Variable(16, Integer), Variable(17, Qubit), )
            Jump(6)
        Block 8:Block:
            Return
        Block 9:Block:
            Call id(2), args( Variable(7, Qubit), )
            Jump(8)"#]],
    );
}

#[test]
fn runtime_recursion_with_qubit_allocation_in_dynamic_range_is_rejected() {
    use super::get_partial_evaluation_error_with_capabilities;
    let error = get_partial_evaluation_error_with_capabilities(
        r#"
        namespace Test {
            operation Rec(n : Int) : Unit {
                for _ in 1..n { use q = Qubit(); X(q); X(q); }
                if n > 0 { Rec(n - 1); }
            }
            @EntryPoint()
            operation Main() : Unit {
                use flag = Qubit(); X(flag);
                let n = if MResetZ(flag) == One { 2 } else { 0 };
                Rec(n);
            }
        }
        "#,
        qsc::target::Profile::Adaptive.into(),
    );
    assert!(
        matches!(
            &error,
            crate::Error::Unexpected(message, _) if message == "dynamic variable in Range expression"
        ),
        "{error:?}"
    );
}

#[test]
fn runtime_recursion_with_conditionally_allocated_local_qubit_is_rejected() {
    assert_runtime_recursion_is_rejected(
        "if n > 0 { use q = Qubit(); X(q); X(q); } if n > 0 { Rec(n - 1); }",
        "",
    );
}

#[test]
fn runtime_recursion_with_local_qubit_allocated_in_loop_is_rejected() {
    assert_runtime_recursion_is_rejected(
        "for _ in 1..2 { use q = Qubit(); X(q); X(q); } if n > 0 { Rec(n - 1); }",
        "",
    );
}

#[test]
fn runtime_recursion_with_local_qubit_aliased_in_tuple_is_rejected() {
    assert_runtime_recursion_is_rejected(
        "{ use q = Qubit(); let (a, b) = (q, q); X(a); X(b); } if n > 0 { Rec(n - 1); }",
        "",
    );
}

#[test]
fn runtime_recursion_with_borrowed_local_qubit_is_rejected() {
    assert_runtime_recursion_is_rejected(
        r#"{ let q = QIR.Runtime.__quantum__rt__qubit_borrow();
            X(q); X(q); QIR.Runtime.__quantum__rt__qubit_release(q); }
            if n > 0 { Rec(n - 1); }"#,
        "",
    );
}

#[test]
fn runtime_recursion_with_local_qubit_returned_by_helper_is_rejected() {
    assert_runtime_recursion_is_rejected(
        "{ use q = Qubit(); X(Alias(q)); X(q); } if n > 0 { Rec(n - 1); }",
        "operation Alias(q : Qubit) : Qubit { q }",
    );
}

#[test]
fn runtime_recursion_with_local_qubit_captured_by_lambda_is_rejected() {
    assert_runtime_recursion_is_rejected(
        "{ use q = Qubit(); let flip = () => X(q); flip(); X(q); } if n > 0 { Rec(n - 1); }",
        "",
    );
}

#[test]
fn unresolved_callable_body_inlines_with_static_arguments() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            function Inc(value : Int) : Int { value + 1 }
            function Twice(value : Int) : Int { value * 2 }
            function Pick(flag : Bool, index : Int) : Int {
                let actions = [Inc, Twice];
                let selected = if flag { actions[index] } else { Inc };
                selected(3)
            }
            @EntryPoint()
            operation Main() : Int { Pick(true, 0) * 10 + Pick(true, 1) }
        }
        "#,
    );
    assert_ir_function_names(&program, &expect!["[]"]);
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Integer(46), Tag(0, 3), )
                Return Integer(0)"#]],
    );
}

#[test]
fn eligible_void_operation_called_twice_emits_one_callable_and_two_calls() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
                ApplyX(q);
            }
        }
        "#,
    );

    // `ApplyX` is emitted exactly once as a bodied `Regular` callable.
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyX",
                "X",
            ]"#]],
    );

    // The entry block calls the single emitted `ApplyX` callable twice, passing the concrete
    // call-site qubit; the body of `ApplyX` applies `X` to the parameter variable.
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Call id(3), args( Variable(0, Qubit), )
                Return
            Block 2:Block:
                Call id(4), args( Variable(1, Qubit), )
                Return"#]],
    );

    assert_callable(
        &program,
        CallableId(2),
        &expect![[r#"
            Callable:
                name: ApplyX
                call_type: Regular
                input_type:
                    [0]: Qubit
                input_vars:
                    [0]: 0
                output_type: <VOID>
                body: 1"#]],
    );
}

#[test]
fn scalar_and_qubit_parameters_are_threaded_as_variables() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyRz(theta : Double, q : Qubit) : Unit {
                Rz(theta, q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyRz(1.0, q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyRz",
                "Rz",
            ]"#]],
    );

    // The double and qubit parameters become RIR variables in the emitted body.
    assert_callable(
        &program,
        CallableId(2),
        &expect![[r#"
            Callable:
                name: ApplyRz
                call_type: Regular
                input_type:
                    [0]: Double
                    [1]: Qubit
                input_vars:
                    [0]: 0
                    [1]: 1
                output_type: <VOID>
                body: 1"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Double(1), Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Variable(2, Double) = Store Variable(0, Double)
                Variable(3, Qubit) = Store Variable(1, Qubit)
                Call id(3), args( Variable(2, Double), Variable(3, Qubit), )
                Return
            Block 2:Block:
                Variable(6, Double) = Store Variable(4, Double)
                Variable(7, Qubit) = Store Variable(5, Qubit)
                Call id(4), args( Variable(6, Double), Variable(7, Qubit), )
                Return"#]],
    );
}

#[test]
fn body_and_adjoint_specializations_emit_distinct_functions() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyS(q : Qubit) : Unit is Adj {
                S(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyS(q);
                Adjoint ApplyS(q);
            }
        }
        "#,
    );

    // The body and adjoint specializations are emitted as two distinct IR functions.
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyS",
                "ApplyS__Adj",
                "S",
                "S__Adj",
            ]"#]],
    );
}

#[test]
fn controlled_specialization_is_inlined() {
    // Controlled specializations take a synthesized dynamic-length `Qubit[]` control register which
    // has no base-phase RIR representation, so the `ApplyS` controlled specialization is always
    // inlined (no IR function is emitted for it). The standard-library gate it calls is emitted
    // separately as its own IR function.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyS(q : Qubit) : Unit is Ctl {
                S(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use (c, q) = (Qubit(), Qubit());
                Controlled ApplyS([c], q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "CNOT",
            "CS",
            "T",
            "T__Adj",
        ]"#]],
    );
}

#[test]
fn entry_callable_is_not_emitted_as_ir_function() {
    // The entry-point callable (`Main`) is the body of the entry function itself and must never be
    // emitted as a separate IR function, even though it is a reachable, void, parameterless
    // user-package callable that would otherwise satisfy every eligibility criterion. Only the
    // genuine non-entry user callable (`ApplyX`) is emitted as an IR function.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
            }
        }
        "#,
    );

    let names = ir_function_names(&program);
    assert!(
        !names.contains(&"Main".to_string()),
        "entry callable `Main` must not be emitted as an IR function, got {names:?}"
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyX",
                "X",
            ]"#]],
    );
}

#[test]
fn un_promoted_tuple_parameter_callee_is_inlined() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyToPair(qs : (Qubit, Qubit)) : Unit {
                let (a, b) = qs;
                X(a);
                X(b);
            }
            @EntryPoint()
            operation Main() : Unit {
                use (a, b) = (Qubit(), Qubit());
                ApplyToPair((a, b));
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
}

#[test]
fn recursive_callee_is_emitted() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Recurse(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    X(q);
                    Recurse(n - 1, q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Recurse(2, q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Recurse",
                "X",
            ]"#]],
    );
}

#[test]
fn recursive_immediate_measurement_comparison_is_emitted() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {
            if n > 0 {
                if MResetZ(q) == One { X(target); }
                Rec(n - 1, q, target);
            }
        }
        operation Main() : Unit {
            use (q, target) = (Qubit(), Qubit());
            Rec(if MResetZ(q) == One { 2 } else { 0 }, q, target);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Rec",
                "X",
            ]"#]],
    );
}

#[test]
fn recursive_materialized_boolean_survives_reentry() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {
            if n > 0 {
                let saved = M(q) == One;
                X(q);
                Rec(n - 1, q, target);
                if saved { X(target); }
            }
        }
        operation Main() : Result {
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let n = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(n, q, target);
            Reset(q);
            MResetZ(target)
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Rec",
                "Reset",
                "X",
            ]"#]],
    );
    assert_eq!(program.get_callable(CallableId(2)).name, "X");
    assert_eq!(
        program.get_callable(CallableId(5)).name,
        "__quantum__rt__read_result"
    );
    assert_eq!(program.get_callable(CallableId(6)).name, "Rec");
    assert_eq!(
        program.get_callable(CallableId(7)).name,
        "__quantum__qis__m__body"
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(4), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(5), args( Result(0), )
                Variable(2, Boolean) = Store Variable(1, Boolean)
                Branch Variable(2, Boolean), 3, 4
            Block 1:Block:
                Call id(3), args( Variable(0, Qubit), )
                Return
            Block 2:Block:
                Variable(4, Integer) = Store Variable(3, Integer)
                Variable(5, Integer) = Store Variable(4, Integer)
                Call id(6), args( Variable(5, Integer), Qubit(1), Qubit(2), )
                Call id(8), args( Qubit(1), )
                Call id(4), args( Qubit(2), Result(2), )
                Call id(10), args( Result(2), Tag(0, 3), )
                Return Integer(0)
            Block 3:Block:
                Variable(3, Integer) = Store Integer(2)
                Jump(2)
            Block 4:Block:
                Variable(3, Integer) = Store Integer(0)
                Jump(2)
            Block 5:Block:
                Variable(9, Integer) = Store Variable(6, Integer)
                Variable(10, Boolean) = Icmp Sgt, Variable(9, Integer), Integer(0)
                Branch Variable(10, Boolean), 7, 6
            Block 6:Block:
                Return
            Block 7:Block:
                Call id(7), args( Variable(7, Qubit), Result(1), )
                Variable(11, Boolean) = Call id(5), args( Result(1), )
                Variable(12, Boolean) = Store Variable(11, Boolean)
                Variable(13, Boolean) = Store Variable(12, Boolean)
                Call id(2), args( Variable(7, Qubit), )
                Variable(14, Integer) = Store Variable(6, Integer)
                Variable(15, Integer) = Sub Variable(14, Integer), Integer(1)
                Variable(16, Integer) = Store Variable(15, Integer)
                Variable(17, Qubit) = Store Variable(7, Qubit)
                Variable(18, Qubit) = Store Variable(8, Qubit)
                Call id(6), args( Variable(16, Integer), Variable(17, Qubit), Variable(18, Qubit), )
                Branch Variable(13, Boolean), 9, 8
            Block 8:Block:
                Jump(6)
            Block 9:Block:
                Call id(2), args( Variable(8, Qubit), )
                Jump(8)
            Block 10:Block:
                Call id(9), args( Variable(19, Qubit), )
                Return"#]],
    );
}

#[test]
fn recursive_boolean_measurement_helper_is_emitted() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Observe(q : Qubit) : Bool { M(q) == One }
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {
            if n > 0 {
                let saved = Observe(q);
                X(q);
                Rec(n - 1, q, target);
                if saved { X(target); }
            }
        }
        operation Main() : Unit {
            use (q, target) = (Qubit(), Qubit());
            Rec(if MResetZ(q) == One { 2 } else { 0 }, q, target);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Observe",
                "Rec",
                "Reset",
                "X",
            ]"#]],
    );
}

#[test]
fn mutual_recursive_boolean_measurement_is_emitted() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation A(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let saved = M(q) == One;
                X(q);
                B(n - 1, q);
                if saved { X(q); }
            }
        }
        operation B(n : Int, q : Qubit) : Unit { A(n, q); }
        operation Main() : Unit {
            use q = Qubit();
            B(if MResetZ(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "A",
            "B",
            "Reset",
            "X",
        ]"#]],
    );
}

#[test]
fn recursive_measurement_proof_uses_selected_adjoint() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Step(n : Int, q : Qubit) : Unit is Adj {
            body (...) {
                let raw = M(q);
                Rec(n - 1, q);
                if raw == One { X(q); }
            }
            adjoint (...) {
                if One != M(q) { X(q); }
                Rec(n - 1, q);
            }
        }
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 { Adjoint Step(n, q); }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(if MResetZ(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Rec",
            "Reset",
            "Step__Adj",
            "X",
        ]"#]],
    );
}

#[test]
fn recursive_measurement_proof_rejects_live_controlled_adjoint_result() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Step(n : Int, q : Qubit) : Unit is Adj + Ctl {
            body (...) {}
            adjoint (...) {}
            controlled (cs, ...) {}
            controlled adjoint (cs, ...) {
                let raw = M(q);
                Rec(n - 1, cs[0], q);
                if raw == One { X(q); }
            }
        }
        operation Rec(n : Int, control : Qubit, q : Qubit) : Unit {
            if n > 0 { Controlled Adjoint Step([control], (n, q)); }
        }
        operation Main() : Unit {
            use (control, q) = (Qubit(), Qubit());
            Rec(2, control, q);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Reset",
            "X",
        ]"#]],
    );
}

#[test]
fn recursive_measurement_with_unused_result_parameter_keeps_inline_fallback() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Rec(n : Int, q : Qubit, unused : Result) : Unit {
            if n > 0 {
                if M(q) == One { X(q); }
                Rec(n - 1, q, Zero);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(2, q, Zero);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Reset",
            "X",
        ]"#]],
    );
}

#[test]
fn recursive_result_comparison_with_effectful_rhs_keeps_inline_fallback() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 {
                if M(q) == { X(q); Rec(n - 1, q); M(q) } { X(q); }
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(2, q);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Reset",
            "X",
        ]"#]],
    );
}

#[test]
fn raw_result_in_acyclic_caller_keeps_inline_fallback() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 {
                if M(q) == One { X(q); }
                Rec(n - 1, q);
            }
        }
        operation Outer(n : Int, q : Qubit) : Unit {
            let raw = M(q);
            Rec(n, q);
            if raw == One { X(q); }
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(2, q);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Rec",
            "Reset",
            "X",
        ]"#]],
    );
}

#[test]
fn recursive_measurement_argument_with_live_result_keeps_inline_fallback() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 {
                if M({
                    let raw = M(q);
                    Rec(n - 1, q);
                    if raw == One { X(q); }
                    q
                }) == One { X(q); }
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(2, q);
            Reset(q);
        }
        "#,
    );
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "Reset",
            "X",
        ]"#]],
    );
}

#[test]
fn recursive_operation_factory_is_inlined_before_calling_returned_operation() {
    let source = r#"
        namespace Test {
            operation Flip(q : Qubit) : Unit { X(q); }
            function Make(n : Int) : Qubit => Unit {
                if n == 0 {
                    Flip
                } else {
                    let next = Make(n - 1);
                    next
                }
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Make(2)(q);
                MResetZ(q)
            }
        }
        "#;

    for keep_rca_inline_hint in [true, false] {
        let capabilities = qsc_data_structures::target::Profile::Adaptive.into();
        let mut context = super::CompilationContext::new(source, capabilities);
        if !keep_rca_inline_hint {
            // The emitter must validate the resolved return ABI even if a
            // call-site analysis result lacks the inline hint.
            for (package_id, _) in &context.fir_store {
                for properties in context
                    .compute_properties
                    .get_mut(package_id, false)
                    .exprs
                    .values_mut()
                {
                    if let qsc_rca::ComputeKind::Dynamic {
                        runtime_features, ..
                    } = &mut properties.inherent
                    {
                        runtime_features.remove(qsc_rca::RuntimeFeatureFlags::MustBeInlined);
                    }
                }
            }
        }
        let program = crate::partially_evaluate(
            &context.fir_store,
            &context.compute_properties,
            &context.entry,
            capabilities,
            crate::PartialEvalConfig {
                generate_debug_metadata: false,
            },
        )
        .expect("the recursive factory must inline");
        assert_ir_function_names(
            &program,
            &expect![[r#"
        [
            "Flip",
            "X",
        ]"#]],
        );
        assert_blocks(
            &program,
            &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Qubit(0), )
            Call id(5), args( Qubit(0), Result(0), )
            Call id(6), args( Result(0), Tag(0, 3), )
            Return Integer(0)
        Block 1:Block:
            Call id(3), args( Variable(0, Qubit), )
            Return
        Block 2:Block:
            Call id(4), args( Variable(1, Qubit), )
            Return"#]],
        );
    }
}

#[test]
fn recursive_factories_with_callable_payloads_are_inlined() {
    for (output, value, invocation) in [
        (
            "Int -> Int",
            "x -> x + 1",
            "if Make(2)(3) == 4 { Flip(q); }",
        ),
        (
            "(Int, Qubit => Unit)",
            "(7, Flip)",
            "let (n, op) = Make(2); if n == 7 { op(q); }",
        ),
        ("(Qubit => Unit)[]", "[Flip]", "Make(2)[0](q);"),
    ] {
        let source = format!(
            r#"
            namespace Test {{
                operation Flip(q : Qubit) : Unit {{ X(q); }}
                function Make(n : Int) : {output} {{
                    if n == 0 {{ {value} }} else {{ Make(n - 1) }}
                }}
                @EntryPoint()
                operation Main() : Result {{
                    use q = Qubit();
                    {invocation}
                    MResetZ(q)
                }}
            }}
        "#
        );
        let program = get_rir_program_with_adaptive_profile(&source);
        assert_eq!(ir_function_names(&program), ["Flip", "X"], "{source}");
    }
}

#[test]
fn callable_input_parameters_are_inlined_instead_of_emitted_as_operands() {
    // These helpers use FIR without the transform pipeline, so the signature
    // guard is exercised even when defunctionalization has not removed arrows.
    for (input, body, argument) in [
        ("op : Qubit => Unit", "op(q);", "Flip"),
        ("ops : (Qubit => Unit)[]", "ops[0](q);", "[Flip]"),
        (
            "pair : (Int, Qubit => Unit)",
            "let (_, op) = pair; op(q);",
            "(7, Flip)",
        ),
        ("f : Int -> Int", "if f(3) == 4 { Flip(q); }", "x -> x + 1"),
    ] {
        let source = format!(
            r#"
            namespace Test {{
                operation Flip(q : Qubit) : Unit {{ X(q); }}
                operation Apply({input}, q : Qubit) : Unit {{ {body} }}
                @EntryPoint()
                operation Main() : Result {{
                    use q = Qubit();
                    Apply({argument}, q);
                    MResetZ(q)
                }}
            }}
        "#
        );
        let program = get_rir_program_with_adaptive_profile(&source);
        assert_ir_function_names(
            &program,
            &expect![[r#"
            [
                "Flip",
                "X",
            ]"#]],
        );
    }
}

#[test]
fn cross_package_callee_is_emitted() {
    // `Microsoft.Quantum.Intrinsic.X` is a standard-library (cross-package) operation whose body
    // wraps the `__quantum__qis__x__body` intrinsic. Reachable eligible callables from any package
    // are emitted as standalone IR functions, so the cross-package `X` wrapper is emitted (and
    // called) rather than inlined.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                X(q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "X",
            ]"#]],
    );
}

#[test]
fn qubit_allocating_callee_is_inlined_when_dynamic_allocation_disabled() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation AllocAndX() : Unit {
                use a = Qubit();
                X(a);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocAndX();
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
}

#[test]
fn qubit_allocating_callee_is_emitted_when_dynamic_allocation_enabled() {
    let program = get_rir_program_with_dynamic_qubit_allocation(
        r#"
        namespace Test {
            operation AllocAndX() : Unit {
                use a = Qubit();
                X(a);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocAndX();
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "AllocAndX",
                "X",
            ]"#]],
    );
}

#[test]
fn dynamic_qubit_allocation_inside_ir_function_emits_runtime_alloc_and_release_calls() {
    let program = get_rir_program_with_dynamic_qubit_allocation(
        r#"
        namespace Test {
            operation AllocAndX() : Unit {
                use a = Qubit();
                X(a);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocAndX();
            }
        }
        "#,
    );

    // The program reports that it uses dynamic qubit management, and no static qubits are required.
    assert!(program.use_dynamic_qubit_management);
    assert_eq!(program.num_qubits, 0);

    // The `AllocAndX` IR-function body allocates a runtime qubit via a value-returning
    // `__quantum__rt__qubit_allocate` call, applies `X` to it, then releases it via
    // `__quantum__rt__qubit_release`. The runtime qubit is threaded as a `Variable`, not a static
    // `Qubit` literal.
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( )
                Call id(7), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Variable(0, Qubit) = Call id(3), args( )
                Variable(1, Qubit) = Store Variable(0, Qubit)
                Call id(4), args( Variable(1, Qubit), )
                Call id(6), args( Variable(1, Qubit), )
                Return
            Block 2:Block:
                Call id(5), args( Variable(2, Qubit), )
                Return"#]],
    );

    assert_callable(
        &program,
        CallableId(3),
        &expect![[r#"
            Callable:
                name: __quantum__rt__qubit_allocate
                call_type: Regular
                input_type: <VOID>
                output_type: Qubit
                body: <NONE>"#]],
    );

    assert_callable(
        &program,
        CallableId(5),
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
}

#[test]
fn int_returning_callee_emits_typed_ir_function_and_binds_output_var() {
    // An eligible `Int`-returning callable is emitted as a typed IR function. The body materializes
    // the trailing value as the `Return` operand and the call site binds a fresh output variable so
    // the returned value is threaded back into the caller instead of being dropped.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyAndReturn(q : Qubit) : Int {
                X(q);
                42
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = ApplyAndReturn(q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyAndReturn",
                "X",
            ]"#]],
    );

    assert_callable(
        &program,
        CallableId(2),
        &expect![[r#"
            Callable:
                name: ApplyAndReturn
                call_type: Regular
                input_type:
                    [0]: Qubit
                input_vars:
                    [0]: 0
                output_type: Integer
                body: 1"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(2, Integer) = Call id(2), args( Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Call id(3), args( Variable(0, Qubit), )
                Return Integer(42)
            Block 2:Block:
                Call id(4), args( Variable(1, Qubit), )
                Return"#]],
    );
}

#[test]
fn double_returning_callee_emits_typed_ir_function() {
    // The quantum side effect (`X(q)`) prevents the call from being folded classically, so the
    // `Double`-returning callee is emitted as a typed IR function whose body materializes the
    // trailing value as the `Return` operand, with the call site binding a fresh output variable.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyAndAngle(q : Qubit) : Double {
                X(q);
                1.5
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = ApplyAndAngle(q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyAndAngle",
                "X",
            ]"#]],
    );
    assert_callable(
        &program,
        CallableId(2),
        &expect![[r#"
            Callable:
                name: ApplyAndAngle
                call_type: Regular
                input_type:
                    [0]: Qubit
                input_vars:
                    [0]: 0
                output_type: Double
                body: 1"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(2, Double) = Call id(2), args( Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Call id(3), args( Variable(0, Qubit), )
                Return Double(1.5)
            Block 2:Block:
                Call id(4), args( Variable(1, Qubit), )
                Return"#]],
    );
}

#[test]
fn bool_returning_callee_emits_typed_ir_function() {
    // The quantum side effect (`X(q)`) prevents the call from being folded classically, so the
    // `Bool`-returning callee is emitted as a typed IR function whose body materializes the trailing
    // value as the `Return` operand, with the call site binding a fresh output variable.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyAndFlag(q : Qubit) : Bool {
                X(q);
                true
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = ApplyAndFlag(q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyAndFlag",
                "X",
            ]"#]],
    );
    assert_callable(
        &program,
        CallableId(2),
        &expect![[r#"
            Callable:
                name: ApplyAndFlag
                call_type: Regular
                input_type:
                    [0]: Qubit
                input_vars:
                    [0]: 0
                output_type: Boolean
                body: 1"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(2, Boolean) = Call id(2), args( Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Call id(3), args( Variable(0, Qubit), )
                Return Bool(true)
            Block 2:Block:
                Call id(4), args( Variable(1, Qubit), )
                Return"#]],
    );
}

#[test]
fn result_returning_callee_is_inlined() {
    // A `Result`-returning callable has no by-value single-exit RIR representation, so it continues
    // to inline (no IR function is emitted) even though it otherwise satisfies eligibility.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation MeasureIt(q : Qubit) : Result {
                MResetZ(q)
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = MeasureIt(q);
            }
        }
        "#,
    );

    assert_ir_function_names(&program, &expect!["[]"]);
}

#[test]
fn qubit_returning_callee_is_inlined() {
    // A `Qubit`-returning callable has no by-value single-exit RIR representation, so `Echo`
    // continues to inline (no IR function is emitted for it). The standard-library gate called
    // afterwards is emitted separately as its own IR function.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Echo(q : Qubit) : Qubit {
                q
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let q2 = Echo(q);
                X(q2);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
}

#[test]
fn store_backed_value_returning_ir_function_reloads_after_same_block_store() {
    // A value-returning IR function whose returned mutable is read, updated, and stored again in
    // the same merge block must reload the freshly stored value before `Return`. The
    // `x = x + 1; x` sequence reads `x`, adds one, stores it, then returns it from the same
    // block, so after the non-SSA alloca/load transform the returning block ends with a `Load` that
    // follows the final `Store` and feeds `Return`.
    let mut program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Foo(q : Qubit) : Int {
                mutable x = 0;
                if MResetZ(q) == One {
                    x = 5;
                }
                x = x + 1;
                x
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return Foo(q);
            }
        }
        "#,
    );

    // `Foo` is emitted as a bodied `Regular` IR function.
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Foo",
            ]"#]],
    );

    // Run the non-SSA alloca/load transform so the store-backed reads become explicit loads.
    qsc_rir::passes::check_and_transform(&mut program);

    // The merge block of `Foo` (Block 3) loads `x`, adds one, stores the result, then loads `x`
    // again and returns the reloaded value rather than the stale pre-store load.
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(6, Integer) = Call id(2), args( Qubit(0), )
                Call id(5), args( Variable(6, Integer), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Variable(1, Integer) = Alloca
                Variable(4, Integer) = Alloca
                Variable(1, Integer) = Store Integer(0)
                Call id(3), args( Variable(0, Qubit), Result(0), )
                Variable(2, Boolean) = Call id(4), args( Result(0), )
                Branch Variable(2, Boolean), 2, 3
            Block 2:Block:
                Variable(1, Integer) = Store Integer(5)
                Jump(3)
            Block 3:Block:
                Variable(8, Integer) = Load Variable(1, Integer)
                Variable(4, Integer) = Store Variable(8, Integer)
                Variable(10, Integer) = Load Variable(4, Integer)
                Variable(5, Integer) = Add Variable(10, Integer), Integer(1)
                Variable(1, Integer) = Store Variable(5, Integer)
                Variable(12, Integer) = Load Variable(1, Integer)
                Return Variable(12, Integer)"#]],
    );
}

#[test]
fn ir_functions_are_not_emitted_without_call_support_capability() {
    // Without the `CallSupport` capability (e.g. the `AdaptiveRIF` profile), every callable is
    // inlined exactly as before, so no IR functions are emitted.
    let program = get_rir_program(
        r#"
        namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
                ApplyX(q);
            }
        }
        "#,
    );

    assert_ir_function_names(&program, &expect!["[]"]);
}

#[test]
fn tuple_discarded_parameter_is_threaded_as_call_site_operand() {
    // The callee has a discarded tuple leaf parameter (`_ : Int`, d : `Double``).
    // When the call is emitted as an IR function, the discarded argument has no
    // body binding, but its call-site value must still be mapped to an operand
    // and passed in input-parameter order. This exercises the `Arg::Discard`
    //branch of the call-site operand mapping. The quantum side effect (`Rz`)
    // keeps the call from being folded classically so the IR function is actually emitted.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Foo((_ : Int, d : Double), q : Qubit) : Double {
                Rz(d, q);
                d
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = Foo((5, 1.5), q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Foo",
                "Rz",
            ]"#]],
    );

    // The discarded `Int` leaf occupies the first input slot of the emitted callable but is not
    // bound in the body; only the `Double` and `Qubit` leaves are threaded as body variables.
    assert_callable(
        &program,
        CallableId(2),
        &expect![[r#"
            Callable:
                name: Foo
                call_type: Regular
                input_type:
                    [0]: Integer
                    [1]: Double
                    [2]: Qubit
                input_vars:
                    [0]: 0
                    [1]: 1
                    [2]: 2
                output_type: Double
                body: 1"#]],
    );

    // The call site passes the discarded literal `Integer(5)` as the first operand, exercising the
    // `Arg::Discard` operand mapping branch.
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(9, Double) = Call id(2), args( Integer(5), Double(1.5), Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Variable(3, Double) = Store Variable(1, Double)
                Variable(4, Qubit) = Store Variable(2, Qubit)
                Call id(3), args( Variable(3, Double), Variable(4, Qubit), )
                Return Variable(1, Double)
            Block 2:Block:
                Variable(7, Double) = Store Variable(5, Double)
                Variable(8, Qubit) = Store Variable(6, Qubit)
                Call id(4), args( Variable(7, Double), Variable(8, Qubit), )
                Return"#]],
    );
}

#[test]
fn discarded_parameter_is_threaded_as_call_site_operand() {
    // The callee has a discarded leaf parameter `_ : Int`. When the call is emitted as an IR
    // function, the discarded argument has no body binding, but its call-site value must still be
    // mapped to an operand and passed in input-parameter order. This exercises the `Arg::Discard`
    // branch of the call-site operand mapping. The quantum side effect (`Rz`) keeps the call from
    // being folded classically so the IR function is actually emitted.
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation Foo(_ : Int, d : Double, q : Qubit) : Double {
                Rz(d, q);
                d
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = Foo(5, 1.5, q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Foo",
                "Rz",
            ]"#]],
    );

    // The discarded `Int` leaf occupies the first input slot of the emitted callable but is not
    // bound in the body; only the `Double` and `Qubit` leaves are threaded as body variables.
    assert_callable(
        &program,
        CallableId(2),
        &expect![[r#"
            Callable:
                name: Foo
                call_type: Regular
                input_type:
                    [0]: Integer
                    [1]: Double
                    [2]: Qubit
                input_vars:
                    [0]: 0
                    [1]: 1
                    [2]: 2
                output_type: Double
                body: 1"#]],
    );

    // The call site passes the discarded literal `Integer(5)` as the first operand, exercising the
    // `Arg::Discard` operand mapping branch.
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(9, Double) = Call id(2), args( Integer(5), Double(1.5), Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Variable(3, Double) = Store Variable(1, Double)
                Variable(4, Qubit) = Store Variable(2, Qubit)
                Call id(3), args( Variable(3, Double), Variable(4, Qubit), )
                Return Variable(1, Double)
            Block 2:Block:
                Variable(7, Double) = Store Variable(5, Double)
                Variable(8, Qubit) = Store Variable(6, Qubit)
                Call id(4), args( Variable(7, Double), Variable(8, Qubit), )
                Return"#]],
    );
}

/// Eligible callables whose names nothing else competes for are emitted under
/// their bare source names. Both the user `ApplyX` operation and the foreign
/// `X` intrinsic wrapper it calls are emitted, and the name registry keeps each
/// bare name in the common (non-colliding) case rather than spuriously
/// appending a package/item discriminator.
#[test]
fn non_colliding_callable_keeps_bare_name() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
            }
        }
        "#,
    );

    // Each emitted IR function uses its bare source name with no discriminator suffix.
    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "ApplyX",
                "X",
            ]"#]],
    );
}

/// Targeting the `AdaptiveRIF` profile (which does not enable the `CallSupport`
/// capability) keeps every eligible callable inlined: no bodied `Regular`
/// callables are emitted as IR functions. This is the baseline the
/// `CallSupport`-gated emission relaxes.
#[test]
fn adaptive_rif_profile_inlines_all_callables() {
    let program = get_rir_program(
        r#"
        namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
                ApplyX(q);
            }
        }
        "#,
    );

    // With IR-function emission disabled, no standalone callables are emitted; the
    // entry point inlines the body of `ApplyX` at every call site.
    assert_ir_function_names(&program, &expect![[r#"[]"#]]);
}

#[test]
fn operation_with_integer_input_used_in_loop_range_is_inlined() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation RepeatX(num : Int, q : Qubit) : Unit {
                for i in 1..num {
                    X(q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                RepeatX(3, q);
            }
        }
        "#,
    );

    // Because emitting `RepeatX` as an IR function would require handling a variable
    // used as a range bound, which is not yet supported, we inline `RepeatX`.
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
}

#[test]
fn operation_without_integer_input_used_in_loop_range_is_emitted() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            operation RepeatX(q : Qubit) : Unit {
                for i in 1..5 {
                    X(q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                RepeatX(q);
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "RepeatX",
                "X",
            ]"#]],
    );
}

#[test]
fn function_with_dynamic_constant_input_is_inlined() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            function Exponent(n : Int) : Int {
                2 ^ n
            }
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[5];
                let exp = Exponent(Length(qs));
                for i in 1..exp {
                    X(qs[0]);
                }
            }
        }
        "#,
    );

    // `Exponent` is inlined because it is a function with no dynamic variable input, so it can be fully
    // evaluated during codegen.
    assert_ir_function_names(
        &program,
        &expect![[r#"
        [
            "X",
        ]"#]],
    );
}

#[test]
fn function_with_dynamic_variable_input_is_emitted() {
    let program = get_rir_program_with_adaptive_profile(
        r#"
        namespace Test {
            function Compute(n : Int) : Int {
                2 * n
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let x = M(q) == One ? 1 | 0;
                let res = Compute(x);
                res
            }
        }
        "#,
    );

    assert_ir_function_names(
        &program,
        &expect![[r#"
            [
                "Compute",
            ]"#]],
    );
}
