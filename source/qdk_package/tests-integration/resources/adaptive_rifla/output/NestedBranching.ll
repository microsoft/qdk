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
@array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array1 = internal constant [4 x ptr] [ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 6 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_7 = alloca i1
  %var_12 = alloca i1
  %var_17 = alloca i1
  %var_22 = alloca i1
  %var_27 = alloca i1
  %var_32 = alloca i1
  %var_35 = alloca i64
  %var_41 = alloca i64
  %var_42 = alloca i64
  %var_45 = alloca ptr
  %var_61 = alloca i1
  %var_72 = alloca i1
  %var_87 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_2
  %var_3 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_4 = icmp eq i1 %var_3, false
  br i1 %var_4, label %block_1, label %block_2
block_1:
  %var_5 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_6 = icmp eq i1 %var_5, false
  store i1 false, ptr %var_7
  br i1 %var_6, label %block_3, label %block_5
block_2:
  %var_20 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_21 = icmp eq i1 %var_20, false
  store i1 false, ptr %var_22
  br i1 %var_21, label %block_4, label %block_6
block_3:
  %var_8 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_9 = icmp eq i1 %var_8, false
  store i1 %var_9, ptr %var_7
  br label %block_5
block_4:
  %var_23 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_24 = icmp eq i1 %var_23, false
  store i1 %var_24, ptr %var_22
  br label %block_6
block_5:
  %var_137 = load i1, ptr %var_7
  br i1 %var_137, label %block_7, label %block_8
block_6:
  %var_95 = load i1, ptr %var_22
  br i1 %var_95, label %block_9, label %block_10
block_7:
  store i64 0, ptr %var_2
  br label %block_11
block_8:
  %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_11 = icmp eq i1 %var_10, false
  store i1 false, ptr %var_12
  br i1 %var_11, label %block_12, label %block_15
block_9:
  store i64 4, ptr %var_2
  br label %block_13
block_10:
  %var_25 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_26 = icmp eq i1 %var_25, false
  store i1 false, ptr %var_27
  br i1 %var_26, label %block_14, label %block_17
block_11:
  br label %block_16
block_12:
  %var_13 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_13, ptr %var_12
  br label %block_15
block_13:
  br label %block_16
block_14:
  %var_28 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_28, ptr %var_27
  br label %block_17
block_15:
  %var_139 = load i1, ptr %var_12
  br i1 %var_139, label %block_18, label %block_19
block_16:
  store i64 0, ptr %var_35
  br label %block_20
block_17:
  %var_97 = load i1, ptr %var_27
  br i1 %var_97, label %block_21, label %block_22
block_18:
  store i64 1, ptr %var_2
  br label %block_23
block_19:
  %var_15 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_17
  br i1 %var_15, label %block_24, label %block_29
block_20:
  %var_102 = load i64, ptr %var_35
  %var_36 = icmp slt i64 %var_102, 3
  br i1 %var_36, label %block_25, label %block_26
block_21:
  store i64 5, ptr %var_2
  br label %block_27
block_22:
  %var_30 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_32
  br i1 %var_30, label %block_28, label %block_31
block_23:
  br label %block_11
block_24:
  %var_18 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_19 = icmp eq i1 %var_18, false
  store i1 %var_19, ptr %var_17
  br label %block_29
block_25:
  %var_127 = load i64, ptr %var_35
  %var_37 = getelementptr ptr, ptr @array0, i64 %var_127
  %var_128 = load ptr, ptr %var_37
  call void @__quantum__qis__reset__body(ptr %var_128)
  %var_39 = add i64 %var_127, 1
  store i64 %var_39, ptr %var_35
  br label %block_20
block_26:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  store i64 7, ptr %var_41
  store i64 0, ptr %var_42
  br label %block_30
block_27:
  br label %block_13
block_28:
  %var_33 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_34 = icmp eq i1 %var_33, false
  store i1 %var_34, ptr %var_32
  br label %block_31
block_29:
  %var_141 = load i1, ptr %var_17
  br i1 %var_141, label %block_32, label %block_33
block_30:
  %var_105 = load i64, ptr %var_42
  %var_43 = icmp slt i64 %var_105, 4
  br i1 %var_43, label %block_34, label %block_35
block_31:
  %var_99 = load i1, ptr %var_32
  br i1 %var_99, label %block_36, label %block_37
block_32:
  store i64 2, ptr %var_2
  br label %block_38
block_33:
  store i64 3, ptr %var_2
  br label %block_38
block_34:
  %var_118 = load i64, ptr %var_42
  %var_44 = getelementptr ptr, ptr @array1, i64 %var_118
  %var_119 = load ptr, ptr %var_44
  store ptr %var_119, ptr %var_45
  %var_121 = load i64, ptr %var_41
  %var_46 = and i64 %var_121, 1
  %var_47 = icmp eq i64 %var_46, 1
  br i1 %var_47, label %block_39, label %block_43
block_35:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 6 to ptr))
  %var_51 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_52 = icmp eq i1 %var_51, false
  br i1 %var_52, label %block_40, label %block_41
block_36:
  store i64 6, ptr %var_2
  br label %block_42
block_37:
  store i64 7, ptr %var_2
  br label %block_42
block_38:
  br label %block_23
block_39:
  %var_126 = load ptr, ptr %var_45
  call void @__quantum__qis__x__body(ptr %var_126)
  br label %block_43
block_40:
  %var_53 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_54 = icmp eq i1 %var_53, false
  br i1 %var_54, label %block_44, label %block_45
block_41:
  %var_59 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_60 = icmp eq i1 %var_59, false
  store i1 false, ptr %var_61
  br i1 %var_60, label %block_46, label %block_51
block_42:
  br label %block_27
block_43:
  %var_122 = load i64, ptr %var_41
  %var_48 = ashr i64 %var_122, 1
  store i64 %var_48, ptr %var_41
  %var_124 = load i64, ptr %var_42
  %var_49 = add i64 %var_124, 1
  store i64 %var_49, ptr %var_42
  br label %block_30
block_44:
  %var_55 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_56 = icmp eq i1 %var_55, false
  br i1 %var_56, label %block_47, label %block_48
block_45:
  %var_57 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_58 = icmp eq i1 %var_57, false
  br i1 %var_58, label %block_49, label %block_50
block_46:
  %var_62 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  store i1 %var_62, ptr %var_61
  br label %block_51
block_47:
  br label %block_52
block_48:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_52
block_49:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_53
block_50:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  br label %block_53
block_51:
  %var_107 = load i1, ptr %var_61
  br i1 %var_107, label %block_54, label %block_55
block_52:
  br label %block_56
block_53:
  br label %block_56
block_54:
  %var_64 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_65 = icmp eq i1 %var_64, false
  br i1 %var_65, label %block_57, label %block_58
block_55:
  %var_70 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  store i1 false, ptr %var_72
  br i1 %var_70, label %block_59, label %block_65
block_56:
  br label %block_60
block_57:
  %var_66 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_67 = icmp eq i1 %var_66, false
  br i1 %var_67, label %block_61, label %block_62
block_58:
  %var_68 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_69 = icmp eq i1 %var_68, false
  br i1 %var_69, label %block_63, label %block_64
block_59:
  %var_73 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_74 = icmp eq i1 %var_73, false
  store i1 %var_74, ptr %var_72
  br label %block_65
block_60:
  store i64 0, ptr %var_87
  br label %block_66
block_61:
  br label %block_67
block_62:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_67
block_63:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_68
block_64:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  br label %block_68
block_65:
  %var_109 = load i1, ptr %var_72
  br i1 %var_109, label %block_69, label %block_70
block_66:
  %var_111 = load i64, ptr %var_87
  %var_88 = icmp slt i64 %var_111, 4
  br i1 %var_88, label %block_71, label %block_72
block_67:
  br label %block_73
block_68:
  br label %block_73
block_69:
  %var_75 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_76 = icmp eq i1 %var_75, false
  br i1 %var_76, label %block_74, label %block_75
block_70:
  %var_81 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_82 = icmp eq i1 %var_81, false
  br i1 %var_82, label %block_76, label %block_77
block_71:
  %var_113 = load i64, ptr %var_87
  %var_89 = getelementptr ptr, ptr @array1, i64 %var_113
  %var_114 = load ptr, ptr %var_89
  call void @__quantum__qis__reset__body(ptr %var_114)
  %var_91 = add i64 %var_113, 1
  store i64 %var_91, ptr %var_87
  br label %block_66
block_72:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 7 to ptr))
  %var_92 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @1)
  call void @__quantum__rt__array_record_output(i64 3, ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  %var_112 = load i64, ptr %var_2
  call void @__quantum__rt__int_record_output(i64 %var_112, ptr @6)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @7)
  call void @__quantum__rt__array_record_output(i64 4, ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @9)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @10)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @11)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 6 to ptr), ptr @12)
  call void @__quantum__rt__bool_record_output(i1 %var_92, ptr @13)
  ret i64 0
block_73:
  br label %block_78
block_74:
  %var_77 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_78 = icmp eq i1 %var_77, false
  br i1 %var_78, label %block_79, label %block_80
block_75:
  %var_79 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_80 = icmp eq i1 %var_79, false
  br i1 %var_80, label %block_81, label %block_82
block_76:
  %var_83 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_84 = icmp eq i1 %var_83, false
  br i1 %var_84, label %block_83, label %block_84
block_77:
  %var_85 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_86 = icmp eq i1 %var_85, false
  br i1 %var_86, label %block_85, label %block_86
block_78:
  br label %block_60
block_79:
  br label %block_87
block_80:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_87
block_81:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_88
block_82:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  br label %block_88
block_83:
  br label %block_89
block_84:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_89
block_85:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_90
block_86:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  br label %block_90
block_87:
  br label %block_91
block_88:
  br label %block_91
block_89:
  br label %block_92
block_90:
  br label %block_92
block_91:
  br label %block_93
block_92:
  br label %block_93
block_93:
  br label %block_78
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__qis__y__body(ptr)

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="8" "required_num_results"="8" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
