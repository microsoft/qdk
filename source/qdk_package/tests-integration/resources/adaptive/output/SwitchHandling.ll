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
  %var_37 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_37, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_57 = load i64, ptr %var_3
  %var_58_offset_chk = icmp slt i64 %var_57, 0
  %var_58_offset = select i1 %var_58_offset_chk, i64 1, i64 0
  %var_58 = getelementptr [2 x ptr], ptr @array0, i64 %var_58_offset, i64 %var_57
  %var_5 = load ptr, ptr %var_58
  call void @X(ptr %var_5)
  %var_8 = add i64 %var_57, 1
  store i64 %var_8, ptr %var_3
  br label %block_1
block_3:
  store i64 0, ptr %var_9
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_11
  br label %block_4
block_4:
  %var_40 = load i64, ptr %var_11
  %var_12 = icmp slt i64 %var_40, 2
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_49 = load i64, ptr %var_11
  %var_50_offset_chk = icmp slt i64 %var_49, 0
  %var_50_offset = select i1 %var_50_offset_chk, i64 1, i64 0
  %var_50 = getelementptr [2 x ptr], ptr @array1, i64 %var_50_offset, i64 %var_49
  %var_13 = load ptr, ptr %var_50
  %var_51 = load i64, ptr %var_9
  %var_15 = shl i64 %var_51, 1
  store i64 %var_15, ptr %var_9
  %var_16 = call zeroext i1 @__quantum__rt__read_result(ptr %var_13)
  br i1 %var_16, label %block_7, label %block_9
block_6:
  store i64 0, ptr %var_20
  br label %block_8
block_7:
  %var_55 = load i64, ptr %var_9
  %var_18 = add i64 %var_55, 1
  store i64 %var_18, ptr %var_9
  br label %block_9
block_8:
  %var_42 = load i64, ptr %var_20
  %var_21 = icmp slt i64 %var_42, 2
  br i1 %var_21, label %block_10, label %block_11
block_9:
  %var_53 = load i64, ptr %var_11
  %var_19 = add i64 %var_53, 1
  store i64 %var_19, ptr %var_11
  br label %block_4
block_10:
  %var_46 = load i64, ptr %var_20
  %var_47_offset_chk = icmp slt i64 %var_46, 0
  %var_47_offset = select i1 %var_47_offset_chk, i64 1, i64 0
  %var_47 = getelementptr [2 x ptr], ptr @array0, i64 %var_47_offset, i64 %var_46
  %var_22 = load ptr, ptr %var_47
  call void @Reset(ptr %var_22)
  %var_25 = add i64 %var_46, 1
  store i64 %var_25, ptr %var_20
  br label %block_8
block_11:
  %var_43 = load i64, ptr %var_9
  %var_26 = icmp eq i64 %var_43, 0
  br i1 %var_26, label %block_12, label %block_13
block_12:
  call void @ApplyGlobalPhase(double -1.5707963267948966)
  br label %block_14
block_13:
  %var_44 = load i64, ptr %var_9
  %var_30 = icmp eq i64 %var_44, 1
  br i1 %var_30, label %block_15, label %block_16
block_14:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
  ret i64 0
block_15:
  call void @Ry(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_17
block_16:
  %var_45 = load i64, ptr %var_9
  %var_35 = icmp eq i64 %var_45, 2
  br i1 %var_35, label %block_18, label %block_19
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

define internal void @X(ptr %var_7) {
block_21:
  call void @__quantum__qis__x__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @Reset(ptr %var_24) {
block_22:
  call void @__quantum__qis__reset__body(ptr %var_24)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

define internal void @ApplyGlobalPhase(double %var_27) {
block_23:
  call void @ControllableGlobalPhase(double %var_27)
  ret void
}

define internal void @ControllableGlobalPhase(double %var_28) {
block_24:
  ret void
}

define internal void @Ry(double %var_31, ptr %var_32) {
block_25:
  call void @__quantum__qis__ry__body(double %var_31, ptr %var_32)
  ret void
}

declare void @__quantum__qis__ry__body(double, ptr)

define internal void @Rz(double %var_36, ptr %var_37) {
block_26:
  call void @__quantum__qis__rz__body(double %var_36, ptr %var_37)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

define internal void @Rx(double %var_40, ptr %var_41) {
block_27:
  call void @__quantum__qis__rx__body(double %var_40, ptr %var_41)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
