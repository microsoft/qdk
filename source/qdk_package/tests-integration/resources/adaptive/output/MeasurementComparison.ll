@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1b\00"
@3 = internal constant [6 x i8] c"3_t2b\00"
@4 = internal constant [6 x i8] c"4_t3b\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_9 = alloca i1
  %var_12 = alloca i1
  %var_16 = alloca i1
  %var_19 = alloca i1
  %var_20 = alloca i1
  %var_21 = alloca i1
  %var_22 = alloca i1
  %var_23 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @Reset(ptr inttoptr (i64 0 to ptr))
  call void @Reset(ptr inttoptr (i64 1 to ptr))
  %var_7 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_7, ptr %var_9
  %var_10 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_11 = icmp eq i1 %var_10, false
  store i1 %var_11, ptr %var_12
  %var_13 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_14 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_15 = icmp eq i1 %var_13, %var_14
  store i1 %var_15, ptr %var_16
  %var_17 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_18 = icmp eq i1 %var_17, false
  br i1 %var_18, label %block_1, label %block_2
block_1:
  store i1 false, ptr %var_19
  br label %block_3
block_2:
  store i1 true, ptr %var_19
  br label %block_3
block_3:
  %var_28 = load i1, ptr %var_19
  store i1 %var_28, ptr %var_20
  %var_30 = load i1, ptr %var_9
  store i1 %var_30, ptr %var_21
  %var_32 = load i1, ptr %var_12
  store i1 %var_32, ptr %var_22
  %var_34 = load i1, ptr %var_16
  store i1 %var_34, ptr %var_23
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_36 = load i1, ptr %var_21
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_36, ptr @1)
  %var_37 = load i1, ptr %var_22
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_37, ptr @2)
  %var_38 = load i1, ptr %var_23
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_38, ptr @3)
  %var_39 = load i1, ptr %var_20
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_39, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_1) {
block_4:
  call void @__quantum__qis__x__body(ptr %var_1)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @CNOT(ptr %var_2, ptr %var_3) {
block_5:
  call void @__quantum__qis__cx__body(ptr %var_2, ptr %var_3)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_6) {
block_6:
  call void @__quantum__qis__reset__body(ptr %var_6)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
