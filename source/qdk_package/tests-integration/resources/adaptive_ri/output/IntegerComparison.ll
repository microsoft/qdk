%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1b\00"
@3 = internal constant [6 x i8] c"3_t2b\00"

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
  %var_39 = phi i64 [0, %block_0], [1, %block_1]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_6 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_6, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_8 = add i64 %var_39, 1
  br label %block_4
block_4:
  %var_40 = phi i64 [%var_39, %block_2], [%var_8, %block_3]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_9 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_9, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_11 = add i64 %var_40, 1
  br label %block_6
block_6:
  %var_41 = phi i64 [%var_40, %block_4], [%var_11, %block_5]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_12 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_12, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_14 = add i64 %var_41, 1
  br label %block_8
block_8:
  %var_42 = phi i64 [%var_41, %block_6], [%var_14, %block_7]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  %var_15 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_15, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_17 = add i64 %var_42, 1
  br label %block_10
block_10:
  %var_43 = phi i64 [%var_42, %block_8], [%var_17, %block_9]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  %var_18 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_18, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_20 = add i64 %var_43, 1
  br label %block_12
block_12:
  %var_44 = phi i64 [%var_43, %block_10], [%var_20, %block_11]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  %var_21 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
  br i1 %var_21, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_23 = add i64 %var_44, 1
  br label %block_14
block_14:
  %var_45 = phi i64 [%var_44, %block_12], [%var_23, %block_13]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_24 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_24, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_26 = add i64 %var_45, 1
  br label %block_16
block_16:
  %var_46 = phi i64 [%var_45, %block_14], [%var_26, %block_15]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
  %var_27 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
  br i1 %var_27, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_29 = add i64 %var_46, 1
  br label %block_18
block_18:
  %var_47 = phi i64 [%var_46, %block_16], [%var_29, %block_17]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
  %var_30 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_30, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_32 = add i64 %var_47, 1
  br label %block_20
block_20:
  %var_48 = phi i64 [%var_47, %block_18], [%var_32, %block_19]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_33 = icmp sgt i64 %var_48, 5
  %var_34 = icmp slt i64 %var_48, 5
  %var_35 = icmp eq i64 %var_48, 10
  call void @__quantum__rt__tuple_record_output(i64 3, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_33, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_34, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 %var_35, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__bool_record_output(i1, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="10" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
