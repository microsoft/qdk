@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i1
  %var_15 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i1 false, ptr %var_1
  call void @H(ptr inttoptr (i64 0 to ptr))
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @X(ptr inttoptr (i64 2 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_9 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_9, label %block_1, label %block_2
block_1:
  call void @X(ptr inttoptr (i64 1 to ptr))
  br label %block_2
block_2:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  %var_11 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 %var_11, ptr %var_1
  %var_23 = load i1, ptr %var_1
  br i1 %var_23, label %block_3, label %block_4
block_3:
  call void @Z(ptr inttoptr (i64 1 to ptr))
  br label %block_4
block_4:
  call void @H(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @H__Adj(ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_15
  br label %block_5
block_5:
  %var_25 = load i64, ptr %var_15
  %var_16 = icmp slt i64 %var_25, 2
  br i1 %var_16, label %block_6, label %block_7
block_6:
  %var_26 = load i64, ptr %var_15
  %var_17 = getelementptr ptr, ptr @array0, i64 %var_26
  %var_27 = load ptr, ptr %var_17
  call void @Reset(ptr %var_27)
  %var_20 = add i64 %var_26, 1
  store i64 %var_20, ptr %var_15
  br label %block_5
block_7:
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define void @H(ptr %var_5) {
block_8:
  call void @__quantum__qis__h__body(ptr %var_5)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @CNOT(ptr %var_6, ptr %var_7) {
block_9:
  call void @__quantum__qis__cx__body(ptr %var_6, ptr %var_7)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define void @X(ptr %var_8) {
block_10:
  call void @__quantum__qis__x__body(ptr %var_8)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

define void @Z(ptr %var_13) {
block_11:
  call void @__quantum__qis__z__body(ptr %var_13)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define void @H__Adj(ptr %var_14) {
block_12:
  call void @__quantum__qis__h__body(ptr %var_14)
  ret void
}

define void @Reset(ptr %var_19) {
block_13:
  call void @__quantum__qis__reset__body(ptr %var_19)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
