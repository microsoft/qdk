@0 = internal constant [4 x i8] c"0_b\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_6 = call zeroext i1 @boolNot(i1 zeroext %var_1)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_6, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_0) {
block_1:
  call void @__quantum__qis__x__body(ptr %var_0)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal zeroext i1 @boolNot(i1 zeroext %var_4) {
block_2:
  %var_5 = xor i1 %var_4, true
  ret i1 %var_5
}

declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
attributes #1 = { "irreversible" }
attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
!8 = !{i32 1, !"ir_functions", i1 true}
!9 = !{i32 1, !"writable_results", i1 true}
