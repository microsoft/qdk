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
  %var_4 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_4, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_2
block_2:
  %var_86 = phi double [0.0, %block_0], [1.0, %block_1]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_6 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_6, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_8 = fadd double %var_86, 1.0
  %var_9 = fmul double %var_8, 1.0
  %var_10 = fsub double %var_9, 1.0
  %var_11 = fdiv double %var_10, 1.0
  %var_12 = fadd double %var_11, 1.0
  br label %block_4
block_4:
  %var_87 = phi double [%var_86, %block_2], [%var_12, %block_3]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_13 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_13, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_15 = fadd double %var_87, 1.0
  %var_16 = fmul double %var_15, 1.0
  %var_17 = fsub double %var_16, 1.0
  %var_18 = fdiv double %var_17, 1.0
  %var_19 = fadd double %var_18, 1.0
  br label %block_6
block_6:
  %var_88 = phi double [%var_87, %block_4], [%var_19, %block_5]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_20 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_20, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_22 = fadd double %var_88, 1.0
  %var_23 = fmul double %var_22, 1.0
  %var_24 = fsub double %var_23, 1.0
  %var_25 = fdiv double %var_24, 1.0
  %var_26 = fadd double %var_25, 1.0
  br label %block_8
block_8:
  %var_89 = phi double [%var_88, %block_6], [%var_26, %block_7]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  %var_27 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_27, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_29 = fadd double %var_89, 1.0
  %var_30 = fmul double %var_29, 1.0
  %var_31 = fsub double %var_30, 1.0
  %var_32 = fdiv double %var_31, 1.0
  %var_33 = fadd double %var_32, 1.0
  br label %block_10
block_10:
  %var_90 = phi double [%var_89, %block_8], [%var_33, %block_9]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  %var_34 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_34, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_36 = fadd double %var_90, 1.0
  %var_37 = fmul double %var_36, 1.0
  %var_38 = fsub double %var_37, 1.0
  %var_39 = fdiv double %var_38, 1.0
  %var_40 = fadd double %var_39, 1.0
  br label %block_12
block_12:
  %var_91 = phi double [%var_90, %block_10], [%var_40, %block_11]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  %var_41 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
  br i1 %var_41, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_43 = fadd double %var_91, 1.0
  %var_44 = fmul double %var_43, 1.0
  %var_45 = fsub double %var_44, 1.0
  %var_46 = fdiv double %var_45, 1.0
  %var_47 = fadd double %var_46, 1.0
  br label %block_14
block_14:
  %var_92 = phi double [%var_91, %block_12], [%var_47, %block_13]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_48 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_48, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_50 = fadd double %var_92, 1.0
  %var_51 = fmul double %var_50, 1.0
  %var_52 = fsub double %var_51, 1.0
  %var_53 = fdiv double %var_52, 1.0
  %var_54 = fadd double %var_53, 1.0
  br label %block_16
block_16:
  %var_93 = phi double [%var_92, %block_14], [%var_54, %block_15]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
  %var_55 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
  br i1 %var_55, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_57 = fadd double %var_93, 1.0
  %var_58 = fmul double %var_57, 1.0
  %var_59 = fsub double %var_58, 1.0
  %var_60 = fdiv double %var_59, 1.0
  %var_61 = fadd double %var_60, 1.0
  br label %block_18
block_18:
  %var_94 = phi double [%var_93, %block_16], [%var_61, %block_17]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
  %var_62 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_62, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_64 = fadd double %var_94, 1.0
  %var_65 = fmul double %var_64, 1.0
  %var_66 = fsub double %var_65, 1.0
  %var_67 = fdiv double %var_66, 1.0
  %var_68 = fadd double %var_67, 1.0
  br label %block_20
block_20:
  %var_95 = phi double [%var_94, %block_18], [%var_68, %block_19]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_69 = fptosi double %var_95 to i64
  %var_71 = sitofp i64 %var_69 to double
  %var_73 = fcmp ogt double %var_95, 5.0
  %var_74 = fcmp olt double %var_95, 5.0
  %var_75 = fcmp oge double %var_95, 10.0
  %var_76 = fcmp oeq double %var_95, 10.0
  %var_77 = fcmp one double %var_95, 10.0
  call void @__quantum__rt__tuple_record_output(i64 8, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__double_record_output(double %var_95, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_73, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_74, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_75, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_76, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_77, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @6, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_69, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @7, i64 0, i64 0))
  call void @__quantum__rt__double_record_output(double %var_71, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @8, i64 0, i64 0))
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
