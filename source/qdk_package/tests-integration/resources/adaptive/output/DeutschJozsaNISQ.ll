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
  %var_4 = alloca i64
  %var_11 = alloca i64
  %var_18 = alloca i64
  %var_26 = alloca i64
  %var_31 = alloca i64
  %var_37 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 4 to ptr))
  call void @H(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_4
  br label %block_1
block_1:
  %var_43 = load i64, ptr %var_4
  %var_5 = icmp slt i64 %var_43, 4
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_69 = load i64, ptr %var_4
  %var_6 = getelementptr ptr, ptr @array0, i64 %var_69
  %var_70 = load ptr, ptr %var_6
  call void @H(ptr %var_70)
  %var_8 = add i64 %var_69, 1
  store i64 %var_8, ptr %var_4
  br label %block_1
block_3:
  call void @CX(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_11
  br label %block_4
block_4:
  %var_45 = load i64, ptr %var_11
  %var_12 = icmp sge i64 %var_45, 0
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_66 = load i64, ptr %var_11
  %var_13 = getelementptr ptr, ptr @array0, i64 %var_66
  %var_67 = load ptr, ptr %var_13
  call void @H__Adj(ptr %var_67)
  %var_16 = add i64 %var_66, -1
  store i64 %var_16, ptr %var_11
  br label %block_4
block_6:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_18
  br label %block_7
block_7:
  %var_47 = load i64, ptr %var_18
  %var_19 = icmp slt i64 %var_47, 4
  br i1 %var_19, label %block_8, label %block_9
block_8:
  %var_63 = load i64, ptr %var_18
  %var_20 = getelementptr ptr, ptr @array0, i64 %var_63
  %var_64 = load ptr, ptr %var_20
  call void @Reset(ptr %var_64)
  %var_23 = add i64 %var_63, 1
  store i64 %var_23, ptr %var_18
  br label %block_7
block_9:
  call void @Reset(ptr inttoptr (i64 4 to ptr))
  call void @X(ptr inttoptr (i64 4 to ptr))
  call void @H(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_26
  br label %block_10
block_10:
  %var_49 = load i64, ptr %var_26
  %var_27 = icmp slt i64 %var_49, 4
  br i1 %var_27, label %block_11, label %block_12
block_11:
  %var_60 = load i64, ptr %var_26
  %var_28 = getelementptr ptr, ptr @array0, i64 %var_60
  %var_61 = load ptr, ptr %var_28
  call void @H(ptr %var_61)
  %var_30 = add i64 %var_60, 1
  store i64 %var_30, ptr %var_26
  br label %block_10
block_12:
  call void @X(ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_31
  br label %block_13
block_13:
  %var_51 = load i64, ptr %var_31
  %var_32 = icmp sge i64 %var_51, 0
  br i1 %var_32, label %block_14, label %block_15
block_14:
  %var_57 = load i64, ptr %var_31
  %var_33 = getelementptr ptr, ptr @array0, i64 %var_57
  %var_58 = load ptr, ptr %var_33
  call void @H__Adj(ptr %var_58)
  %var_35 = add i64 %var_57, -1
  store i64 %var_35, ptr %var_31
  br label %block_13
block_15:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  store i64 0, ptr %var_37
  br label %block_16
block_16:
  %var_53 = load i64, ptr %var_37
  %var_38 = icmp slt i64 %var_53, 4
  br i1 %var_38, label %block_17, label %block_18
block_17:
  %var_54 = load i64, ptr %var_37
  %var_39 = getelementptr ptr, ptr @array0, i64 %var_54
  %var_55 = load ptr, ptr %var_39
  call void @Reset(ptr %var_55)
  %var_41 = add i64 %var_54, 1
  store i64 %var_41, ptr %var_37
  br label %block_16
block_18:
  call void @Reset(ptr inttoptr (i64 4 to ptr))
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

define void @X(ptr %var_2) {
block_19:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @H(ptr %var_3) {
block_20:
  call void @__quantum__qis__h__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @CX(ptr %var_9, ptr %var_10) {
block_21:
  call void @__quantum__qis__cx__body(ptr %var_9, ptr %var_10)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define void @H__Adj(ptr %var_15) {
block_22:
  call void @__quantum__qis__h__body(ptr %var_15)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

define void @Reset(ptr %var_22) {
block_23:
  call void @__quantum__qis__reset__body(ptr %var_22)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="8" }
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
