@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"
@5 = internal constant [6 x i8] c"5_a4r\00"
@array0 = internal constant [5 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_3 = alloca i64
  %var_4 = alloca i64
  %var_9 = alloca i64
  %var_16 = alloca i64
  %var_17 = alloca i64
  %var_19 = alloca i64
  %var_23 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_3
  br label %block_1
block_1:
  %var_26 = load i64, ptr %var_3
  store i64 %var_26, ptr %var_4
  %var_28 = load i64, ptr %var_4
  %var_5 = icmp slt i64 %var_28, 5
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_40 = load i64, ptr %var_3
  %var_41_offset_chk = icmp slt i64 %var_40, 0
  %var_41_offset = select i1 %var_41_offset_chk, i64 1, i64 0
  %var_41 = getelementptr [5 x ptr], ptr @array0, i64 %var_41_offset, i64 %var_40
  %var_6 = load ptr, ptr %var_41
  call void @H(ptr %var_6)
  store i64 %var_40, ptr %var_9
  %var_43 = load i64, ptr %var_9
  %var_10 = add i64 %var_43, 1
  store i64 %var_10, ptr %var_3
  br label %block_1
block_3:
  call void @H(ptr inttoptr (i64 5 to ptr))
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @CNOT(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 4, ptr %var_16
  br label %block_4
block_4:
  %var_30 = load i64, ptr %var_16
  store i64 %var_30, ptr %var_17
  %var_32 = load i64, ptr %var_17
  %var_18 = icmp sge i64 %var_32, 0
  br i1 %var_18, label %block_5, label %block_6
block_5:
  %var_33 = load i64, ptr %var_16
  store i64 %var_33, ptr %var_19
  %var_35 = load i64, ptr %var_19
  %var_36_offset_chk = icmp slt i64 %var_35, 0
  %var_36_offset = select i1 %var_36_offset_chk, i64 1, i64 0
  %var_36 = getelementptr [5 x ptr], ptr @array0, i64 %var_36_offset, i64 %var_35
  %var_20 = load ptr, ptr %var_36
  call void @H__Adj(ptr %var_20)
  store i64 %var_33, ptr %var_23
  %var_38 = load i64, ptr %var_23
  %var_24 = add i64 %var_38, -1
  store i64 %var_24, ptr %var_16
  br label %block_4
block_6:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @Reset(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__rt__array_record_output(i64 5, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @5)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_2) {
block_7:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @H(ptr %var_8) {
block_8:
  call void @__quantum__qis__h__body(ptr %var_8)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @CNOT(ptr %var_12, ptr %var_13) {
block_9:
  call void @__quantum__qis__cx__body(ptr %var_12, ptr %var_13)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define internal void @H__Adj(ptr %var_22) {
block_10:
  call void @__quantum__qis__h__body(ptr %var_22)
  ret void
}

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

define internal void @Reset(ptr %var_26) {
block_11:
  call void @__quantum__qis__reset__body(ptr %var_26)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="5" }
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
