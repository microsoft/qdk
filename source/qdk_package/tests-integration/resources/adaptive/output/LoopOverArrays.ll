@0 = internal constant [4 x i8] c"0_r\00"
@array0 = internal constant [3 x double] [double 6.283185307179586, double 3.141592653589793, double 6.283185307179586]
@array1 = internal constant [3 x double] [double 3.141592653589793, double 3.141592653589793, double 3.141592653589793]
@array2 = internal constant [1 x double] [double 6.283185307179586]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i64
  %var_8 = alloca i64
  %var_13 = alloca i64
  %var_18 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_0
  br label %block_1
block_1:
  %var_24 = load i64, ptr %var_0
  %var_1 = icmp slt i64 %var_24, 3
  br i1 %var_1, label %block_2, label %block_3
block_2:
  %var_40 = load i64, ptr %var_0
  %var_2 = getelementptr double, ptr @array0, i64 %var_40
  %var_41 = load double, ptr %var_2
  call void @Rx(double %var_41, ptr inttoptr (i64 0 to ptr))
  %var_6 = add i64 %var_40, 1
  store i64 %var_6, ptr %var_0
  br label %block_1
block_3:
  store i64 0, ptr %var_8
  br label %block_4
block_4:
  %var_26 = load i64, ptr %var_8
  %var_9 = icmp slt i64 %var_26, 3
  br i1 %var_9, label %block_5, label %block_6
block_5:
  %var_37 = load i64, ptr %var_8
  %var_10 = getelementptr double, ptr @array1, i64 %var_37
  %var_38 = load double, ptr %var_10
  call void @Rx(double %var_38, ptr inttoptr (i64 0 to ptr))
  %var_12 = add i64 %var_37, 1
  store i64 %var_12, ptr %var_8
  br label %block_4
block_6:
  store i64 0, ptr %var_13
  br label %block_7
block_7:
  %var_28 = load i64, ptr %var_13
  %var_14 = icmp slt i64 %var_28, 3
  br i1 %var_14, label %block_8, label %block_9
block_8:
  %var_34 = load i64, ptr %var_13
  %var_15 = getelementptr double, ptr @array0, i64 %var_34
  %var_35 = load double, ptr %var_15
  call void @Rx(double %var_35, ptr inttoptr (i64 0 to ptr))
  %var_17 = add i64 %var_34, 1
  store i64 %var_17, ptr %var_13
  br label %block_7
block_9:
  store i64 0, ptr %var_18
  br label %block_10
block_10:
  %var_30 = load i64, ptr %var_18
  %var_19 = icmp slt i64 %var_30, 1
  br i1 %var_19, label %block_11, label %block_12
block_11:
  %var_31 = load i64, ptr %var_18
  %var_20 = getelementptr double, ptr @array2, i64 %var_31
  %var_32 = load double, ptr %var_20
  call void @Rx(double %var_32, ptr inttoptr (i64 0 to ptr))
  %var_22 = add i64 %var_31, 1
  store i64 %var_22, ptr %var_18
  br label %block_10
block_12:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define void @Rx(double %var_4, ptr %var_5) {
block_13:
  call void @__quantum__qis__rx__body(double %var_4, ptr %var_5)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
