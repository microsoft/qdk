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
  %var_4 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_4, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_2
block_2:
  %var_57 = phi i64 [0, %block_0], [1, %block_1]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_6 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_6, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_9 = add i64 %var_57, 1
  br label %block_4
block_4:
  %var_58 = phi i64 [%var_57, %block_2], [%var_9, %block_3]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_10 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_10, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_13 = add i64 %var_58, 1
  br label %block_6
block_6:
  %var_59 = phi i64 [%var_58, %block_4], [%var_13, %block_5]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_14 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_14, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_17 = add i64 %var_59, 1
  br label %block_8
block_8:
  %var_60 = phi i64 [%var_59, %block_6], [%var_17, %block_7]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  %var_18 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_18, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_21 = add i64 %var_60, 1
  br label %block_10
block_10:
  %var_61 = phi i64 [%var_60, %block_8], [%var_21, %block_9]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  %var_22 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_22, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_25 = add i64 %var_61, 1
  br label %block_12
block_12:
  %var_62 = phi i64 [%var_61, %block_10], [%var_25, %block_11]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  %var_26 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
  br i1 %var_26, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_29 = add i64 %var_62, 1
  br label %block_14
block_14:
  %var_63 = phi i64 [%var_62, %block_12], [%var_29, %block_13]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_30 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_30, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_33 = add i64 %var_63, 1
  br label %block_16
block_16:
  %var_64 = phi i64 [%var_63, %block_14], [%var_33, %block_15]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
  %var_34 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
  br i1 %var_34, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_37 = add i64 %var_64, 1
  br label %block_18
block_18:
  %var_65 = phi i64 [%var_64, %block_16], [%var_37, %block_17]
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
  %var_38 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_38, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_41 = add i64 %var_65, 1
  br label %block_20
block_20:
  %var_66 = phi i64 [%var_65, %block_18], [%var_41, %block_19]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %var_43 = icmp sgt i64 %var_66, 5
  %var_46 = icmp slt i64 %var_66, 5
  %var_49 = icmp eq i64 %var_66, 10
  call void @__quantum__rt__tuple_record_output(i64 3, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_43, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_46, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_49, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare zeroext i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__bool_record_output(i1 zeroext, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="10" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
