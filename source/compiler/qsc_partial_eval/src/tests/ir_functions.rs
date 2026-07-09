// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for QIR "IR function" emission in the partial evaluator. Eligible user-package,
//! non-composite specializations returning Unit or a scalar (Int/Double/Bool) are emitted as
//! `Regular` RIR callables with bodies and called via `Instruction::Call` instead of being inlined.
//! Scalar-returning callables bind a fresh call-site output variable. Every ineligible callable
//! (including `Result`/`Qubit` returns) continues to inline, preserving the previous behavior.

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
                Call id(3), args( Variable(0, Double), Variable(1, Qubit), )
                Return
            Block 2:Block:
                Call id(4), args( Variable(2, Double), Variable(3, Qubit), )
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
                Variable(5, Integer) = Call id(2), args( Qubit(0), )
                Call id(5), args( Variable(5, Integer), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Variable(1, Integer) = Alloca
                Variable(1, Integer) = Store Integer(0)
                Call id(3), args( Variable(0, Qubit), Result(0), )
                Variable(2, Boolean) = Call id(4), args( Result(0), )
                Branch Variable(2, Boolean), 2, 3
            Block 2:Block:
                Variable(1, Integer) = Store Integer(5)
                Jump(3)
            Block 3:Block:
                Variable(7, Integer) = Load Variable(1, Integer)
                Variable(4, Integer) = Add Variable(7, Integer), Integer(1)
                Variable(1, Integer) = Store Variable(4, Integer)
                Variable(9, Integer) = Load Variable(1, Integer)
                Return Variable(9, Integer)"#]],
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
                Variable(5, Double) = Call id(2), args( Integer(5), Double(1.5), Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Call id(3), args( Variable(1, Double), Variable(2, Qubit), )
                Return Variable(1, Double)
            Block 2:Block:
                Call id(4), args( Variable(3, Double), Variable(4, Qubit), )
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
                Variable(5, Double) = Call id(2), args( Integer(5), Double(1.5), Qubit(0), )
                Call id(5), args( Integer(0), Tag(0, 3), )
                Return Integer(0)
            Block 1:Block:
                Call id(3), args( Variable(1, Double), Variable(2, Qubit), )
                Return Variable(1, Double)
            Block 2:Block:
                Call id(4), args( Variable(3, Double), Variable(4, Qubit), )
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
fn operation_without_integer_input_used_in_loop_range_is_inlined() {
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
