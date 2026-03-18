@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"
@5 = internal constant [6 x i8] c"5_a4r\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 9 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 8 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 8 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 9 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 8 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 9 to ptr))
  call void @__quantum__rt__array_record_output(i64 5, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @5)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="10" "required_num_results"="5" }
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
