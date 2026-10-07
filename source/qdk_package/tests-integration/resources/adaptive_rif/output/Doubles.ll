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
  %var_4 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_4, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_2
block_2:
  %var_152 = phi double [0.0, %block_0], [1.0, %block_1]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_6 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_6, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_9 = fadd double %var_152, 1.0
  %var_11 = fmul double %var_9, 1.0
  %var_13 = fsub double %var_11, 1.0
  %var_15 = fdiv double %var_13, 1.0
  %var_17 = fadd double %var_15, 1.0
  br label %block_4
block_4:
  %var_153 = phi double [%var_152, %block_2], [%var_17, %block_3]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_18 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_18, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_21 = fadd double %var_153, 1.0
  %var_23 = fmul double %var_21, 1.0
  %var_25 = fsub double %var_23, 1.0
  %var_27 = fdiv double %var_25, 1.0
  %var_29 = fadd double %var_27, 1.0
  br label %block_6
block_6:
  %var_154 = phi double [%var_153, %block_4], [%var_29, %block_5]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_30 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_30, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_33 = fadd double %var_154, 1.0
  %var_35 = fmul double %var_33, 1.0
  %var_37 = fsub double %var_35, 1.0
  %var_39 = fdiv double %var_37, 1.0
  %var_41 = fadd double %var_39, 1.0
  br label %block_8
block_8:
  %var_155 = phi double [%var_154, %block_6], [%var_41, %block_7]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  %var_42 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_42, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_45 = fadd double %var_155, 1.0
  %var_47 = fmul double %var_45, 1.0
  %var_49 = fsub double %var_47, 1.0
  %var_51 = fdiv double %var_49, 1.0
  %var_53 = fadd double %var_51, 1.0
  br label %block_10
block_10:
  %var_156 = phi double [%var_155, %block_8], [%var_53, %block_9]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  %var_54 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_54, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_57 = fadd double %var_156, 1.0
  %var_59 = fmul double %var_57, 1.0
  %var_61 = fsub double %var_59, 1.0
  %var_63 = fdiv double %var_61, 1.0
  %var_65 = fadd double %var_63, 1.0
  br label %block_12
block_12:
  %var_157 = phi double [%var_156, %block_10], [%var_65, %block_11]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  %var_66 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
  br i1 %var_66, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_69 = fadd double %var_157, 1.0
  %var_71 = fmul double %var_69, 1.0
  %var_73 = fsub double %var_71, 1.0
  %var_75 = fdiv double %var_73, 1.0
  %var_77 = fadd double %var_75, 1.0
  br label %block_14
block_14:
  %var_158 = phi double [%var_157, %block_12], [%var_77, %block_13]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_78 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_78, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_81 = fadd double %var_158, 1.0
  %var_83 = fmul double %var_81, 1.0
  %var_85 = fsub double %var_83, 1.0
  %var_87 = fdiv double %var_85, 1.0
  %var_89 = fadd double %var_87, 1.0
  br label %block_16
block_16:
  %var_159 = phi double [%var_158, %block_14], [%var_89, %block_15]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
  %var_90 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
  br i1 %var_90, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_93 = fadd double %var_159, 1.0
  %var_95 = fmul double %var_93, 1.0
  %var_97 = fsub double %var_95, 1.0
  %var_99 = fdiv double %var_97, 1.0
  %var_101 = fadd double %var_99, 1.0
  br label %block_18
block_18:
  %var_160 = phi double [%var_159, %block_16], [%var_101, %block_17]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
  %var_102 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_102, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_105 = fadd double %var_160, 1.0
  %var_107 = fmul double %var_105, 1.0
  %var_109 = fsub double %var_107, 1.0
  %var_111 = fdiv double %var_109, 1.0
  %var_113 = fadd double %var_111, 1.0
  br label %block_20
block_20:
  %var_161 = phi double [%var_160, %block_18], [%var_113, %block_19]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_114 = fptosi double %var_161 to i64
  %var_116 = sitofp i64 %var_114 to double
  %var_120 = fcmp ogt double %var_161, 5.0
  %var_123 = fcmp olt double %var_161, 5.0
  %var_126 = fcmp oge double %var_161, 10.0
  %var_129 = fcmp oeq double %var_161, 10.0
  %var_132 = fcmp one double %var_161, 10.0
  call void @__quantum__rt__tuple_record_output(i64 8, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__double_record_output(double %var_161, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_120, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_123, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_126, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_129, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_132, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @6, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_114, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @7, i64 0, i64 0))
  call void @__quantum__rt__double_record_output(double %var_116, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @8, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare zeroext i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__double_record_output(double, i8*)

declare void @__quantum__rt__bool_record_output(i1 zeroext, i8*)

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
