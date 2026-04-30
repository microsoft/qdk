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
  %var_3 = alloca i64
  %var_6 = alloca ptr
  %var_12 = alloca i64
  %var_14 = alloca i1
  %var_18 = alloca i64
  %var_19 = alloca i64
  %var_22 = alloca ptr
  %var_29 = alloca i64
  %var_31 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  store i64 33, ptr %var_2
  store i64 0, ptr %var_3
  br label %block_1
block_1:
  %var_37 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_37, 6
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_70 = load i64, ptr %var_3
  %var_5 = getelementptr ptr, ptr @array0, i64 %var_70
  %var_71 = load ptr, ptr %var_5
  store ptr %var_71, ptr %var_6
  %var_73 = load i64, ptr %var_2
  %var_7 = and i64 %var_73, 1
  %var_8 = icmp ne i64 %var_7, 0
  br i1 %var_8, label %block_4, label %block_6
block_3:
  %var_38 = load i64, ptr %var_2
  %var_11 = icmp eq i64 %var_38, 0
  store i64 0, ptr %var_12
  br label %block_5
block_4:
  %var_78 = load ptr, ptr %var_6
  call void @__quantum__qis__x__body(ptr %var_78)
  br label %block_6
block_5:
  %var_40 = load i64, ptr %var_12
  %var_13 = icmp sle i64 %var_40, 2
  store i1 true, ptr %var_14
  br i1 %var_13, label %block_7, label %block_8
block_6:
  %var_74 = load i64, ptr %var_2
  %var_9 = ashr i64 %var_74, 1
  store i64 %var_9, ptr %var_2
  %var_76 = load i64, ptr %var_3
  %var_10 = add i64 %var_76, 1
  store i64 %var_10, ptr %var_3
  br label %block_1
block_7:
  %var_43 = load i1, ptr %var_14
  br i1 %var_43, label %block_9, label %block_10
block_8:
  store i1 false, ptr %var_14
  br label %block_7
block_9:
  %var_66 = load i64, ptr %var_12
  %var_15 = getelementptr ptr, ptr @array1, i64 %var_66
  %var_67 = load ptr, ptr %var_15
  %var_16 = getelementptr ptr, ptr @array2, i64 %var_66
  %var_68 = load ptr, ptr %var_16
  call void @__quantum__qis__cz__body(ptr %var_67, ptr %var_68)
  %var_17 = add i64 %var_66, 1
  store i64 %var_17, ptr %var_12
  br label %block_5
block_10:
  store i64 33, ptr %var_18
  store i64 0, ptr %var_19
  br label %block_11
block_11:
  %var_46 = load i64, ptr %var_19
  %var_20 = icmp slt i64 %var_46, 6
  br i1 %var_20, label %block_12, label %block_13
block_12:
  %var_57 = load i64, ptr %var_19
  %var_21 = getelementptr ptr, ptr @array0, i64 %var_57
  %var_58 = load ptr, ptr %var_21
  store ptr %var_58, ptr %var_22
  %var_60 = load i64, ptr %var_18
  %var_23 = and i64 %var_60, 1
  %var_24 = icmp ne i64 %var_23, 0
  br i1 %var_24, label %block_14, label %block_16
block_13:
  %var_47 = load i64, ptr %var_18
  %var_27 = icmp eq i64 %var_47, 0
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_29
  br label %block_15
block_14:
  %var_65 = load ptr, ptr %var_22
  call void @__quantum__qis__x__body(ptr %var_65)
  br label %block_16
block_15:
  %var_49 = load i64, ptr %var_29
  %var_30 = icmp sle i64 %var_49, 2
  store i1 true, ptr %var_31
  br i1 %var_30, label %block_17, label %block_18
block_16:
  %var_61 = load i64, ptr %var_18
  %var_25 = ashr i64 %var_61, 1
  store i64 %var_25, ptr %var_18
  %var_63 = load i64, ptr %var_19
  %var_26 = add i64 %var_63, 1
  store i64 %var_26, ptr %var_19
  br label %block_11
block_17:
  %var_52 = load i1, ptr %var_31
  br i1 %var_52, label %block_19, label %block_20
block_18:
  store i1 false, ptr %var_31
  br label %block_17
block_19:
  %var_53 = load i64, ptr %var_29
  %var_32 = getelementptr ptr, ptr @array1, i64 %var_53
  %var_54 = load ptr, ptr %var_32
  %var_33 = getelementptr ptr, ptr @array2, i64 %var_53
  %var_55 = load ptr, ptr %var_33
  call void @__quantum__qis__cz__body(ptr %var_54, ptr %var_55)
  %var_34 = add i64 %var_53, 1
  store i64 %var_34, ptr %var_29
  br label %block_15
block_20:
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
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
