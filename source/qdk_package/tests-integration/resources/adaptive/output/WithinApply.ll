@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_8 = alloca i64
  %var_13 = alloca i64
  %var_14 = alloca i64
  %var_16 = alloca i64
  %var_20 = alloca i64
  %var_23 = alloca i64
  %var_24 = alloca i64
  %var_29 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_32 = load i64, ptr %var_2
  store i64 %var_32, ptr %var_3
  %var_34 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_34, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_55 = load i64, ptr %var_2
  %var_56_offset_chk = icmp slt i64 %var_55, 0
  %var_56_offset = select i1 %var_56_offset_chk, i64 1, i64 0
  %var_56 = getelementptr [2 x ptr], ptr @array0, i64 %var_56_offset, i64 %var_55
  %var_5 = load ptr, ptr %var_56
  call void @X(ptr %var_5)
  store i64 %var_55, ptr %var_8
  %var_58 = load i64, ptr %var_8
  %var_9 = add i64 %var_58, 1
  store i64 %var_9, ptr %var_2
  br label %block_1
block_3:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  store i64 1, ptr %var_13
  br label %block_4
block_4:
  %var_36 = load i64, ptr %var_13
  store i64 %var_36, ptr %var_14
  %var_38 = load i64, ptr %var_14
  %var_15 = icmp sge i64 %var_38, 0
  br i1 %var_15, label %block_5, label %block_6
block_5:
  %var_48 = load i64, ptr %var_13
  store i64 %var_48, ptr %var_16
  %var_50 = load i64, ptr %var_16
  %var_51_offset_chk = icmp slt i64 %var_50, 0
  %var_51_offset = select i1 %var_51_offset_chk, i64 1, i64 0
  %var_51 = getelementptr [2 x ptr], ptr @array0, i64 %var_51_offset, i64 %var_50
  %var_17 = load ptr, ptr %var_51
  call void @X__Adj(ptr %var_17)
  store i64 %var_48, ptr %var_20
  %var_53 = load i64, ptr %var_20
  %var_21 = add i64 %var_53, -1
  store i64 %var_21, ptr %var_13
  br label %block_4
block_6:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_23
  br label %block_7
block_7:
  %var_40 = load i64, ptr %var_23
  store i64 %var_40, ptr %var_24
  %var_42 = load i64, ptr %var_24
  %var_25 = icmp slt i64 %var_42, 2
  br i1 %var_25, label %block_8, label %block_9
block_8:
  %var_43 = load i64, ptr %var_23
  %var_44_offset_chk = icmp slt i64 %var_43, 0
  %var_44_offset = select i1 %var_44_offset_chk, i64 1, i64 0
  %var_44 = getelementptr [2 x ptr], ptr @array0, i64 %var_44_offset, i64 %var_43
  %var_26 = load ptr, ptr %var_44
  call void @Reset(ptr %var_26)
  store i64 %var_43, ptr %var_29
  %var_46 = load i64, ptr %var_29
  %var_30 = add i64 %var_46, 1
  store i64 %var_30, ptr %var_23
  br label %block_7
block_9:
  call void @Reset(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__rt__array_record_output(i64 3, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_7) {
block_10:
  call void @__quantum__qis__x__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

define internal void @X__Adj(ptr %var_19) {
block_11:
  call void @__quantum__qis__x__body(ptr %var_19)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_28) {
block_12:
  call void @__quantum__qis__reset__body(ptr %var_28)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
