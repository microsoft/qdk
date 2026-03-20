@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1a\00"
@5 = internal constant [8 x i8] c"5_t1a0r\00"
@6 = internal constant [8 x i8] c"6_t1a1r\00"
@7 = internal constant [6 x i8] c"7_t2a\00"
@8 = internal constant [8 x i8] c"8_t2a0r\00"
@9 = internal constant [8 x i8] c"9_t2a1r\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_6 = alloca i64
  %var_8 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  store i64 1, ptr %var_6
  br label %block_1
block_1:
  %var_11 = load i64, ptr %var_6
  %var_7 = icmp sle i64 %var_11, 8
  store i1 true, ptr %var_8
  br i1 %var_7, label %block_2, label %block_3
block_2:
  %var_14 = load i1, ptr %var_8
  br i1 %var_14, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_8
  br label %block_2
block_4:
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__ry__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__ry__body(double 1.5707963267948966, ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__rz__body(double 1.5707963267948966, ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__rz__body(double 1.5707963267948966, ptr inttoptr (i64 5 to ptr))
  %var_15 = load i64, ptr %var_6
  %var_9 = add i64 %var_15, 1
  store i64 %var_9, ptr %var_6
  br label %block_1
block_5:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 3, ptr @0)
  call void @__quantum__rt__array_record_output(i64 2, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__array_record_output(i64 2, ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @6)
  call void @__quantum__rt__array_record_output(i64 2, ptr @7)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @9)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__y__body(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__ry__body(double, ptr)

declare void @__quantum__qis__rz__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="6" }
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
