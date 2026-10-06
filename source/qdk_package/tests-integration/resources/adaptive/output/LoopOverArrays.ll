@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [3 x double] [double 6.283185307179586, double 3.141592653589793, double 6.283185307179586]
@array1 = internal constant [3 x double] [double 3.141592653589793, double 3.141592653589793, double 3.141592653589793]
@array2 = internal constant [1 x double] [double 6.283185307179586]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i64
  %var_11 = alloca i64
  %var_17 = alloca i64
  %var_23 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_0
  br label %block_1
block_1:
  %var_30 = load i64, ptr %var_0
  %var_1 = icmp slt i64 %var_30, 3
  br i1 %var_1, label %block_2, label %block_3
block_2:
  %var_46 = load i64, ptr %var_0
  %var_47_offset_chk = icmp slt i64 %var_46, 0
  %var_47_offset = select i1 %var_47_offset_chk, i64 1, i64 0
  %var_47 = getelementptr [3 x double], ptr @array0, i64 %var_47_offset, i64 %var_46
  %var_2 = load double, ptr %var_47
  call void @Rx(double %var_2, ptr inttoptr (i64 0 to ptr))
  %var_9 = add i64 %var_46, 1
  store i64 %var_9, ptr %var_0
  br label %block_1
block_3:
  store i64 0, ptr %var_11
  br label %block_4
block_4:
  %var_32 = load i64, ptr %var_11
  %var_12 = icmp slt i64 %var_32, 3
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_43 = load i64, ptr %var_11
  %var_44_offset_chk = icmp slt i64 %var_43, 0
  %var_44_offset = select i1 %var_44_offset_chk, i64 1, i64 0
  %var_44 = getelementptr [3 x double], ptr @array1, i64 %var_44_offset, i64 %var_43
  %var_13 = load double, ptr %var_44
  call void @Rx(double %var_13, ptr inttoptr (i64 0 to ptr))
  %var_16 = add i64 %var_43, 1
  store i64 %var_16, ptr %var_11
  br label %block_4
block_6:
  store i64 0, ptr %var_17
  br label %block_7
block_7:
  %var_34 = load i64, ptr %var_17
  %var_18 = icmp slt i64 %var_34, 3
  br i1 %var_18, label %block_8, label %block_9
block_8:
  %var_40 = load i64, ptr %var_17
  %var_41_offset_chk = icmp slt i64 %var_40, 0
  %var_41_offset = select i1 %var_41_offset_chk, i64 1, i64 0
  %var_41 = getelementptr [3 x double], ptr @array0, i64 %var_41_offset, i64 %var_40
  %var_19 = load double, ptr %var_41
  call void @Rx(double %var_19, ptr inttoptr (i64 0 to ptr))
  %var_22 = add i64 %var_40, 1
  store i64 %var_22, ptr %var_17
  br label %block_7
block_9:
  store i64 0, ptr %var_23
  br label %block_10
block_10:
  %var_36 = load i64, ptr %var_23
  %var_24 = icmp slt i64 %var_36, 1
  br i1 %var_24, label %block_11, label %block_12
block_11:
  %var_37 = load i64, ptr %var_23
  %var_38_offset_chk = icmp slt i64 %var_37, 0
  %var_38_offset = select i1 %var_38_offset_chk, i64 1, i64 0
  %var_38 = getelementptr [1 x double], ptr @array2, i64 %var_38_offset, i64 %var_37
  %var_25 = load double, ptr %var_38
  call void @Rx(double %var_25, ptr inttoptr (i64 0 to ptr))
  %var_28 = add i64 %var_37, 1
  store i64 %var_28, ptr %var_23
  br label %block_10
block_12:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @Rx(double %var_5, ptr %var_6) {
block_13:
  call void @__quantum__qis__rx__body(double %var_5, ptr %var_6)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__result_record_output(ptr, ptr)

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
