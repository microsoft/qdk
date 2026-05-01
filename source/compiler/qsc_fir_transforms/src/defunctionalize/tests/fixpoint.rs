// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;
use std::fmt::Write;

#[test]
fn program_without_hofs_converges_without_changes() {
    check(
        r#"
        operation Main() : Unit {
            use q = Qubit();
            H(q);
        }
        "#,
        &expect![[r#"
            Main: input_ty=Unit"#]],
    );
}

#[test]
fn fixpoint_no_hof_call_sites_prunes_dead_callable_local_chain() {
    check_invariants(
        r#"
        operation Main() : Unit {
            let first : Int -> Bool = (value) -> value == 0;
            let second : Int -> Bool = first;
        }
        "#,
    );
}

#[test]
fn fixpoint_multi_level_hof() {
    check_invariants(
        r#"
        operation ApplyInner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation ApplyOuter(op : Qubit => Unit, q : Qubit) : Unit {
            ApplyInner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOuter(H, q);
        }
        "#,
    );
}

#[test]
fn invariant_after_fixpoint() {
    check_invariants(
        r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(op : Qubit => Unit, q : Qubit) : Unit {
            Inner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(H, q);
        }
        "#,
    );
}

#[test]
fn full_pipeline_succeeds_for_simple_hof() {
    check_pipeline(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_two_levels() {
    check_invariants(
        r#"
        operation Level1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Level2(op : Qubit => Unit, q : Qubit) : Unit {
            Level1(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Level2(H, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_convergence() {
    check_invariants(
        r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(op : Qubit => Unit, q : Qubit) : Unit {
            L1(op, q);
        }
        operation L3(op : Qubit => Unit, q : Qubit) : Unit {
            L2(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L3(H, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_forwarding_with_adjoint() {
    check_invariants(
        r#"
        operation Inner(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Inner(Adjoint op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(S, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_controlled_forwarding() {
    check_invariants(
        r#"
        operation Inner(op : Qubit => Unit is Ctl, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(op : Qubit => Unit is Ctl, q : Qubit) : Unit {
            Inner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(X, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_four_levels() {
    check_invariants(
        r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(op : Qubit => Unit, q : Qubit) : Unit {
            L1(op, q);
        }
        operation L3(op : Qubit => Unit, q : Qubit) : Unit {
            L2(op, q);
        }
        operation L4(op : Qubit => Unit, q : Qubit) : Unit {
            L3(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L4(H, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_two_call_sites_different_args() {
    check_invariants(
        r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(op : Qubit => Unit, q : Qubit) : Unit {
            Inner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(H, q);
            Outer(X, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_forwarding_adj_autogen() {
    check_invariants(
        r#"
        operation Inner(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            op(q);
        }
        operation Outer(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            Inner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(S, q);
            Adjoint Outer(S, q);
        }
        "#,
    );
}

#[test]
fn nested_hof_requires_multi_iteration_convergence() {
    check(
        r#"
        operation ApplyTwice(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
            op(q);
        }

        operation ApplyAndMeasure(action : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Result {
            action(op, q);
            M(q)
        }

        operation Main() : Result {
            use q = Qubit();
            ApplyAndMeasure(ApplyTwice, H, q)
        }
        "#,
        &expect![[r#"
            ApplyAndMeasure<Empty, AdjCtl>{ApplyTwice<Empty>}{H}: input_ty=Qubit
            ApplyTwice<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
    check_invariants(
        r#"
        operation ApplyTwice(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
            op(q);
        }

        operation ApplyAndMeasure(action : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Result {
            action(op, q);
            M(q)
        }

        operation Main() : Result {
            use q = Qubit();
            ApplyAndMeasure(ApplyTwice, H, q)
        }
        "#,
    );
}

#[test]
fn five_level_hof_chain_converges_at_max_iterations_boundary() {
    check_invariants(
        r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(op : Qubit => Unit, q : Qubit) : Unit {
            L1(op, q);
        }
        operation L3(op : Qubit => Unit, q : Qubit) : Unit {
            L2(op, q);
        }
        operation L4(op : Qubit => Unit, q : Qubit) : Unit {
            L3(op, q);
        }
        operation L5(op : Qubit => Unit, q : Qubit) : Unit {
            L4(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L5(H, q);
        }
        "#,
    );
}

#[test]
fn transient_dynamic_resolves_after_outer_hof_specialization() {
    check_errors(
        r#"
        operation ApplyInner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation ApplyMiddle(op : Qubit => Unit, q : Qubit) : Unit {
            ApplyInner(op, q);
        }

        operation ApplyOuter(action : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Unit {
            action(op, q);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyOuter(ApplyMiddle, H, q);
        }
        "#,
        &expect!["(no error)"],
    );
    check(
        r#"
        operation ApplyInner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation ApplyMiddle(op : Qubit => Unit, q : Qubit) : Unit {
            ApplyInner(op, q);
        }

        operation ApplyOuter(action : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Unit {
            action(op, q);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyOuter(ApplyMiddle, H, q);
        }
        "#,
        &expect![[r#"
            ApplyInner<Empty>{H}: input_ty=Qubit
            ApplyMiddle<Empty>{H}: input_ty=Qubit
            ApplyOuter<Empty, AdjCtl>{ApplyMiddle<Empty>}{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
}

/// Regression test for producer-body closure cleanup: a producer function
/// that returns a partial-application closure causes convergence failure
/// when the closure node survives in the producer body after HOF
/// specialization. The closure cleanup pass must replace consumed closures
/// with Unit so that `remaining_callable_value_info` no longer counts them.
#[test]
fn producer_body_closure_cleanup_converges() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation InnerOp(extra : Bool, q : Qubit) : Unit {
            H(q);
        }
        function MakeOp(extra : Bool) : Qubit => Unit {
            return InnerOp(extra, _);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = MakeOp(true);
            ApplyOp(op, q);
        }
        "#,
    );
}

/// Two callable arguments passed to a multi-parameter HOF: one partial
/// application closure and one global callable. Both must survive cleanup
/// because they are still live as call arguments.
#[test]
fn closure_in_active_call_arg_survives_cleanup() {
    check_invariants(
        r#"
        operation Apply2(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Inner(extra : Bool, q : Qubit) : Unit {
            H(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op1 = Inner(true, _);
            Apply2(op1, X, q);
        }
        "#,
    );
}

/// When a mutable callable variable is reassigned in a loop, the analysis
/// resolves it to `Dynamic` (overdefined). The fixpoint loop detects no
/// progress — remaining callable count is unchanged and no new call sites are
/// discovered — and breaks via stuck detection. The `DynamicCallable` error
/// from the current iteration survives, preventing the post-loop
/// `FixpointNotReached` from firing (which only fires when `errors.is_empty()`).
#[test]
fn stuck_detection_with_unresolvable_callable_emits_dynamic_error() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            for _ in 0..3 {
                op = X;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// Multi-level HOF chain where each fixpoint iteration resolves one level.
/// Confirms that the before/after progress tracking does not cause premature
/// exit when each iteration successfully reduces the remaining count.
#[test]
fn progress_tracking_allows_multi_iteration_convergence() {
    check_invariants(
        r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(inner : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Unit {
            inner(op, q);
        }
        operation L3(mid : ((Qubit => Unit, Qubit) => Unit, Qubit => Unit, Qubit) => Unit, inner : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Unit {
            mid(inner, op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L3(L2, L1, H, q);
        }
        "#,
    );
}

#[test]
fn pipeline_resolves_conditional_callable_binding() {
    check_pipeline(
        r#"
        operation ApplyPower(power : Int, op : Qubit => Unit is Adj, target : Qubit) : Unit is Adj {
            let u = if power >= 0 { op } else { Adjoint op };
            for _ in 1..power {
                u(target);
            }
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            ApplyPower(3, S, q);
        }
        "#,
    );
}

#[test]
fn pipeline_callable_from_tuple_destructured_array_iteration() {
    check_pipeline(
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
    );
}

#[test]
fn pipeline_teleportation_pattern_callable_from_array_of_tuples() {
    check_pipeline(
        r#"
        namespace Test {
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                H(q);
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                X(q);
                H(q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let ops = [
                    (I, PauliZ),
                    (X, PauliZ),
                    (SetToPlus, PauliX),
                    (SetToMinus, PauliX),
                ];
                for (initializer, _basis) in ops {
                    use q = Qubit();
                    initializer(q);
                }
            }
        }
        "#,
    );
}

#[test]
fn pipeline_callable_at_middle_of_three_tuple_from_array_iteration() {
    check_pipeline(
        r#"
        namespace Test {
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                H(q);
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                X(q);
                H(q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let ops = [
                    (PauliZ, I, false),
                    (PauliZ, X, false),
                    (PauliX, SetToPlus, true),
                    (PauliX, SetToMinus, true),
                ];
                for (_basis, initializer, _flag) in ops {
                    use q = Qubit();
                    initializer(q);
                }
            }
        }
        "#,
    );
}

#[test]
fn pipeline_teleportation_like_callable_from_string_tagged_triple_array() {
    check_pipeline(
        r#"
        namespace Test {
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                H(q);
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                X(q);
                H(q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let ops = [
                    (I, PauliZ),
                    (X, PauliZ),
                    (SetToPlus, PauliX),
                    (SetToMinus, PauliX),
                ];
                for (initializer, basis) in ops {
                    use q = Qubit();
                    initializer(q);
                    let _ = Measure([basis], [q]);
                    Reset(q);
                }
            }
        }
        "#,
    );
}

#[test]
fn pipeline_callable_array_iteration_exceeding_old_multi_cap() {
    check_pipeline(
        r#"
        namespace Test {
            operation SX(q : Qubit) : Unit is Adj + Ctl {
                Rx(Microsoft.Quantum.Math.PI() / 2.0, q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let gates = [H, X, Y, Z, S, Adjoint S, SX];
                use q = Qubit();
                for gate in gates {
                    gate(q);
                }
            }
        }
        "#,
    );
}

fn nested_hof_source(level_count: usize) -> String {
    assert!(level_count > 0);

    let mut source = String::new();
    source.push_str("operation Level01(op : Qubit => Unit, q : Qubit) : Unit {\n    op(q);\n}\n");

    for level in 2..=level_count {
        write!(
            &mut source,
            "operation Level{level:02}(op : Qubit => Unit, q : Qubit) : Unit {{\n    Level{previous:02}(op, q);\n}}\n",
            previous = level - 1,
        ).expect("failed to write source string");
    }

    write!(
        &mut source,
        "@EntryPoint()\noperation Main() : Unit {{\n    use q = Qubit();\n    Level{level_count:02}(H, q);\n}}\n"
    ).expect("failed to write source string");
    source
}

#[test]
fn defunc_20_level_hof_returns_fixpoint_reached() {
    // Regression test: 20-level HOF nesting is under the convergence cap.
    let source = nested_hof_source(20);

    let (mut fir_store, fir_pkg_id) = crate::test_utils::compile_to_monomorphized_fir(&source);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = super::super::defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);

    assert!(
        errors.is_empty(),
        "Expected defunctionalization to succeed for 20-level HOF, got: {:?}",
        errors.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
}

#[test]
fn defunc_21_level_hof_returns_static_resolution_error() {
    // Regression test: 21-level HOF nesting exceeds the current static
    // resolution depth, but still reports a defunctionalization diagnostic
    // instead of panicking or lowering invalid FIR.
    let source = nested_hof_source(21);

    let (mut fir_store, fir_pkg_id) = crate::test_utils::compile_to_monomorphized_fir(&source);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = super::super::defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);

    assert!(
        !errors.is_empty(),
        "Expected defunctionalization error for 21-level HOF"
    );

    assert!(
        matches!(errors.as_slice(), [super::super::Error::DynamicCallable(_)]),
        "Expected DynamicCallable error, got: {:?}",
        errors.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
}
