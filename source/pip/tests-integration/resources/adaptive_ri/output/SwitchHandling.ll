%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_5 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_5, label %block_1, label %block_2
block_1:
  br label %block_2
block_2:
  %var_16 = phi i64 [0, %block_0], [1, %block_1]
  %var_7 = shl i64 %var_16, 1
  %var_8 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_8, label %block_3, label %block_4
block_3:
  %var_10 = add i64 %var_7, 1
  br label %block_4
block_4:
  %var_17 = phi i64 [%var_7, %block_2], [%var_10, %block_3]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  %var_12 = icmp eq i64 %var_17, 0
  br i1 %var_12, label %block_5, label %block_6
block_5:
  br label %block_13
block_6:
  %var_13 = icmp eq i64 %var_17, 1
  br i1 %var_13, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__ry__body(double 3.141592653589793, %Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_12
block_8:
  %var_14 = icmp eq i64 %var_17, 2
  br i1 %var_14, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__rz__body(double 3.141592653589793, %Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_11
block_10:
  call void @__quantum__qis__rx__body(double 3.141592653589793, %Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_11
block_11:
  br label %block_12
block_12:
  br label %block_13
block_13:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
  ret i64 0
}

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__qis__ry__body(double, %Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__rx__body(double, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 1, !"int_computations", !"i64"}
