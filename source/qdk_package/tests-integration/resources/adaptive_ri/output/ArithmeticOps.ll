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
  %var_9 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_9, label %block_1, label %block_2
block_1:
  br label %block_2
block_2:
  %var_67 = phi i64 [10, %block_0], [8, %block_1]
  %var_66 = phi i64 [0, %block_0], [5, %block_1]
  %var_65 = phi i64 [0, %block_0], [1, %block_1]
  %var_64 = phi i64 [1, %block_0], [3, %block_1]
  %var_11 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_11, label %block_3, label %block_4
block_3:
  %var_14 = add i64 %var_65, 1
  %var_16 = add i64 %var_66, 5
  %var_18 = sub i64 %var_67, 2
  %var_20 = mul i64 %var_64, 3
  br label %block_4
block_4:
  %var_71 = phi i64 [%var_67, %block_2], [%var_18, %block_3]
  %var_70 = phi i64 [%var_66, %block_2], [%var_16, %block_3]
  %var_69 = phi i64 [%var_65, %block_2], [%var_14, %block_3]
  %var_68 = phi i64 [%var_64, %block_2], [%var_20, %block_3]
  %var_21 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_21, label %block_5, label %block_6
block_5:
  %var_24 = add i64 %var_69, 1
  %var_26 = add i64 %var_70, 5
  %var_28 = sub i64 %var_71, 2
  %var_30 = mul i64 %var_68, 3
  br label %block_6
block_6:
  %var_75 = phi i64 [%var_71, %block_4], [%var_28, %block_5]
  %var_74 = phi i64 [%var_70, %block_4], [%var_26, %block_5]
  %var_73 = phi i64 [%var_69, %block_4], [%var_24, %block_5]
  %var_72 = phi i64 [%var_68, %block_4], [%var_30, %block_5]
  %var_31 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_31, label %block_7, label %block_8
block_7:
  %var_34 = add i64 %var_73, 1
  %var_36 = add i64 %var_74, 5
  %var_38 = sub i64 %var_75, 2
  %var_40 = mul i64 %var_72, 3
  br label %block_8
block_8:
  %var_79 = phi i64 [%var_74, %block_6], [%var_36, %block_7]
  %var_78 = phi i64 [%var_73, %block_6], [%var_34, %block_7]
  %var_77 = phi i64 [%var_72, %block_6], [%var_40, %block_7]
  %var_76 = phi i64 [%var_75, %block_6], [%var_38, %block_7]
  %var_41 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_41, label %block_9, label %block_10
block_9:
  %var_44 = add i64 %var_78, 1
  %var_46 = add i64 %var_79, 5
  %var_48 = sub i64 %var_76, 2
  %var_50 = mul i64 %var_77, 3
  br label %block_10
block_10:
  %var_83 = phi i64 [%var_76, %block_8], [%var_48, %block_9]
  %var_82 = phi i64 [%var_79, %block_8], [%var_46, %block_9]
  %var_81 = phi i64 [%var_78, %block_8], [%var_44, %block_9]
  %var_80 = phi i64 [%var_77, %block_8], [%var_50, %block_9]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_81, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_82, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_83, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_80, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare zeroext i1 @__quantum__rt__read_result(%Result*)

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
