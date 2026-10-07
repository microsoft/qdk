@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1a\00"
@5 = internal constant [8 x i8] c"5_t1a0r\00"
@6 = internal constant [8 x i8] c"6_t1a1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_7 = alloca i64
  %var_8 = alloca i64
  %var_13 = alloca i64
  %var_18 = alloca i64
  %var_19 = alloca i64
  %var_23 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_7
  br label %block_1
block_1:
  %var_26 = load i64, ptr %var_7
  store i64 %var_26, ptr %var_8
  %var_28 = load i64, ptr %var_8
  %var_9 = icmp slt i64 %var_28, 2
  br i1 %var_9, label %block_2, label %block_3
block_2:
  %var_38 = load i64, ptr %var_7
  %var_39_offset_chk = icmp slt i64 %var_38, 0
  %var_39_offset = select i1 %var_39_offset_chk, i64 1, i64 0
  %var_39 = getelementptr [2 x ptr], ptr @array0, i64 %var_39_offset, i64 %var_38
  %var_10 = load ptr, ptr %var_39
  call void @Reset(ptr %var_10)
  store i64 %var_38, ptr %var_13
  %var_41 = load i64, ptr %var_13
  %var_14 = add i64 %var_41, 1
  store i64 %var_14, ptr %var_7
  br label %block_1
block_3:
  call void @X(ptr inttoptr (i64 2 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_18
  br label %block_4
block_4:
  %var_30 = load i64, ptr %var_18
  store i64 %var_30, ptr %var_19
  %var_32 = load i64, ptr %var_19
  %var_20 = icmp slt i64 %var_32, 2
  br i1 %var_20, label %block_5, label %block_6
block_5:
  %var_33 = load i64, ptr %var_18
  %var_34_offset_chk = icmp slt i64 %var_33, 0
  %var_34_offset = select i1 %var_34_offset_chk, i64 1, i64 0
  %var_34 = getelementptr [2 x ptr], ptr @array1, i64 %var_34_offset, i64 %var_33
  %var_21 = load ptr, ptr %var_34
  call void @Reset(ptr %var_21)
  store i64 %var_33, ptr %var_23
  %var_36 = load i64, ptr %var_23
  %var_24 = add i64 %var_36, 1
  store i64 %var_24, ptr %var_18
  br label %block_4
block_6:
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__array_record_output(i64 2, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__array_record_output(i64 2, ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @6)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @CNOT(ptr %var_2, ptr %var_3) {
block_7:
  call void @__quantum__qis__cx__body(ptr %var_2, ptr %var_3)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_12) {
block_8:
  call void @__quantum__qis__reset__body(ptr %var_12)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

define internal void @X(ptr %var_16) {
block_9:
  call void @__quantum__qis__x__body(ptr %var_16)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

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
