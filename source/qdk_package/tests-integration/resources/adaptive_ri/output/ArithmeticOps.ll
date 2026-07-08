%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@3 = internal constant [6 x i8] c"3_t2i\00"
@4 = internal constant [6 x i8] c"4_t3i\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  %var_9 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_9, label %block_1, label %block_2
block_1:
  br label %block_2
block_2:
  %var_43 = phi i64 [10, %block_0], [8, %block_1]
  %var_42 = phi i64 [0, %block_0], [5, %block_1]
  %var_41 = phi i64 [0, %block_0], [1, %block_1]
  %var_40 = phi i64 [1, %block_0], [3, %block_1]
  %var_11 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_11, label %block_3, label %block_4
block_3:
  %var_13 = add i64 %var_41, 1
  %var_14 = add i64 %var_42, 5
  %var_15 = sub i64 %var_43, 2
  %var_16 = mul i64 %var_40, 3
  br label %block_4
block_4:
  %var_47 = phi i64 [%var_43, %block_2], [%var_15, %block_3]
  %var_46 = phi i64 [%var_42, %block_2], [%var_14, %block_3]
  %var_45 = phi i64 [%var_41, %block_2], [%var_13, %block_3]
  %var_44 = phi i64 [%var_40, %block_2], [%var_16, %block_3]
  %var_17 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_17, label %block_5, label %block_6
block_5:
  %var_19 = add i64 %var_45, 1
  %var_20 = add i64 %var_46, 5
  %var_21 = sub i64 %var_47, 2
  %var_22 = mul i64 %var_44, 3
  br label %block_6
block_6:
  %var_51 = phi i64 [%var_47, %block_4], [%var_21, %block_5]
  %var_50 = phi i64 [%var_46, %block_4], [%var_20, %block_5]
  %var_49 = phi i64 [%var_45, %block_4], [%var_19, %block_5]
  %var_48 = phi i64 [%var_44, %block_4], [%var_22, %block_5]
  %var_23 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_23, label %block_7, label %block_8
block_7:
  %var_25 = add i64 %var_49, 1
  %var_26 = add i64 %var_50, 5
  %var_27 = sub i64 %var_51, 2
  %var_28 = mul i64 %var_48, 3
  br label %block_8
block_8:
  %var_55 = phi i64 [%var_51, %block_6], [%var_27, %block_7]
  %var_54 = phi i64 [%var_50, %block_6], [%var_26, %block_7]
  %var_53 = phi i64 [%var_49, %block_6], [%var_25, %block_7]
  %var_52 = phi i64 [%var_48, %block_6], [%var_28, %block_7]
  %var_29 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_29, label %block_9, label %block_10
block_9:
  %var_31 = add i64 %var_53, 1
  %var_32 = add i64 %var_54, 5
  %var_33 = sub i64 %var_55, 2
  %var_34 = mul i64 %var_52, 3
  br label %block_10
block_10:
  %var_59 = phi i64 [%var_55, %block_8], [%var_33, %block_9]
  %var_58 = phi i64 [%var_54, %block_8], [%var_32, %block_9]
  %var_57 = phi i64 [%var_53, %block_8], [%var_31, %block_9]
  %var_56 = phi i64 [%var_52, %block_8], [%var_34, %block_9]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_57, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_58, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_59, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_56, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__int_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
