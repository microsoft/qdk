@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"
@5 = internal constant [6 x i8] c"5_a4r\00"
@array0 = internal constant [10 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 8 to ptr), ptr inttoptr (i64 9 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_7 = alloca i64
  %var_10 = alloca i64
  %var_11 = alloca i64
  %var_16 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 9, ptr %var_2
  br label %block_1
block_1:
  %var_19 = load i64, ptr %var_2
  store i64 %var_19, ptr %var_3
  %var_21 = load i64, ptr %var_3
  %var_4 = icmp sge i64 %var_21, 5
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_31 = load i64, ptr %var_2
  %var_32_offset_chk = icmp slt i64 %var_31, 0
  %var_32_offset = select i1 %var_32_offset_chk, i64 1, i64 0
  %var_32 = getelementptr [10 x ptr], ptr @array0, i64 %var_32_offset, i64 %var_31
  %var_5 = load ptr, ptr %var_32
  call void @X(ptr %var_5)
  store i64 %var_31, ptr %var_7
  %var_34 = load i64, ptr %var_7
  %var_8 = add i64 %var_34, -1
  store i64 %var_8, ptr %var_2
  br label %block_1
block_3:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 8 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 9 to ptr), ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_10
  br label %block_4
block_4:
  %var_23 = load i64, ptr %var_10
  store i64 %var_23, ptr %var_11
  %var_25 = load i64, ptr %var_11
  %var_12 = icmp slt i64 %var_25, 10
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_26 = load i64, ptr %var_10
  %var_27_offset_chk = icmp slt i64 %var_26, 0
  %var_27_offset = select i1 %var_27_offset_chk, i64 1, i64 0
  %var_27 = getelementptr [10 x ptr], ptr @array0, i64 %var_27_offset, i64 %var_26
  %var_13 = load ptr, ptr %var_27
  call void @Reset(ptr %var_13)
  store i64 %var_26, ptr %var_16
  %var_29 = load i64, ptr %var_16
  %var_17 = add i64 %var_29, 1
  store i64 %var_17, ptr %var_10
  br label %block_4
block_6:
  call void @__quantum__rt__array_record_output(i64 5, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @5)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_6) {
block_7:
  call void @__quantum__qis__x__body(ptr %var_6)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_15) {
block_8:
  call void @__quantum__qis__reset__body(ptr %var_15)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="10" "required_num_results"="5" }
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
