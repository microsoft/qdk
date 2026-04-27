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
  %var_3 = alloca i64
  %var_8 = alloca i1
  %var_13 = alloca i1
  %var_18 = alloca i1
  %var_23 = alloca i1
  %var_28 = alloca i1
  %var_33 = alloca i1
  %var_36 = alloca i64
  %var_42 = alloca i64
  %var_43 = alloca i64
  %var_46 = alloca ptr
  %var_62 = alloca i1
  %var_73 = alloca i1
  %var_88 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_3
  %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_5 = icmp eq i1 %var_4, false
  br i1 %var_5, label %block_1, label %block_2
block_1:
  %var_6 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_7 = icmp eq i1 %var_6, false
  store i1 false, ptr %var_8
  br i1 %var_7, label %block_3, label %block_5
block_2:
  %var_21 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_22 = icmp eq i1 %var_21, false
  store i1 false, ptr %var_23
  br i1 %var_22, label %block_4, label %block_6
block_3:
  %var_9 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_10 = icmp eq i1 %var_9, false
  store i1 %var_10, ptr %var_8
  br label %block_5
block_4:
  %var_24 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_25 = icmp eq i1 %var_24, false
  store i1 %var_25, ptr %var_23
  br label %block_6
block_5:
  %var_138 = load i1, ptr %var_8
  br i1 %var_138, label %block_7, label %block_8
block_6:
  %var_96 = load i1, ptr %var_23
  br i1 %var_96, label %block_9, label %block_10
block_7:
  store i64 0, ptr %var_3
  br label %block_11
block_8:
  %var_11 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_12 = icmp eq i1 %var_11, false
  store i1 false, ptr %var_13
  br i1 %var_12, label %block_12, label %block_15
block_9:
  store i64 4, ptr %var_3
  br label %block_13
block_10:
  %var_26 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_27 = icmp eq i1 %var_26, false
  store i1 false, ptr %var_28
  br i1 %var_27, label %block_14, label %block_17
block_11:
  br label %block_16
block_12:
  %var_14 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_14, ptr %var_13
  br label %block_15
block_13:
  br label %block_16
block_14:
  %var_29 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_29, ptr %var_28
  br label %block_17
block_15:
  %var_140 = load i1, ptr %var_13
  br i1 %var_140, label %block_18, label %block_19
block_16:
  store i64 0, ptr %var_36
  br label %block_20
block_17:
  %var_98 = load i1, ptr %var_28
  br i1 %var_98, label %block_21, label %block_22
block_18:
  store i64 1, ptr %var_3
  br label %block_23
block_19:
  %var_16 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_18
  br i1 %var_16, label %block_24, label %block_29
block_20:
  %var_103 = load i64, ptr %var_36
  %var_37 = icmp slt i64 %var_103, 3
  br i1 %var_37, label %block_25, label %block_26
block_21:
  store i64 5, ptr %var_3
  br label %block_27
block_22:
  %var_31 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_33
  br i1 %var_31, label %block_28, label %block_31
block_23:
  br label %block_11
block_24:
  %var_19 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_20 = icmp eq i1 %var_19, false
  store i1 %var_20, ptr %var_18
  br label %block_29
block_25:
  %var_128 = load i64, ptr %var_36
  %var_38 = getelementptr ptr, ptr @array0, i64 %var_128
  %var_129 = load ptr, ptr %var_38
  call void @__quantum__qis__reset__body(ptr %var_129)
  %var_40 = add i64 %var_128, 1
  store i64 %var_40, ptr %var_36
  br label %block_20
block_26:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  store i64 7, ptr %var_42
  store i64 0, ptr %var_43
  br label %block_30
block_27:
  br label %block_13
block_28:
  %var_34 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_35 = icmp eq i1 %var_34, false
  store i1 %var_35, ptr %var_33
  br label %block_31
block_29:
  %var_142 = load i1, ptr %var_18
  br i1 %var_142, label %block_32, label %block_33
block_30:
  %var_106 = load i64, ptr %var_43
  %var_44 = icmp slt i64 %var_106, 4
  br i1 %var_44, label %block_34, label %block_35
block_31:
  %var_100 = load i1, ptr %var_33
  br i1 %var_100, label %block_36, label %block_37
block_32:
  store i64 2, ptr %var_3
  br label %block_38
block_33:
  store i64 3, ptr %var_3
  br label %block_38
block_34:
  %var_119 = load i64, ptr %var_43
  %var_45 = getelementptr ptr, ptr @array1, i64 %var_119
  %var_120 = load ptr, ptr %var_45
  store ptr %var_120, ptr %var_46
  %var_122 = load i64, ptr %var_42
  %var_47 = and i64 %var_122, 1
  %var_48 = icmp eq i64 %var_47, 1
  br i1 %var_48, label %block_39, label %block_43
block_35:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 6 to ptr))
  %var_52 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_53 = icmp eq i1 %var_52, false
  br i1 %var_53, label %block_40, label %block_41
block_36:
  store i64 6, ptr %var_3
  br label %block_42
block_37:
  store i64 7, ptr %var_3
  br label %block_42
block_38:
  br label %block_23
block_39:
  %var_127 = load ptr, ptr %var_46
  call void @__quantum__qis__x__body(ptr %var_127)
  br label %block_43
block_40:
  %var_54 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_55 = icmp eq i1 %var_54, false
  br i1 %var_55, label %block_44, label %block_45
block_41:
  %var_60 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_61 = icmp eq i1 %var_60, false
  store i1 false, ptr %var_62
  br i1 %var_61, label %block_46, label %block_51
block_42:
  br label %block_27
block_43:
  %var_123 = load i64, ptr %var_42
  %var_49 = ashr i64 %var_123, 1
  store i64 %var_49, ptr %var_42
  %var_125 = load i64, ptr %var_43
  %var_50 = add i64 %var_125, 1
  store i64 %var_50, ptr %var_43
  br label %block_30
block_44:
  %var_56 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_57 = icmp eq i1 %var_56, false
  br i1 %var_57, label %block_47, label %block_48
block_45:
  %var_58 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_59 = icmp eq i1 %var_58, false
  br i1 %var_59, label %block_49, label %block_50
block_46:
  %var_63 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  store i1 %var_63, ptr %var_62
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
  %var_108 = load i1, ptr %var_62
  br i1 %var_108, label %block_54, label %block_55
block_52:
  br label %block_56
block_53:
  br label %block_56
block_54:
  %var_65 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_66 = icmp eq i1 %var_65, false
  br i1 %var_66, label %block_57, label %block_58
block_55:
  %var_71 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  store i1 false, ptr %var_73
  br i1 %var_71, label %block_59, label %block_65
block_56:
  br label %block_60
block_57:
  %var_67 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_68 = icmp eq i1 %var_67, false
  br i1 %var_68, label %block_61, label %block_62
block_58:
  %var_69 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_70 = icmp eq i1 %var_69, false
  br i1 %var_70, label %block_63, label %block_64
block_59:
  %var_74 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_75 = icmp eq i1 %var_74, false
  store i1 %var_75, ptr %var_73
  br label %block_65
block_60:
  store i64 0, ptr %var_88
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
  %var_110 = load i1, ptr %var_73
  br i1 %var_110, label %block_69, label %block_70
block_66:
  %var_112 = load i64, ptr %var_88
  %var_89 = icmp slt i64 %var_112, 4
  br i1 %var_89, label %block_71, label %block_72
block_67:
  br label %block_73
block_68:
  br label %block_73
block_69:
  %var_76 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_77 = icmp eq i1 %var_76, false
  br i1 %var_77, label %block_74, label %block_75
block_70:
  %var_82 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_83 = icmp eq i1 %var_82, false
  br i1 %var_83, label %block_76, label %block_77
block_71:
  %var_114 = load i64, ptr %var_88
  %var_90 = getelementptr ptr, ptr @array1, i64 %var_114
  %var_115 = load ptr, ptr %var_90
  call void @__quantum__qis__reset__body(ptr %var_115)
  %var_92 = add i64 %var_114, 1
  store i64 %var_92, ptr %var_88
  br label %block_66
block_72:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 7 to ptr))
  %var_93 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @1)
  call void @__quantum__rt__array_record_output(i64 3, ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  %var_113 = load i64, ptr %var_3
  call void @__quantum__rt__int_record_output(i64 %var_113, ptr @6)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @7)
  call void @__quantum__rt__array_record_output(i64 4, ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @9)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @10)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @11)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 6 to ptr), ptr @12)
  call void @__quantum__rt__bool_record_output(i1 %var_93, ptr @13)
  ret i64 0
block_73:
  br label %block_78
block_74:
  %var_78 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_79 = icmp eq i1 %var_78, false
  br i1 %var_79, label %block_79, label %block_80
block_75:
  %var_80 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_81 = icmp eq i1 %var_80, false
  br i1 %var_81, label %block_81, label %block_82
block_76:
  %var_84 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_85 = icmp eq i1 %var_84, false
  br i1 %var_85, label %block_83, label %block_84
block_77:
  %var_86 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_87 = icmp eq i1 %var_86, false
  br i1 %var_87, label %block_85, label %block_86
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
