use crate::tests::{check_qasm_to_qir, check_qasm_to_qsharp};
use expect_test::expect;

#[test]
fn gate_def_with_intrinsic_call_in_body_compiles() {
    let src = "
    @qdk.qir.intrinsic
    def intrinsic() {}
    gate test_gate q {
        intrinsic();
    }
    qubit q;
    test_gate q;
    ";
    check_qasm_to_qsharp(
        src,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            @SimulatableIntrinsic()
            operation intrinsic () : Unit {}
            operation test_gate(q : Qubit) : Unit {
                intrinsic ();
            }
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            test_gate(q);
        "#]],
    );
}

#[test]
fn intrinsic_in_modifiable_gate_def_errors() {
    let src = "
    #pragma qdk.qir.profile Adaptive_RI
    @qdk.qir.intrinsic
    def intrinsic() {}
    gate test_gate q {
        intrinsic();
    }
    qubit q;
    inv @ test_gate q;
    bit result = measure q;
    ";
    check_qasm_to_qir(
        src,
        &expect![[r#"
            Qsc.AdjGen.MissingAdjFunctor

              x operation does not support the adjoint functor
               ,-[Test.qasm:6:9]
             5 |     gate test_gate q {
             6 |         intrinsic();
               :         ^^^^^^^^^
             7 |     }
               `----
              help: each operation called inside an operation with compiler-generated
                    adjoint specializations must support the adjoint functor
        "#]],
    );
}

#[test]
fn non_modified_gate_doesnt_implement_functors() {
    let src = "
    gate test_gate q {}
    qubit q;
    test_gate q;
    ";
    check_qasm_to_qsharp(
        src,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            operation test_gate(q : Qubit) : Unit {}
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            test_gate(q);
        "#]],
    );
}

#[test]
fn controlled_gate_implements_ctrl_functor() {
    let src = "
    gate test_gate q {}
    qubit[2] q;
    ctrl @ test_gate q[0], q[1];
    ";
    check_qasm_to_qsharp(
        src,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            operation test_gate(q : Qubit) : Unit is Ctl {}
            let q = QIR.Runtime.AllocateQubitArray(2);
            Controlled test_gate([q[0]], q[1]);
        "#]],
    );
}

#[test]
fn inverted_gate_implements_adj_functor() {
    let src = "
    gate test_gate q {}
    qubit q;
    inv @ test_gate q;
    ";
    check_qasm_to_qsharp(
        src,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            operation test_gate(q : Qubit) : Unit is Adj {}
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            Adjoint test_gate(q);
        "#]],
    );
}

/// The pow modifier can have negative arguments,
/// which means applying pow of the inverse.
/// Therefore, the pow functor requires gates to
/// implement the Adj functor.
#[test]
fn pow_on_gate_implements_adj_functor() {
    let src = "
    gate test_gate q {}
    qubit q;
    pow(2) @ test_gate q;
    ";
    check_qasm_to_qsharp(
        src,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            operation test_gate(q : Qubit) : Unit is Adj {}
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            ApplyOperationPowerA(2, test_gate, (q));
        "#]],
    );
}

#[test]
fn functor_constraints_propagate() {
    let src = "
    gate test_gate_1 q {}
    gate test_gate_2 q {
        test_gate_1 q;
    }
    gate test_gate_3 q {
        test_gate_2 q;
    }

    qubit q;
    inv @ test_gate_3 q;
    ";
    check_qasm_to_qsharp(
        src,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            operation test_gate_1(q : Qubit) : Unit is Adj {}
            operation test_gate_2(q : Qubit) : Unit is Adj {
                test_gate_1(q);
            }
            operation test_gate_3(q : Qubit) : Unit is Adj {
                test_gate_2(q);
            }
            let q = QIR.Runtime.__quantum__rt__qubit_allocate();
            Adjoint test_gate_3(q);
        "#]],
    );
}

#[test]
fn gates_dont_implement_unnecessary_functors() {
    let src = "
    gate test_gate_1 q {}
    gate test_gate_2 q {
        test_gate_1 q;
    }
    gate test_gate_3 q {
        test_gate_2 q;
    }

    qubit[2] q;
    ctrl @ test_gate_1 q[0], q[1];
    inv @ test_gate_2 q[0];
    test_gate_3 q[0];
    ";
    check_qasm_to_qsharp(
        src,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            operation test_gate_1(q : Qubit) : Unit is Adj + Ctl {}
            operation test_gate_2(q : Qubit) : Unit is Adj {
                test_gate_1(q);
            }
            operation test_gate_3(q : Qubit) : Unit {
                test_gate_2(q);
            }
            let q = QIR.Runtime.AllocateQubitArray(2);
            Controlled test_gate_1([q[0]], q[1]);
            Adjoint test_gate_2(q[0]);
            test_gate_3(q[0]);
        "#]],
    );
}
