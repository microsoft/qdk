@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1i\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i64
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_6 = alloca i1
  %var_9 = alloca i64
  %var_14 = alloca i64
  %var_15 = alloca i64
  %var_16 = alloca i64
  %var_20 = alloca i1
  %var_22 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 1, ptr %var_0
  store i64 0, ptr %var_1
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_3 = load i64, ptr %var_2
  %var_4 = icmp sle i64 %var_3, 0
  store i1 true, ptr %var_6
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_35 = load i1, ptr %var_6
  br i1 %var_35, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_6
  br label %block_2
block_4:
  %var_7 = load i64, ptr %var_2
  call void @X(ptr inttoptr (i64 0 to ptr))
  %var_48 = load i64, ptr %var_0
  store i64 %var_48, ptr %var_9
  %var_11 = icmp eq i64 %var_7, 7
  br i1 %var_11, label %block_6, label %block_8
block_5:
  store i64 1, ptr %var_14
  store i64 0, ptr %var_15
  store i64 0, ptr %var_16
  br label %block_7
block_6:
  call void @X(ptr inttoptr (i64 0 to ptr))
  br label %block_8
block_7:
  %var_17 = load i64, ptr %var_16
  %var_18 = icmp sle i64 %var_17, 1
  store i1 true, ptr %var_20
  br i1 %var_18, label %block_9, label %block_10
block_8:
  store i64 5, ptr %var_0
  %var_51 = load i64, ptr %var_9
  store i64 %var_51, ptr %var_1
  %var_12 = load i64, ptr %var_2
  %var_13 = add i64 %var_12, 1
  store i64 %var_13, ptr %var_2
  br label %block_1
block_9:
  %var_41 = load i1, ptr %var_20
  br i1 %var_41, label %block_11, label %block_12
block_10:
  store i1 false, ptr %var_20
  br label %block_9
block_11:
  %var_21 = load i64, ptr %var_16
  call void @X(ptr inttoptr (i64 0 to ptr))
  %var_42 = load i64, ptr %var_14
  store i64 %var_42, ptr %var_22
  %var_24 = icmp eq i64 %var_21, 7
  br i1 %var_24, label %block_13, label %block_14
block_12:
  call void @Reset(ptr inttoptr (i64 0 to ptr))
  %var_28 = load i64, ptr %var_1
  %var_29 = load i64, ptr %var_15
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__int_record_output(i64 %var_28, ptr @1)
  call void @__quantum__rt__int_record_output(i64 %var_29, ptr @2)
  ret i64 0
block_13:
  call void @X(ptr inttoptr (i64 0 to ptr))
  br label %block_14
block_14:
  store i64 5, ptr %var_14
  %var_45 = load i64, ptr %var_22
  store i64 %var_45, ptr %var_15
  %var_25 = load i64, ptr %var_16
  %var_26 = add i64 %var_25, 1
  store i64 %var_26, ptr %var_16
  br label %block_7
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_8) {
block_15:
  call void @__quantum__qis__x__body(ptr %var_8)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @Reset(ptr %var_27) {
block_16:
  call void @__quantum__qis__reset__body(ptr %var_27)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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
