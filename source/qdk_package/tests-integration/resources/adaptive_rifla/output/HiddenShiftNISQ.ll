@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"
@5 = internal constant [6 x i8] c"5_a4r\00"
@6 = internal constant [6 x i8] c"6_a5r\00"
@array0 = internal constant [6 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr)]
@array1 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array2 = internal constant [3 x ptr] [ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_6 = alloca i64
  %var_7 = alloca i64
  %var_10 = alloca ptr
  %var_16 = alloca i64
  %var_18 = alloca i1
  %var_22 = alloca i64
  %var_23 = alloca i64
  %var_26 = alloca ptr
  %var_32 = alloca i64
  %var_37 = alloca i64
  %var_39 = alloca i1
  %var_43 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  br label %block_1
block_1:
  %var_49 = load i64, ptr %var_1
  %var_2 = icmp slt i64 %var_49, 6
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_104 = load i64, ptr %var_1
  %var_3 = getelementptr ptr, ptr @array0, i64 %var_104
  %var_105 = load ptr, ptr %var_3
  call void @__quantum__qis__h__body(ptr %var_105)
  %var_5 = add i64 %var_104, 1
  store i64 %var_5, ptr %var_1
  br label %block_1
block_3:
  store i64 33, ptr %var_6
  store i64 0, ptr %var_7
  br label %block_4
block_4:
  %var_52 = load i64, ptr %var_7
  %var_8 = icmp slt i64 %var_52, 6
  br i1 %var_8, label %block_5, label %block_6
block_5:
  %var_95 = load i64, ptr %var_7
  %var_9 = getelementptr ptr, ptr @array0, i64 %var_95
  %var_96 = load ptr, ptr %var_9
  store ptr %var_96, ptr %var_10
  %var_98 = load i64, ptr %var_6
  %var_11 = and i64 %var_98, 1
  %var_12 = icmp ne i64 %var_11, 0
  br i1 %var_12, label %block_7, label %block_9
block_6:
  %var_53 = load i64, ptr %var_6
  %var_15 = icmp eq i64 %var_53, 0
  store i64 0, ptr %var_16
  br label %block_8
block_7:
  %var_103 = load ptr, ptr %var_10
  call void @__quantum__qis__x__body(ptr %var_103)
  br label %block_9
block_8:
  %var_55 = load i64, ptr %var_16
  %var_17 = icmp sle i64 %var_55, 2
  store i1 true, ptr %var_18
  br i1 %var_17, label %block_10, label %block_11
block_9:
  %var_99 = load i64, ptr %var_6
  %var_13 = ashr i64 %var_99, 1
  store i64 %var_13, ptr %var_6
  %var_101 = load i64, ptr %var_7
  %var_14 = add i64 %var_101, 1
  store i64 %var_14, ptr %var_7
  br label %block_4
block_10:
  %var_58 = load i1, ptr %var_18
  br i1 %var_58, label %block_12, label %block_13
block_11:
  store i1 false, ptr %var_18
  br label %block_10
block_12:
  %var_91 = load i64, ptr %var_16
  %var_19 = getelementptr ptr, ptr @array1, i64 %var_91
  %var_92 = load ptr, ptr %var_19
  %var_20 = getelementptr ptr, ptr @array2, i64 %var_91
  %var_93 = load ptr, ptr %var_20
  call void @__quantum__qis__cz__body(ptr %var_92, ptr %var_93)
  %var_21 = add i64 %var_91, 1
  store i64 %var_21, ptr %var_16
  br label %block_8
block_13:
  store i64 33, ptr %var_22
  store i64 0, ptr %var_23
  br label %block_14
block_14:
  %var_61 = load i64, ptr %var_23
  %var_24 = icmp slt i64 %var_61, 6
  br i1 %var_24, label %block_15, label %block_16
block_15:
  %var_82 = load i64, ptr %var_23
  %var_25 = getelementptr ptr, ptr @array0, i64 %var_82
  %var_83 = load ptr, ptr %var_25
  store ptr %var_83, ptr %var_26
  %var_85 = load i64, ptr %var_22
  %var_27 = and i64 %var_85, 1
  %var_28 = icmp ne i64 %var_27, 0
  br i1 %var_28, label %block_17, label %block_19
block_16:
  %var_62 = load i64, ptr %var_22
  %var_31 = icmp eq i64 %var_62, 0
  store i64 0, ptr %var_32
  br label %block_18
block_17:
  %var_90 = load ptr, ptr %var_26
  call void @__quantum__qis__x__body(ptr %var_90)
  br label %block_19
block_18:
  %var_64 = load i64, ptr %var_32
  %var_33 = icmp slt i64 %var_64, 6
  br i1 %var_33, label %block_20, label %block_21
block_19:
  %var_86 = load i64, ptr %var_22
  %var_29 = ashr i64 %var_86, 1
  store i64 %var_29, ptr %var_22
  %var_88 = load i64, ptr %var_23
  %var_30 = add i64 %var_88, 1
  store i64 %var_30, ptr %var_23
  br label %block_14
block_20:
  %var_79 = load i64, ptr %var_32
  %var_34 = getelementptr ptr, ptr @array0, i64 %var_79
  %var_80 = load ptr, ptr %var_34
  call void @__quantum__qis__h__body(ptr %var_80)
  %var_36 = add i64 %var_79, 1
  store i64 %var_36, ptr %var_32
  br label %block_18
block_21:
  store i64 0, ptr %var_37
  br label %block_22
block_22:
  %var_66 = load i64, ptr %var_37
  %var_38 = icmp sle i64 %var_66, 2
  store i1 true, ptr %var_39
  br i1 %var_38, label %block_23, label %block_24
block_23:
  %var_69 = load i1, ptr %var_39
  br i1 %var_69, label %block_25, label %block_26
block_24:
  store i1 false, ptr %var_39
  br label %block_23
block_25:
  %var_75 = load i64, ptr %var_37
  %var_40 = getelementptr ptr, ptr @array1, i64 %var_75
  %var_76 = load ptr, ptr %var_40
  %var_41 = getelementptr ptr, ptr @array2, i64 %var_75
  %var_77 = load ptr, ptr %var_41
  call void @__quantum__qis__cz__body(ptr %var_76, ptr %var_77)
  %var_42 = add i64 %var_75, 1
  store i64 %var_42, ptr %var_37
  br label %block_22
block_26:
  store i64 5, ptr %var_43
  br label %block_27
block_27:
  %var_71 = load i64, ptr %var_43
  %var_44 = icmp sge i64 %var_71, 0
  br i1 %var_44, label %block_28, label %block_29
block_28:
  %var_72 = load i64, ptr %var_43
  %var_45 = getelementptr ptr, ptr @array0, i64 %var_72
  %var_73 = load ptr, ptr %var_45
  call void @__quantum__qis__h__body(ptr %var_73)
  %var_47 = add i64 %var_72, -1
  store i64 %var_47, ptr %var_43
  br label %block_27
block_29:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__rt__array_record_output(i64 6, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @5)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @6)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

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
