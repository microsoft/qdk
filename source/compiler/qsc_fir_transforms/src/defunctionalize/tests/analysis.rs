// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::defunctionalize::analysis::{LocalState, resolve_captures};

use super::*;
use expect_test::expect;
use qsc_data_structures::index_map::IndexMap;
use qsc_fir::fir::{LocalVarId, Package};
use rustc_hash::FxHashSet;

#[test]
fn analysis_no_callable_params() {
    check_analysis(
        "operation Main() : Unit { }",
        &expect![[r#"
            callable_params: 0
            call_sites: 0"#]],
    );
}

#[test]
fn analysis_single_callable_param() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn analysis_multiple_callable_params() {
    check_analysis(
        r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyTwo(H, X, q);
        }
        "#,
        &expect![[r#"
            callable_params: 2
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
              param: callable_id=3, path=[1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyTwo<AdjCtl, AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyTwo<AdjCtl, AdjCtl>, arg=Global(X, Body)"#]],
    );
}

#[test]
fn analysis_callable_param_in_tuple() {
    check_analysis(
        r#"
        operation ApplySecond(q : Qubit, op : Qubit => Unit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplySecond(q, H);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplySecond<AdjCtl>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn analysis_global_callable_arg() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(X, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=4, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)"#]],
    );
}

#[test]
fn analysis_closure_callable_arg() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=4, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn analysis_adjoint_callable_arg() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Adjoint S, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(S, Adj)"#]],
    );
}

#[test]
fn analysis_controlled_callable_arg() {
    check_analysis(
        r#"
        operation ApplyOp(op : (Qubit[], Qubit) => Unit is Ctl, q : Qubit) : Unit {
            op([], q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Controlled X, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=4, path=[0], ty=(((Qubit)[], Qubit) => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Ctl)"#]],
    );
}

#[test]
fn analysis_multiple_call_sites_same_hof() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)"#]],
    );
}

#[test]
fn analysis_single_assignment_local_traced() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(myH, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
}

#[test]
fn analysis_dynamic_callable_detected() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            op = X;
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Single(X:Body)"#]],
    );
}

#[test]
fn udt_field_single_level_direct() {
    check_analysis(
        r#"
        struct Config { Apply : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Apply = H };
            ApplyOp(config.Apply, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=5, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn udt_field_via_let_binding() {
    check_analysis(
        r#"
        struct Config { Apply : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Apply = H };
            let f = config.Apply;
            ApplyOp(f, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=5, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)
            lattice states:
              callable Main:
                3: Single(H:Body)"#]],
    );
}

#[test]
fn udt_field_in_hof_body() {
    check_analysis(
        r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#,
        &expect![[r#"
            callable_params: 2
              param: callable_id=6, path=[0], ty=(Qubit => Unit)
              param: callable_id=3, path=[0, 0], ty=(Qubit => Unit)
            call_sites: 2
              site: hof=RunWithConfig, arg=Global(H, Body)
              site: hof=ApplyOp<Empty>, arg=Dynamic"#]],
    );
}

#[test]
fn udt_field_in_hof_body_defunctionalizes_end_to_end() {
    check(
        r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit
            RunWithConfig{H}: input_ty=(Unit, Qubit)"#]],
    );
}

#[test]
fn udt_field_in_hof_body_full_pipeline_invariants() {
    check_pipeline(
        r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#,
    );
}

#[test]
fn udt_field_nested_two_level() {
    check_analysis(
        r#"
        struct InnerConfig { Apply : Qubit => Unit }
        struct OuterConfig { Inner : InnerConfig, Label : Int }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let outer = new OuterConfig {
                Inner = new InnerConfig { Apply = H },
                Label = 0,
            };
            ApplyOp(outer.Inner.Apply, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=6, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn udt_field_nested_two_level_defunctionalizes_end_to_end() {
    check(
        r#"
        struct InnerConfig { Apply : Qubit => Unit }
        struct OuterConfig { Inner : InnerConfig, Label : Int }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let outer = new OuterConfig {
                Inner = new InnerConfig { Apply = H },
                Label = 0,
            };
            ApplyOp(outer.Inner.Apply, q);
        }
        "#,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn udt_field_closure_value() {
    check_analysis(
        r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            let config = new Config { Op = q1 => Rx(angle, q1) };
            ApplyOp(config.Op, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=6, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=4, Body)"#]],
    );
}

#[test]
fn udt_field_from_parameter_dynamic() {
    check_analysis(
        r#"
        struct Config { Op : Qubit => Unit }
        operation MakeConfig() : Config {
            new Config { Op = H }
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let c = MakeConfig();
            ApplyOp(c.Op, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=6, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Dynamic"#]],
    );
}

#[test]
fn identity_closure_over_global_callable_collapses() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(a => H(a), q);
        }
        "#,
    );
}

#[test]
fn identity_closure_wrapping_param() {
    check_invariants(
        r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(action : Qubit => Unit, q : Qubit) : Unit {
            Inner(a => action(a), q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(H, q);
        }
        "#,
    );
}

#[test]
fn non_identity_closure_preserved() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(a => { H(a); X(a); }, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=4, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=3, Body)"#]],
    );
}

#[test]
fn identity_closure_tuple_args() {
    check_invariants(
        r#"
        operation Pair(a : Qubit, b : Qubit) : Unit {
            H(a);
            H(b);
        }
        operation HOF2(op : (Qubit, Qubit) => Unit, q1 : Qubit, q2 : Qubit) : Unit {
            op(q1, q2);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            HOF2((a, b) => Pair(a, b), q1, q2);
        }
        "#,
    );
}

#[test]
fn closure_with_captures_not_identity() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(a => Rx(angle, a), q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=4, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=3, Body)"#]],
    );
}

#[test]
fn partial_application_lambda_analysis_shape() {
    check(
        r#"
        operation ApplyOp(op : Qubit[] => Unit, register : Qubit[]) : Unit {
            op(register);
        }
        operation Shifted(shift : Int, register : Qubit[]) : Unit {
            ApplyXorInPlace(shift, register);
        }
        operation Main() : Unit {
            use register = Qubit[2];
            ApplyOp(register => Shifted(1, register), register);
        }
        "#,
        &expect![
            "<lambda>: input_ty=((Qubit)[],)\nApplyOp<Empty>{closure}: input_ty=(Qubit)[]\nMain: input_ty=Unit\nShifted: input_ty=(Int, (Qubit)[])"
        ],
    );
}

#[test]
fn reaching_def_mutable_single_assign() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            ApplyOp(op, q);
        }
        "#,
    );
}

#[test]
fn reaching_def_conditional_both_known() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            ApplyOp(f, q);
        }
        "#,
    );
}

#[test]
fn reaching_def_mutable_multi_assign() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; }
            ApplyOp(op, q);
        }
        "#,
    );
}

#[test]
fn reaching_def_mutable_both_branches() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; } else { set op = S; }
            ApplyOp(op, q);
        }
        "#,
    );
}

#[test]
fn reaching_def_mutable_in_loop_dynamic() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            for _ in 0..3 { set op = X; }
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn analysis_closure_through_multiple_levels() {
    check_analysis(
        r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Outer(op : Qubit => Unit, q : Qubit) : Unit { Inner(op, q); }
        operation Main() : Unit {
            use q = Qubit();
            Outer(q1 => H(q1), q);
        }
        "#,
        &expect![[r#"
            callable_params: 2
              param: callable_id=5, path=[0], ty=(Qubit => Unit)
              param: callable_id=7, path=[0], ty=(Qubit => Unit)
            call_sites: 2
              site: hof=Inner<Empty>, arg=Dynamic
              site: hof=Outer<Empty>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn analysis_callable_returned_from_function() {
    check_analysis(
        r#"
        operation GetOp() : Qubit => Unit { H }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let op = GetOp();
            ApplyOp(op, q);
        }
        "#,
        &expect![
            "callable_params: 1\n  param: callable_id=5, path=[0], ty=(Qubit => Unit)\ncall_sites: 1\n  site: hof=ApplyOp<Empty>, arg=Global(H, Body)\nlattice states:\n  callable Main:\n    2: Single(H:Body)"
        ],
    );
}

#[test]
fn callable_from_function_return_resolves_statically() {
    check_invariants(
        r#"
        function GetOp() : (Qubit => Unit) { H }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(GetOp(), q);
        }
        "#,
    );
}

#[test]
fn callable_returning_partial_application_resolves_statically() {
    check_invariants(
        r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation MakeParity(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bits, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let op = MakeParity([true]);
            ApplyOp(op, register, target);
        }
        "#,
    );
}

#[test]
fn analysis_callable_returning_partial_application_with_explicit_return() {
    check_analysis(
        r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation MakeParity(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bits, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let op = MakeParity([true]);
            ApplyOp(op, register, target);
        }
        "#,
        &expect![
            "callable_params: 1\n  param: callable_id=7, path=[0], ty=(((Qubit)[], Qubit) => Unit)\ncall_sites: 1\n  site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)\nlattice states:\n  callable Main:\n    3: Single(Closure(5):Body)"
        ],
    );
}

#[test]
fn callable_returning_partial_application_from_local_arg_preserves_capture_expr() {
    check_invariants(
        r#"
        operation UseOracle(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
            use register = Qubit[n];
            use target = Qubit();
            oracle(register, target);
            Reset(target);
            ResetAll(register);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation Encode(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            ApplyParityOperation(bits, _, _)
        }

        operation Main() : Unit {
            let bits = [true];
            let oracle = Encode(bits);
            UseOracle(oracle, Length(bits));
        }
        "#,
    );
}

#[test]
fn callable_from_array_index_resolves_statically() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let ops = [H, X];
            ApplyOp(ops[0], q);
        }
        "#,
    );
}

#[test]
fn callable_returning_partial_application_from_function_resolves_statically() {
    check_invariants(
        r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
            if value == 1 {
                CNOT(register[0], target);
            }
        }

        function Encode(value : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(value, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let value = 1;
            let oracle = Encode(value);
            ApplyOp(oracle, register, target);
        }
        "#,
    );
}

#[test]
fn analysis_callable_from_constant_callable_array_loop() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let ops = [H, X];
            for op in ops {
                ApplyOp(op, q);
            }
        }
                "#,
        &expect![
            "callable_params: 1\n  param: callable_id=4, path=[0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 2\n  site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)\n  site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)\nlattice states:\n  callable Main:\n    7: Multi([H:Body, X:Body])"
        ],
    );
}

#[test]
fn analysis_callable_returning_partial_application_from_function_in_loop() {
    check_analysis(
        r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
            if value == 1 {
                CNOT(register[0], target);
            }
        }

        function Encode(value : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(value, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            for value in [1, 2] {
                let oracle = Encode(value);
                ApplyOp(oracle, register, target);
            }
        }
                "#,
        &expect![
            "callable_params: 1\n  param: callable_id=8, path=[0], ty=(((Qubit)[], Qubit) => Unit)\ncall_sites: 1\n  site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)\nlattice states:\n  callable Main:\n    8: Single(Closure(5):Body)"
        ],
    );
}

#[test]
fn reaching_def_mutable_in_while_loop() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            mutable n = 3;
            while n > 0 {
                op = X;
                n -= 1;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn analysis_nested_callable_in_tuple_param() {
    check_analysis(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![
            "callable_params: 1\n  param: callable_id=3, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 1\n  site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"
        ],
    );
}

#[test]
fn analysis_nested_callable_second_element() {
    check_analysis(
        r#"
        operation Wrapper(pair : (Int, Qubit => Unit), q : Qubit) : Unit {
            let (_, op) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((42, H), q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0, 1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn analysis_nested_callable_single_param_supported() {
    check_analysis(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int)) : Unit {
            let (op, _) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((H, 42));
        }
        "#,
        &expect![
            "callable_params: 1\n  param: callable_id=3, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 1\n  site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"
        ],
    );
}

#[test]
fn analysis_branch_split_nested_callable_in_tuple() {
    check_analysis(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            Wrapper((f, 42), q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)
              site: hof=Wrapper<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Multi([H:Body, X:Body])"#]],
    );
}

#[test]
fn analysis_nested_callable_single_param_second_element_supported() {
    check_analysis(
        r#"
        operation Wrapper(pair : (Int, Qubit => Unit)) : Unit {
            let (_, op) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((42, H));
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0, 1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
}

#[test]
fn analysis_nested_callable_single_param_recursive_supported() {
    check_analysis(
        r#"
        operation Wrapper(bundle : (((Qubit => Unit, Int), Double), Qubit)) : Unit {
            let (((op, n), angle), q) = bundle;
            let _ = n;
            let _ = angle;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((((H, 42), 1.0), q));
        }
        "#,
        &expect![
            "callable_params: 1\n  param: callable_id=3, path=[0, 0, 0, 0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 1\n  site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"
        ],
    );
}

#[test]
fn identity_closure_adjoint_wrapped_collapses() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => Adjoint S(q1), q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=4, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(S, Adj)"#]],
    );
}

#[test]
fn single_use_immutable_local_promoted() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
}

#[test]
fn multi_use_immutable_local_not_promoted() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            let op = H;
            ApplyOp(op, q1);
            ApplyOp(op, q2);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                3: Single(H:Body)"#]],
    );
}

#[test]
fn mutable_local_not_promoted() {
    check_analysis(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            op = X;
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            callable_params: 1
              param: callable_id=3, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Single(X:Body)"#]],
    );
}

#[test]
fn analysis_conditional_callable_binding_produces_multi_lattice() {
    check_analysis(
        r#"
        operation ApplyConditional(power : Int, target : Qubit) : Unit {
            let u = if power >= 0 { S } else { Adjoint S };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyConditional(3, q);
        }
        "#,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            lattice states:
              callable ApplyConditional:
                3: Multi([S:Body, S:Adj])"#]],
    );
}

#[test]
fn analysis_callable_from_tuple_destructured_array_iteration() {
    check_analysis(
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                let arr = [(S, PauliZ), (T, PauliX)];
                for (op, _basis) in arr {
                    use q = Qubit();
                    op(q);
                }
            }
        }
        "#,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            lattice states:
              callable Main:
                5: Multi([S:Body, T:Body])"#]],
    );
}

#[test]
fn resolve_captures_missing_binding_returns_none() {
    let package = Package {
        items: IndexMap::new(),
        entry: None,
        entry_exec_graph: qsc_fir::fir::ExecGraph::default(),
        blocks: IndexMap::new(),
        exprs: IndexMap::new(),
        pats: IndexMap::new(),
        stmts: IndexMap::new(),
    };
    let locals = LocalState::default();
    let missing_var = LocalVarId::from(99usize);

    let captures = resolve_captures(&package, &locals, &[missing_var], &FxHashSet::default());

    assert!(
        captures.is_none(),
        "missing capture bindings should degrade analysis instead of panicking"
    );
}
