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
  %var_8 = alloca i64
  %var_9 = alloca i64
  %var_12 = alloca ptr
  %var_19 = alloca i64
  %var_21 = alloca i1
  %var_27 = alloca i64
  %var_28 = alloca i64
  %var_31 = alloca ptr
  %var_37 = alloca i64
  %var_42 = alloca i64
  %var_44 = alloca i1
  %var_48 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_55 = load i64, ptr %var_2
  %var_3 = icmp slt i64 %var_55, 6
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_110 = load i64, ptr %var_2
  %var_4 = getelementptr ptr, ptr @array0, i64 %var_110
  %var_111 = load ptr, ptr %var_4
  call void @H(ptr %var_111)
  %var_7 = add i64 %var_110, 1
  store i64 %var_7, ptr %var_2
  br label %block_1
block_3:
  store i64 33, ptr %var_8
  store i64 0, ptr %var_9
  br label %block_4
block_4:
  %var_58 = load i64, ptr %var_9
  %var_10 = icmp slt i64 %var_58, 6
  br i1 %var_10, label %block_5, label %block_6
block_5:
  %var_101 = load i64, ptr %var_9
  %var_11 = getelementptr ptr, ptr @array0, i64 %var_101
  %var_102 = load ptr, ptr %var_11
  store ptr %var_102, ptr %var_12
  %var_104 = load i64, ptr %var_8
  %var_13 = and i64 %var_104, 1
  %var_14 = icmp ne i64 %var_13, 0
  br i1 %var_14, label %block_7, label %block_9
block_6:
  %var_59 = load i64, ptr %var_8
  %var_18 = icmp eq i64 %var_59, 0
  store i64 0, ptr %var_19
  br label %block_8
block_7:
  %var_109 = load ptr, ptr %var_12
  call void @X(ptr %var_109)
  br label %block_9
block_8:
  %var_61 = load i64, ptr %var_19
  %var_20 = icmp sle i64 %var_61, 2
  store i1 true, ptr %var_21
  br i1 %var_20, label %block_10, label %block_11
block_9:
  %var_105 = load i64, ptr %var_8
  %var_16 = ashr i64 %var_105, 1
  store i64 %var_16, ptr %var_8
  %var_107 = load i64, ptr %var_9
  %var_17 = add i64 %var_107, 1
  store i64 %var_17, ptr %var_9
  br label %block_4
block_10:
  %var_64 = load i1, ptr %var_21
  br i1 %var_64, label %block_12, label %block_13
block_11:
  store i1 false, ptr %var_21
  br label %block_10
block_12:
  %var_97 = load i64, ptr %var_19
  %var_22 = getelementptr ptr, ptr @array1, i64 %var_97
  %var_98 = load ptr, ptr %var_22
  %var_23 = getelementptr ptr, ptr @array2, i64 %var_97
  %var_99 = load ptr, ptr %var_23
  call void @CZ(ptr %var_98, ptr %var_99)
  %var_26 = add i64 %var_97, 1
  store i64 %var_26, ptr %var_19
  br label %block_8
block_13:
  store i64 33, ptr %var_27
  store i64 0, ptr %var_28
  br label %block_14
block_14:
  %var_67 = load i64, ptr %var_28
  %var_29 = icmp slt i64 %var_67, 6
  br i1 %var_29, label %block_15, label %block_16
block_15:
  %var_88 = load i64, ptr %var_28
  %var_30 = getelementptr ptr, ptr @array0, i64 %var_88
  %var_89 = load ptr, ptr %var_30
  store ptr %var_89, ptr %var_31
  %var_91 = load i64, ptr %var_27
  %var_32 = and i64 %var_91, 1
  %var_33 = icmp ne i64 %var_32, 0
  br i1 %var_33, label %block_17, label %block_19
block_16:
  %var_68 = load i64, ptr %var_27
  %var_36 = icmp eq i64 %var_68, 0
  store i64 0, ptr %var_37
  br label %block_18
block_17:
  %var_96 = load ptr, ptr %var_31
  call void @X(ptr %var_96)
  br label %block_19
block_18:
  %var_70 = load i64, ptr %var_37
  %var_38 = icmp slt i64 %var_70, 6
  br i1 %var_38, label %block_20, label %block_21
block_19:
  %var_92 = load i64, ptr %var_27
  %var_34 = ashr i64 %var_92, 1
  store i64 %var_34, ptr %var_27
  %var_94 = load i64, ptr %var_28
  %var_35 = add i64 %var_94, 1
  store i64 %var_35, ptr %var_28
  br label %block_14
block_20:
  %var_85 = load i64, ptr %var_37
  %var_39 = getelementptr ptr, ptr @array0, i64 %var_85
  %var_86 = load ptr, ptr %var_39
  call void @H(ptr %var_86)
  %var_41 = add i64 %var_85, 1
  store i64 %var_41, ptr %var_37
  br label %block_18
block_21:
  store i64 0, ptr %var_42
  br label %block_22
block_22:
  %var_72 = load i64, ptr %var_42
  %var_43 = icmp sle i64 %var_72, 2
  store i1 true, ptr %var_44
  br i1 %var_43, label %block_23, label %block_24
block_23:
  %var_75 = load i1, ptr %var_44
  br i1 %var_75, label %block_25, label %block_26
block_24:
  store i1 false, ptr %var_44
  br label %block_23
block_25:
  %var_81 = load i64, ptr %var_42
  %var_45 = getelementptr ptr, ptr @array1, i64 %var_81
  %var_82 = load ptr, ptr %var_45
  %var_46 = getelementptr ptr, ptr @array2, i64 %var_81
  %var_83 = load ptr, ptr %var_46
  call void @CZ(ptr %var_82, ptr %var_83)
  %var_47 = add i64 %var_81, 1
  store i64 %var_47, ptr %var_42
  br label %block_22
block_26:
  store i64 5, ptr %var_48
  br label %block_27
block_27:
  %var_77 = load i64, ptr %var_48
  %var_49 = icmp sge i64 %var_77, 0
  br i1 %var_49, label %block_28, label %block_29
block_28:
  %var_78 = load i64, ptr %var_48
  %var_50 = getelementptr ptr, ptr @array0, i64 %var_78
  %var_79 = load ptr, ptr %var_50
  call void @H__Adj(ptr %var_79)
  %var_53 = add i64 %var_78, -1
  store i64 %var_53, ptr %var_48
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

define void @H(ptr %var_6) {
block_30:
  call void @__quantum__qis__h__body(ptr %var_6)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @X(ptr %var_15) {
block_31:
  call void @__quantum__qis__x__body(ptr %var_15)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @CZ(ptr %var_24, ptr %var_25) {
block_32:
  call void @__quantum__qis__cz__body(ptr %var_24, ptr %var_25)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

define void @H__Adj(ptr %var_52) {
block_33:
  call void @__quantum__qis__h__body(ptr %var_52)
  ret void
}

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="6" }
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
