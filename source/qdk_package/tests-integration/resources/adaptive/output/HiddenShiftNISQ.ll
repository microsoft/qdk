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
  %var_2 = alloca i64
  %var_7 = alloca i64
  %var_8 = alloca i64
  %var_11 = alloca ptr
  %var_17 = alloca i64
  %var_19 = alloca i1
  %var_23 = alloca i64
  %var_24 = alloca i64
  %var_27 = alloca ptr
  %var_33 = alloca i64
  %var_38 = alloca i64
  %var_40 = alloca i1
  %var_44 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_50 = load i64, ptr %var_2
  %var_3 = icmp slt i64 %var_50, 6
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_105 = load i64, ptr %var_2
  %var_4 = getelementptr ptr, ptr @array0, i64 %var_105
  %var_106 = load ptr, ptr %var_4
  call void @__quantum__qis__h__body(ptr %var_106)
  %var_6 = add i64 %var_105, 1
  store i64 %var_6, ptr %var_2
  br label %block_1
block_3:
  store i64 33, ptr %var_7
  store i64 0, ptr %var_8
  br label %block_4
block_4:
  %var_53 = load i64, ptr %var_8
  %var_9 = icmp slt i64 %var_53, 6
  br i1 %var_9, label %block_5, label %block_6
block_5:
  %var_96 = load i64, ptr %var_8
  %var_10 = getelementptr ptr, ptr @array0, i64 %var_96
  %var_97 = load ptr, ptr %var_10
  store ptr %var_97, ptr %var_11
  %var_99 = load i64, ptr %var_7
  %var_12 = and i64 %var_99, 1
  %var_13 = icmp ne i64 %var_12, 0
  br i1 %var_13, label %block_7, label %block_9
block_6:
  %var_54 = load i64, ptr %var_7
  %var_16 = icmp eq i64 %var_54, 0
  store i64 0, ptr %var_17
  br label %block_8
block_7:
  %var_104 = load ptr, ptr %var_11
  call void @__quantum__qis__x__body(ptr %var_104)
  br label %block_9
block_8:
  %var_56 = load i64, ptr %var_17
  %var_18 = icmp sle i64 %var_56, 2
  store i1 true, ptr %var_19
  br i1 %var_18, label %block_10, label %block_11
block_9:
  %var_100 = load i64, ptr %var_7
  %var_14 = ashr i64 %var_100, 1
  store i64 %var_14, ptr %var_7
  %var_102 = load i64, ptr %var_8
  %var_15 = add i64 %var_102, 1
  store i64 %var_15, ptr %var_8
  br label %block_4
block_10:
  %var_59 = load i1, ptr %var_19
  br i1 %var_59, label %block_12, label %block_13
block_11:
  store i1 false, ptr %var_19
  br label %block_10
block_12:
  %var_92 = load i64, ptr %var_17
  %var_20 = getelementptr ptr, ptr @array1, i64 %var_92
  %var_93 = load ptr, ptr %var_20
  %var_21 = getelementptr ptr, ptr @array2, i64 %var_92
  %var_94 = load ptr, ptr %var_21
  call void @__quantum__qis__cz__body(ptr %var_93, ptr %var_94)
  %var_22 = add i64 %var_92, 1
  store i64 %var_22, ptr %var_17
  br label %block_8
block_13:
  store i64 33, ptr %var_23
  store i64 0, ptr %var_24
  br label %block_14
block_14:
  %var_62 = load i64, ptr %var_24
  %var_25 = icmp slt i64 %var_62, 6
  br i1 %var_25, label %block_15, label %block_16
block_15:
  %var_83 = load i64, ptr %var_24
  %var_26 = getelementptr ptr, ptr @array0, i64 %var_83
  %var_84 = load ptr, ptr %var_26
  store ptr %var_84, ptr %var_27
  %var_86 = load i64, ptr %var_23
  %var_28 = and i64 %var_86, 1
  %var_29 = icmp ne i64 %var_28, 0
  br i1 %var_29, label %block_17, label %block_19
block_16:
  %var_63 = load i64, ptr %var_23
  %var_32 = icmp eq i64 %var_63, 0
  store i64 0, ptr %var_33
  br label %block_18
block_17:
  %var_91 = load ptr, ptr %var_27
  call void @__quantum__qis__x__body(ptr %var_91)
  br label %block_19
block_18:
  %var_65 = load i64, ptr %var_33
  %var_34 = icmp slt i64 %var_65, 6
  br i1 %var_34, label %block_20, label %block_21
block_19:
  %var_87 = load i64, ptr %var_23
  %var_30 = ashr i64 %var_87, 1
  store i64 %var_30, ptr %var_23
  %var_89 = load i64, ptr %var_24
  %var_31 = add i64 %var_89, 1
  store i64 %var_31, ptr %var_24
  br label %block_14
block_20:
  %var_80 = load i64, ptr %var_33
  %var_35 = getelementptr ptr, ptr @array0, i64 %var_80
  %var_81 = load ptr, ptr %var_35
  call void @__quantum__qis__h__body(ptr %var_81)
  %var_37 = add i64 %var_80, 1
  store i64 %var_37, ptr %var_33
  br label %block_18
block_21:
  store i64 0, ptr %var_38
  br label %block_22
block_22:
  %var_67 = load i64, ptr %var_38
  %var_39 = icmp sle i64 %var_67, 2
  store i1 true, ptr %var_40
  br i1 %var_39, label %block_23, label %block_24
block_23:
  %var_70 = load i1, ptr %var_40
  br i1 %var_70, label %block_25, label %block_26
block_24:
  store i1 false, ptr %var_40
  br label %block_23
block_25:
  %var_76 = load i64, ptr %var_38
  %var_41 = getelementptr ptr, ptr @array1, i64 %var_76
  %var_77 = load ptr, ptr %var_41
  %var_42 = getelementptr ptr, ptr @array2, i64 %var_76
  %var_78 = load ptr, ptr %var_42
  call void @__quantum__qis__cz__body(ptr %var_77, ptr %var_78)
  %var_43 = add i64 %var_76, 1
  store i64 %var_43, ptr %var_38
  br label %block_22
block_26:
  store i64 5, ptr %var_44
  br label %block_27
block_27:
  %var_72 = load i64, ptr %var_44
  %var_45 = icmp sge i64 %var_72, 0
  br i1 %var_45, label %block_28, label %block_29
block_28:
  %var_73 = load i64, ptr %var_44
  %var_46 = getelementptr ptr, ptr @array0, i64 %var_73
  %var_74 = load ptr, ptr %var_46
  call void @__quantum__qis__h__body(ptr %var_74)
  %var_48 = add i64 %var_73, -1
  store i64 %var_48, ptr %var_44
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
