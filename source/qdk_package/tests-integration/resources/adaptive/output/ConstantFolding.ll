@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array1 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array2 = internal constant [1 x ptr] [ptr inttoptr (i64 3 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_3 = alloca i64
  %var_4 = alloca i64
  %var_7 = alloca i1
  %var_8 = alloca i64
  %var_9 = alloca i64
  %var_18 = alloca i64
  %var_20 = alloca i64
  %var_29 = alloca i64
  %var_30 = alloca i64
  %var_35 = alloca i64
  %var_37 = alloca i64
  %var_38 = alloca i64
  %var_42 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  store i64 1, ptr %var_3
  br label %block_1
block_1:
  %var_45 = load i64, ptr %var_3
  store i64 %var_45, ptr %var_4
  %var_47 = load i64, ptr %var_4
  %var_5 = icmp sle i64 %var_47, 9
  store i1 true, ptr %var_7
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_50 = load i1, ptr %var_7
  br i1 %var_50, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_7
  br label %block_2
block_4:
  store i64 0, ptr %var_8
  br label %block_6
block_5:
  call void @Rx(double 3.141592653589793, ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_29
  br label %block_7
block_6:
  %var_70 = load i64, ptr %var_8
  store i64 %var_70, ptr %var_9
  %var_72 = load i64, ptr %var_9
  %var_10 = icmp slt i64 %var_72, 2
  br i1 %var_10, label %block_8, label %block_9
block_7:
  %var_52 = load i64, ptr %var_29
  store i64 %var_52, ptr %var_30
  %var_54 = load i64, ptr %var_30
  %var_31 = icmp slt i64 %var_54, 3
  br i1 %var_31, label %block_10, label %block_11
block_8:
  %var_77 = load i64, ptr %var_8
  %var_78_offset_chk = icmp slt i64 %var_77, 0
  %var_78_offset = select i1 %var_78_offset_chk, i64 1, i64 0
  %var_78 = getelementptr [2 x ptr], ptr @array0, i64 %var_78_offset, i64 %var_77
  %var_11 = load ptr, ptr %var_78
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr %var_11)
  store i64 %var_77, ptr %var_18
  %var_80 = load i64, ptr %var_18
  %var_19 = add i64 %var_80, 1
  store i64 %var_19, ptr %var_8
  br label %block_6
block_9:
  %var_73 = load i64, ptr %var_3
  store i64 %var_73, ptr %var_20
  %var_75 = load i64, ptr %var_20
  %var_21 = add i64 %var_75, 1
  store i64 %var_21, ptr %var_3
  br label %block_1
block_10:
  %var_64 = load i64, ptr %var_29
  %var_65_offset_chk = icmp slt i64 %var_64, 0
  %var_65_offset = select i1 %var_65_offset_chk, i64 1, i64 0
  %var_65 = getelementptr [3 x ptr], ptr @array1, i64 %var_65_offset, i64 %var_64
  %var_32 = load ptr, ptr %var_65
  call void @Reset(ptr %var_32)
  store i64 %var_64, ptr %var_35
  %var_67 = load i64, ptr %var_35
  %var_36 = add i64 %var_67, 1
  store i64 %var_36, ptr %var_29
  br label %block_7
block_11:
  store i64 0, ptr %var_37
  br label %block_12
block_12:
  %var_56 = load i64, ptr %var_37
  store i64 %var_56, ptr %var_38
  %var_58 = load i64, ptr %var_38
  %var_39 = icmp slt i64 %var_58, 1
  br i1 %var_39, label %block_13, label %block_14
block_13:
  %var_59 = load i64, ptr %var_37
  %var_60_offset_chk = icmp slt i64 %var_59, 0
  %var_60_offset = select i1 %var_60_offset_chk, i64 1, i64 0
  %var_60 = getelementptr [1 x ptr], ptr @array2, i64 %var_60_offset, i64 %var_59
  %var_40 = load ptr, ptr %var_60
  call void @Reset(ptr %var_40)
  store i64 %var_59, ptr %var_42
  %var_62 = load i64, ptr %var_42
  %var_43 = add i64 %var_62, 1
  store i64 %var_43, ptr %var_37
  br label %block_12
block_14:
  call void @__quantum__rt__array_record_output(i64 4, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_2) {
block_15:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @CNOT(ptr %var_14, ptr %var_15) {
block_16:
  call void @__quantum__qis__cx__body(ptr %var_14, ptr %var_15)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define internal void @Rx(double %var_23, ptr %var_24) {
block_17:
  call void @__quantum__qis__rx__body(double %var_23, ptr %var_24)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_34) {
block_18:
  call void @__quantum__qis__reset__body(ptr %var_34)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
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
