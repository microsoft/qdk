%Result = type opaque
%Qubit = type opaque

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
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %var_3 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_3, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_2
block_2:
  %var_77 = phi double [0.0, %block_0], [1.0, %block_1]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_5 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_5, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_7 = fadd double %var_77, 1.0
  %var_8 = fmul double %var_7, 1.0
  %var_9 = fsub double %var_8, 1.0
  %var_10 = fdiv double %var_9, 1.0
  %var_11 = fadd double %var_10, 1.0
  br label %block_4
block_4:
  %var_78 = phi double [%var_77, %block_2], [%var_11, %block_3]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_12 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_12, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_14 = fadd double %var_78, 1.0
  %var_15 = fmul double %var_14, 1.0
  %var_16 = fsub double %var_15, 1.0
  %var_17 = fdiv double %var_16, 1.0
  %var_18 = fadd double %var_17, 1.0
  br label %block_6
block_6:
  %var_79 = phi double [%var_78, %block_4], [%var_18, %block_5]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_19 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_19, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_21 = fadd double %var_79, 1.0
  %var_22 = fmul double %var_21, 1.0
  %var_23 = fsub double %var_22, 1.0
  %var_24 = fdiv double %var_23, 1.0
  %var_25 = fadd double %var_24, 1.0
  br label %block_8
block_8:
  %var_80 = phi double [%var_79, %block_6], [%var_25, %block_7]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  %var_26 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_26, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_28 = fadd double %var_80, 1.0
  %var_29 = fmul double %var_28, 1.0
  %var_30 = fsub double %var_29, 1.0
  %var_31 = fdiv double %var_30, 1.0
  %var_32 = fadd double %var_31, 1.0
  br label %block_10
block_10:
  %var_81 = phi double [%var_80, %block_8], [%var_32, %block_9]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  %var_33 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_33, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_35 = fadd double %var_81, 1.0
  %var_36 = fmul double %var_35, 1.0
  %var_37 = fsub double %var_36, 1.0
  %var_38 = fdiv double %var_37, 1.0
  %var_39 = fadd double %var_38, 1.0
  br label %block_12
block_12:
  %var_82 = phi double [%var_81, %block_10], [%var_39, %block_11]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  %var_40 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
  br i1 %var_40, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_42 = fadd double %var_82, 1.0
  %var_43 = fmul double %var_42, 1.0
  %var_44 = fsub double %var_43, 1.0
  %var_45 = fdiv double %var_44, 1.0
  %var_46 = fadd double %var_45, 1.0
  br label %block_14
block_14:
  %var_83 = phi double [%var_82, %block_12], [%var_46, %block_13]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_47 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_47, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_49 = fadd double %var_83, 1.0
  %var_50 = fmul double %var_49, 1.0
  %var_51 = fsub double %var_50, 1.0
  %var_52 = fdiv double %var_51, 1.0
  %var_53 = fadd double %var_52, 1.0
  br label %block_16
block_16:
  %var_84 = phi double [%var_83, %block_14], [%var_53, %block_15]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
  %var_54 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
  br i1 %var_54, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_56 = fadd double %var_84, 1.0
  %var_57 = fmul double %var_56, 1.0
  %var_58 = fsub double %var_57, 1.0
  %var_59 = fdiv double %var_58, 1.0
  %var_60 = fadd double %var_59, 1.0
  br label %block_18
block_18:
  %var_85 = phi double [%var_84, %block_16], [%var_60, %block_17]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
  %var_61 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_61, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_63 = fadd double %var_85, 1.0
  %var_64 = fmul double %var_63, 1.0
  %var_65 = fsub double %var_64, 1.0
  %var_66 = fdiv double %var_65, 1.0
  %var_67 = fadd double %var_66, 1.0
  br label %block_20
block_20:
  %var_86 = phi double [%var_85, %block_18], [%var_67, %block_19]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_68 = fptosi double %var_86 to i64
  %var_70 = sitofp i64 %var_68 to double
  %var_72 = fcmp ogt double %var_86, 5.0
  %var_73 = fcmp olt double %var_86, 5.0
  %var_74 = fcmp oge double %var_86, 10.0
  %var_75 = fcmp oeq double %var_86, 10.0
  %var_76 = fcmp one double %var_86, 10.0
  call void @__quantum__rt__tuple_record_output(i64 8, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__double_record_output(double %var_86, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_72, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_73, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_74, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_75, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_76, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @6, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_68, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @7, i64 0, i64 0))
  call void @__quantum__rt__double_record_output(double %var_70, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @8, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__double_record_output(double, i8*)

declare void @__quantum__rt__bool_record_output(i1, i8*)

declare void @__quantum__rt__int_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="10" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
