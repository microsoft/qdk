@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1b\00"
@3 = internal constant [6 x i8] c"3_t2b\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i1
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_4 = alloca i64
  %var_7 = alloca i1
  %var_11 = alloca i64
  %var_13 = alloca i64
  %var_16 = alloca i64
  %var_19 = alloca i64
  %var_22 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i1 false, ptr %var_0
  store i64 0, ptr %var_2
  store i64 1, ptr %var_3
  br label %block_1
block_1:
  %var_27 = load i64, ptr %var_3
  store i64 %var_27, ptr %var_4
  %var_29 = load i64, ptr %var_4
  %var_5 = icmp sle i64 %var_29, 10
  store i1 true, ptr %var_7
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_32 = load i1, ptr %var_7
  br i1 %var_32, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_7
  br label %block_2
block_4:
  call void @X(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_9 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_9, ptr %var_0
  %var_41 = load i1, ptr %var_0
  br i1 %var_41, label %block_6, label %block_7
block_5:
  call void @Reset(ptr inttoptr (i64 0 to ptr))
  %var_33 = load i64, ptr %var_2
  store i64 %var_33, ptr %var_16
  %var_35 = load i64, ptr %var_16
  %var_17 = icmp sgt i64 %var_35, 5
  store i64 %var_33, ptr %var_19
  %var_37 = load i64, ptr %var_19
  %var_20 = icmp slt i64 %var_37, 5
  store i64 %var_33, ptr %var_22
  %var_39 = load i64, ptr %var_22
  %var_23 = icmp eq i64 %var_39, 10
  call void @__quantum__rt__tuple_record_output(i64 3, ptr @0)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_17, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_20, ptr @2)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_23, ptr @3)
  ret i64 0
block_6:
  call void @X(ptr inttoptr (i64 0 to ptr))
  %var_46 = load i64, ptr %var_2
  store i64 %var_46, ptr %var_11
  %var_48 = load i64, ptr %var_11
  %var_12 = add i64 %var_48, 1
  store i64 %var_12, ptr %var_2
  br label %block_7
block_7:
  %var_42 = load i64, ptr %var_3
  store i64 %var_42, ptr %var_13
  %var_44 = load i64, ptr %var_13
  %var_14 = add i64 %var_44, 1
  store i64 %var_14, ptr %var_3
  br label %block_1
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_8) {
block_8:
  call void @__quantum__qis__x__body(ptr %var_8)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @Reset(ptr %var_15) {
block_9:
  call void @__quantum__qis__reset__body(ptr %var_15)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
