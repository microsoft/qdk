// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the defunctionalization pre-pass rewrites.
//!
//! The pre-pass runs two key optimizations before collecting call sites:
//! 1. Promotes single-use immutable callable locals to direct item references
//! 2. Replaces identity closures `(args) => f(args)` with direct references to `f`

use super::*;
use expect_test::expect;

mod single_use_callable_local_promotion {
    use super::*;

    /// Single-use callable local with simple item reference should be promoted.
    #[test]
    fn promote_simple_item_reference() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            op(q);
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }

    /// Single-use callable local in HOF call should be promoted.
    #[test]
    fn promote_single_use_in_hof_call() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Multiple-use callable local still resolves through the later analysis.
    #[test]
    fn multiple_use_callable_local_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(op, q);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local captured by an identity closure still resolves to its item.
    #[test]
    fn callable_local_captured_by_identity_closure_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(q1 => op(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Mutable callable local with a static value still resolves through analysis.
    #[test]
    fn mutable_callable_local_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with identity-closure initializer should be simplified.
    #[test]
    fn callable_local_with_identity_closure_initializer_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = q1 => H(q1);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with a partial-application initializer resolves through closure lifting.
    #[test]
    fn no_promote_partial_application_initializer_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Parametrized(angle : Double, q : Qubit) : Unit {
            Rz(angle, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 0.5;
            let op = Parametrized(angle, _);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            <lambda>: input_ty=(Double, Qubit)
            ApplyOp<Empty>{closure}: input_ty=(Qubit, Double)
            Main: input_ty=Unit
            Parametrized: input_ty=(Double, Qubit)"#]],
        );
    }

    /// Single-use callable local in nested scope should be promoted.
    #[test]
    fn promote_in_nested_scope() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            if true {
                let op = H;
                ApplyOp(op, q);
            }
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Unused callable local (zero uses) is irrelevant but shouldn't cause issues.
    #[test]
    fn no_promote_zero_uses() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ()
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }

    /// Single-use callable local with non-callable type should NOT be promoted.
    #[test]
    fn no_promote_non_callable_type() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let x = 42;
            let y = x;
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }
}

mod identity_closure_peephole_optimization {
    use super::*;

    /// Basic identity closure `(q) => H(q)` should be replaced with `H`.
    #[test]
    fn identity_closure_basic() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure with multiple parameters should be replaced.
    #[test]
    fn identity_closure_multiple_params() {
        check(
            r#"
        operation ApplyTwo(f : (Qubit, Qubit) => Unit, q1 : Qubit, q2 : Qubit) : Unit {
            f(q1, q2);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            ApplyTwo((control, target) => CNOT(control, target), q1, q2);
        }
        "#,
            &expect![[r#"
            ApplyTwo<Empty>{CNOT}: input_ty=(Qubit, Qubit)
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure with captured variable should be replaced.
    #[test]
    fn identity_closure_with_capture() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(q1 => myH(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Adjoint identity closure `(q) => Adjoint H(q)` should be optimized.
    #[test]
    fn identity_closure_adjoint() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit is Adj, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => Adjoint H(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Adj>{Adj H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Controlled identity closure `(q) => Controlled X([], q)` should be optimized.
    #[test]
    fn identity_closure_controlled() {
        check(
            r#"
        operation ApplyOp(f : (Qubit[], Qubit) => Unit is Ctl, q : Qubit) : Unit {
            f([], q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp((ctrls, tgt) => Controlled X(ctrls, tgt), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Ctl>{Ctl X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Non-identity closure should NOT be optimized (argument reordering).
    #[test]
    fn no_optimize_reordered_args() {
        check(
            r#"
        operation ApplyTwo(f : (Qubit, Qubit) => Unit, q1 : Qubit, q2 : Qubit) : Unit {
            f(q1, q2);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            ApplyTwo((a, b) => H(b), q1, q2);
        }
        "#,
            &expect![[r#"
            <lambda>: input_ty=((Qubit, Qubit),)
            ApplyTwo<Empty>{closure}: input_ty=(Qubit, Qubit)
            Main: input_ty=Unit"#]],
        );
    }

    /// Non-identity closure with capture in args should NOT be optimized.
    #[test]
    fn no_optimize_capture_in_args() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myQ = q;
            ApplyOp(q1 => H(myQ), q);
        }
        "#,
            &expect![[r#"
            <lambda>: input_ty=(Qubit, Qubit)
            ApplyOp<Empty>{closure}: input_ty=(Qubit, Qubit)
            Main: input_ty=Unit"#]],
        );
    }

    /// Closure that does not forward its parameter should NOT be optimized.
    #[test]
    fn no_optimize_non_forwarded_param() {
        check(
            r#"
        operation ApplyOp(f : (Unit => Unit), _ : Unit) : Unit {
            f(());
        }
        operation Main() : Unit {
            use other = Qubit();
            ApplyOp(u => H(other), ());
            Reset(other);
        }
        "#,
            &expect![[r#"
            <lambda>: input_ty=(Qubit, Unit)
            ApplyOp<Empty>{closure}: input_ty=(Unit, Qubit)
            Main: input_ty=Unit"#]],
        );
    }

    /// Closure with multiple statements should NOT be optimized (not identity).
    #[test]
    fn no_optimize_multiple_statements() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => { H(q1); X(q1) }, q);
        }
        "#,
            &expect![[r#"
            <lambda>: input_ty=(Qubit,)
            ApplyOp<Empty>{closure}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Closure body that's not a call should NOT be optimized.
    #[test]
    fn no_optimize_non_call_body() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Int, q : Qubit) : Int {
            f(q)
        }
        operation Main() : Unit {
            use q = Qubit();
            let result = ApplyOp(q1 => 42, q);
        }
        "#,
            &expect![[r#"
            <lambda>: input_ty=(Qubit,)
            ApplyOp<Empty>{closure}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }
}

mod combined_promotion_and_peephole_optimizations {
    use super::*;

    /// Single-use local with identity closure should both be optimized.
    #[test]
    fn combined_promotion_and_identity_closure() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = q1 => H(q1);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Multiple single-use locals with identity closures.
    #[test]
    fn multiple_promoted_identity_closures() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op1 = q1 => H(q1);
            let op2 = q1 => X(q1);
            ApplyOp(op1, q);
            ApplyOp(op2, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            ApplyOp<Empty>{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Promoted local used in identity closure.
    #[test]
    fn promoted_local_in_identity_closure() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(q1 => myH(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }
}

mod edge_cases_and_complex_scenarios {
    use super::*;

    /// Identity closure with adjoint and captured variable.
    #[test]
    fn identity_closure_adjoint_captured() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit is Adj, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(q1 => Adjoint op(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Adj>{Adj H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Complex HOF with mixed promoted and identity closures.
    #[test]
    fn complex_hof_mixed_optimizations() {
        check(
            r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyTwo(op, q1 => X(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyTwo<AdjCtl, Empty>{H}{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure with parameter passed to a nested operation.
    #[test]
    fn identity_closure_param_to_nested_op() {
        check(
            r#"
        operation Inner(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Outer(g : Qubit => Unit, q : Qubit) : Unit {
            Inner(g, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(q1 => H(q1), q);
        }
        "#,
            &expect![[r#"
            Inner<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit
            Outer<Empty>{H}: input_ty=Qubit"#]],
        );
    }

    /// Single-use callable local assigned from another single-use callable local (chain).
    #[test]
    fn promoted_local_chain() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op1 = H;
            let op2 = op1;
            ApplyOp(op2, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure capturing a single-use promoted local.
    #[test]
    fn identity_closure_captures_promoted_local() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            let op = q1 => myH(q1);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Intrinsic callable should not cause issues in identity closure detection.
    #[test]
    fn identity_closure_with_intrinsic() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with discard pattern should NOT be promoted.
    #[test]
    fn no_promote_discard_pattern() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let _ = H;
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with tuple destructuring still resolves through analysis.
    #[test]
    fn tuple_destructured_callable_local_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let (op, _) = (H, X);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }
}

mod parameter_extraction_and_validation_helpers {
    use super::*;

    /// Identity closure with tuple of single parameters should work.
    #[test]
    fn identity_closure_tuple_params() {
        check(
            r#"
        operation ApplyTwo(f : (Int, Qubit) => Unit, q : Qubit, n : Int) : Unit {
            f(n, q);
        }
        operation UseIntQubit(i : Int, q : Qubit) : Unit {
            if i == 42 {
                H(q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            let n = 42;
            ApplyTwo((i, q1) => UseIntQubit(i, q1), q, n);
        }
        "#,
            &expect![[r#"
            ApplyTwo<Empty>{UseIntQubit}: input_ty=(Qubit, Int)
            Main: input_ty=Unit
            UseIntQubit: input_ty=(Int, Qubit)"#]],
        );
    }
}

mod nested_function_scopes {
    use super::*;

    /// Single-use callable local in nested function scope.
    #[test]
    fn promote_in_nested_function() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Outer() : Unit {
            use q = Qubit();
            if true {
                let op = H;
                ApplyOp(op, q);
            }
        }
        operation Main() : Unit {
            Outer();
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit
            Outer: input_ty=Unit"#]],
        );
    }

    /// Identity closure in nested function scope.
    #[test]
    fn identity_closure_nested_function() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Outer() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        operation Main() : Unit {
            Outer();
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit
            Outer: input_ty=Unit"#]],
        );
    }
}
