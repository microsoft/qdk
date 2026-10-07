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
  %var_4 = alloca i64
  %var_10 = alloca i1
  %var_16 = alloca i1
  %var_22 = alloca i1
  %var_28 = alloca i1
  %var_34 = alloca i1
  %var_40 = alloca i1
  %var_43 = alloca i64
  %var_44 = alloca i64
  %var_49 = alloca i64
  %var_52 = alloca i64
  %var_53 = alloca i64
  %var_54 = alloca i64
  %var_57 = alloca ptr
  %var_58 = alloca i64
  %var_62 = alloca i64
  %var_64 = alloca i64
  %var_80 = alloca i1
  %var_92 = alloca i1
  %var_107 = alloca i64
  %var_108 = alloca i64
  %var_112 = alloca i64
  %var_114 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  call void @X(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_4
  %var_5 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_6 = icmp eq i1 %var_5, false
  br i1 %var_6, label %block_1, label %block_2
block_1:
  %var_7 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_8 = icmp eq i1 %var_7, false
  store i1 false, ptr %var_10
  br i1 %var_8, label %block_3, label %block_5
block_2:
  %var_25 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_26 = icmp eq i1 %var_25, false
  store i1 false, ptr %var_28
  br i1 %var_26, label %block_4, label %block_6
block_3:
  %var_11 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_12 = icmp eq i1 %var_11, false
  store i1 %var_12, ptr %var_10
  br label %block_5
block_4:
  %var_29 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_30 = icmp eq i1 %var_29, false
  store i1 %var_30, ptr %var_28
  br label %block_6
block_5:
  %var_178 = load i1, ptr %var_10
  br i1 %var_178, label %block_7, label %block_8
block_6:
  %var_118 = load i1, ptr %var_28
  br i1 %var_118, label %block_9, label %block_10
block_7:
  store i64 0, ptr %var_4
  br label %block_11
block_8:
  %var_13 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_14 = icmp eq i1 %var_13, false
  store i1 false, ptr %var_16
  br i1 %var_14, label %block_12, label %block_15
block_9:
  store i64 4, ptr %var_4
  br label %block_13
block_10:
  %var_31 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_32 = icmp eq i1 %var_31, false
  store i1 false, ptr %var_34
  br i1 %var_32, label %block_14, label %block_17
block_11:
  br label %block_16
block_12:
  %var_17 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_17, ptr %var_16
  br label %block_15
block_13:
  br label %block_16
block_14:
  %var_35 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_35, ptr %var_34
  br label %block_17
block_15:
  %var_180 = load i1, ptr %var_16
  br i1 %var_180, label %block_18, label %block_19
block_16:
  store i64 0, ptr %var_43
  br label %block_20
block_17:
  %var_120 = load i1, ptr %var_34
  br i1 %var_120, label %block_21, label %block_22
block_18:
  store i64 1, ptr %var_4
  br label %block_23
block_19:
  %var_19 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_22
  br i1 %var_19, label %block_24, label %block_29
block_20:
  %var_125 = load i64, ptr %var_43
  store i64 %var_125, ptr %var_44
  %var_127 = load i64, ptr %var_44
  %var_45 = icmp slt i64 %var_127, 3
  br i1 %var_45, label %block_25, label %block_26
block_21:
  store i64 5, ptr %var_4
  br label %block_27
block_22:
  %var_37 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_40
  br i1 %var_37, label %block_28, label %block_31
block_23:
  br label %block_11
block_24:
  %var_23 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_24 = icmp eq i1 %var_23, false
  store i1 %var_24, ptr %var_22
  br label %block_29
block_25:
  %var_166 = load i64, ptr %var_43
  %var_167_offset_chk = icmp slt i64 %var_166, 0
  %var_167_offset = select i1 %var_167_offset_chk, i64 1, i64 0
  %var_167 = getelementptr [3 x ptr], ptr @array0, i64 %var_167_offset, i64 %var_166
  %var_46 = load ptr, ptr %var_167
  call void @Reset(ptr %var_46)
  store i64 %var_166, ptr %var_49
  %var_169 = load i64, ptr %var_49
  %var_50 = add i64 %var_169, 1
  store i64 %var_50, ptr %var_43
  br label %block_20
block_26:
  call void @X(ptr inttoptr (i64 7 to ptr))
  store i64 7, ptr %var_52
  store i64 0, ptr %var_53
  br label %block_30
block_27:
  br label %block_13
block_28:
  %var_41 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_42 = icmp eq i1 %var_41, false
  store i1 %var_42, ptr %var_40
  br label %block_31
block_29:
  %var_182 = load i1, ptr %var_22
  br i1 %var_182, label %block_32, label %block_33
block_30:
  %var_130 = load i64, ptr %var_53
  store i64 %var_130, ptr %var_54
  %var_132 = load i64, ptr %var_54
  %var_55 = icmp slt i64 %var_132, 4
  br i1 %var_55, label %block_34, label %block_35
block_31:
  %var_122 = load i1, ptr %var_40
  br i1 %var_122, label %block_36, label %block_37
block_32:
  store i64 2, ptr %var_4
  br label %block_38
block_33:
  store i64 3, ptr %var_4
  br label %block_38
block_34:
  %var_151 = load i64, ptr %var_53
  %var_152_offset_chk = icmp slt i64 %var_151, 0
  %var_152_offset = select i1 %var_152_offset_chk, i64 1, i64 0
  %var_152 = getelementptr [4 x ptr], ptr @array1, i64 %var_152_offset, i64 %var_151
  %var_56 = load ptr, ptr %var_152
  store ptr %var_56, ptr %var_57
  %var_154 = load i64, ptr %var_52
  store i64 %var_154, ptr %var_58
  %var_156 = load i64, ptr %var_58
  %var_59 = and i64 %var_156, 1
  %var_61 = icmp eq i64 %var_59, 1
  br i1 %var_61, label %block_39, label %block_43
block_35:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 6 to ptr))
  %var_67 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_68 = icmp eq i1 %var_67, false
  br i1 %var_68, label %block_40, label %block_41
block_36:
  store i64 6, ptr %var_4
  br label %block_42
block_37:
  store i64 7, ptr %var_4
  br label %block_42
block_38:
  br label %block_23
block_39:
  %var_165 = load ptr, ptr %var_57
  call void @X(ptr %var_165)
  br label %block_43
block_40:
  %var_69 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_70 = icmp eq i1 %var_69, false
  br i1 %var_70, label %block_44, label %block_45
block_41:
  %var_77 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_78 = icmp eq i1 %var_77, false
  store i1 false, ptr %var_80
  br i1 %var_78, label %block_46, label %block_51
block_42:
  br label %block_27
block_43:
  %var_157 = load i64, ptr %var_52
  store i64 %var_157, ptr %var_62
  %var_159 = load i64, ptr %var_62
  %var_63 = ashr i64 %var_159, 1
  store i64 %var_63, ptr %var_52
  %var_161 = load i64, ptr %var_53
  store i64 %var_161, ptr %var_64
  %var_163 = load i64, ptr %var_64
  %var_65 = add i64 %var_163, 1
  store i64 %var_65, ptr %var_53
  br label %block_30
block_44:
  %var_71 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_72 = icmp eq i1 %var_71, false
  br i1 %var_72, label %block_47, label %block_48
block_45:
  %var_73 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_74 = icmp eq i1 %var_73, false
  br i1 %var_74, label %block_49, label %block_50
block_46:
  %var_81 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  store i1 %var_81, ptr %var_80
  br label %block_51
block_47:
  br label %block_52
block_48:
  call void @X(ptr inttoptr (i64 7 to ptr))
  call void @X(ptr inttoptr (i64 7 to ptr))
  br label %block_52
block_49:
  call void @Y(ptr inttoptr (i64 7 to ptr))
  call void @Y(ptr inttoptr (i64 7 to ptr))
  br label %block_53
block_50:
  call void @Z(ptr inttoptr (i64 7 to ptr))
  call void @Z(ptr inttoptr (i64 7 to ptr))
  br label %block_53
block_51:
  %var_134 = load i1, ptr %var_80
  br i1 %var_134, label %block_54, label %block_55
block_52:
  br label %block_56
block_53:
  br label %block_56
block_54:
  %var_83 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_84 = icmp eq i1 %var_83, false
  br i1 %var_84, label %block_57, label %block_58
block_55:
  %var_89 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  store i1 false, ptr %var_92
  br i1 %var_89, label %block_59, label %block_65
block_56:
  br label %block_60
block_57:
  %var_85 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_86 = icmp eq i1 %var_85, false
  br i1 %var_86, label %block_61, label %block_62
block_58:
  %var_87 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_88 = icmp eq i1 %var_87, false
  br i1 %var_88, label %block_63, label %block_64
block_59:
  %var_93 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_94 = icmp eq i1 %var_93, false
  store i1 %var_94, ptr %var_92
  br label %block_65
block_60:
  store i64 0, ptr %var_107
  br label %block_66
block_61:
  br label %block_67
block_62:
  call void @X(ptr inttoptr (i64 7 to ptr))
  call void @X(ptr inttoptr (i64 7 to ptr))
  br label %block_67
block_63:
  call void @Y(ptr inttoptr (i64 7 to ptr))
  call void @Y(ptr inttoptr (i64 7 to ptr))
  br label %block_68
block_64:
  call void @Z(ptr inttoptr (i64 7 to ptr))
  call void @Z(ptr inttoptr (i64 7 to ptr))
  br label %block_68
block_65:
  %var_136 = load i1, ptr %var_92
  br i1 %var_136, label %block_69, label %block_70
block_66:
  %var_138 = load i64, ptr %var_107
  store i64 %var_138, ptr %var_108
  %var_140 = load i64, ptr %var_108
  %var_109 = icmp slt i64 %var_140, 4
  br i1 %var_109, label %block_71, label %block_72
block_67:
  br label %block_73
block_68:
  br label %block_73
block_69:
  %var_95 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_96 = icmp eq i1 %var_95, false
  br i1 %var_96, label %block_74, label %block_75
block_70:
  %var_101 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_102 = icmp eq i1 %var_101, false
  br i1 %var_102, label %block_76, label %block_77
block_71:
  %var_144 = load i64, ptr %var_107
  %var_145_offset_chk = icmp slt i64 %var_144, 0
  %var_145_offset = select i1 %var_145_offset_chk, i64 1, i64 0
  %var_145 = getelementptr [4 x ptr], ptr @array1, i64 %var_145_offset, i64 %var_144
  %var_110 = load ptr, ptr %var_145
  call void @Reset(ptr %var_110)
  store i64 %var_144, ptr %var_112
  %var_147 = load i64, ptr %var_112
  %var_113 = add i64 %var_147, 1
  store i64 %var_113, ptr %var_107
  br label %block_66
block_72:
  %var_141 = load i64, ptr %var_4
  store i64 %var_141, ptr %var_114
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 7 to ptr))
  %var_115 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @1)
  call void @__quantum__rt__array_record_output(i64 3, ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  %var_143 = load i64, ptr %var_114
  call void @__quantum__rt__int_record_output(i64 %var_143, ptr @6)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @7)
  call void @__quantum__rt__array_record_output(i64 4, ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @9)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @10)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @11)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 6 to ptr), ptr @12)
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_115, ptr @13)
  ret i64 0
block_73:
  br label %block_78
block_74:
  %var_97 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_98 = icmp eq i1 %var_97, false
  br i1 %var_98, label %block_79, label %block_80
block_75:
  %var_99 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_100 = icmp eq i1 %var_99, false
  br i1 %var_100, label %block_81, label %block_82
block_76:
  %var_103 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_104 = icmp eq i1 %var_103, false
  br i1 %var_104, label %block_83, label %block_84
block_77:
  %var_105 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_106 = icmp eq i1 %var_105, false
  br i1 %var_106, label %block_85, label %block_86
block_78:
  br label %block_60
block_79:
  br label %block_87
block_80:
  call void @X(ptr inttoptr (i64 7 to ptr))
  call void @X(ptr inttoptr (i64 7 to ptr))
  br label %block_87
block_81:
  call void @Y(ptr inttoptr (i64 7 to ptr))
  call void @Y(ptr inttoptr (i64 7 to ptr))
  br label %block_88
block_82:
  call void @Z(ptr inttoptr (i64 7 to ptr))
  call void @Z(ptr inttoptr (i64 7 to ptr))
  br label %block_88
block_83:
  br label %block_89
block_84:
  call void @X(ptr inttoptr (i64 7 to ptr))
  call void @X(ptr inttoptr (i64 7 to ptr))
  br label %block_89
block_85:
  call void @Y(ptr inttoptr (i64 7 to ptr))
  call void @Y(ptr inttoptr (i64 7 to ptr))
  br label %block_90
block_86:
  call void @Z(ptr inttoptr (i64 7 to ptr))
  call void @Z(ptr inttoptr (i64 7 to ptr))
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

define internal void @X(ptr %var_2) {
block_94:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @Reset(ptr %var_48) {
block_95:
  call void @__quantum__qis__reset__body(ptr %var_48)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

define internal void @Y(ptr %var_75) {
block_96:
  call void @__quantum__qis__y__body(ptr %var_75)
  ret void
}

declare void @__quantum__qis__y__body(ptr)

define internal void @Z(ptr %var_76) {
block_97:
  call void @__quantum__qis__z__body(ptr %var_76)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="8" "required_num_results"="8" }
attributes #1 = { "irreversible" }
attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
!8 = !{i32 1, !"ir_functions", i1 true}
!9 = !{i32 1, !"writable_results", i1 true}
