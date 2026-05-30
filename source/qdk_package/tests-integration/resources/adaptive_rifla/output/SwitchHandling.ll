@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_6 = alloca i64
  %var_8 = alloca i64
  %var_17 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  br label %block_1
block_1:
  %var_26 = load i64, ptr %var_1
  %var_2 = icmp slt i64 %var_26, 2
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_46 = load i64, ptr %var_1
  %var_3 = getelementptr ptr, ptr @array0, i64 %var_46
  %var_47 = load ptr, ptr %var_3
  call void @__quantum__qis__x__body(ptr %var_47)
  %var_5 = add i64 %var_46, 1
  store i64 %var_5, ptr %var_1
  br label %block_1
block_3:
  store i64 0, ptr %var_6
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_8
  br label %block_4
block_4:
  %var_29 = load i64, ptr %var_8
  %var_9 = icmp slt i64 %var_29, 2
  br i1 %var_9, label %block_5, label %block_6
block_5:
  %var_38 = load i64, ptr %var_8
  %var_10 = getelementptr ptr, ptr @array1, i64 %var_38
  %var_39 = load ptr, ptr %var_10
  %var_40 = load i64, ptr %var_6
  %var_12 = shl i64 %var_40, 1
  store i64 %var_12, ptr %var_6
  %var_13 = call i1 @__quantum__rt__read_result(ptr %var_39)
  br i1 %var_13, label %block_7, label %block_9
block_6:
  store i64 0, ptr %var_17
  br label %block_8
block_7:
  %var_44 = load i64, ptr %var_6
  %var_15 = add i64 %var_44, 1
  store i64 %var_15, ptr %var_6
  br label %block_9
block_8:
  %var_31 = load i64, ptr %var_17
  %var_18 = icmp slt i64 %var_31, 2
  br i1 %var_18, label %block_10, label %block_11
block_9:
  %var_42 = load i64, ptr %var_8
  %var_16 = add i64 %var_42, 1
  store i64 %var_16, ptr %var_8
  br label %block_4
block_10:
  %var_35 = load i64, ptr %var_17
  %var_19 = getelementptr ptr, ptr @array0, i64 %var_35
  %var_36 = load ptr, ptr %var_19
  call void @__quantum__qis__reset__body(ptr %var_36)
  %var_21 = add i64 %var_35, 1
  store i64 %var_21, ptr %var_17
  br label %block_8
block_11:
  %var_32 = load i64, ptr %var_6
  %var_22 = icmp eq i64 %var_32, 0
  br i1 %var_22, label %block_12, label %block_13
block_12:
  br label %block_14
block_13:
  %var_33 = load i64, ptr %var_6
  %var_23 = icmp eq i64 %var_33, 1
  br i1 %var_23, label %block_15, label %block_16
block_14:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
  ret i64 0
block_15:
  call void @__quantum__qis__ry__body(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_17
block_16:
  %var_34 = load i64, ptr %var_6
  %var_24 = icmp eq i64 %var_34, 2
  br i1 %var_24, label %block_18, label %block_19
block_17:
  br label %block_14
block_18:
  call void @__quantum__qis__rz__body(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_20
block_19:
  call void @__quantum__qis__rx__body(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_20
block_20:
  br label %block_17
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
