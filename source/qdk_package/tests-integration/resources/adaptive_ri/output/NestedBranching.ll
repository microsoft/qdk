%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0t\00"
@2 = internal constant [8 x i8] c"2_t0t0a\00"
@3 = internal constant [10 x i8] c"3_t0t0a0r\00"
@4 = internal constant [10 x i8] c"4_t0t0a1r\00"
@5 = internal constant [10 x i8] c"5_t0t0a2r\00"
@6 = internal constant [8 x i8] c"6_t0t1i\00"
@7 = internal constant [6 x i8] c"7_t1t\00"
@8 = internal constant [8 x i8] c"8_t1t0a\00"
@9 = internal constant [10 x i8] c"9_t1t0a0r\00"
@10 = internal constant [11 x i8] c"10_t1t0a1r\00"
@11 = internal constant [11 x i8] c"11_t1t0a2r\00"
@12 = internal constant [11 x i8] c"12_t1t0a3r\00"
@13 = internal constant [9 x i8] c"13_t1t1b\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  %var_4 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  %var_5 = icmp eq i1 %var_4, false
  br i1 %var_5, label %block_1, label %block_2
block_1:
  %var_6 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  %var_7 = icmp eq i1 %var_6, false
  br i1 %var_7, label %block_3, label %block_5
block_2:
  %var_24 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  %var_25 = icmp eq i1 %var_24, false
  br i1 %var_25, label %block_4, label %block_6
block_3:
  %var_10 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  %var_11 = icmp eq i1 %var_10, false
  br label %block_5
block_4:
  %var_28 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  %var_29 = icmp eq i1 %var_28, false
  br label %block_6
block_5:
  %var_90 = phi i1 [false, %block_1], [%var_11, %block_3]
  br i1 %var_90, label %block_7, label %block_8
block_6:
  %var_91 = phi i1 [false, %block_2], [%var_29, %block_4]
  br i1 %var_91, label %block_9, label %block_10
block_7:
  br label %block_31
block_8:
  %var_12 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  %var_13 = icmp eq i1 %var_12, false
  br i1 %var_13, label %block_11, label %block_13
block_9:
  br label %block_32
block_10:
  %var_30 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  %var_31 = icmp eq i1 %var_30, false
  br i1 %var_31, label %block_12, label %block_14
block_11:
  %var_16 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br label %block_13
block_12:
  %var_34 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  br label %block_14
block_13:
  %var_92 = phi i1 [false, %block_8], [%var_16, %block_11]
  br i1 %var_92, label %block_15, label %block_16
block_14:
  %var_93 = phi i1 [false, %block_10], [%var_34, %block_12]
  br i1 %var_93, label %block_17, label %block_18
block_15:
  br label %block_29
block_16:
  %var_18 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_18, label %block_19, label %block_21
block_17:
  br label %block_30
block_18:
  %var_36 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %var_36, label %block_20, label %block_22
block_19:
  %var_22 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  %var_23 = icmp eq i1 %var_22, false
  br label %block_21
block_20:
  %var_40 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
  %var_41 = icmp eq i1 %var_40, false
  br label %block_22
block_21:
  %var_94 = phi i1 [false, %block_16], [%var_23, %block_19]
  br i1 %var_94, label %block_23, label %block_24
block_22:
  %var_95 = phi i1 [false, %block_18], [%var_41, %block_20]
  br i1 %var_95, label %block_25, label %block_26
block_23:
  br label %block_27
block_24:
  br label %block_27
block_25:
  br label %block_28
block_26:
  br label %block_28
block_27:
  %var_96 = phi i64 [2, %block_23], [3, %block_24]
  br label %block_29
block_28:
  %var_97 = phi i64 [6, %block_25], [7, %block_26]
  br label %block_30
block_29:
  %var_98 = phi i64 [1, %block_15], [%var_96, %block_27]
  br label %block_31
block_30:
  %var_99 = phi i64 [5, %block_17], [%var_97, %block_28]
  br label %block_32
block_31:
  %var_100 = phi i64 [0, %block_7], [%var_98, %block_29]
  br label %block_33
block_32:
  %var_101 = phi i64 [4, %block_9], [%var_99, %block_30]
  br label %block_33
block_33:
  %var_102 = phi i64 [%var_100, %block_31], [%var_101, %block_32]
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 5 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
  %var_47 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  %var_48 = icmp eq i1 %var_47, false
  br i1 %var_48, label %block_34, label %block_35
block_34:
  %var_49 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  %var_50 = icmp eq i1 %var_49, false
  br i1 %var_50, label %block_36, label %block_37
block_35:
  %var_55 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  %var_56 = icmp eq i1 %var_55, false
  br i1 %var_56, label %block_38, label %block_43
block_36:
  %var_51 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_52 = icmp eq i1 %var_51, false
  br i1 %var_52, label %block_39, label %block_40
block_37:
  %var_53 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_54 = icmp eq i1 %var_53, false
  br i1 %var_54, label %block_41, label %block_42
block_38:
  %var_59 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  br label %block_43
block_39:
  br label %block_44
block_40:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_44
block_41:
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_45
block_42:
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_45
block_43:
  %var_103 = phi i1 [false, %block_35], [%var_59, %block_38]
  br i1 %var_103, label %block_46, label %block_47
block_44:
  br label %block_48
block_45:
  br label %block_48
block_46:
  %var_61 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  %var_62 = icmp eq i1 %var_61, false
  br i1 %var_62, label %block_49, label %block_50
block_47:
  %var_67 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
  br i1 %var_67, label %block_51, label %block_56
block_48:
  br label %block_82
block_49:
  %var_63 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_64 = icmp eq i1 %var_63, false
  br i1 %var_64, label %block_52, label %block_53
block_50:
  %var_65 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_66 = icmp eq i1 %var_65, false
  br i1 %var_66, label %block_54, label %block_55
block_51:
  %var_71 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  %var_72 = icmp eq i1 %var_71, false
  br label %block_56
block_52:
  br label %block_57
block_53:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_57
block_54:
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_58
block_55:
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_58
block_56:
  %var_104 = phi i1 [false, %block_47], [%var_72, %block_51]
  br i1 %var_104, label %block_59, label %block_60
block_57:
  br label %block_61
block_58:
  br label %block_61
block_59:
  %var_73 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  %var_74 = icmp eq i1 %var_73, false
  br i1 %var_74, label %block_62, label %block_63
block_60:
  %var_79 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
  %var_80 = icmp eq i1 %var_79, false
  br i1 %var_80, label %block_64, label %block_65
block_61:
  br label %block_81
block_62:
  %var_75 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_76 = icmp eq i1 %var_75, false
  br i1 %var_76, label %block_66, label %block_67
block_63:
  %var_77 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_78 = icmp eq i1 %var_77, false
  br i1 %var_78, label %block_68, label %block_69
block_64:
  %var_81 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_82 = icmp eq i1 %var_81, false
  br i1 %var_82, label %block_70, label %block_71
block_65:
  %var_83 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
  %var_84 = icmp eq i1 %var_83, false
  br i1 %var_84, label %block_72, label %block_73
block_66:
  br label %block_74
block_67:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_74
block_68:
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_75
block_69:
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_75
block_70:
  br label %block_76
block_71:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_76
block_72:
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_77
block_73:
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  br label %block_77
block_74:
  br label %block_78
block_75:
  br label %block_78
block_76:
  br label %block_79
block_77:
  br label %block_79
block_78:
  br label %block_80
block_79:
  br label %block_80
block_80:
  br label %block_81
block_81:
  br label %block_82
block_82:
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 5 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 6 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 7 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
  %var_87 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__array_record_output(i64 3, i8* getelementptr inbounds ([8 x i8], [8 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([10 x i8], [10 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([10 x i8], [10 x i8]* @4, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([10 x i8], [10 x i8]* @5, i64 0, i64 0))
  call void @__quantum__rt__int_record_output(i64 %var_102, i8* getelementptr inbounds ([8 x i8], [8 x i8]* @6, i64 0, i64 0))
  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @7, i64 0, i64 0))
  call void @__quantum__rt__array_record_output(i64 4, i8* getelementptr inbounds ([8 x i8], [8 x i8]* @8, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([10 x i8], [10 x i8]* @9, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([11 x i8], [11 x i8]* @10, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 5 to %Result*), i8* getelementptr inbounds ([11 x i8], [11 x i8]* @11, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 6 to %Result*), i8* getelementptr inbounds ([11 x i8], [11 x i8]* @12, i64 0, i64 0))
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_87, i8* getelementptr inbounds ([9 x i8], [9 x i8]* @13, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare zeroext i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__rt__int_record_output(i64, i8*)

declare void @__quantum__rt__bool_record_output(i1 zeroext, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="8" "required_num_results"="8" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
