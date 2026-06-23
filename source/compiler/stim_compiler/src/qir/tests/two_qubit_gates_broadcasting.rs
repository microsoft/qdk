// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn cx_gate_yields_expected_qir() {
    let source = "CX 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cnot_gate_yields_expected_qir() {
    let source = "CNOT 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn zcx_gate_yields_expected_qir() {
    let source = "ZCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cxswap_gate_yields_expected_qir() {
    let source = "CXSWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cy_gate_yields_expected_qir() {
    let source = "CY 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cy__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cy__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cy__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn zcy_gate_yields_expected_qir() {
    let source = "ZCY 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cy__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cy__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cy__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn cz_gate_yields_expected_qir() {
    let source = "CZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__qis__cz__body(ptr, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn zcz_gate_yields_expected_qir() {
    let source = "ZCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__qis__cz__body(ptr, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn czswap_gate_yields_expected_qir() {
    let source = "CZSWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn swapcz_gate_yields_expected_qir() {
    let source = "SWAPCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn ii_gate_yields_expected_qir() {
    let source = "II 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn iswap_gate_yields_expected_qir() {
    let source = "ISWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn iswap_dag_gate_yields_expected_qir() {
    let source = "ISWAP_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn sqrt_xx_gate_yields_expected_qir() {
    let source = "SQRT_XX 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn sqrt_xx_dag_gate_yields_expected_qir() {
    let source = "SQRT_XX_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn sqrt_yy_gate_yields_expected_qir() {
    let source = "SQRT_YY 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn sqrt_yy_dag_gate_yields_expected_qir() {
    let source = "SQRT_YY_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn sqrt_zz_gate_yields_expected_qir() {
    let source = "SQRT_ZZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn sqrt_zz_dag_gate_yields_expected_qir() {
    let source = "SQRT_ZZ_DAG 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn swap_gate_yields_expected_qir() {
    let source = "SWAP 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__swap__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__swap__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__swap__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn swapcx_gate_yields_expected_qir() {
    let source = "SWAPCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn xcx_gate_yields_expected_qir() {
    let source = "XCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn xcy_gate_yields_expected_qir() {
    let source = "XCY 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn xcz_gate_yields_expected_qir() {
    let source = "XCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn ycx_gate_yields_expected_qir() {
    let source = "YCX 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn ycy_gate_yields_expected_qir() {
    let source = "YCY 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__h__body(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}

#[test]
fn ycz_gate_yields_expected_qir() {
    let source = "YCZ 0 1 2 3";
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__adj(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__s__adj(ptr)
        declare void @__quantum__qis__s__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
    "#]],
    );
}
