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
  %var_0 = alloca i1
  %var_2 = alloca double
  %var_3 = alloca i64
  %var_7 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  store i1 false, ptr %var_0
  store double 0.0, ptr %var_2
  store i64 1, ptr %var_3
  br label %block_1
block_1:
  %var_4 = load i64, ptr %var_3
  %var_5 = icmp sle i64 %var_4, 10
  store i1 true, ptr %var_7
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_48 = load i1, ptr %var_7
  br i1 %var_48, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_7
  br label %block_2
block_4:
  call void @X(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_9 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_9, ptr %var_0
  %var_51 = load i1, ptr %var_0
  br i1 %var_51, label %block_6, label %block_7
block_5:
  call void @Reset(ptr inttoptr (i64 0 to ptr))
  %var_49 = load double, ptr %var_2
  %var_24 = fptosi double %var_49 to i64
  %var_26 = sitofp i64 %var_24 to double
  %var_28 = load double, ptr %var_2
  %var_29 = load double, ptr %var_2
  %var_30 = fcmp ogt double %var_29, 5.0
  %var_32 = load double, ptr %var_2
  %var_33 = fcmp olt double %var_32, 5.0
  %var_35 = load double, ptr %var_2
  %var_36 = fcmp oge double %var_35, 10.0
  %var_38 = load double, ptr %var_2
  %var_39 = fcmp oeq double %var_38, 10.0
  %var_41 = load double, ptr %var_2
  %var_42 = fcmp one double %var_41, 10.0
  call void @__quantum__rt__tuple_record_output(i64 8, ptr @0)
  call void @__quantum__rt__double_record_output(double %var_28, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_30, ptr @2)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_33, ptr @3)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_36, ptr @4)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_39, ptr @5)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_42, ptr @6)
  call void @__quantum__rt__int_record_output(i64 %var_24, ptr @7)
  call void @__quantum__rt__double_record_output(double %var_26, ptr @8)
  ret i64 0
block_6:
  call void @X(ptr inttoptr (i64 0 to ptr))
  %var_11 = load double, ptr %var_2
  %var_12 = fadd double %var_11, 1.0
  store double %var_12, ptr %var_2
  %var_13 = load double, ptr %var_2
  %var_14 = fmul double %var_13, 1.0
  store double %var_14, ptr %var_2
  %var_15 = load double, ptr %var_2
  %var_16 = fsub double %var_15, 1.0
  store double %var_16, ptr %var_2
  %var_17 = load double, ptr %var_2
  %var_18 = fdiv double %var_17, 1.0
  store double %var_18, ptr %var_2
  %var_19 = load double, ptr %var_2
  %var_20 = fadd double %var_19, 1.0
  store double %var_20, ptr %var_2
  br label %block_7
block_7:
  %var_21 = load i64, ptr %var_3
  %var_22 = add i64 %var_21, 1
  store i64 %var_22, ptr %var_3
  br label %block_1
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_8) {
block_8:
  call void @__quantum__qis__x__body(ptr %var_8)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @Reset(ptr %var_23) {
block_9:
  call void @__quantum__qis__reset__body(ptr %var_23)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__double_record_output(double, ptr)

declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

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
