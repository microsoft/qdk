// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! QIR checks for the straight-line return-suffix rule.
//!
//! These programs compile with that rule alone. They do not depend on
//! snapshotting qubit resource state across dynamic branches.

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
fn range_loop_with_qubit_array_and_early_return_compiles() {
    let source = r#"@EntryPoint(Adaptive) operation Main() : Bool { for e in 0..0 { use qs = Qubit[4]; return false; } return false; }"#;
    let _ = compile_source_to_qir(source, Profile::Adaptive.into());
}

#[test]
fn loop_with_local_qubit_array_and_early_return_compiles() {
    let source = r#"
        @EntryPoint(Adaptive)
        operation Main() : Bool {
            for e0 in [0] {
                use qa0 = Qubit[4];
                let ret0 = false;
                return ret0;
            }
            return false;
        }
    "#;
    let _ = compile_source_to_qir(source, Profile::Adaptive.into());
}
