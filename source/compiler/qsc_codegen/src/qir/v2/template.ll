{}
{}

attributes #0 = {{ "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="{}" "required_num_results"="{}" }}
attributes #1 = {{ "irreversible" }}{}

; module flags

!llvm.module.flags = !{{!0, !1, !2, !3, !4, !5, !6, !7}}

!0 = !{{i32 1, !"qir_major_version", i32 2}}
!1 = !{{i32 7, !"qir_minor_version", i32 1}}
!2 = !{{i32 1, !"dynamic_qubit_management", i1 false}}
!3 = !{{i32 1, !"dynamic_result_management", i1 false}}
!4 = !{{i32 5, !"int_computations", !{{!"i64"}}}}
!5 = !{{i32 5, !"float_computations", !{{!"double"}}}}
!6 = !{{i32 7, !"backwards_branching", i2 3}}
!7 = !{{i32 1, !"arrays", i1 true}}
