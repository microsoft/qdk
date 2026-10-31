// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compilation-only checks for the straight-line return suffix rule.

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
fn straight_line_nested_return_keeps_operand_effect_and_drops_dead_effects() {
    let qir = compile_body(
        "for _ in [0] { Before(q); { { return Value(q); } After(q); } Tail(q); } false",
    );
    assert!(qir.contains("call void @Before("), "{qir}");
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(!qir.contains("call void @After("), "{qir}");
    assert!(!qir.contains("call void @Tail("), "{qir}");
}

#[test]
fn conditional_return_preserves_fallthrough_effects() {
    let qir =
        compile_body("for _ in [0] { { if stop { return Value(q); } } After(q); } Tail(q); false");
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
    assert!(qir.contains("call void @Tail("), "{qir}");
}

#[test]
fn loop_return_preserves_zero_iteration_fallthrough_effects() {
    let qir = compile_body("{ while stop { return Value(q); } } After(q); false");
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
}

#[test]
fn nested_array_and_scalar_cleanup_keeps_return_operand_live() {
    let qir = compile_body(
        "use outer = Qubit(); for _ in [0] { use qs = Qubit[2]; { use inner = Qubit(); Before(outer); Before(qs[0]); return Value(inner); } After(q); } Tail(q); false",
    );
    assert!(qir.contains("call void @Before("), "{qir}");
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(!qir.contains("call void @After("), "{qir}");
    assert!(!qir.contains("call void @Tail("), "{qir}");
}

#[test]
fn conditional_array_cleanup_preserves_both_paths() {
    let qir = compile_body(
        "for _ in [0] { use qs = Qubit[2]; if stop { return Value(qs[0]); } After(qs[1]); } Tail(q); false",
    );
    assert!(qir.contains("call void @Operand("), "{qir}");
    assert!(qir.contains("call void @After("), "{qir}");
    assert!(qir.contains("call void @Tail("), "{qir}");
}

#[test]
fn conditional_operand_return_compiles_for_adaptive_profiles() {
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
