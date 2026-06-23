@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_8 = alloca i64
  %var_18 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_32 = load i64, ptr %var_2
  %var_3 = icmp slt i64 %var_32, 2
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_49 = load i64, ptr %var_2
  %var_4 = getelementptr ptr, ptr @array0, i64 %var_49
  %var_50 = load ptr, ptr %var_4
  call void @X(ptr %var_50)
  %var_7 = add i64 %var_49, 1
  store i64 %var_7, ptr %var_2
  br label %block_1
block_3:
  store i64 0, ptr %var_8
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_8
  %var_11 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_11, label %block_4, label %block_5
block_4:
  %var_47 = load i64, ptr %var_8
  %var_13 = add i64 %var_47, 1
  store i64 %var_13, ptr %var_8
  br label %block_5
block_5:
  %var_35 = load i64, ptr %var_8
  %var_14 = shl i64 %var_35, 1
  store i64 %var_14, ptr %var_8
  %var_15 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_15, label %block_6, label %block_7
block_6:
  %var_45 = load i64, ptr %var_8
  %var_17 = add i64 %var_45, 1
  store i64 %var_17, ptr %var_8
  br label %block_7
block_7:
  store i64 0, ptr %var_18
  br label %block_8
block_8:
  %var_38 = load i64, ptr %var_18
  %var_19 = icmp slt i64 %var_38, 2
  br i1 %var_19, label %block_9, label %block_10
block_9:
  %var_42 = load i64, ptr %var_18
  %var_20 = getelementptr ptr, ptr @array0, i64 %var_42
  %var_43 = load ptr, ptr %var_20
  call void @Reset(ptr %var_43)
  %var_23 = add i64 %var_42, 1
  store i64 %var_23, ptr %var_18
  br label %block_8
block_10:
  %var_39 = load i64, ptr %var_8
  %var_24 = icmp eq i64 %var_39, 0
  br i1 %var_24, label %block_11, label %block_12
block_11:
  call void @ApplyGlobalPhase(double -1.5707963267948966)
  br label %block_13
block_12:
  %var_40 = load i64, ptr %var_8
  %var_27 = icmp eq i64 %var_40, 1
  br i1 %var_27, label %block_14, label %block_15
block_13:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
  ret i64 0
block_14:
  call void @Ry(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_16
block_15:
  %var_41 = load i64, ptr %var_8
  %var_30 = icmp eq i64 %var_41, 2
  br i1 %var_30, label %block_17, label %block_18
block_16:
  br label %block_13
block_17:
  call void @Rz(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_19
block_18:
  call void @Rx(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_19
block_19:
  br label %block_16
}

declare void @__quantum__rt__initialize(ptr)

define void @X(ptr %var_6) {
block_20:
  call void @__quantum__qis__x__body(ptr %var_6)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

define void @Reset(ptr %var_22) {
block_21:
  call void @__quantum__qis__reset__body(ptr %var_22)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

define void @ApplyGlobalPhase(double %var_25) {
block_22:
  call void @ControllableGlobalPhase(double %var_25)
  ret void
}

define void @ControllableGlobalPhase(double %var_26) {
block_23:
  ret void
}

define void @Ry(double %var_28, ptr %var_29) {
block_24:
  call void @__quantum__qis__ry__body(double %var_28, ptr %var_29)
  ret void
}

declare void @__quantum__qis__ry__body(double, ptr)

define void @Rz(double %var_31, ptr %var_32) {
block_25:
  call void @__quantum__qis__rz__body(double %var_31, ptr %var_32)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

define void @Rx(double %var_33, ptr %var_34) {
block_26:
  call void @__quantum__qis__rx__body(double %var_33, ptr %var_34)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

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
