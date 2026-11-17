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
  %var_15 = alloca i64
  %var_25 = alloca i64
  %var_35 = alloca i64
  %var_42 = alloca i64
  %var_51 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 4 to ptr))
  call void @H(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_4
  br label %block_1
block_1:
  %var_5 = load i64, ptr %var_4
  %var_6 = icmp slt i64 %var_5, 4
  br i1 %var_6, label %block_2, label %block_3
block_2:
  %var_77 = load i64, ptr %var_4
  %var_78_offset_chk = icmp slt i64 %var_77, 0
  %var_78_offset = select i1 %var_78_offset_chk, i64 1, i64 0
  %var_78 = getelementptr [4 x ptr], ptr @array0, i64 %var_78_offset, i64 %var_77
  %var_7 = load ptr, ptr %var_78
  call void @H(ptr %var_7)
  %var_9 = load i64, ptr %var_4
  %var_10 = add i64 %var_9, 1
  store i64 %var_10, ptr %var_4
  br label %block_1
block_3:
  call void @CX(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_15
  br label %block_4
block_4:
  %var_16 = load i64, ptr %var_15
  %var_17 = icmp sge i64 %var_16, 0
  br i1 %var_17, label %block_5, label %block_6
block_5:
  %var_18 = load i64, ptr %var_15
  %var_75_offset_chk = icmp slt i64 %var_18, 0
  %var_75_offset = select i1 %var_75_offset_chk, i64 1, i64 0
  %var_75 = getelementptr [4 x ptr], ptr @array0, i64 %var_75_offset, i64 %var_18
  %var_19 = load ptr, ptr %var_75
  call void @H__Adj(ptr %var_19)
  %var_22 = load i64, ptr %var_15
  %var_23 = add i64 %var_22, -1
  store i64 %var_23, ptr %var_15
  br label %block_4
block_6:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_25
  br label %block_7
block_7:
  %var_26 = load i64, ptr %var_25
  %var_27 = icmp slt i64 %var_26, 4
  br i1 %var_27, label %block_8, label %block_9
block_8:
  %var_72 = load i64, ptr %var_25
  %var_73_offset_chk = icmp slt i64 %var_72, 0
  %var_73_offset = select i1 %var_73_offset_chk, i64 1, i64 0
  %var_73 = getelementptr [4 x ptr], ptr @array0, i64 %var_73_offset, i64 %var_72
  %var_28 = load ptr, ptr %var_73
  call void @Reset(ptr %var_28)
  %var_31 = load i64, ptr %var_25
  %var_32 = add i64 %var_31, 1
  store i64 %var_32, ptr %var_25
  br label %block_7
block_9:
  call void @Reset(ptr inttoptr (i64 4 to ptr))
  call void @X(ptr inttoptr (i64 4 to ptr))
  call void @H(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_35
  br label %block_10
block_10:
  %var_36 = load i64, ptr %var_35
  %var_37 = icmp slt i64 %var_36, 4
  br i1 %var_37, label %block_11, label %block_12
block_11:
  %var_69 = load i64, ptr %var_35
  %var_70_offset_chk = icmp slt i64 %var_69, 0
  %var_70_offset = select i1 %var_70_offset_chk, i64 1, i64 0
  %var_70 = getelementptr [4 x ptr], ptr @array0, i64 %var_70_offset, i64 %var_69
  %var_38 = load ptr, ptr %var_70
  call void @H(ptr %var_38)
  %var_40 = load i64, ptr %var_35
  %var_41 = add i64 %var_40, 1
  store i64 %var_41, ptr %var_35
  br label %block_10
block_12:
  call void @X(ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_42
  br label %block_13
block_13:
  %var_43 = load i64, ptr %var_42
  %var_44 = icmp sge i64 %var_43, 0
  br i1 %var_44, label %block_14, label %block_15
block_14:
  %var_45 = load i64, ptr %var_42
  %var_67_offset_chk = icmp slt i64 %var_45, 0
  %var_67_offset = select i1 %var_67_offset_chk, i64 1, i64 0
  %var_67 = getelementptr [4 x ptr], ptr @array0, i64 %var_67_offset, i64 %var_45
  %var_46 = load ptr, ptr %var_67
  call void @H__Adj(ptr %var_46)
  %var_48 = load i64, ptr %var_42
  %var_49 = add i64 %var_48, -1
  store i64 %var_49, ptr %var_42
  br label %block_13
block_15:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  store i64 0, ptr %var_51
  br label %block_16
block_16:
  %var_52 = load i64, ptr %var_51
  %var_53 = icmp slt i64 %var_52, 4
  br i1 %var_53, label %block_17, label %block_18
block_17:
  %var_64 = load i64, ptr %var_51
  %var_65_offset_chk = icmp slt i64 %var_64, 0
  %var_65_offset = select i1 %var_65_offset_chk, i64 1, i64 0
  %var_65 = getelementptr [4 x ptr], ptr @array0, i64 %var_65_offset, i64 %var_64
  %var_54 = load ptr, ptr %var_65
  call void @Reset(ptr %var_54)
  %var_56 = load i64, ptr %var_51
  %var_57 = add i64 %var_56, 1
  store i64 %var_57, ptr %var_51
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

define internal void @X(ptr %var_2) {
block_19:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @H(ptr %var_3) {
block_20:
  call void @__quantum__qis__h__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @CX(ptr %var_11, ptr %var_12) {
block_21:
  call void @__quantum__qis__cx__body(ptr %var_11, ptr %var_12)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define internal void @H__Adj(ptr %var_21) {
block_22:
  call void @__quantum__qis__h__body(ptr %var_21)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_30) {
block_23:
  call void @__quantum__qis__reset__body(ptr %var_30)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="8" }
attributes #1 = { "irreversible" }
attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
!8 = !{i32 1, !"ir_functions", i1 true}
!9 = !{i32 1, !"writable_results", i1 true}
