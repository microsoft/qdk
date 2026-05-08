@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0d\00"
@2 = internal constant [6 x i8] c"2_t1b\00"
@3 = internal constant [6 x i8] c"3_t2b\00"
@4 = internal constant [6 x i8] c"4_t3b\00"
@5 = internal constant [6 x i8] c"5_t4b\00"
@6 = internal constant [6 x i8] c"6_t5b\00"
@7 = internal constant [6 x i8] c"7_t6i\00"
@8 = internal constant [6 x i8] c"8_t7d\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca double
  %var_1 = alloca i64
  %var_3 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  store double 0.0, ptr %var_0
  store i64 1, ptr %var_1
  br label %block_1
block_1:
  %var_23 = load i64, ptr %var_1
  %var_2 = icmp sle i64 %var_23, 10
  store i1 true, ptr %var_3
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_26 = load i1, ptr %var_3
  br i1 %var_26, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_3
  br label %block_2
block_4:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_4, label %block_6, label %block_7
block_5:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  %var_27 = load double, ptr %var_0
  %var_12 = fptosi double %var_27 to i64
  %var_14 = sitofp i64 %var_12 to double
  %var_16 = fcmp ogt double %var_27, 5.0
  %var_17 = fcmp olt double %var_27, 5.0
  %var_18 = fcmp oge double %var_27, 10.0
  %var_19 = fcmp oeq double %var_27, 10.0
  %var_20 = fcmp one double %var_27, 10.0
  call void @__quantum__rt__tuple_record_output(i64 8, ptr @0)
  call void @__quantum__rt__double_record_output(double %var_27, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 %var_16, ptr @2)
  call void @__quantum__rt__bool_record_output(i1 %var_17, ptr @3)
  call void @__quantum__rt__bool_record_output(i1 %var_18, ptr @4)
  call void @__quantum__rt__bool_record_output(i1 %var_19, ptr @5)
  call void @__quantum__rt__bool_record_output(i1 %var_20, ptr @6)
  call void @__quantum__rt__int_record_output(i64 %var_12, ptr @7)
  call void @__quantum__rt__double_record_output(double %var_14, ptr @8)
  ret i64 0
block_6:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_30 = load double, ptr %var_0
  %var_6 = fadd double %var_30, 1.0
  store double %var_6, ptr %var_0
  %var_7 = fmul double %var_30, 1.0
  store double %var_7, ptr %var_0
  %var_8 = fsub double %var_30, 1.0
  store double %var_8, ptr %var_0
  %var_9 = fdiv double %var_30, 1.0
  store double %var_9, ptr %var_0
  %var_10 = fadd double %var_30, 1.0
  store double %var_10, ptr %var_0
  br label %block_7
block_7:
  %var_28 = load i64, ptr %var_1
  %var_11 = add i64 %var_28, 1
  store i64 %var_11, ptr %var_1
  br label %block_1
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__double_record_output(double, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
