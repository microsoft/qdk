@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0t\00"
@2 = internal constant [8 x i8] c"2_t0t0b\00"
@3 = internal constant [8 x i8] c"3_t0t1b\00"
@4 = internal constant [6 x i8] c"4_t1t\00"
@5 = internal constant [8 x i8] c"5_t1t0b\00"
@6 = internal constant [8 x i8] c"6_t1t1b\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_11 = alloca i1
  %var_17 = alloca i1
  %var_32 = alloca i1
  %var_33 = alloca i1
  %var_34 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @CreateEntangledPair(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_8 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_8, ptr %var_11
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  %var_14 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 %var_14, ptr %var_17
  %var_42 = load i1, ptr %var_11
  %var_43 = load i1, ptr %var_17
  call void @SuperdenseEncode(i1 %var_42, i1 %var_43, ptr inttoptr (i64 0 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_25 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  %var_29 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  store i1 %var_25, ptr %var_32
  store i1 %var_29, ptr %var_33
  store i64 0, ptr %var_34
  br label %block_1
block_1:
  %var_47 = load i64, ptr %var_34
  %var_35 = icmp slt i64 %var_47, 2
  br i1 %var_35, label %block_2, label %block_3
block_2:
  %var_52 = load i64, ptr %var_34
  %var_36 = getelementptr ptr, ptr @array0, i64 %var_52
  %var_53 = load ptr, ptr %var_36
  call void @Reset(ptr %var_53)
  %var_39 = add i64 %var_52, 1
  store i64 %var_39, ptr %var_34
  br label %block_1
block_3:
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @1)
  %var_48 = load i1, ptr %var_11
  call void @__quantum__rt__bool_record_output(i1 %var_48, ptr @2)
  %var_49 = load i1, ptr %var_17
  call void @__quantum__rt__bool_record_output(i1 %var_49, ptr @3)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @4)
  %var_50 = load i1, ptr %var_32
  call void @__quantum__rt__bool_record_output(i1 %var_50, ptr @5)
  %var_51 = load i1, ptr %var_33
  call void @__quantum__rt__bool_record_output(i1 %var_51, ptr @6)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define void @CreateEntangledPair(ptr %var_1, ptr %var_2) {
block_4:
  call void @H(ptr %var_1)
  call void @CNOT(ptr %var_1, ptr %var_2)
  ret void
}

define void @H(ptr %var_3) {
block_5:
  call void @__quantum__qis__h__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @CNOT(ptr %var_4, ptr %var_5) {
block_6:
  call void @__quantum__qis__cx__body(ptr %var_4, ptr %var_5)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

define void @SuperdenseEncode(i1 %var_18, i1 %var_19, ptr %var_20) {
block_7:
  br i1 %var_18, label %block_8, label %block_9
block_8:
  call void @Z(ptr %var_20)
  br label %block_9
block_9:
  br i1 %var_19, label %block_10, label %block_11
block_10:
  call void @X(ptr %var_20)
  br label %block_11
block_11:
  ret void
}

define void @Z(ptr %var_21) {
block_12:
  call void @__quantum__qis__z__body(ptr %var_21)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define void @X(ptr %var_22) {
block_13:
  call void @__quantum__qis__x__body(ptr %var_22)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @H__Adj(ptr %var_24) {
block_14:
  call void @__quantum__qis__h__body(ptr %var_24)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

define void @Reset(ptr %var_38) {
block_15:
  call void @__quantum__qis__reset__body(ptr %var_38)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="4" }
attributes #1 = { "irreversible" }

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
