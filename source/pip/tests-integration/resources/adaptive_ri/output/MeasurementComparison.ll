%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  %var_0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %var_2 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %var_3 = icmp eq i1 %var_2, false
  %var_4 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %var_5 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %var_6 = icmp eq i1 %var_4, %var_5
  %var_7 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %var_8 = icmp eq i1 %var_7, false
  br i1 %var_8, label %block_1, label %block_2
block_1:
  br label %block_3
block_2:
  br label %block_3
block_3:
  %var_10 = phi i1 [false, %block_1], [true, %block_2]
  call void @__quantum__rt__tuple_record_output(i64 4, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_0, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_3, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_6, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_10, i8* null)
  ret i64 0
}

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__bool_record_output(i1, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
