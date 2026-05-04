@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@3 = internal constant [6 x i8] c"3_t2i\00"
@4 = internal constant [6 x i8] c"4_t3i\00"
@array0 = internal constant [5 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i64
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_5 = alloca i64
  %var_42 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_0
  store i64 0, ptr %var_1
  store i64 10, ptr %var_2
  store i64 1, ptr %var_3
  store i64 0, ptr %var_5
  br label %block_1
block_1:
  %var_52 = load i64, ptr %var_5
  %var_6 = icmp slt i64 %var_52, 5
  br i1 %var_6, label %block_2, label %block_3
block_2:
  %var_102 = load i64, ptr %var_5
  %var_7 = getelementptr ptr, ptr @array0, i64 %var_102
  %var_103 = load ptr, ptr %var_7
  call void @__quantum__qis__x__body(ptr %var_103)
  %var_9 = add i64 %var_102, 1
  store i64 %var_9, ptr %var_5
  br label %block_1
block_3:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  %var_12 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_12, label %block_4, label %block_5
block_4:
  %var_94 = load i64, ptr %var_0
  %var_14 = add i64 %var_94, 1
  store i64 %var_14, ptr %var_0
  %var_96 = load i64, ptr %var_1
  %var_15 = add i64 %var_96, 5
  store i64 %var_15, ptr %var_1
  %var_98 = load i64, ptr %var_2
  %var_16 = sub i64 %var_98, 2
  store i64 %var_16, ptr %var_2
  %var_100 = load i64, ptr %var_3
  %var_17 = mul i64 %var_100, 3
  store i64 %var_17, ptr %var_3
  br label %block_5
block_5:
  %var_18 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_18, label %block_6, label %block_7
block_6:
  %var_86 = load i64, ptr %var_0
  %var_20 = add i64 %var_86, 1
  store i64 %var_20, ptr %var_0
  %var_88 = load i64, ptr %var_1
  %var_21 = add i64 %var_88, 5
  store i64 %var_21, ptr %var_1
  %var_90 = load i64, ptr %var_2
  %var_22 = sub i64 %var_90, 2
  store i64 %var_22, ptr %var_2
  %var_92 = load i64, ptr %var_3
  %var_23 = mul i64 %var_92, 3
  store i64 %var_23, ptr %var_3
  br label %block_7
block_7:
  %var_24 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  br i1 %var_24, label %block_8, label %block_9
block_8:
  %var_78 = load i64, ptr %var_0
  %var_26 = add i64 %var_78, 1
  store i64 %var_26, ptr %var_0
  %var_80 = load i64, ptr %var_1
  %var_27 = add i64 %var_80, 5
  store i64 %var_27, ptr %var_1
  %var_82 = load i64, ptr %var_2
  %var_28 = sub i64 %var_82, 2
  store i64 %var_28, ptr %var_2
  %var_84 = load i64, ptr %var_3
  %var_29 = mul i64 %var_84, 3
  store i64 %var_29, ptr %var_3
  br label %block_9
block_9:
  %var_30 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_30, label %block_10, label %block_11
block_10:
  %var_70 = load i64, ptr %var_0
  %var_32 = add i64 %var_70, 1
  store i64 %var_32, ptr %var_0
  %var_72 = load i64, ptr %var_1
  %var_33 = add i64 %var_72, 5
  store i64 %var_33, ptr %var_1
  %var_74 = load i64, ptr %var_2
  %var_34 = sub i64 %var_74, 2
  store i64 %var_34, ptr %var_2
  %var_76 = load i64, ptr %var_3
  %var_35 = mul i64 %var_76, 3
  store i64 %var_35, ptr %var_3
  br label %block_11
block_11:
  %var_36 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  br i1 %var_36, label %block_12, label %block_13
block_12:
  %var_62 = load i64, ptr %var_0
  %var_38 = add i64 %var_62, 1
  store i64 %var_38, ptr %var_0
  %var_64 = load i64, ptr %var_1
  %var_39 = add i64 %var_64, 5
  store i64 %var_39, ptr %var_1
  %var_66 = load i64, ptr %var_2
  %var_40 = sub i64 %var_66, 2
  store i64 %var_40, ptr %var_2
  %var_68 = load i64, ptr %var_3
  %var_41 = mul i64 %var_68, 3
  store i64 %var_41, ptr %var_3
  br label %block_13
block_13:
  store i64 0, ptr %var_42
  br label %block_14
block_14:
  %var_54 = load i64, ptr %var_42
  %var_43 = icmp slt i64 %var_54, 5
  br i1 %var_43, label %block_15, label %block_16
block_15:
  %var_59 = load i64, ptr %var_42
  %var_44 = getelementptr ptr, ptr @array0, i64 %var_59
  %var_60 = load ptr, ptr %var_44
  call void @__quantum__qis__reset__body(ptr %var_60)
  %var_46 = add i64 %var_59, 1
  store i64 %var_46, ptr %var_42
  br label %block_14
block_16:
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_55 = load i64, ptr %var_0
  call void @__quantum__rt__int_record_output(i64 %var_55, ptr @1)
  %var_56 = load i64, ptr %var_1
  call void @__quantum__rt__int_record_output(i64 %var_56, ptr @2)
  %var_57 = load i64, ptr %var_2
  call void @__quantum__rt__int_record_output(i64 %var_57, ptr @3)
  %var_58 = load i64, ptr %var_3
  call void @__quantum__rt__int_record_output(i64 %var_58, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
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
