@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0b\00"
@2 = internal constant [6 x i8] c"2_a1b\00"
@array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_7 = alloca [4 x i1]
  %var_16 = alloca [3 x i1]
  %var_17 = alloca i64
  %var_19 = alloca i1
  %var_21 = alloca i64
  %var_24 = alloca [2 x i1]
  %var_25 = alloca [2 x i1]
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  br label %block_1
block_1:
  %var_29 = load i64, ptr %var_1
  %var_2 = icmp slt i64 %var_29, 3
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_48 = load i64, ptr %var_1
  %var_3_offset_chk = icmp slt i64 %var_48, 0
  %var_3_offset = select i1 %var_3_offset_chk, i64 1, i64 0
  %var_3 = getelementptr [3 x ptr], ptr @array0, i64 %var_3_offset, i64 %var_48
  %var_49 = load ptr, ptr %var_3
  call void @X(ptr %var_49)
  %var_6 = add i64 %var_48, 1
  store i64 %var_6, ptr %var_1
  br label %block_1
block_3:
  %var_7_0 = getelementptr [4 x i1], ptr %var_7, i64 0, i64 0
  store i1 false, ptr %var_7_0
  %var_7_1 = getelementptr [4 x i1], ptr %var_7, i64 0, i64 1
  store i1 false, ptr %var_7_1
  %var_7_2 = getelementptr [4 x i1], ptr %var_7, i64 0, i64 2
  store i1 false, ptr %var_7_2
  %var_7_3 = getelementptr [4 x i1], ptr %var_7, i64 0, i64 3
  store i1 false, ptr %var_7_3
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_12 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_14 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_16_0 = getelementptr [3 x i1], ptr %var_16, i64 0, i64 0
  store i1 %var_10, ptr %var_16_0
  %var_16_1 = getelementptr [3 x i1], ptr %var_16, i64 0, i64 1
  store i1 %var_12, ptr %var_16_1
  %var_16_2 = getelementptr [3 x i1], ptr %var_16, i64 0, i64 2
  store i1 %var_14, ptr %var_16_2
  store i64 0, ptr %var_17
  br label %block_4
block_4:
  %var_33 = load i64, ptr %var_17
  %var_18 = icmp sle i64 %var_33, 2
  store i1 true, ptr %var_19
  br i1 %var_18, label %block_5, label %block_6
block_5:
  %var_36 = load i1, ptr %var_19
  br i1 %var_36, label %block_7, label %block_8
block_6:
  store i1 false, ptr %var_19
  br label %block_5
block_7:
  %var_42 = load i64, ptr %var_17
  %var_20 = add i64 %var_42, 1
  store i64 %var_20, ptr %var_21
  %var_22_offset_chk = icmp slt i64 %var_42, 0
  %var_22_offset = select i1 %var_22_offset_chk, i64 1, i64 0
  %var_22 = getelementptr [3 x i1], ptr %var_16, i64 %var_22_offset, i64 %var_42
  %var_44 = load i1, ptr %var_22
  %var_45 = load i64, ptr %var_21
  %var_46_offset_chk = icmp slt i64 %var_45, 0
  %var_46_offset = select i1 %var_46_offset_chk, i64 1, i64 0
  %var_46 = getelementptr [4 x i1], ptr %var_7, i64 %var_46_offset, i64 %var_45
  store i1 %var_44, ptr %var_46
  %var_23 = add i64 %var_42, 1
  store i64 %var_23, ptr %var_17
  br label %block_4
block_8:
  %var_24_0_src = getelementptr [4 x i1], ptr %var_7, i64 0, i64 1
  %var_24_0 = load i1, ptr %var_24_0_src
  %var_24_0_dst = getelementptr [2 x i1], ptr %var_24, i64 0, i64 0
  store i1 %var_24_0, ptr %var_24_0_dst
  %var_24_1_src = getelementptr [4 x i1], ptr %var_7, i64 0, i64 3
  %var_24_1 = load i1, ptr %var_24_1_src
  %var_24_1_dst = getelementptr [2 x i1], ptr %var_24, i64 0, i64 1
  store i1 %var_24_1, ptr %var_24_1_dst
  %var_38 = load [2 x i1], ptr %var_24
  store [2 x i1] %var_38, ptr %var_25
  %var_26_offset_chk = icmp slt i64 0, 0
  %var_26_offset = select i1 %var_26_offset_chk, i64 1, i64 0
  %var_26 = getelementptr [2 x i1], ptr %var_25, i64 %var_26_offset, i64 0
  %var_40 = load i1, ptr %var_26
  %var_27_offset_chk = icmp slt i64 1, 0
  %var_27_offset = select i1 %var_27_offset_chk, i64 1, i64 0
  %var_27 = getelementptr [2 x i1], ptr %var_25, i64 %var_27_offset, i64 1
  %var_41 = load i1, ptr %var_27
  call void @__quantum__rt__array_record_output(i64 2, ptr @0)
  call void @__quantum__rt__bool_record_output(i1 %var_40, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 %var_41, ptr @2)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_5) {
block_9:
  call void @__quantum__qis__x__body(ptr %var_5)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr) #2

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
attributes #1 = { "irreversible" }
attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

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
