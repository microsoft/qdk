// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;

#[test]
fn specialize_single_global_callable() {
    check(
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
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_two_different_callables() {
    check(
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
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            ApplyOp<AdjCtl>{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_same_callable_reuse() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(H, q);
        }
        "#,
        &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_no_hof_unchanged() {
    check(
        r#"
        operation Foo(q : Qubit) : Unit {
            H(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Foo(q);
        }
        "#,
        &expect![[r#"
            Foo: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_closure_no_captures() {
    check(
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
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_closure_with_captures() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
        &expect![[r#"
            <lambda>: input_ty=(Double, Qubit)
            ApplyOp<Empty>{closure}: input_ty=(Qubit, Double)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_closure_capture_types_preserved() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let n = 3;
            ApplyOp(q1 => { for _ in 0..n { H(q1); } }, q);
        }
        "#,
        &expect![[r#"
            <lambda>: input_ty=(Int, Qubit)
            ApplyOp<Empty>{closure}: input_ty=(Qubit, Int)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_creation_site_adjoint() {
    check(
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
            ApplyOp<AdjCtl>{Adj S}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_body_side_adjoint() {
    check(
        r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(S, q);
        }
        "#,
        &expect![[r#"
            ApplyAdj<AdjCtl>{S}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_double_adjoint_cancels() {
    check(
        r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(Adjoint S, q);
        }
        "#,
        &expect![[r#"
            ApplyAdj<AdjCtl>{Adj S}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_body_side_controlled() {
    check(
        r#"
        operation ApplyCtl(op : Qubit => Unit is Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtl(X, ctl, q);
        }
        "#,
        &expect![[r#"
            ApplyCtl<AdjCtl>{X}: input_ty=(Qubit, Qubit)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_body_controlled_adjoint_nested() {
    check(
        r#"
        operation ApplyCtlAdj(op : Qubit => Unit is Adj + Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled Adjoint op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtlAdj(S, ctl, q);
        }
        "#,
        &expect![[r#"
            ApplyCtlAdj<AdjCtl>{S}: input_ty=(Qubit, Qubit)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_creation_adjoint_body_controlled() {
    check(
        r#"
        operation ApplyCtl(op : Qubit => Unit is Adj + Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtl(Adjoint S, ctl, q);
        }
        "#,
        &expect![[r#"
            ApplyCtl<AdjCtl>{Adj S}: input_ty=(Qubit, Qubit)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_hof_with_adj_autogen() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            body ... { op(q); }
            adjoint auto;
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(S, q);
            Adjoint ApplyOp(S, q);
        }
        "#,
        &expect![[r#"
            ApplyOp<AdjCtl>{S}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_hof_with_ctl_autogen() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit is Ctl, q : Qubit) : Unit is Ctl {
            body ... { op(q); }
            controlled auto;
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(X, q);
        }
        "#,
        &expect![[r#"
            ApplyOp<AdjCtl>{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_hof_with_adj_ctl_autogen() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {
            body ... { op(q); }
            adjoint auto;
            controlled auto;
            controlled adjoint auto;
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyOp(S, q);
        }
        "#,
        &expect![[r#"
            ApplyOp<AdjCtl>{S}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_single_assignment_local() {
    check(
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
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn defunctionalized_call_site_drops_callable_argument() {
    check(
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
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn rewrite_closure_capture_args_inserted() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
        &expect![[r#"
            <lambda>: input_ty=(Double, Qubit)
            ApplyOp<Empty>{closure}: input_ty=(Qubit, Double)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn multiple_callable_parameters_specialize_independently() {
    check(
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
            ApplyTwo<AdjCtl, AdjCtl>{H}{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn capture_local_ids_are_reasonable() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
    );
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    for (_, pat) in &package.pats {
        if let fir::PatKind::Bind(ident) = &pat.kind {
            let id: u32 = ident.id.into();
            assert!(
                id < 10_000,
                "LocalVarId {id} is unreasonably large -- capture IDs should be sequential, not u32::MAX-based"
            );
        }
    }
}

#[test]
fn pipeline_with_captures_no_sroa_panic() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let pair = (1.0, 2.0);
            let (a, b) = pair;
            ApplyOp(q1 => Rx(a + b, q1), q);
        }
        "#,
        PipelineStage::Full,
    );
}

#[test]
fn multiple_captures_sequential_ids() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let a = 1.0;
            let b = 2.0;
            let c = 3.0;
            ApplyOp(q1 => { Rx(a, q1); Ry(b, q1); Rz(c, q1); }, q);
        }
        "#,
    );
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut capture_ids: Vec<u32> = Vec::new();
    for (_, pat) in &package.pats {
        if let fir::PatKind::Bind(ident) = &pat.kind
            && ident.name.starts_with("__capture_")
        {
            let id: u32 = ident.id.into();
            capture_ids.push(id);
        }
    }

    assert!(
        capture_ids.len() >= 3,
        "expected at least 3 capture bindings, found {}",
        capture_ids.len()
    );

    for &id in &capture_ids {
        assert!(id < 10_000, "capture LocalVarId {id} is unreasonably large");
    }

    capture_ids.sort_unstable();
    for window in capture_ids.windows(2) {
        assert_eq!(
            window[1] - window[0],
            1,
            "capture IDs should be sequential, got {} and {}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn specialize_closure_capturing_immutable_variable() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
        &expect![[r#"
            <lambda>: input_ty=(Double, Qubit)
            ApplyOp<Empty>{closure}: input_ty=(Qubit, Double)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_closure_in_while_loop_body() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable n = 3;
            while n > 0 {
                ApplyOp(q1 => H(q1), q);
                n -= 1;
            }
        }
        "#,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn specialize_multiple_closures_same_signature() {
    check(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
            ApplyOp(q1 => X(q1), q);
        }
        "#,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            ApplyOp<Empty>{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn branch_split_two_callees() {
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
fn branch_split_three_callees() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } elif false { X } else { S };
            ApplyOp(f, q);
        }
        "#,
    );
}

#[test]
fn branch_split_mutable_conditional() {
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
fn branch_split_nested_callable_in_tuple() {
    check_invariants(
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
    );
}

#[test]
fn branch_split_nested_callable_in_tuple_args_consistency() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
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
    );
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut mismatches = Vec::new();
    for (expr_id, expr) in &package.exprs {
        if let fir::ExprKind::Call(_callee_id, args_id) = &expr.kind {
            let args_expr = package.get_expr(*args_id);
            if let fir::ExprKind::Tuple(elements) = &args_expr.kind
                && let qsc_fir::ty::Ty::Tuple(type_elems) = &args_expr.ty
            {
                if elements.len() != type_elems.len() {
                    mismatches.push(format!(
                        "Call expr {expr_id}: args tuple has {} elements but type has {} elements",
                        elements.len(),
                        type_elems.len()
                    ));
                }
                for (i, (&elem_id, ty_elem)) in elements.iter().zip(type_elems.iter()).enumerate() {
                    let elem_expr = package.get_expr(elem_id);
                    let elem_is_tuple = matches!(elem_expr.kind, fir::ExprKind::Tuple(_));
                    let ty_is_tuple = matches!(ty_elem, qsc_fir::ty::Ty::Tuple(_));
                    if elem_is_tuple != ty_is_tuple {
                        mismatches.push(format!(
                            "Call expr {expr_id}: args[{i}] is_tuple={elem_is_tuple} but type is_tuple={ty_is_tuple} (elem_ty={}, type_elem={ty_elem})",
                            elem_expr.ty,
                        ));
                    }
                }
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "Type/value mismatches in branch-split args:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn branch_split_nested_callable_full_pipeline() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
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
        PipelineStage::Full,
    );
}

#[test]
fn specialize_nested_callable_first_element() {
    check(
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
        &expect![[r#"
            Main: input_ty=Unit
            Wrapper<AdjCtl>{H}: input_ty=(Int, Qubit)"#]],
    );
}

#[test]
fn specialize_nested_callable_second_element() {
    check(
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
            Main: input_ty=Unit
            Wrapper<AdjCtl>{H}: input_ty=(Int, Qubit)"#]],
    );
}

#[test]
fn specialize_nested_callable_both_fields_used() {
    check(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, n) = pair;
            op(q);
            let _ = n;
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            Main: input_ty=Unit
            Wrapper<AdjCtl>{H}: input_ty=(Int, Qubit)"#]],
    );
}

#[test]
fn specialize_nested_callable_transitive_alias() {
    check(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            let f = op;
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            Main: input_ty=Unit
            Wrapper<AdjCtl>{H}: input_ty=(Int, Qubit)"#]],
    );
}

#[test]
fn specialize_nested_callable_invariants() {
    check_invariants(
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
    );
}

#[test]
fn specialize_nested_callable_full_pipeline() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
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
        PipelineStage::Full,
    );
}

#[test]
fn branch_split_nested_callable_adj_ctl_args_consistency() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Op1(q : Qubit) : Unit is Adj + Ctl { H(q); }
        operation Op2(q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation Wrapper(pair : (Qubit => Unit is Adj + Ctl, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let b = true;
            let f = if b { Op1 } else { Op2 };
            Wrapper((f, 42), q);
        }
        "#,
    );
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut mismatches = Vec::new();
    for (expr_id, expr) in &package.exprs {
        if let fir::ExprKind::Call(_callee_id, args_id) = &expr.kind {
            let args_expr = package.get_expr(*args_id);
            if let fir::ExprKind::Tuple(elements) = &args_expr.kind
                && let qsc_fir::ty::Ty::Tuple(type_elems) = &args_expr.ty
                && elements.len() != type_elems.len()
            {
                mismatches.push(format!(
                    "Call expr {expr_id}: args tuple has {} elements but type has {} elements",
                    elements.len(),
                    type_elems.len()
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "Type/value mismatches in branch-split args:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn closure_with_multiple_captures_threads_all_captures() {
    check(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let angle1 = 1.0;
            let angle2 = 2.0;
            let myOp = (q) => { Rx(angle1, q); Ry(angle2, q); };
            Apply(myOp, q);
        }
        "#,
        &expect![[r#"
            <lambda>: input_ty=(Double, Double, Qubit)
            Apply<Empty>{closure}: input_ty=(Qubit, Double, Double)
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn single_param_tuple_containing_arrow_specializes_end_to_end() {
    check(
        r#"
        operation Apply(pair : (Qubit => Unit, Qubit)) : Unit {
            let (op, q) = pair;
            op(q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply((H, q));
        }
        "#,
        &expect![[r#"
            Apply<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn single_param_tuple_second_element_specializes_end_to_end() {
    check(
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
            Main: input_ty=Unit
            Wrapper<AdjCtl>{H}: input_ty=Int"#]],
    );
}

#[test]
fn single_param_recursive_tuple_callable_specializes_end_to_end() {
    check(
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
        &expect![[r#"
            Main: input_ty=Unit
            Wrapper<AdjCtl>{H}: input_ty=((Int, Double), Qubit)"#]],
    );
}

#[test]
fn single_param_recursive_tuple_callable_closure_capture_invariants() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Wrapper(bundle : (((Qubit => Unit, Int), Double), Qubit)) : Unit {
            let (((op, n), angle), q) = bundle;
            ApplyOp(
                q1 => {
                    if n == 0 {
                        Rx(angle, q1);
                    }
                    op(q1);
                },
                q
            );
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((((H, 0), 1.0), q));
        }
        "#,
    );
}

#[test]
fn three_branch_conditional_callable_generates_branch_split() {
    check_errors(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let n = 2;
            mutable op = H;
            if n == 0 {
                op = X;
            } elif n == 1 {
                op = Y;
            } else {
                op = Z;
            }
            Apply(op, q);
        }
        "#,
        &expect!["(no error)"],
    );
}

#[test]
fn identity_closure_peephole_replaces_wrapper() {
    check(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let wrapper = q => H(q);
            Apply(wrapper, q);
        }
        "#,
        &expect![[r#"
            Apply<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn excessive_specializations_warning_emitted() {
    // A HOF called with > 10 different concrete closures triggers the
    // ExcessiveSpecializations warning. Each distinct Rx(angle, _) partial
    // application with a different angle creates a distinct closure, and
    // all closures map to the same functorless Apply<Empty> variant.
    check_errors(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
            Apply(q1 => Rx(4.0, q1), q);
            Apply(q1 => Rx(5.0, q1), q);
            Apply(q1 => Rx(6.0, q1), q);
            Apply(q1 => Rx(7.0, q1), q);
            Apply(q1 => Rx(8.0, q1), q);
            Apply(q1 => Rx(9.0, q1), q);
            Apply(q1 => Rx(10.0, q1), q);
            Apply(q1 => Rx(11.0, q1), q);
        }
        "#,
        &expect![[r#"
            higher-order function `Apply<Empty>` generated 11 specializations, exceeding the warning threshold"#]],
    );
}

#[test]
fn below_threshold_no_excessive_specializations_warning() {
    // A HOF with exactly 10 specializations should NOT trigger the warning.
    check_errors(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Apply(X, q);
            Apply(Y, q);
            Apply(Z, q);
            Apply(S, q);
            Apply(T, q);
            Apply(I, q);
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
        }
        "#,
        &expect!["(no error)"],
    );
}

#[test]
fn excessive_specializations_warning_does_not_block_compilation() {
    // A program that triggers ExcessiveSpecializations should still compile
    // successfully — the warning is non-fatal. We verify by running the
    // full defunctionalization and checking PostDefunc invariants hold.
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
            Apply(q1 => Rx(4.0, q1), q);
            Apply(q1 => Rx(5.0, q1), q);
            Apply(q1 => Rx(6.0, q1), q);
            Apply(q1 => Rx(7.0, q1), q);
            Apply(q1 => Rx(8.0, q1), q);
            Apply(q1 => Rx(9.0, q1), q);
            Apply(q1 => Rx(10.0, q1), q);
            Apply(q1 => Rx(11.0, q1), q);
        }
        "#,
    );
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);

    // Should have exactly one warning, no fatal errors.
    let warnings: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, super::super::Error::ExcessiveSpecializations(..)))
        .collect();
    let fatal: Vec<_> = errors
        .iter()
        .filter(|e| !matches!(e, super::super::Error::ExcessiveSpecializations(..)))
        .collect();
    assert_eq!(warnings.len(), 1, "expected exactly one warning");
    assert!(fatal.is_empty(), "expected no fatal errors, got: {fatal:?}");

    // PostDefunc invariants must still hold.
    fir_invariants::check(&fir_store, fir_pkg_id, InvariantLevel::PostDefunc);
}

#[test]
fn zero_capture_conditional_alias_dispatches_correctly() {
    check(
        r#"
        operation ZeroCaptureConditionalAlias(q : Qubit, useAdj : Bool) : Unit {
            let u = if useAdj { Adjoint H } else { H };
            u(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ZeroCaptureConditionalAlias(q, true);
        }
        "#,
        &expect![[r#"
            Main: input_ty=Unit
            ZeroCaptureConditionalAlias: input_ty=(Qubit, Bool)"#]],
    );
}
