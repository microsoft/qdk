%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
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
  %var_8 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_8, label %block_1, label %block_2
block_1:
  br label %block_2
block_2:
  %var_39 = phi i64 [1, %block_0], [3, %block_1]
  %var_38 = phi i64 [10, %block_0], [8, %block_1]
  %var_37 = phi i64 [0, %block_0], [5, %block_1]
  %var_36 = phi i64 [0, %block_0], [1, %block_1]
  %var_10 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_10, label %block_3, label %block_4
block_3:
  %var_12 = add i64 %var_36, 1
  %var_13 = add i64 %var_37, 5
  %var_14 = sub i64 %var_38, 2
  %var_15 = mul i64 %var_39, 3
  br label %block_4
block_4:
  %var_43 = phi i64 [%var_39, %block_2], [%var_15, %block_3]
  %var_42 = phi i64 [%var_38, %block_2], [%var_14, %block_3]
  %var_41 = phi i64 [%var_37, %block_2], [%var_13, %block_3]
  %var_40 = phi i64 [%var_36, %block_2], [%var_12, %block_3]
  %var_16 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_16, label %block_5, label %block_6
block_5:
  %var_18 = add i64 %var_40, 1
  %var_19 = add i64 %var_41, 5
  %var_20 = sub i64 %var_42, 2
  %var_21 = mul i64 %var_43, 3
  br label %block_6
block_6:
  %var_47 = phi i64 [%var_43, %block_4], [%var_21, %block_5]
  %var_46 = phi i64 [%var_42, %block_4], [%var_20, %block_5]
  %var_45 = phi i64 [%var_41, %block_4], [%var_19, %block_5]
  %var_44 = phi i64 [%var_40, %block_4], [%var_18, %block_5]
  %var_22 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_22, label %block_7, label %block_8
block_7:
  %var_24 = add i64 %var_44, 1
  %var_25 = add i64 %var_45, 5
  %var_26 = sub i64 %var_46, 2
  %var_27 = mul i64 %var_47, 3
  br label %block_8
block_8:
  %var_51 = phi i64 [%var_47, %block_6], [%var_27, %block_7]
  %var_50 = phi i64 [%var_46, %block_6], [%var_26, %block_7]
  %var_49 = phi i64 [%var_45, %block_6], [%var_25, %block_7]
  %var_48 = phi i64 [%var_44, %block_6], [%var_24, %block_7]
  %var_28 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_28, label %block_9, label %block_10
block_9:
  %var_30 = add i64 %var_48, 1
  %var_31 = add i64 %var_49, 5
  %var_32 = sub i64 %var_50, 2
  %var_33 = mul i64 %var_51, 3
  br label %block_10
block_10:
  %var_55 = phi i64 [%var_51, %block_8], [%var_33, %block_9]
  %var_54 = phi i64 [%var_50, %block_8], [%var_32, %block_9]
  %var_53 = phi i64 [%var_49, %block_8], [%var_31, %block_9]
  %var_52 = phi i64 [%var_48, %block_8], [%var_30, %block_9]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 4, i8* null)
  call void @__quantum__rt__int_record_output(i64 %var_52, i8* null)
  call void @__quantum__rt__int_record_output(i64 %var_53, i8* null)
  call void @__quantum__rt__int_record_output(i64 %var_54, i8* null)
  call void @__quantum__rt__int_record_output(i64 %var_55, i8* null)
  ret i64 0
}

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare i1 @__quantum__qis__read_result__body(%Result*)

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
!4 = !{i32 1, !"int_computations", !"i64"}
