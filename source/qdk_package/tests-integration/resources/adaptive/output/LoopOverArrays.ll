@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [3 x double] [double 6.283185307179586, double 3.141592653589793, double 6.283185307179586]
@array1 = internal constant [3 x double] [double 3.141592653589793, double 3.141592653589793, double 3.141592653589793]
@array2 = internal constant [1 x double] [double 6.283185307179586]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i64
  %var_1 = alloca i64
  %var_10 = alloca i64
  %var_13 = alloca i64
  %var_14 = alloca i64
  %var_19 = alloca i64
  %var_21 = alloca i64
  %var_22 = alloca i64
  %var_27 = alloca i64
  %var_29 = alloca i64
  %var_30 = alloca i64
  %var_35 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_0
  br label %block_1
block_1:
  %var_38 = load i64, ptr %var_0
  store i64 %var_38, ptr %var_1
  %var_40 = load i64, ptr %var_1
  %var_2 = icmp slt i64 %var_40, 3
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_68 = load i64, ptr %var_0
  %var_69_offset_chk = icmp slt i64 %var_68, 0
  %var_69_offset = select i1 %var_69_offset_chk, i64 1, i64 0
  %var_69 = getelementptr [3 x double], ptr @array0, i64 %var_69_offset, i64 %var_68
  %var_3 = load double, ptr %var_69
  call void @Rx(double %var_3, ptr inttoptr (i64 0 to ptr))
  store i64 %var_68, ptr %var_10
  %var_71 = load i64, ptr %var_10
  %var_11 = add i64 %var_71, 1
  store i64 %var_11, ptr %var_0
  br label %block_1
block_3:
  store i64 0, ptr %var_13
  br label %block_4
block_4:
  %var_42 = load i64, ptr %var_13
  store i64 %var_42, ptr %var_14
  %var_44 = load i64, ptr %var_14
  %var_15 = icmp slt i64 %var_44, 3
  br i1 %var_15, label %block_5, label %block_6
block_5:
  %var_63 = load i64, ptr %var_13
  %var_64_offset_chk = icmp slt i64 %var_63, 0
  %var_64_offset = select i1 %var_64_offset_chk, i64 1, i64 0
  %var_64 = getelementptr [3 x double], ptr @array1, i64 %var_64_offset, i64 %var_63
  %var_16 = load double, ptr %var_64
  call void @Rx(double %var_16, ptr inttoptr (i64 0 to ptr))
  store i64 %var_63, ptr %var_19
  %var_66 = load i64, ptr %var_19
  %var_20 = add i64 %var_66, 1
  store i64 %var_20, ptr %var_13
  br label %block_4
block_6:
  store i64 0, ptr %var_21
  br label %block_7
block_7:
  %var_46 = load i64, ptr %var_21
  store i64 %var_46, ptr %var_22
  %var_48 = load i64, ptr %var_22
  %var_23 = icmp slt i64 %var_48, 3
  br i1 %var_23, label %block_8, label %block_9
block_8:
  %var_58 = load i64, ptr %var_21
  %var_59_offset_chk = icmp slt i64 %var_58, 0
  %var_59_offset = select i1 %var_59_offset_chk, i64 1, i64 0
  %var_59 = getelementptr [3 x double], ptr @array0, i64 %var_59_offset, i64 %var_58
  %var_24 = load double, ptr %var_59
  call void @Rx(double %var_24, ptr inttoptr (i64 0 to ptr))
  store i64 %var_58, ptr %var_27
  %var_61 = load i64, ptr %var_27
  %var_28 = add i64 %var_61, 1
  store i64 %var_28, ptr %var_21
  br label %block_7
block_9:
  store i64 0, ptr %var_29
  br label %block_10
block_10:
  %var_50 = load i64, ptr %var_29
  store i64 %var_50, ptr %var_30
  %var_52 = load i64, ptr %var_30
  %var_31 = icmp slt i64 %var_52, 1
  br i1 %var_31, label %block_11, label %block_12
block_11:
  %var_53 = load i64, ptr %var_29
  %var_54_offset_chk = icmp slt i64 %var_53, 0
  %var_54_offset = select i1 %var_54_offset_chk, i64 1, i64 0
  %var_54 = getelementptr [1 x double], ptr @array2, i64 %var_54_offset, i64 %var_53
  %var_32 = load double, ptr %var_54
  call void @Rx(double %var_32, ptr inttoptr (i64 0 to ptr))
  store i64 %var_53, ptr %var_35
  %var_56 = load i64, ptr %var_35
  %var_36 = add i64 %var_56, 1
  store i64 %var_36, ptr %var_29
  br label %block_10
block_12:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @Rx(double %var_6, ptr %var_7) {
block_13:
  call void @__quantum__qis__rx__body(double %var_6, ptr %var_7)
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
