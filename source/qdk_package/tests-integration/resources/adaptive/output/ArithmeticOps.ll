@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@3 = internal constant [6 x i8] c"3_t2i\00"
@4 = internal constant [6 x i8] c"4_t3i\00"
@array0 = internal constant [5 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_4 = alloca i64
  %var_6 = alloca i64
  %var_44 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  store i64 0, ptr %var_2
  store i64 10, ptr %var_3
  store i64 1, ptr %var_4
  store i64 0, ptr %var_6
  br label %block_1
block_1:
  %var_55 = load i64, ptr %var_6
  %var_7 = icmp slt i64 %var_55, 5
  br i1 %var_7, label %block_2, label %block_3
block_2:
  %var_105 = load i64, ptr %var_6
  %var_8 = getelementptr ptr, ptr @array0, i64 %var_105
  %var_106 = load ptr, ptr %var_8
  call void @X(ptr %var_106)
  %var_11 = add i64 %var_105, 1
  store i64 %var_11, ptr %var_6
  br label %block_1
block_3:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  %var_14 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_14, label %block_4, label %block_5
block_4:
  %var_97 = load i64, ptr %var_1
  %var_16 = add i64 %var_97, 1
  store i64 %var_16, ptr %var_1
  %var_99 = load i64, ptr %var_2
  %var_17 = add i64 %var_99, 5
  store i64 %var_17, ptr %var_2
  %var_101 = load i64, ptr %var_3
  %var_18 = sub i64 %var_101, 2
  store i64 %var_18, ptr %var_3
  %var_103 = load i64, ptr %var_4
  %var_19 = mul i64 %var_103, 3
  store i64 %var_19, ptr %var_4
  br label %block_5
block_5:
  %var_20 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_20, label %block_6, label %block_7
block_6:
  %var_89 = load i64, ptr %var_1
  %var_22 = add i64 %var_89, 1
  store i64 %var_22, ptr %var_1
  %var_91 = load i64, ptr %var_2
  %var_23 = add i64 %var_91, 5
  store i64 %var_23, ptr %var_2
  %var_93 = load i64, ptr %var_3
  %var_24 = sub i64 %var_93, 2
  store i64 %var_24, ptr %var_3
  %var_95 = load i64, ptr %var_4
  %var_25 = mul i64 %var_95, 3
  store i64 %var_25, ptr %var_4
  br label %block_7
block_7:
  %var_26 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  br i1 %var_26, label %block_8, label %block_9
block_8:
  %var_81 = load i64, ptr %var_1
  %var_28 = add i64 %var_81, 1
  store i64 %var_28, ptr %var_1
  %var_83 = load i64, ptr %var_2
  %var_29 = add i64 %var_83, 5
  store i64 %var_29, ptr %var_2
  %var_85 = load i64, ptr %var_3
  %var_30 = sub i64 %var_85, 2
  store i64 %var_30, ptr %var_3
  %var_87 = load i64, ptr %var_4
  %var_31 = mul i64 %var_87, 3
  store i64 %var_31, ptr %var_4
  br label %block_9
block_9:
  %var_32 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_32, label %block_10, label %block_11
block_10:
  %var_73 = load i64, ptr %var_1
  %var_34 = add i64 %var_73, 1
  store i64 %var_34, ptr %var_1
  %var_75 = load i64, ptr %var_2
  %var_35 = add i64 %var_75, 5
  store i64 %var_35, ptr %var_2
  %var_77 = load i64, ptr %var_3
  %var_36 = sub i64 %var_77, 2
  store i64 %var_36, ptr %var_3
  %var_79 = load i64, ptr %var_4
  %var_37 = mul i64 %var_79, 3
  store i64 %var_37, ptr %var_4
  br label %block_11
block_11:
  %var_38 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  br i1 %var_38, label %block_12, label %block_13
block_12:
  %var_65 = load i64, ptr %var_1
  %var_40 = add i64 %var_65, 1
  store i64 %var_40, ptr %var_1
  %var_67 = load i64, ptr %var_2
  %var_41 = add i64 %var_67, 5
  store i64 %var_41, ptr %var_2
  %var_69 = load i64, ptr %var_3
  %var_42 = sub i64 %var_69, 2
  store i64 %var_42, ptr %var_3
  %var_71 = load i64, ptr %var_4
  %var_43 = mul i64 %var_71, 3
  store i64 %var_43, ptr %var_4
  br label %block_13
block_13:
  store i64 0, ptr %var_44
  br label %block_14
block_14:
  %var_57 = load i64, ptr %var_44
  %var_45 = icmp slt i64 %var_57, 5
  br i1 %var_45, label %block_15, label %block_16
block_15:
  %var_62 = load i64, ptr %var_44
  %var_46 = getelementptr ptr, ptr @array0, i64 %var_62
  %var_63 = load ptr, ptr %var_46
  call void @Reset(ptr %var_63)
  %var_49 = add i64 %var_62, 1
  store i64 %var_49, ptr %var_44
  br label %block_14
block_16:
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_58 = load i64, ptr %var_1
  call void @__quantum__rt__int_record_output(i64 %var_58, ptr @1)
  %var_59 = load i64, ptr %var_2
  call void @__quantum__rt__int_record_output(i64 %var_59, ptr @2)
  %var_60 = load i64, ptr %var_3
  call void @__quantum__rt__int_record_output(i64 %var_60, ptr @3)
  %var_61 = load i64, ptr %var_4
  call void @__quantum__rt__int_record_output(i64 %var_61, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define void @X(ptr %var_10) {
block_17:
  call void @__quantum__qis__x__body(ptr %var_10)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

define void @Reset(ptr %var_48) {
block_18:
  call void @__quantum__qis__reset__body(ptr %var_48)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
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
