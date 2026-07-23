@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_3 = alloca i64
  %var_9 = alloca i64
  %var_11 = alloca i64
  %var_20 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_3
  br label %block_1
block_1:
  %var_34 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_34, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_54 = load i64, ptr %var_3
  %var_5 = getelementptr ptr, ptr @array0, i64 %var_54
  %var_55 = load ptr, ptr %var_5
  call void @X(ptr %var_55)
  %var_8 = add i64 %var_54, 1
  store i64 %var_8, ptr %var_3
  br label %block_1
block_3:
  store i64 0, ptr %var_9
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_11
  br label %block_4
block_4:
  %var_37 = load i64, ptr %var_11
  %var_12 = icmp slt i64 %var_37, 2
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_46 = load i64, ptr %var_11
  %var_13 = getelementptr ptr, ptr @array1, i64 %var_46
  %var_47 = load ptr, ptr %var_13
  %var_48 = load i64, ptr %var_9
  %var_15 = shl i64 %var_48, 1
  store i64 %var_15, ptr %var_9
  %var_16 = call i1 @__quantum__rt__read_result(ptr %var_47)
  br i1 %var_16, label %block_7, label %block_9
block_6:
  store i64 0, ptr %var_20
  br label %block_8
block_7:
  %var_52 = load i64, ptr %var_9
  %var_18 = add i64 %var_52, 1
  store i64 %var_18, ptr %var_9
  br label %block_9
block_8:
  %var_39 = load i64, ptr %var_20
  %var_21 = icmp slt i64 %var_39, 2
  br i1 %var_21, label %block_10, label %block_11
block_9:
  %var_50 = load i64, ptr %var_11
  %var_19 = add i64 %var_50, 1
  store i64 %var_19, ptr %var_11
  br label %block_4
block_10:
  %var_43 = load i64, ptr %var_20
  %var_22 = getelementptr ptr, ptr @array0, i64 %var_43
  %var_44 = load ptr, ptr %var_22
  call void @Reset(ptr %var_44)
  %var_25 = add i64 %var_43, 1
  store i64 %var_25, ptr %var_20
  br label %block_8
block_11:
  %var_40 = load i64, ptr %var_9
  %var_26 = icmp eq i64 %var_40, 0
  br i1 %var_26, label %block_12, label %block_13
block_12:
  call void @ApplyGlobalPhase(double -1.5707963267948966)
  br label %block_14
block_13:
  %var_41 = load i64, ptr %var_9
  %var_29 = icmp eq i64 %var_41, 1
  br i1 %var_29, label %block_15, label %block_16
block_14:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
  ret i64 0
block_15:
  call void @Ry(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_17
block_16:
  %var_42 = load i64, ptr %var_9
  %var_32 = icmp eq i64 %var_42, 2
  br i1 %var_32, label %block_18, label %block_19
block_17:
  br label %block_14
block_18:
  call void @Rz(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_20
block_19:
  call void @Rx(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_20
block_20:
  br label %block_17
}

declare void @__quantum__rt__initialize(ptr)

define void @X(ptr %var_7) {
block_21:
  call void @__quantum__qis__x__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

define void @Reset(ptr %var_24) {
block_22:
  call void @__quantum__qis__reset__body(ptr %var_24)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

define void @ApplyGlobalPhase(double %var_27) {
block_23:
  call void @ControllableGlobalPhase(double %var_27)
  ret void
}

define void @ControllableGlobalPhase(double %var_28) {
block_24:
  ret void
}

define void @Ry(double %var_30, ptr %var_31) {
block_25:
  call void @__quantum__qis__ry__body(double %var_30, ptr %var_31)
  ret void
}

declare void @__quantum__qis__ry__body(double, ptr)

define void @Rz(double %var_33, ptr %var_34) {
block_26:
  call void @__quantum__qis__rz__body(double %var_33, ptr %var_34)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

define void @Rx(double %var_35, ptr %var_36) {
block_27:
  call void @__quantum__qis__rx__body(double %var_35, ptr %var_36)
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
