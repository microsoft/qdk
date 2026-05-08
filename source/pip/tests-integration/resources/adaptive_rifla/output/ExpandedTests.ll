@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [1 x ptr] [ptr inttoptr (i64 0 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_6 = alloca i64
  %var_8 = alloca i1
  %var_9 = alloca i64
  %var_14 = alloca i64
  %var_19 = alloca i64
  %var_24 = alloca i64
  %var_29 = alloca i64
  %var_34 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  br label %block_1
block_1:
  %var_41 = load i64, ptr %var_1
  %var_2 = icmp slt i64 %var_41, 2
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_79 = load i64, ptr %var_1
  %var_3 = getelementptr ptr, ptr @array0, i64 %var_79
  %var_80 = load ptr, ptr %var_3
  call void @__quantum__qis__h__body(ptr %var_80)
  %var_5 = add i64 %var_79, 1
  store i64 %var_5, ptr %var_1
  br label %block_1
block_3:
  store i64 0, ptr %var_6
  br label %block_4
block_4:
  %var_43 = load i64, ptr %var_6
  %var_7 = icmp sle i64 %var_43, 0
  store i1 true, ptr %var_8
  br i1 %var_7, label %block_5, label %block_6
block_5:
  %var_46 = load i1, ptr %var_8
  br i1 %var_46, label %block_7, label %block_8
block_6:
  store i1 false, ptr %var_8
  br label %block_5
block_7:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_9
  br label %block_9
block_8:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double -1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rz__body(double -1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rzz__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__array_record_output(i64 2, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @4)
  ret i64 0
block_9:
  %var_48 = load i64, ptr %var_9
  %var_10 = icmp slt i64 %var_48, 1
  br i1 %var_10, label %block_10, label %block_11
block_10:
  %var_76 = load i64, ptr %var_9
  %var_11 = getelementptr ptr, ptr @array1, i64 %var_76
  %var_77 = load ptr, ptr %var_11
  call void @__quantum__qis__x__body(ptr %var_77)
  %var_13 = add i64 %var_76, 1
  store i64 %var_13, ptr %var_9
  br label %block_9
block_11:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_14
  br label %block_12
block_12:
  %var_50 = load i64, ptr %var_14
  %var_15 = icmp sge i64 %var_50, 0
  br i1 %var_15, label %block_13, label %block_14
block_13:
  %var_73 = load i64, ptr %var_14
  %var_16 = getelementptr ptr, ptr @array1, i64 %var_73
  %var_74 = load ptr, ptr %var_16
  call void @__quantum__qis__x__body(ptr %var_74)
  %var_18 = add i64 %var_73, -1
  store i64 %var_18, ptr %var_14
  br label %block_12
block_14:
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_19
  br label %block_15
block_15:
  %var_52 = load i64, ptr %var_19
  %var_20 = icmp sge i64 %var_52, 0
  br i1 %var_20, label %block_16, label %block_17
block_16:
  %var_70 = load i64, ptr %var_19
  %var_21 = getelementptr ptr, ptr @array0, i64 %var_70
  %var_71 = load ptr, ptr %var_21
  call void @__quantum__qis__h__body(ptr %var_71)
  %var_23 = add i64 %var_70, -1
  store i64 %var_23, ptr %var_19
  br label %block_15
block_17:
  store i64 0, ptr %var_24
  br label %block_18
block_18:
  %var_54 = load i64, ptr %var_24
  %var_25 = icmp slt i64 %var_54, 2
  br i1 %var_25, label %block_19, label %block_20
block_19:
  %var_67 = load i64, ptr %var_24
  %var_26 = getelementptr ptr, ptr @array0, i64 %var_67
  %var_68 = load ptr, ptr %var_26
  call void @__quantum__qis__x__body(ptr %var_68)
  %var_28 = add i64 %var_67, 1
  store i64 %var_28, ptr %var_24
  br label %block_18
block_20:
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 1, ptr %var_29
  br label %block_21
block_21:
  %var_56 = load i64, ptr %var_29
  %var_30 = icmp sge i64 %var_56, 0
  br i1 %var_30, label %block_22, label %block_23
block_22:
  %var_64 = load i64, ptr %var_29
  %var_31 = getelementptr ptr, ptr @array0, i64 %var_64
  %var_65 = load ptr, ptr %var_31
  call void @__quantum__qis__x__body(ptr %var_65)
  %var_33 = add i64 %var_64, -1
  store i64 %var_33, ptr %var_29
  br label %block_21
block_23:
  store i64 0, ptr %var_34
  br label %block_24
block_24:
  %var_58 = load i64, ptr %var_34
  %var_35 = icmp slt i64 %var_58, 2
  br i1 %var_35, label %block_25, label %block_26
block_25:
  %var_61 = load i64, ptr %var_34
  %var_36 = getelementptr ptr, ptr @array0, i64 %var_61
  %var_62 = load ptr, ptr %var_36
  call void @__quantum__qis__h__body(ptr %var_62)
  %var_38 = add i64 %var_61, 1
  store i64 %var_38, ptr %var_34
  br label %block_24
block_26:
  %var_59 = load i64, ptr %var_6
  %var_39 = add i64 %var_59, 1
  store i64 %var_39, ptr %var_6
  br label %block_4
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__rz__body(double, ptr)

declare void @__quantum__qis__rzz__body(double, ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
