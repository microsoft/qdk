// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compilation-only controls for return cleanup and continuation liveness.

use super::compile_source_to_qir;
use qsc_data_structures::target::Profile;

fn compile_body(body: &str) -> String {
    compile_source_to_qir(
        &format!(
            r#"
            operation Before(q : Qubit) : Unit {{ body intrinsic; }}
            operation Operand(q : Qubit) : Unit {{ body intrinsic; }}
            operation After(q : Qubit) : Unit {{ body intrinsic; }}
            operation Tail(q : Qubit) : Unit {{ body intrinsic; }}
            operation Value(q : Qubit) : Bool {{ Operand(q); false }}
            operation Run(stop : Bool, q : Qubit) : Bool {{ {body} }}
            @EntryPoint(Adaptive) operation Main() : Bool {{
                use q = Qubit();
                Run(MResetZ(q) == One, q)
            }}
            "#
        ),
        Profile::Adaptive.into(),
    )
}

#[test]
fn issue_3940_conditional_return_preserves_fallthrough_effects() {
    let qir =
        compile_body("for _ in [0] { { if stop { return Value(q); } } After(q); } Tail(q); false");
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
    assert!(qir.contains("call void @Tail("), "{qir}");
}

#[test]
fn issue_3940_loop_return_preserves_zero_iteration_fallthrough_effects() {
    let qir = compile_body("{ while stop { return Value(q); } } After(q); false");
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_nested_array_and_scalar_cleanup_keeps_return_operand_live() {
    let qir = compile_body(
        "use outer = Qubit(); for _ in [0] { use qs = Qubit[2]; { use inner = Qubit(); Before(outer); Before(qs[0]); return Value(inner); } After(q); } Tail(q); false",
    );
    assert!(qir.contains("call void @Before("), "{qir}");
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(!qir.contains("call void @After("), "{qir}");
    assert!(!qir.contains("call void @Tail("), "{qir}");
}

#[test]
fn issue_3940_conditional_array_cleanup_preserves_both_paths() {
    let qir = compile_body(
        "for _ in [0] { use qs = Qubit[2]; if stop { return Value(qs[0]); } After(qs[1]); } Tail(q); false",
    );
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
    assert!(qir.contains("call void @Tail("), "{qir}");
}

#[test]
fn issue_3940_conditional_operand_return_compiles_for_adaptive_profiles() {
    // Return-bearing conditional operands compile across adaptive target profiles.
    let source = r#"
        function Run(stop : Bool) : Int {
            let v = stop ? 1 + (return 1) | 2;
            v
        }
        @EntryPoint(Adaptive_RIF) operation Main() : Int { Run(true) }
    "#;
    let _ = compile_source_to_qir(source, Profile::AdaptiveRIF.into());
    let source = r#"
        operation Run(stop : Bool, q : Qubit) : Int {
            X(q);
            let m = MResetZ(q) == One ? 1 | 0;
            let v = stop ? 1 + (return 100 + m) | m + 1;
            v * 10 + m
        }
        @EntryPoint(Adaptive_RIF) operation Main() : (Int, Int) {
            use q = Qubit();
            (Run(true, q), Run(false, q))
        }
    "#;
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let _ = compile_source_to_qir(source, profile.into());
    }
}

#[test]
fn issue_3940_liveness_control_nested_conditional_cleanup_preserves_continuation() {
    let qir = compile_body(
        r#"
        use qs = Qubit[2];
        if stop {
            { if MResetZ(qs[0]) == One { return Value(qs[0]); } }
            Before(qs[1]);
        }
        After(qs[1]);
        false
        "#,
    );
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @Before("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_liveness_control_explicit_else_cleanup_preserves_continuation() {
    let qir = compile_body(
        r#"
        use qs = Qubit[2];
        if stop {
            Before(qs[1]);
        } else {
            return Value(qs[0]);
        }
        After(qs[1]);
        false
        "#,
    );
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @Before("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_liveness_control_both_partially_returning_arms_preserve_continuation() {
    let qir = compile_body(
        r#"
        use qs = Qubit[2];
        if stop {
            if MResetZ(qs[0]) == One { return Value(qs[0]); }
            Before(qs[1]);
        } else {
            if MResetZ(qs[0]) == One { return Value(qs[0]); }
            Tail(qs[1]);
        }
        After(qs[1]);
        false
        "#,
    );
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @Before("), "{qir}");
    assert!(qir.contains("call void @Tail("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_liveness_control_loop_exit_cleanup_preserves_continuation() {
    let qir = compile_body(
        r#"
        use qs = Qubit[2];
        while stop {
            if MResetZ(qs[0]) == One { return Value(qs[0]); }
            Before(qs[1]);
        }
        After(qs[1]);
        false
        "#,
    );
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @Before("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_liveness_control_callee_early_return_keeps_unreleased_caller_qubit_live() {
    let qir = compile_source_to_qir(
        r#"
        namespace Test {
            operation Op(q : Qubit) : Unit { body intrinsic; }
            operation Keep(stop : Bool, q : Qubit) : Unit {
                if stop { return (); }
                Op(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use control = Qubit();
                let q = QIR.Runtime.__quantum__rt__qubit_allocate();
                Keep(MResetZ(control) == One, q);
                Op(q);
                QIR.Runtime.__quantum__rt__qubit_release(q);
            }
        }
        "#,
        Profile::Adaptive.into(),
    );
    assert!(qir.contains("call void @Op("), "{qir}");
}

fn assert_review_liveness_diagnostic(source: &str, diagnostic: &str) {
    let errors = super::compile_source_to_qir_result(source, Profile::Adaptive.into())
        .expect_err("resource-invalid source must fail through normal Q# to QIR compilation");
    let diagnostics = super::format_interpret_errors(errors);
    assert!(diagnostics.contains(diagnostic), "{diagnostics}");
}

#[test]
fn issue_3940_review_control_dynamic_callee_release_return_rejects_caller_use() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        operation Release(stop : Bool, q : Qubit) : Unit {
            if stop {
                QIR.Runtime.ReleaseQubitArray([q]);
                return ();
            }
            Op(q);
        }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            Release(MResetZ(control) == One, q);
            Op(q);
        }
        "#,
        "qubit used after release",
    );
}

#[test]
fn issue_3940_review_control_dynamic_callee_release_return_rejects_caller_release() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        operation Release(stop : Bool, q : Qubit) : Unit {
            if stop {
                QIR.Runtime.ReleaseQubitArray([q]);
                return ();
            }
            Op(q);
        }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            Release(MResetZ(control) == One, q);
            QIR.Runtime.ReleaseQubitArray([q]);
        }
        "#,
        "qubit double release",
    );
}

#[test]
fn issue_3940_review_control_release_return_preserves_internal_continuation() {
    let qir = compile_body(
        r#"
        let owned = QIR.Runtime.__quantum__rt__qubit_allocate();
        if stop {
            QIR.Runtime.ReleaseQubitArray([owned]);
            return false;
        }
        After(owned);
        QIR.Runtime.ReleaseQubitArray([owned]);
        false
        "#,
    );
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_review_control_mutating_condition_does_not_reuse_resource_fact() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            mutable released = MResetZ(control) == One;
            if released { QIR.Runtime.ReleaseQubitArray([q]); }
            set released = false;
            if not released { Op(q); }
        }
        "#,
        "qubit used after release",
    );
}

#[test]
fn issue_3940_review_control_mutated_condition_after_return_preserves_live_continuation() {
    let qir = compile_body(
        r#"
        let owned = QIR.Runtime.__quantum__rt__qubit_allocate();
        mutable done = stop;
        if done {
            QIR.Runtime.ReleaseQubitArray([owned]);
            return false;
        }
        set done = true;
        if done { After(owned); }
        QIR.Runtime.ReleaseQubitArray([owned]);
        false
        "#,
    );
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_review_control_dynamic_array_release_return_rejects_caller_use() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        operation Release(stop : Bool, qs : Qubit[]) : Unit {
            if stop {
                QIR.Runtime.ReleaseQubitArray(qs);
                return ();
            }
            Op(qs[1]);
        }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            let qs = QIR.Runtime.AllocateQubitArray(2);
            Release(MResetZ(control) == One, qs);
            Op(qs[1]);
        }
        "#,
        "qubit used after release",
    );
}

#[test]
fn issue_3940_review_control_released_array_rejects_second_array_release() {
    assert_review_liveness_diagnostic(
        r#"
        @EntryPoint() operation Main() : Unit {
            let qs = QIR.Runtime.AllocateQubitArray(2);
            QIR.Runtime.ReleaseQubitArray(qs);
            QIR.Runtime.ReleaseQubitArray(qs);
        }
        "#,
        "qubit double release",
    );
}

#[test]
fn issue_3940_review_control_branch_allocations_and_fresh_allocation_preserve_capacity() {
    let qir = compile_source_to_qir(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            if MResetZ(control) == One {
                let q = QIR.Runtime.__quantum__rt__qubit_allocate();
                Op(q);
                QIR.Runtime.ReleaseQubitArray([q]);
            } else {
                let q = QIR.Runtime.__quantum__rt__qubit_allocate();
                Op(q);
                QIR.Runtime.ReleaseQubitArray([q]);
            }
            let fresh = QIR.Runtime.__quantum__rt__qubit_allocate();
            Op(fresh);
            QIR.Runtime.__quantum__rt__qubit_release(fresh);
        }
        "#,
        Profile::Adaptive.into(),
    );
    assert_eq!(qir.matches("call void @Op(").count(), 3, "{qir}");
    assert!(qir.contains(r#""required_num_qubits"="2""#), "{qir}");
}

#[test]
fn issue_3940_review_control_dynamic_callee_release_does_not_revive_alias_after_allocation() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        operation Release(stop : Bool, q : Qubit) : Unit {
            if stop {
                QIR.Runtime.ReleaseQubitArray([q]);
                return ();
            }
            Op(q);
        }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            let alias = q;
            Release(MResetZ(control) == One, q);
            let fresh = QIR.Runtime.__quantum__rt__qubit_allocate();
            Op(fresh);
            Op(alias);
        }
        "#,
        "qubit used after release",
    );
}

#[test]
fn issue_3940_review_control_array_release_return_preserves_internal_continuation() {
    let qir = compile_body(
        r#"
        let owned = QIR.Runtime.AllocateQubitArray(2);
        if stop {
            QIR.Runtime.ReleaseQubitArray(owned);
            return false;
        }
        After(owned[1]);
        QIR.Runtime.ReleaseQubitArray(owned);
        false
        "#,
    );
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn issue_3940_review_control_compound_assignment_invalidates_old_resource_fact() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            mutable released = false;
            if MResetZ(control) == One {
                set released = true;
                QIR.Runtime.ReleaseQubitArray([q]);
            }
            set released and= MResetZ(control) == One;
            if not released { Op(q); }
        }
        "#,
        "qubit used after release",
    );
}

#[test]
fn issue_3940_review_control_tuple_assignment_invalidates_old_resource_fact() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        @EntryPoint() operation Main() : Unit {
            use control = Qubit();
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            mutable released = false;
            mutable spare = false;
            if MResetZ(control) == One {
                set released = true;
                QIR.Runtime.ReleaseQubitArray([q]);
            }
            set (released, spare) = (MResetZ(control) == One, true);
            if not released { Op(q); }
        }
        "#,
        "qubit used after release",
    );
}

#[test]
fn issue_3940_review_control_delayed_release_rejects_use_before_physical_free() {
    assert_review_liveness_diagnostic(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        @EntryPoint() operation Main() : Unit {
            parallel {
                let q = QIR.Runtime.__quantum__rt__qubit_allocate();
                QIR.Runtime.ReleaseQubitArray([q]);
                Op(q);
            }
        }
        "#,
        "qubit used after release",
    );
}

#[test]
fn issue_3940_review_control_delayed_capacity_reuse_keeps_fresh_generation_live() {
    let qir = compile_source_to_qir(
        r#"
        operation Op(q : Qubit) : Unit { body intrinsic; }
        @EntryPoint() operation Main() : Unit {
            parallel within 1 {
                { use old = Qubit(); Op(old); }
                use fresh = Qubit();
                Op(fresh);
            }
        }
        "#,
        Profile::Adaptive.into(),
    );
    assert_eq!(qir.matches("call void @Op(").count(), 2, "{qir}");
    assert!(qir.contains(r#""required_num_qubits"="1""#), "{qir}");
}

#[test]
fn issue_3940_loop_body_return_preserves_outer_array_zero_iteration_continuation() {
    let qir = compile_body(
        r#"
        use qs = Qubit[2];
        while stop {
            return Value(qs[0]);
        }
        After(qs[1]);
        false
        "#,
    );
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
}
