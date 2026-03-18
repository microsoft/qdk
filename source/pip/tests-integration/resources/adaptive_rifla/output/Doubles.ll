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
  call void @__quantum__rt__initialize(ptr null)
  store double 0.0, ptr %var_0
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_2, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  store double 1.0, ptr %var_0
  store double 1.0, ptr %var_0
  store double 0.0, ptr %var_0
  store double 0.0, ptr %var_0
  store double 1.0, ptr %var_0
  br label %block_2
block_2:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_4, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_126 = load double, ptr %var_0
  %var_6 = fadd double %var_126, 1.0
  store double %var_6, ptr %var_0
  %var_7 = fmul double %var_126, 1.0
  store double %var_7, ptr %var_0
  %var_8 = fsub double %var_126, 1.0
  store double %var_8, ptr %var_0
  %var_9 = fdiv double %var_126, 1.0
  store double %var_9, ptr %var_0
  %var_10 = fadd double %var_126, 1.0
  store double %var_10, ptr %var_0
  br label %block_4
block_4:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_11 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  br i1 %var_11, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_120 = load double, ptr %var_0
  %var_13 = fadd double %var_120, 1.0
  store double %var_13, ptr %var_0
  %var_14 = fmul double %var_120, 1.0
  store double %var_14, ptr %var_0
  %var_15 = fsub double %var_120, 1.0
  store double %var_15, ptr %var_0
  %var_16 = fdiv double %var_120, 1.0
  store double %var_16, ptr %var_0
  %var_17 = fadd double %var_120, 1.0
  store double %var_17, ptr %var_0
  br label %block_6
block_6:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  %var_18 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_18, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_114 = load double, ptr %var_0
  %var_20 = fadd double %var_114, 1.0
  store double %var_20, ptr %var_0
  %var_21 = fmul double %var_114, 1.0
  store double %var_21, ptr %var_0
  %var_22 = fsub double %var_114, 1.0
  store double %var_22, ptr %var_0
  %var_23 = fdiv double %var_114, 1.0
  store double %var_23, ptr %var_0
  %var_24 = fadd double %var_114, 1.0
  store double %var_24, ptr %var_0
  br label %block_8
block_8:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  %var_25 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  br i1 %var_25, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_108 = load double, ptr %var_0
  %var_27 = fadd double %var_108, 1.0
  store double %var_27, ptr %var_0
  %var_28 = fmul double %var_108, 1.0
  store double %var_28, ptr %var_0
  %var_29 = fsub double %var_108, 1.0
  store double %var_29, ptr %var_0
  %var_30 = fdiv double %var_108, 1.0
  store double %var_30, ptr %var_0
  %var_31 = fadd double %var_108, 1.0
  store double %var_31, ptr %var_0
  br label %block_10
block_10:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 5 to ptr))
  %var_32 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  br i1 %var_32, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_102 = load double, ptr %var_0
  %var_34 = fadd double %var_102, 1.0
  store double %var_34, ptr %var_0
  %var_35 = fmul double %var_102, 1.0
  store double %var_35, ptr %var_0
  %var_36 = fsub double %var_102, 1.0
  store double %var_36, ptr %var_0
  %var_37 = fdiv double %var_102, 1.0
  store double %var_37, ptr %var_0
  %var_38 = fadd double %var_102, 1.0
  store double %var_38, ptr %var_0
  br label %block_12
block_12:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 6 to ptr))
  %var_39 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 6 to ptr))
  br i1 %var_39, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_96 = load double, ptr %var_0
  %var_41 = fadd double %var_96, 1.0
  store double %var_41, ptr %var_0
  %var_42 = fmul double %var_96, 1.0
  store double %var_42, ptr %var_0
  %var_43 = fsub double %var_96, 1.0
  store double %var_43, ptr %var_0
  %var_44 = fdiv double %var_96, 1.0
  store double %var_44, ptr %var_0
  %var_45 = fadd double %var_96, 1.0
  store double %var_45, ptr %var_0
  br label %block_14
block_14:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 7 to ptr))
  %var_46 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  br i1 %var_46, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_90 = load double, ptr %var_0
  %var_48 = fadd double %var_90, 1.0
  store double %var_48, ptr %var_0
  %var_49 = fmul double %var_90, 1.0
  store double %var_49, ptr %var_0
  %var_50 = fsub double %var_90, 1.0
  store double %var_50, ptr %var_0
  %var_51 = fdiv double %var_90, 1.0
  store double %var_51, ptr %var_0
  %var_52 = fadd double %var_90, 1.0
  store double %var_52, ptr %var_0
  br label %block_16
block_16:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 8 to ptr))
  %var_53 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 8 to ptr))
  br i1 %var_53, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_84 = load double, ptr %var_0
  %var_55 = fadd double %var_84, 1.0
  store double %var_55, ptr %var_0
  %var_56 = fmul double %var_84, 1.0
  store double %var_56, ptr %var_0
  %var_57 = fsub double %var_84, 1.0
  store double %var_57, ptr %var_0
  %var_58 = fdiv double %var_84, 1.0
  store double %var_58, ptr %var_0
  %var_59 = fadd double %var_84, 1.0
  store double %var_59, ptr %var_0
  br label %block_18
block_18:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 9 to ptr))
  %var_60 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 9 to ptr))
  br i1 %var_60, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_78 = load double, ptr %var_0
  %var_62 = fadd double %var_78, 1.0
  store double %var_62, ptr %var_0
  %var_63 = fmul double %var_78, 1.0
  store double %var_63, ptr %var_0
  %var_64 = fsub double %var_78, 1.0
  store double %var_64, ptr %var_0
  %var_65 = fdiv double %var_78, 1.0
  store double %var_65, ptr %var_0
  %var_66 = fadd double %var_78, 1.0
  store double %var_66, ptr %var_0
  br label %block_20
block_20:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  %var_77 = load double, ptr %var_0
  %var_67 = fptosi double %var_77 to i64
  %var_69 = sitofp i64 %var_67 to double
  %var_71 = fcmp ogt double %var_77, 5.0
  %var_72 = fcmp olt double %var_77, 5.0
  %var_73 = fcmp oge double %var_77, 10.0
  %var_74 = fcmp oeq double %var_77, 10.0
  %var_75 = fcmp one double %var_77, 10.0
  call void @__quantum__rt__tuple_record_output(i64 8, ptr @0)
  call void @__quantum__rt__double_record_output(double %var_77, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 %var_71, ptr @2)
  call void @__quantum__rt__bool_record_output(i1 %var_72, ptr @3)
  call void @__quantum__rt__bool_record_output(i1 %var_73, ptr @4)
  call void @__quantum__rt__bool_record_output(i1 %var_74, ptr @5)
  call void @__quantum__rt__bool_record_output(i1 %var_75, ptr @6)
  call void @__quantum__rt__int_record_output(i64 %var_67, ptr @7)
  call void @__quantum__rt__double_record_output(double %var_69, ptr @8)
  ret i64 0
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

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="10" }
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
