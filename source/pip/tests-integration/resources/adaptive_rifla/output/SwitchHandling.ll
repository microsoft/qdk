@0 = internal constant [4 x i8] c"0_r\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_2
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_2
  %var_5 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_5, label %block_1, label %block_2
block_1:
  store i64 1, ptr %var_2
  br label %block_2
block_2:
  %var_17 = load i64, ptr %var_2
  %var_7 = shl i64 %var_17, 1
  store i64 %var_7, ptr %var_2
  %var_8 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_8, label %block_3, label %block_4
block_3:
  %var_22 = load i64, ptr %var_2
  %var_10 = add i64 %var_22, 1
  store i64 %var_10, ptr %var_2
  br label %block_4
block_4:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  %var_19 = load i64, ptr %var_2
  %var_12 = icmp eq i64 %var_19, 0
  br i1 %var_12, label %block_5, label %block_6
block_5:
  br label %block_13
block_6:
  %var_20 = load i64, ptr %var_2
  %var_13 = icmp eq i64 %var_20, 1
  br i1 %var_13, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__ry__body(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_12
block_8:
  %var_21 = load i64, ptr %var_2
  %var_14 = icmp eq i64 %var_21, 2
  br i1 %var_14, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__rz__body(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_11
block_10:
  call void @__quantum__qis__rx__body(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_11
block_11:
  br label %block_12
block_12:
  br label %block_13
block_13:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__qis__ry__body(double, ptr)

declare void @__quantum__qis__rz__body(double, ptr)

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
