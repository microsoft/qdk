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
  store i64 1, ptr %var_0
  store i64 5, ptr %var_1
  store i64 8, ptr %var_2
  store i64 3, ptr %var_3
  br label %block_2
block_2:
  %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_10, label %block_3, label %block_4
block_3:
  %var_66 = load i64, ptr %var_0
  %var_12 = add i64 %var_66, 1
  store i64 %var_12, ptr %var_0
  %var_68 = load i64, ptr %var_1
  %var_13 = add i64 %var_68, 5
  store i64 %var_13, ptr %var_1
  %var_70 = load i64, ptr %var_2
  %var_14 = sub i64 %var_70, 2
  store i64 %var_14, ptr %var_2
  %var_72 = load i64, ptr %var_3
  %var_15 = mul i64 %var_72, 3
  store i64 %var_15, ptr %var_3
  br label %block_4
block_4:
  %var_16 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  br i1 %var_16, label %block_5, label %block_6
block_5:
  %var_58 = load i64, ptr %var_0
  %var_18 = add i64 %var_58, 1
  store i64 %var_18, ptr %var_0
  %var_60 = load i64, ptr %var_1
  %var_19 = add i64 %var_60, 5
  store i64 %var_19, ptr %var_1
  %var_62 = load i64, ptr %var_2
  %var_20 = sub i64 %var_62, 2
  store i64 %var_20, ptr %var_2
  %var_64 = load i64, ptr %var_3
  %var_21 = mul i64 %var_64, 3
  store i64 %var_21, ptr %var_3
  br label %block_6
block_6:
  %var_22 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_22, label %block_7, label %block_8
block_7:
  %var_50 = load i64, ptr %var_0
  %var_24 = add i64 %var_50, 1
  store i64 %var_24, ptr %var_0
  %var_52 = load i64, ptr %var_1
  %var_25 = add i64 %var_52, 5
  store i64 %var_25, ptr %var_1
  %var_54 = load i64, ptr %var_2
  %var_26 = sub i64 %var_54, 2
  store i64 %var_26, ptr %var_2
  %var_56 = load i64, ptr %var_3
  %var_27 = mul i64 %var_56, 3
  store i64 %var_27, ptr %var_3
  br label %block_8
block_8:
  %var_28 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  br i1 %var_28, label %block_9, label %block_10
block_9:
  %var_42 = load i64, ptr %var_0
  %var_30 = add i64 %var_42, 1
  store i64 %var_30, ptr %var_0
  %var_44 = load i64, ptr %var_1
  %var_31 = add i64 %var_44, 5
  store i64 %var_31, ptr %var_1
  %var_46 = load i64, ptr %var_2
  %var_32 = sub i64 %var_46, 2
  store i64 %var_32, ptr %var_2
  %var_48 = load i64, ptr %var_3
  %var_33 = mul i64 %var_48, 3
  store i64 %var_33, ptr %var_3
  br label %block_10
block_10:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_38 = load i64, ptr %var_0
  call void @__quantum__rt__int_record_output(i64 %var_38, ptr @1)
  %var_39 = load i64, ptr %var_1
  call void @__quantum__rt__int_record_output(i64 %var_39, ptr @2)
  %var_40 = load i64, ptr %var_2
  call void @__quantum__rt__int_record_output(i64 %var_40, ptr @3)
  %var_41 = load i64, ptr %var_3
  call void @__quantum__rt__int_record_output(i64 %var_41, ptr @4)
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
