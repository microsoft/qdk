@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1i\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_4 = alloca i1
  %var_5 = alloca i64
  %var_7 = alloca i1
  %var_12 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_1
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_2
  br label %block_1
block_1:
  %var_25 = load i64, ptr %var_2
  %var_3 = icmp sle i64 %var_25, 5
  store i1 true, ptr %var_4
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_28 = load i1, ptr %var_4
  br i1 %var_28, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_4
  br label %block_2
block_4:
  store i64 1, ptr %var_5
  br label %block_6
block_5:
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_22 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__bool_record_output(i1 %var_22, ptr @1)
  %var_29 = load i64, ptr %var_1
  call void @__quantum__rt__int_record_output(i64 %var_29, ptr @2)
  ret i64 0
block_6:
  %var_31 = load i64, ptr %var_5
  %var_6 = icmp sle i64 %var_31, 4
  store i1 true, ptr %var_7
  br i1 %var_6, label %block_7, label %block_8
block_7:
  %var_34 = load i1, ptr %var_7
  br i1 %var_34, label %block_9, label %block_10
block_8:
  store i1 false, ptr %var_7
  br label %block_7
block_9:
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  %var_42 = load i64, ptr %var_5
  %var_9 = add i64 %var_42, 1
  store i64 %var_9, ptr %var_5
  br label %block_6
block_10:
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 1 to ptr))
  store i1 true, ptr %var_12
  %var_13 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_13, label %block_11, label %block_12
block_11:
  %var_15 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_15, label %block_13, label %block_14
block_12:
  %var_17 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_17, label %block_15, label %block_16
block_13:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %block_17
block_14:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_17
block_15:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  br label %block_18
block_16:
  store i1 false, ptr %var_12
  br label %block_18
block_17:
  br label %block_19
block_18:
  br label %block_19
block_19:
  %var_37 = load i1, ptr %var_12
  br i1 %var_37, label %block_20, label %block_21
block_20:
  %var_40 = load i64, ptr %var_1
  %var_20 = add i64 %var_40, 1
  store i64 %var_20, ptr %var_1
  br label %block_21
block_21:
  %var_38 = load i64, ptr %var_2
  %var_21 = add i64 %var_38, 1
  store i64 %var_21, ptr %var_2
  br label %block_1
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="3" }
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
