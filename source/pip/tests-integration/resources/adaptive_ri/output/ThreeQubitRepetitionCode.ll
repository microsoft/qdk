%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %var_11 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  br i1 %var_11, label %block_1, label %block_2
block_1:
  %var_13 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_13, label %block_3, label %block_4
block_2:
  %var_15 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_15, label %block_5, label %block_6
block_3:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %block_7
block_4:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_7
block_5:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_8
block_6:
  br label %block_8
block_7:
  br label %block_9
block_8:
  %var_87 = phi i1 [true, %block_5], [false, %block_6]
  br label %block_9
block_9:
  %var_88 = phi i1 [true, %block_7], [%var_87, %block_8]
  br i1 %var_88, label %block_10, label %block_11
block_10:
  br label %block_11
block_11:
  %var_89 = phi i64 [0, %block_9], [1, %block_10]
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  %var_26 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 2 to %Result*))
  br i1 %var_26, label %block_12, label %block_13
block_12:
  %var_28 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_28, label %block_14, label %block_15
block_13:
  %var_30 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_30, label %block_16, label %block_17
block_14:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %block_18
block_15:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_18
block_16:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_19
block_17:
  br label %block_19
block_18:
  br label %block_20
block_19:
  %var_90 = phi i1 [true, %block_16], [false, %block_17]
  br label %block_20
block_20:
  %var_91 = phi i1 [true, %block_18], [%var_90, %block_19]
  br i1 %var_91, label %block_21, label %block_22
block_21:
  %var_33 = add i64 %var_89, 1
  br label %block_22
block_22:
  %var_92 = phi i64 [%var_89, %block_20], [%var_33, %block_21]
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  %var_42 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 4 to %Result*))
  br i1 %var_42, label %block_23, label %block_24
block_23:
  %var_44 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_44, label %block_25, label %block_26
block_24:
  %var_46 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 5 to %Result*))
  br i1 %var_46, label %block_27, label %block_28
block_25:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %block_29
block_26:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_29
block_27:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_30
block_28:
  br label %block_30
block_29:
  br label %block_31
block_30:
  %var_93 = phi i1 [true, %block_27], [false, %block_28]
  br label %block_31
block_31:
  %var_94 = phi i1 [true, %block_29], [%var_93, %block_30]
  br i1 %var_94, label %block_32, label %block_33
block_32:
  %var_49 = add i64 %var_92, 1
  br label %block_33
block_33:
  %var_95 = phi i64 [%var_92, %block_31], [%var_49, %block_32]
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_58 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 6 to %Result*))
  br i1 %var_58, label %block_34, label %block_35
block_34:
  %var_60 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_60, label %block_36, label %block_37
block_35:
  %var_62 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 7 to %Result*))
  br i1 %var_62, label %block_38, label %block_39
block_36:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %block_40
block_37:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_40
block_38:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_41
block_39:
  br label %block_41
block_40:
  br label %block_42
block_41:
  %var_96 = phi i1 [true, %block_38], [false, %block_39]
  br label %block_42
block_42:
  %var_97 = phi i1 [true, %block_40], [%var_96, %block_41]
  br i1 %var_97, label %block_43, label %block_44
block_43:
  %var_65 = add i64 %var_95, 1
  br label %block_44
block_44:
  %var_98 = phi i64 [%var_95, %block_42], [%var_65, %block_43]
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
  %var_74 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 8 to %Result*))
  br i1 %var_74, label %block_45, label %block_46
block_45:
  %var_76 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_76, label %block_47, label %block_48
block_46:
  %var_78 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 9 to %Result*))
  br i1 %var_78, label %block_49, label %block_50
block_47:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %block_51
block_48:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %block_51
block_49:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %block_52
block_50:
  br label %block_52
block_51:
  br label %block_53
block_52:
  %var_99 = phi i1 [true, %block_49], [false, %block_50]
  br label %block_53
block_53:
  %var_100 = phi i1 [true, %block_51], [%var_99, %block_52]
  br i1 %var_100, label %block_54, label %block_55
block_54:
  %var_81 = add i64 %var_98, 1
  br label %block_55
block_55:
  %var_101 = phi i64 [%var_98, %block_53], [%var_81, %block_54]
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 10 to %Result*))
  %var_82 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 10 to %Result*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__bool_record_output(i1 %var_82, i8* null)
  call void @__quantum__rt__int_record_output(i64 %var_101, i8* null)
  ret i64 0
}

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__rx__body(double, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__bool_record_output(i1, i8*)

declare void @__quantum__rt__int_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="11" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 1, !"int_computations", !"i64"}
