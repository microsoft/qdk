%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %var_2 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_2, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_2
block_2:
  %var_72 = phi double [0.0, %block_0], [1.0, %block_1]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_4 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_4, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_6 = fadd double %var_72, 1.0
  %var_7 = fmul double %var_6, 1.0
  %var_8 = fsub double %var_7, 1.0
  %var_9 = fdiv double %var_8, 1.0
  %var_10 = fadd double %var_9, 1.0
  br label %block_4
block_4:
  %var_73 = phi double [%var_72, %block_2], [%var_10, %block_3]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_11 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_11, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_13 = fadd double %var_73, 1.0
  %var_14 = fmul double %var_13, 1.0
  %var_15 = fsub double %var_14, 1.0
  %var_16 = fdiv double %var_15, 1.0
  %var_17 = fadd double %var_16, 1.0
  br label %block_6
block_6:
  %var_74 = phi double [%var_73, %block_4], [%var_17, %block_5]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_18 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_18, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_20 = fadd double %var_74, 1.0
  %var_21 = fmul double %var_20, 1.0
  %var_22 = fsub double %var_21, 1.0
  %var_23 = fdiv double %var_22, 1.0
  %var_24 = fadd double %var_23, 1.0
  br label %block_8
block_8:
  %var_75 = phi double [%var_74, %block_6], [%var_24, %block_7]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  %var_25 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_25, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_27 = fadd double %var_75, 1.0
  %var_28 = fmul double %var_27, 1.0
  %var_29 = fsub double %var_28, 1.0
  %var_30 = fdiv double %var_29, 1.0
  %var_31 = fadd double %var_30, 1.0
  br label %block_10
block_10:
  %var_76 = phi double [%var_75, %block_8], [%var_31, %block_9]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  %var_32 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_32, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_34 = fadd double %var_76, 1.0
  %var_35 = fmul double %var_34, 1.0
  %var_36 = fsub double %var_35, 1.0
  %var_37 = fdiv double %var_36, 1.0
  %var_38 = fadd double %var_37, 1.0
  br label %block_12
block_12:
  %var_77 = phi double [%var_76, %block_10], [%var_38, %block_11]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  %var_39 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 6 to %Result*))
  br i1 %var_39, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_41 = fadd double %var_77, 1.0
  %var_42 = fmul double %var_41, 1.0
  %var_43 = fsub double %var_42, 1.0
  %var_44 = fdiv double %var_43, 1.0
  %var_45 = fadd double %var_44, 1.0
  br label %block_14
block_14:
  %var_78 = phi double [%var_77, %block_12], [%var_45, %block_13]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_46 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_46, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_48 = fadd double %var_78, 1.0
  %var_49 = fmul double %var_48, 1.0
  %var_50 = fsub double %var_49, 1.0
  %var_51 = fdiv double %var_50, 1.0
  %var_52 = fadd double %var_51, 1.0
  br label %block_16
block_16:
  %var_79 = phi double [%var_78, %block_14], [%var_52, %block_15]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
  %var_53 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 8 to %Result*))
  br i1 %var_53, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_55 = fadd double %var_79, 1.0
  %var_56 = fmul double %var_55, 1.0
  %var_57 = fsub double %var_56, 1.0
  %var_58 = fdiv double %var_57, 1.0
  %var_59 = fadd double %var_58, 1.0
  br label %block_18
block_18:
  %var_80 = phi double [%var_79, %block_16], [%var_59, %block_17]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
  %var_60 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_60, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_62 = fadd double %var_80, 1.0
  %var_63 = fmul double %var_62, 1.0
  %var_64 = fsub double %var_63, 1.0
  %var_65 = fdiv double %var_64, 1.0
  %var_66 = fadd double %var_65, 1.0
  br label %block_20
block_20:
  %var_81 = phi double [%var_80, %block_18], [%var_66, %block_19]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_67 = fcmp ogt double %var_81, 5.0
  %var_68 = fcmp olt double %var_81, 5.0
  %var_69 = fcmp oge double %var_81, 10.0
  %var_70 = fcmp oeq double %var_81, 10.0
  %var_71 = fcmp one double %var_81, 10.0
  call void @__quantum__rt__tuple_record_output(i64 6, i8* null)
  call void @__quantum__rt__double_record_output(double %var_81, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_67, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_68, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_69, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_70, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_71, i8* null)
  ret i64 0
}

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__double_record_output(double, i8*)

declare void @__quantum__rt__bool_record_output(i1, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="10" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 1, !"int_computations", !"i64"}
!5 = !{i32 1, !"float_computations", !"f64"}
