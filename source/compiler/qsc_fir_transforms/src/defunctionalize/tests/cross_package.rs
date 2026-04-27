// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;

#[test]
fn analysis_apply_operation_power_ca_consumer() {
    check_analysis_with_capabilities(
        r#"
        operation Consume(apply_power_of_u : (Int, Qubit[]) => Unit is Adj + Ctl, target : Qubit[]) : Unit {
            apply_power_of_u(1, target);
        }

        operation U(qs : Qubit[]) : Unit is Adj + Ctl {
            H(qs[0]);
        }

        operation Main() : Unit {
            use qs = Qubit[1];
            Consume(ApplyOperationPowerCA(_, U, _), qs);
        }
                "#,
        adaptive_qirgen_capabilities(),
        &expect![
            "callable_params: 3\n  param: callable_id=4, path=[0], ty=((Qubit)[] => Unit is Adj + Ctl)\n  param: callable_id=6, path=[1], ty=((Qubit)[] => Unit is Adj + Ctl)\n  param: callable_id=7, path=[0], ty=((Int, (Qubit)[]) => Unit is Adj + Ctl)\ncall_sites: 5\n  site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic\n  site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic\n  site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic\n  site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic\n  site: hof=Consume<AdjCtl>, arg=Closure(target=4, Body)\nlattice states:\n  callable ApplyOperationPowerCA<(Qubit)[], AdjCtl>:\n    3: Dynamic\n    8: Dynamic\n    15: Dynamic\n    21: Dynamic"
        ],
    );
}

#[test]
fn analysis_bernstein_vazirani_sample_shape() {
    check_analysis_with_capabilities(
        r#"
        import Std.Arrays.*;
        import Std.Convert.*;
        import Std.Diagnostics.*;
        import Std.Math.*;
        import Std.Measurement.*;

        operation Main() : Unit {
            let nQubits = 10;
            let integers = [127, 238, 512];
            for integer in integers {
                let parityOperation = EncodeIntegerAsParityOperation(integer);
                let _ = BernsteinVazirani(parityOperation, nQubits);
            }
        }

        operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
            use queryRegister = Qubit[n];
            use target = Qubit();
            X(target);
            within {
                ApplyToEachA(H, queryRegister);
            } apply {
                H(target);
                Uf(queryRegister, target);
            }
            let resultArray = MResetEachZ(queryRegister);
            Reset(target);
            resultArray
        }

        operation ApplyParityOperation(bitStringAsInt : Int, xRegister : Qubit[], yQubit : Qubit) : Unit {
            let requiredBits = BitSizeI(bitStringAsInt);
            let availableQubits = Length(xRegister);
            Fact(availableQubits >= requiredBits, "enough qubits");
            for index in IndexRange(xRegister) {
                if ((bitStringAsInt &&& 2^index) != 0) {
                    CNOT(xRegister[index], yQubit);
                }
            }
        }

        function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bitStringAsInt, _, _);
        }
                "#,
        adaptive_qirgen_capabilities(),
        &expect![
            "callable_params: 2\n  param: callable_id=10, path=[0], ty=(((Qubit)[], Qubit) => Unit)\n  param: callable_id=6, path=[0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 3\n  site: hof=ApplyToEachA<Qubit, AdjCtl>, arg=Global(H, Body)\n  site: hof=ApplyToEachA<Qubit, AdjCtl>, arg=Global(H, Body)\n  site: hof=BernsteinVazirani<Empty>, arg=Closure(target=5, Body)\nlattice states:\n  callable Main:\n    7: Single(Closure(5):Body)"
        ],
    );
}

#[test]
fn analysis_deutsch_jozsa_sample_shape() {
    check_analysis_with_capabilities(
        r#"
        import Std.Diagnostics.*;
        import Std.Math.*;
        import Std.Measurement.*;

        operation Main() : Unit {
            let functionsToTest = [SimpleConstantBoolF, SimpleBalancedBoolF, ConstantBoolF, BalancedBoolF];
            for fn in functionsToTest {
                let _ = DeutschJozsa(fn, 5);
            }
        }

        operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
            use queryRegister = Qubit[n];
            use target = Qubit();
            X(target);
            H(target);
            within {
                for q in queryRegister {
                    H(q);
                }
            } apply {
                Uf(queryRegister, target);
            }
            mutable result = true;
            for q in queryRegister {
                if MResetZ(q) == One {
                    result = false;
                }
            }
            Reset(target);
            result
        }

        operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            X(target);
        }

        operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            CX(args[0], target);
        }

        operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            for i in 0..(2^Length(args)) - 1 {
                ApplyControlledOnInt(i, X, args, target);
            }
        }

        operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            for i in 0..2..(2^Length(args)) - 1 {
                ApplyControlledOnInt(i, X, args, target);
            }
        }
                "#,
        adaptive_qirgen_capabilities(),
        &expect![
            "callable_params: 2\n  param: callable_id=8, path=[1], ty=(Qubit => Unit is Adj + Ctl)\n  param: callable_id=10, path=[0], ty=(((Qubit)[], Qubit) => Unit)\ncall_sites: 6\n  site: hof=ApplyControlledOnInt<Qubit, AdjCtl>, arg=Global(X, Body)\n  site: hof=ApplyControlledOnInt<Qubit, AdjCtl>, arg=Global(X, Body)\n  site: hof=DeutschJozsa<Empty>, arg=Global(SimpleConstantBoolF, Body)\n  site: hof=DeutschJozsa<Empty>, arg=Global(SimpleBalancedBoolF, Body)\n  site: hof=DeutschJozsa<Empty>, arg=Global(ConstantBoolF, Body)\n  site: hof=DeutschJozsa<Empty>, arg=Global(BalancedBoolF, Body)\nlattice states:\n  callable Main:\n    5: Multi([SimpleConstantBoolF:Body, SimpleBalancedBoolF:Body, ConstantBoolF:Body, BalancedBoolF:Body])"
        ],
    );
}

#[test]
fn full_pipeline_handles_stdlib_apply_to_each() {
    check_pipeline(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(H, qs);
        }
        "#,
    );
}

#[test]
fn full_pipeline_handles_stdlib_apply_to_each_with_custom_intrinsic() {
    check_pipeline(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(SX, qs);
        }
        "#,
    );
}

#[test]
fn apply_to_each_body_callable_defunctionalizes() {
    check_invariants(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(H, qs);
        }
        "#,
    );
}

#[test]
fn apply_to_each_a_adjoint_callable_defunctionalizes() {
    check_invariants(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEachA(S, qs);
            Adjoint ApplyToEachA(S, qs);
        }
        "#,
    );
}

#[test]
fn apply_to_each_c_controlled_callable_defunctionalizes() {
    check_invariants(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use (ctl, qs) = (Qubit(), Qubit[3]);
            ApplyToEachC(X, qs);
        }
        "#,
    );
}

#[test]
fn apply_to_each_ca_callable_defunctionalizes() {
    check_invariants(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEachCA(S, qs);
        }
        "#,
    );
}

#[test]
fn cross_package_apply_to_each_closure_arg_defunctionalizes() {
    check_invariants(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            let angle = 1.0;
            ApplyToEach(q => Rx(angle, q), qs);
        }
        "#,
    );
}

#[test]
fn cross_package_apply_to_each_adjoint_arg_defunctionalizes() {
    check_invariants(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(Adjoint S, qs);
        }
        "#,
    );
}

#[test]
fn adjoint_cross_package_apply_to_each_ca_defunctionalizes() {
    check_invariants(
        r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            Adjoint ApplyToEachCA(S, qs);
        }
        "#,
    );
}

#[test]
fn controlled_apply_to_each_ca_keeps_body_callable_static() {
    check_pipeline(
        r#"
        open Std.Canon;

        operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            ApplyToEachCA(H, inputQubits);
        }

        operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            ApplyToEachCA(X, inputQubits);
        }

        @EntryPoint()
        operation Main() : Unit {
            use qs = Qubit[3];
            let register = [qs[1], qs[2]];
            Controlled PrepareUniform([qs[0]], register);
            Controlled PrepareAllOnes([qs[0]], register);
        }
        "#,
    );
}

#[test]
fn cross_package_mapped_defunctionalizes() {
    check_pipeline(
        r#"
        open Std.Arrays;
        function Double(x : Int) : Int { x * 2 }
        @EntryPoint()
        operation Main() : Unit {
            let arr = [1, 2, 3];
            let _ = Mapped(Double, arr);
        }
        "#,
    );
}

#[test]
fn cross_package_for_each_defunctionalizes() {
    check_pipeline(
        r#"
        open Std.Arrays;
        operation Main() : Unit {
            use qs = Qubit[3];
            ForEach(H, qs);
        }
        "#,
    );
}

#[test]
fn stdlib_hof_specialized_with_concrete_callable() {
    check(
        r#"
        open Microsoft.Quantum.Arrays;

        operation Main() : Int[] {
            let arr = [1, 2, 3];
            Mapped(x -> x + 1, arr)
        }
        "#,
        &expect![[r#"
            <lambda>: input_ty=(Int,)
            Length: input_ty=(Int)[]
            Main: input_ty=Unit
            Mapped<Int, Int>{closure}: input_ty=(Int)[]"#]],
    );
}

#[test]
fn lambda_expression_sample_shape_has_no_defunctionalization_errors() {
    check_errors(
        r#"
        import Std.Arrays.*;

        operation Main() : Unit {
            let add = (x, y) -> x + y;
            let _ = add(2, 3);

            use control = Qubit();
            let cnotOnControl = q => CNOT(control, q);

            let intArray = [1, 2, 3, 4, 5];
            let _ = Fold(add, 0, intArray);
            let _ = Mapped(x -> x + 1, intArray);
        }
        "#,
        &expect!["(no error)"],
    );
}

#[test]
fn partial_application_sample_shape_has_no_defunctionalization_errors() {
    check_errors(
        r#"
        import Std.Arrays.*;

        function Main() : Unit {
            let incrementByOne = Add(_, 1);
            let incrementByOneLambda = x -> Add(x, 1);

            let _ = incrementByOne(4);

            let sumAndAddOne = AddMany(_, _, _, 1);
            let sumAndAddOneLambda = (a, b, c) -> AddMany(a, b, c, 1);

            let intArray = [1, 2, 3, 4, 5];
            let _ = Mapped(Add(_, 1), intArray);
        }

        function Add(x : Int, y : Int) : Int {
            return x + y;
        }

        function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
            return a + b + c + d;
        }
        "#,
        &expect!["(no error)"],
    );
}
