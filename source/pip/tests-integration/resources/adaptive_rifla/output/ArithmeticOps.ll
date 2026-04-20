@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@3 = internal constant [6 x i8] c"3_t2i\00"
@4 = internal constant [6 x i8] c"4_t3i\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i64
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_3 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_0
  store i64 0, ptr %var_1
  store i64 10, ptr %var_2
  store i64 1, ptr %var_3
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  %var_8 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_8, label %block_1, label %block_2
block_1:
  %var_78 = load i64, ptr %var_0
  %var_10 = add i64 %var_78, 1
  store i64 %var_10, ptr %var_0
  %var_80 = load i64, ptr %var_1
  %var_11 = add i64 %var_80, 5
  store i64 %var_11, ptr %var_1
  %var_82 = load i64, ptr %var_2
  %var_12 = sub i64 %var_82, 2
  store i64 %var_12, ptr %var_2
  %var_84 = load i64, ptr %var_3
  %var_13 = mul i64 %var_84, 3
  store i64 %var_13, ptr %var_3
  br label %block_2
block_2:
  %var_14 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_14, label %block_3, label %block_4
block_3:
  %var_70 = load i64, ptr %var_0
  %var_16 = add i64 %var_70, 1
  store i64 %var_16, ptr %var_0
  %var_72 = load i64, ptr %var_1
  %var_17 = add i64 %var_72, 5
  store i64 %var_17, ptr %var_1
  %var_74 = load i64, ptr %var_2
  %var_18 = sub i64 %var_74, 2
  store i64 %var_18, ptr %var_2
  %var_76 = load i64, ptr %var_3
  %var_19 = mul i64 %var_76, 3
  store i64 %var_19, ptr %var_3
  br label %block_4
block_4:
  %var_20 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  br i1 %var_20, label %block_5, label %block_6
block_5:
  %var_62 = load i64, ptr %var_0
  %var_22 = add i64 %var_62, 1
  store i64 %var_22, ptr %var_0
  %var_64 = load i64, ptr %var_1
  %var_23 = add i64 %var_64, 5
  store i64 %var_23, ptr %var_1
  %var_66 = load i64, ptr %var_2
  %var_24 = sub i64 %var_66, 2
  store i64 %var_24, ptr %var_2
  %var_68 = load i64, ptr %var_3
  %var_25 = mul i64 %var_68, 3
  store i64 %var_25, ptr %var_3
  br label %block_6
block_6:
  %var_26 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_26, label %block_7, label %block_8
block_7:
  %var_54 = load i64, ptr %var_0
  %var_28 = add i64 %var_54, 1
  store i64 %var_28, ptr %var_0
  %var_56 = load i64, ptr %var_1
  %var_29 = add i64 %var_56, 5
  store i64 %var_29, ptr %var_1
  %var_58 = load i64, ptr %var_2
  %var_30 = sub i64 %var_58, 2
  store i64 %var_30, ptr %var_2
  %var_60 = load i64, ptr %var_3
  %var_31 = mul i64 %var_60, 3
  store i64 %var_31, ptr %var_3
  br label %block_8
block_8:
  %var_32 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  br i1 %var_32, label %block_9, label %block_10
block_9:
  %var_46 = load i64, ptr %var_0
  %var_34 = add i64 %var_46, 1
  store i64 %var_34, ptr %var_0
  %var_48 = load i64, ptr %var_1
  %var_35 = add i64 %var_48, 5
  store i64 %var_35, ptr %var_1
  %var_50 = load i64, ptr %var_2
  %var_36 = sub i64 %var_50, 2
  store i64 %var_36, ptr %var_2
  %var_52 = load i64, ptr %var_3
  %var_37 = mul i64 %var_52, 3
  store i64 %var_37, ptr %var_3
  br label %block_10
block_10:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_42 = load i64, ptr %var_0
  call void @__quantum__rt__int_record_output(i64 %var_42, ptr @1)
  %var_43 = load i64, ptr %var_1
  call void @__quantum__rt__int_record_output(i64 %var_43, ptr @2)
  %var_44 = load i64, ptr %var_2
  call void @__quantum__rt__int_record_output(i64 %var_44, ptr @3)
  %var_45 = load i64, ptr %var_3
  call void @__quantum__rt__int_record_output(i64 %var_45, ptr @4)
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
