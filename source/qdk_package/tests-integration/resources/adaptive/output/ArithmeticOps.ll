@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@3 = internal constant [6 x i8] c"3_t2i\00"
@4 = internal constant [6 x i8] c"4_t3i\00"
@array0 = internal constant [5 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr)]
@array1 = internal constant [5 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_4 = alloca i64
  %var_6 = alloca i64
  %var_15 = alloca i64
  %var_33 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  store i64 0, ptr %var_2
  store i64 10, ptr %var_3
  store i64 1, ptr %var_4
  store i64 0, ptr %var_6
  br label %block_1
block_1:
  %var_7 = load i64, ptr %var_6
  %var_8 = icmp slt i64 %var_7, 5
  br i1 %var_8, label %block_2, label %block_3
block_2:
  %var_62 = load i64, ptr %var_6
  %var_63_offset_chk = icmp slt i64 %var_62, 0
  %var_63_offset = select i1 %var_63_offset_chk, i64 1, i64 0
  %var_63 = getelementptr [5 x ptr], ptr @array0, i64 %var_63_offset, i64 %var_62
  %var_9 = load ptr, ptr %var_63
  call void @X(ptr %var_9)
  %var_12 = load i64, ptr %var_6
  %var_13 = add i64 %var_12, 1
  store i64 %var_13, ptr %var_6
  br label %block_1
block_3:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_15
  br label %block_4
block_4:
  %var_16 = load i64, ptr %var_15
  %var_17 = icmp slt i64 %var_16, 5
  br i1 %var_17, label %block_5, label %block_6
block_5:
  %var_55 = load i64, ptr %var_15
  %var_56_offset_chk = icmp slt i64 %var_55, 0
  %var_56_offset = select i1 %var_56_offset_chk, i64 1, i64 0
  %var_56 = getelementptr [5 x ptr], ptr @array1, i64 %var_56_offset, i64 %var_55
  %var_18 = load ptr, ptr %var_56
  %var_21 = call zeroext i1 @__quantum__rt__read_result(ptr %var_18)
  br i1 %var_21, label %block_7, label %block_9
block_6:
  store i64 0, ptr %var_33
  br label %block_8
block_7:
  %var_23 = load i64, ptr %var_1
  %var_24 = add i64 %var_23, 1
  store i64 %var_24, ptr %var_1
  %var_25 = load i64, ptr %var_2
  %var_26 = add i64 %var_25, 5
  store i64 %var_26, ptr %var_2
  %var_27 = load i64, ptr %var_3
  %var_28 = sub i64 %var_27, 2
  store i64 %var_28, ptr %var_3
  %var_29 = load i64, ptr %var_4
  %var_30 = mul i64 %var_29, 3
  store i64 %var_30, ptr %var_4
  br label %block_9
block_8:
  %var_34 = load i64, ptr %var_33
  %var_35 = icmp slt i64 %var_34, 5
  br i1 %var_35, label %block_10, label %block_11
block_9:
  %var_31 = load i64, ptr %var_15
  %var_32 = add i64 %var_31, 1
  store i64 %var_32, ptr %var_15
  br label %block_4
block_10:
  %var_52 = load i64, ptr %var_33
  %var_53_offset_chk = icmp slt i64 %var_52, 0
  %var_53_offset = select i1 %var_53_offset_chk, i64 1, i64 0
  %var_53 = getelementptr [5 x ptr], ptr @array0, i64 %var_53_offset, i64 %var_52
  %var_36 = load ptr, ptr %var_53
  call void @Reset(ptr %var_36)
  %var_39 = load i64, ptr %var_33
  %var_40 = add i64 %var_39, 1
  store i64 %var_40, ptr %var_33
  br label %block_8
block_11:
  %var_41 = load i64, ptr %var_1
  %var_42 = load i64, ptr %var_2
  %var_43 = load i64, ptr %var_3
  %var_44 = load i64, ptr %var_4
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  call void @__quantum__rt__int_record_output(i64 %var_41, ptr @1)
  call void @__quantum__rt__int_record_output(i64 %var_42, ptr @2)
  call void @__quantum__rt__int_record_output(i64 %var_43, ptr @3)
  call void @__quantum__rt__int_record_output(i64 %var_44, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_11) {
block_12:
  call void @__quantum__qis__x__body(ptr %var_11)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @Reset(ptr %var_38) {
block_13:
  call void @__quantum__qis__reset__body(ptr %var_38)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
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
