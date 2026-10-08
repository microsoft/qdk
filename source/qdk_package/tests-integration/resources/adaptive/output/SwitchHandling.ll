@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_3 = alloca i64
  %var_4 = alloca i64
  %var_9 = alloca i64
  %var_11 = alloca i64
  %var_13 = alloca i64
  %var_14 = alloca i64
  %var_18 = alloca i64
  %var_23 = alloca i64
  %var_25 = alloca i64
  %var_27 = alloca i64
  %var_28 = alloca i64
  %var_33 = alloca i64
  %var_35 = alloca i64
  %var_39 = alloca i64
  %var_45 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_3
  br label %block_1
block_1:
  %var_48 = load i64, ptr %var_3
  store i64 %var_48, ptr %var_4
  %var_50 = load i64, ptr %var_4
  %var_5 = icmp slt i64 %var_50, 2
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_88 = load i64, ptr %var_3
  %var_89_offset_chk = icmp slt i64 %var_88, 0
  %var_89_offset = select i1 %var_89_offset_chk, i64 1, i64 0
  %var_89 = getelementptr [2 x ptr], ptr @array0, i64 %var_89_offset, i64 %var_88
  %var_6 = load ptr, ptr %var_89
  call void @X(ptr %var_6)
  store i64 %var_88, ptr %var_9
  %var_91 = load i64, ptr %var_9
  %var_10 = add i64 %var_91, 1
  store i64 %var_10, ptr %var_3
  br label %block_1
block_3:
  store i64 0, ptr %var_11
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_13
  br label %block_4
block_4:
  %var_53 = load i64, ptr %var_13
  store i64 %var_53, ptr %var_14
  %var_55 = load i64, ptr %var_14
  %var_15 = icmp slt i64 %var_55, 2
  br i1 %var_15, label %block_5, label %block_6
block_5:
  %var_74 = load i64, ptr %var_13
  %var_75_offset_chk = icmp slt i64 %var_74, 0
  %var_75_offset = select i1 %var_75_offset_chk, i64 1, i64 0
  %var_75 = getelementptr [2 x ptr], ptr @array1, i64 %var_75_offset, i64 %var_74
  %var_16 = load ptr, ptr %var_75
  %var_76 = load i64, ptr %var_11
  store i64 %var_76, ptr %var_18
  %var_78 = load i64, ptr %var_18
  %var_19 = shl i64 %var_78, 1
  store i64 %var_19, ptr %var_11
  %var_21 = call zeroext i1 @__quantum__rt__read_result(ptr %var_16)
  br i1 %var_21, label %block_7, label %block_9
block_6:
  store i64 0, ptr %var_27
  br label %block_8
block_7:
  %var_84 = load i64, ptr %var_11
  store i64 %var_84, ptr %var_23
  %var_86 = load i64, ptr %var_23
  %var_24 = add i64 %var_86, 1
  store i64 %var_24, ptr %var_11
  br label %block_9
block_8:
  %var_57 = load i64, ptr %var_27
  store i64 %var_57, ptr %var_28
  %var_59 = load i64, ptr %var_28
  %var_29 = icmp slt i64 %var_59, 2
  br i1 %var_29, label %block_10, label %block_11
block_9:
  %var_80 = load i64, ptr %var_13
  store i64 %var_80, ptr %var_25
  %var_82 = load i64, ptr %var_25
  %var_26 = add i64 %var_82, 1
  store i64 %var_26, ptr %var_13
  br label %block_4
block_10:
  %var_69 = load i64, ptr %var_27
  %var_70_offset_chk = icmp slt i64 %var_69, 0
  %var_70_offset = select i1 %var_70_offset_chk, i64 1, i64 0
  %var_70 = getelementptr [2 x ptr], ptr @array0, i64 %var_70_offset, i64 %var_69
  %var_30 = load ptr, ptr %var_70
  call void @Reset(ptr %var_30)
  store i64 %var_69, ptr %var_33
  %var_72 = load i64, ptr %var_33
  %var_34 = add i64 %var_72, 1
  store i64 %var_34, ptr %var_27
  br label %block_8
block_11:
  %var_60 = load i64, ptr %var_11
  store i64 %var_60, ptr %var_35
  %var_62 = load i64, ptr %var_35
  %var_36 = icmp eq i64 %var_62, 0
  br i1 %var_36, label %block_12, label %block_13
block_12:
  call void @ApplyGlobalPhase(double -1.5707963267948966)
  br label %block_14
block_13:
  %var_63 = load i64, ptr %var_11
  store i64 %var_63, ptr %var_39
  %var_65 = load i64, ptr %var_39
  %var_40 = icmp eq i64 %var_65, 1
  br i1 %var_40, label %block_15, label %block_16
block_14:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
  ret i64 0
block_15:
  call void @Ry(double 3.141592653589793, ptr inttoptr (i64 2 to ptr))
  br label %block_17
block_16:
  %var_66 = load i64, ptr %var_11
  store i64 %var_66, ptr %var_45
  %var_68 = load i64, ptr %var_45
  %var_46 = icmp eq i64 %var_68, 2
  br i1 %var_46, label %block_18, label %block_19
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

define internal void @X(ptr %var_8) {
block_21:
  call void @__quantum__qis__x__body(ptr %var_8)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @Reset(ptr %var_32) {
block_22:
  call void @__quantum__qis__reset__body(ptr %var_32)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

define internal void @ApplyGlobalPhase(double %var_37) {
block_23:
  call void @ControllableGlobalPhase(double %var_37)
  ret void
}

define internal void @ControllableGlobalPhase(double %var_38) {
block_24:
  ret void
}

define internal void @Ry(double %var_41, ptr %var_42) {
block_25:
  call void @__quantum__qis__ry__body(double %var_41, ptr %var_42)
  ret void
}

declare void @__quantum__qis__ry__body(double, ptr)

define internal void @Rz(double %var_47, ptr %var_48) {
block_26:
  call void @__quantum__qis__rz__body(double %var_47, ptr %var_48)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

define internal void @Rx(double %var_51, ptr %var_52) {
block_27:
  call void @__quantum__qis__rx__body(double %var_51, ptr %var_52)
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
