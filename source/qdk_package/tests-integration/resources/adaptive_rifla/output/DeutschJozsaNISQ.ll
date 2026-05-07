@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [8 x i8] c"4_t0a2r\00"
@5 = internal constant [8 x i8] c"5_t0a3r\00"
@6 = internal constant [6 x i8] c"6_t1a\00"
@7 = internal constant [8 x i8] c"7_t1a0r\00"
@8 = internal constant [8 x i8] c"8_t1a1r\00"
@9 = internal constant [8 x i8] c"9_t1a2r\00"
@10 = internal constant [9 x i8] c"10_t1a3r\00"
@array0 = internal constant [4 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_6 = alloca i64
  %var_12 = alloca i64
  %var_18 = alloca i64
  %var_23 = alloca i64
  %var_29 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_1
  br label %block_1
block_1:
  %var_35 = load i64, ptr %var_1
  %var_2 = icmp slt i64 %var_35, 4
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_61 = load i64, ptr %var_1
  %var_3 = getelementptr ptr, ptr @array0, i64 %var_61
  %var_62 = load ptr, ptr %var_3
  call void @__quantum__qis__h__body(ptr %var_62)
  %var_5 = add i64 %var_61, 1
  store i64 %var_5, ptr %var_1
  br label %block_1
block_3:
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_6
  br label %block_4
block_4:
  %var_37 = load i64, ptr %var_6
  %var_7 = icmp sge i64 %var_37, 0
  br i1 %var_7, label %block_5, label %block_6
block_5:
  %var_58 = load i64, ptr %var_6
  %var_8 = getelementptr ptr, ptr @array0, i64 %var_58
  %var_59 = load ptr, ptr %var_8
  call void @__quantum__qis__h__body(ptr %var_59)
  %var_10 = add i64 %var_58, -1
  store i64 %var_10, ptr %var_6
  br label %block_4
block_6:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_12
  br label %block_7
block_7:
  %var_39 = load i64, ptr %var_12
  %var_13 = icmp slt i64 %var_39, 4
  br i1 %var_13, label %block_8, label %block_9
block_8:
  %var_55 = load i64, ptr %var_12
  %var_14 = getelementptr ptr, ptr @array0, i64 %var_55
  %var_56 = load ptr, ptr %var_14
  call void @__quantum__qis__reset__body(ptr %var_56)
  %var_16 = add i64 %var_55, 1
  store i64 %var_16, ptr %var_12
  br label %block_7
block_9:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_18
  br label %block_10
block_10:
  %var_41 = load i64, ptr %var_18
  %var_19 = icmp slt i64 %var_41, 4
  br i1 %var_19, label %block_11, label %block_12
block_11:
  %var_52 = load i64, ptr %var_18
  %var_20 = getelementptr ptr, ptr @array0, i64 %var_52
  %var_53 = load ptr, ptr %var_20
  call void @__quantum__qis__h__body(ptr %var_53)
  %var_22 = add i64 %var_52, 1
  store i64 %var_22, ptr %var_18
  br label %block_10
block_12:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_23
  br label %block_13
block_13:
  %var_43 = load i64, ptr %var_23
  %var_24 = icmp sge i64 %var_43, 0
  br i1 %var_24, label %block_14, label %block_15
block_14:
  %var_49 = load i64, ptr %var_23
  %var_25 = getelementptr ptr, ptr @array0, i64 %var_49
  %var_50 = load ptr, ptr %var_25
  call void @__quantum__qis__h__body(ptr %var_50)
  %var_27 = add i64 %var_49, -1
  store i64 %var_27, ptr %var_23
  br label %block_13
block_15:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  store i64 0, ptr %var_29
  br label %block_16
block_16:
  %var_45 = load i64, ptr %var_29
  %var_30 = icmp slt i64 %var_45, 4
  br i1 %var_30, label %block_17, label %block_18
block_17:
  %var_46 = load i64, ptr %var_29
  %var_31 = getelementptr ptr, ptr @array0, i64 %var_46
  %var_47 = load ptr, ptr %var_31
  call void @__quantum__qis__reset__body(ptr %var_47)
  %var_33 = add i64 %var_46, 1
  store i64 %var_33, ptr %var_29
  br label %block_16
block_18:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__array_record_output(i64 4, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @5)
  call void @__quantum__rt__array_record_output(i64 4, ptr @6)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @7)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 6 to ptr), ptr @9)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 7 to ptr), ptr @10)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="8" }
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
