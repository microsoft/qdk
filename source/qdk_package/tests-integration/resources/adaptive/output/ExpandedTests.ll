@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [1 x ptr] [ptr inttoptr (i64 0 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_7 = alloca i64
  %var_9 = alloca i1
  %var_10 = alloca i64
  %var_15 = alloca i64
  %var_20 = alloca i64
  %var_25 = alloca i64
  %var_30 = alloca i64
  %var_35 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_42 = load i64, ptr %var_2
  %var_3 = icmp slt i64 %var_42, 2
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_80 = load i64, ptr %var_2
  %var_4 = getelementptr ptr, ptr @array0, i64 %var_80
  %var_81 = load ptr, ptr %var_4
  call void @__quantum__qis__h__body(ptr %var_81)
  %var_6 = add i64 %var_80, 1
  store i64 %var_6, ptr %var_2
  br label %block_1
block_3:
  store i64 0, ptr %var_7
  br label %block_4
block_4:
  %var_44 = load i64, ptr %var_7
  %var_8 = icmp sle i64 %var_44, 0
  store i1 true, ptr %var_9
  br i1 %var_8, label %block_5, label %block_6
block_5:
  %var_47 = load i1, ptr %var_9
  br i1 %var_47, label %block_7, label %block_8
block_6:
  store i1 false, ptr %var_9
  br label %block_5
block_7:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_10
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
  %var_49 = load i64, ptr %var_10
  %var_11 = icmp slt i64 %var_49, 1
  br i1 %var_11, label %block_10, label %block_11
block_10:
  %var_77 = load i64, ptr %var_10
  %var_12 = getelementptr ptr, ptr @array1, i64 %var_77
  %var_78 = load ptr, ptr %var_12
  call void @__quantum__qis__x__body(ptr %var_78)
  %var_14 = add i64 %var_77, 1
  store i64 %var_14, ptr %var_10
  br label %block_9
block_11:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_15
  br label %block_12
block_12:
  %var_51 = load i64, ptr %var_15
  %var_16 = icmp sge i64 %var_51, 0
  br i1 %var_16, label %block_13, label %block_14
block_13:
  %var_74 = load i64, ptr %var_15
  %var_17 = getelementptr ptr, ptr @array1, i64 %var_74
  %var_75 = load ptr, ptr %var_17
  call void @__quantum__qis__x__body(ptr %var_75)
  %var_19 = add i64 %var_74, -1
  store i64 %var_19, ptr %var_15
  br label %block_12
block_14:
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_20
  br label %block_15
block_15:
  %var_53 = load i64, ptr %var_20
  %var_21 = icmp sge i64 %var_53, 0
  br i1 %var_21, label %block_16, label %block_17
block_16:
  %var_71 = load i64, ptr %var_20
  %var_22 = getelementptr ptr, ptr @array0, i64 %var_71
  %var_72 = load ptr, ptr %var_22
  call void @__quantum__qis__h__body(ptr %var_72)
  %var_24 = add i64 %var_71, -1
  store i64 %var_24, ptr %var_20
  br label %block_15
block_17:
  store i64 0, ptr %var_25
  br label %block_18
block_18:
  %var_55 = load i64, ptr %var_25
  %var_26 = icmp slt i64 %var_55, 2
  br i1 %var_26, label %block_19, label %block_20
block_19:
  %var_68 = load i64, ptr %var_25
  %var_27 = getelementptr ptr, ptr @array0, i64 %var_68
  %var_69 = load ptr, ptr %var_27
  call void @__quantum__qis__x__body(ptr %var_69)
  %var_29 = add i64 %var_68, 1
  store i64 %var_29, ptr %var_25
  br label %block_18
block_20:
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 1, ptr %var_30
  br label %block_21
block_21:
  %var_57 = load i64, ptr %var_30
  %var_31 = icmp sge i64 %var_57, 0
  br i1 %var_31, label %block_22, label %block_23
block_22:
  %var_65 = load i64, ptr %var_30
  %var_32 = getelementptr ptr, ptr @array0, i64 %var_65
  %var_66 = load ptr, ptr %var_32
  call void @__quantum__qis__x__body(ptr %var_66)
  %var_34 = add i64 %var_65, -1
  store i64 %var_34, ptr %var_30
  br label %block_21
block_23:
  store i64 0, ptr %var_35
  br label %block_24
block_24:
  %var_59 = load i64, ptr %var_35
  %var_36 = icmp slt i64 %var_59, 2
  br i1 %var_36, label %block_25, label %block_26
block_25:
  %var_62 = load i64, ptr %var_35
  %var_37 = getelementptr ptr, ptr @array0, i64 %var_62
  %var_63 = load ptr, ptr %var_37
  call void @__quantum__qis__h__body(ptr %var_63)
  %var_39 = add i64 %var_62, 1
  store i64 %var_39, ptr %var_35
  br label %block_24
block_26:
  %var_60 = load i64, ptr %var_7
  %var_40 = add i64 %var_60, 1
  store i64 %var_40, ptr %var_7
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

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
!8 = !{i32 1, !"ir_functions", i1 true}
