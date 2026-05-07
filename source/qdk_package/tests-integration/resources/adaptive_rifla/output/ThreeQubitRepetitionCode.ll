@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_4 = alloca i1
  %var_5 = alloca i64
  %var_7 = alloca i1
  %var_8 = alloca i64
  %var_15 = alloca i1
  %var_27 = alloca i1
  %var_28 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_1
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_2
  br label %block_1
block_1:
  %var_35 = load i64, ptr %var_2
  %var_3 = icmp sle i64 %var_35, 5
  store i1 true, ptr %var_4
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_38 = load i1, ptr %var_4
  br i1 %var_38, label %block_4, label %block_5
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
  %var_25 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_25, ptr %var_27
  store i64 0, ptr %var_28
  br label %block_7
block_6:
  %var_48 = load i64, ptr %var_5
  %var_6 = icmp sle i64 %var_48, 4
  store i1 true, ptr %var_7
  br i1 %var_6, label %block_8, label %block_9
block_7:
  %var_41 = load i64, ptr %var_28
  %var_29 = icmp slt i64 %var_41, 2
  br i1 %var_29, label %block_10, label %block_11
block_8:
  %var_51 = load i1, ptr %var_7
  br i1 %var_51, label %block_12, label %block_13
block_9:
  store i1 false, ptr %var_7
  br label %block_8
block_10:
  %var_44 = load i64, ptr %var_28
  %var_30 = getelementptr ptr, ptr @array1, i64 %var_44
  %var_45 = load ptr, ptr %var_30
  call void @__quantum__qis__reset__body(ptr %var_45)
  %var_32 = add i64 %var_44, 1
  store i64 %var_32, ptr %var_28
  br label %block_7
block_11:
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  %var_42 = load i1, ptr %var_27
  call void @__quantum__rt__bool_record_output(i1 %var_42, ptr @1)
  %var_43 = load i64, ptr %var_1
  call void @__quantum__rt__int_record_output(i64 %var_43, ptr @2)
  ret i64 0
block_12:
  store i64 0, ptr %var_8
  br label %block_14
block_13:
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 1 to ptr))
  store i1 true, ptr %var_15
  %var_16 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_16, label %block_15, label %block_16
block_14:
  %var_60 = load i64, ptr %var_8
  %var_9 = icmp slt i64 %var_60, 3
  br i1 %var_9, label %block_17, label %block_18
block_15:
  %var_18 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_18, label %block_19, label %block_20
block_16:
  %var_20 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_20, label %block_21, label %block_22
block_17:
  %var_63 = load i64, ptr %var_8
  %var_10 = getelementptr ptr, ptr @array0, i64 %var_63
  %var_64 = load ptr, ptr %var_10
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr %var_64)
  %var_12 = add i64 %var_63, 1
  store i64 %var_12, ptr %var_8
  br label %block_14
block_18:
  %var_61 = load i64, ptr %var_5
  %var_13 = add i64 %var_61, 1
  store i64 %var_13, ptr %var_5
  br label %block_6
block_19:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %block_23
block_20:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_23
block_21:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  br label %block_24
block_22:
  store i1 false, ptr %var_15
  br label %block_24
block_23:
  br label %block_25
block_24:
  br label %block_25
block_25:
  %var_54 = load i1, ptr %var_15
  br i1 %var_54, label %block_26, label %block_27
block_26:
  %var_57 = load i64, ptr %var_1
  %var_23 = add i64 %var_57, 1
  store i64 %var_23, ptr %var_1
  br label %block_27
block_27:
  %var_55 = load i64, ptr %var_2
  %var_24 = add i64 %var_55, 1
  store i64 %var_24, ptr %var_2
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
