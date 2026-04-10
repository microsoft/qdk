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
  %var_2 = alloca i64
  %var_7 = alloca i1
  %var_12 = alloca i1
  %var_17 = alloca i1
  %var_22 = alloca i1
  %var_27 = alloca i1
  %var_32 = alloca i1
  %var_50 = alloca i1
  %var_61 = alloca i1
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
  %var_100 = load i1, ptr %var_7
  br i1 %var_100, label %block_7, label %block_8
block_6:
  %var_80 = load i1, ptr %var_22
  br i1 %var_80, label %block_9, label %block_10
block_7:
  store i64 0, ptr %var_2
  br label %block_31
block_8:
  %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_11 = icmp eq i1 %var_10, false
  store i1 false, ptr %var_12
  br i1 %var_11, label %block_11, label %block_13
block_9:
  store i64 4, ptr %var_2
  br label %block_32
block_10:
  %var_25 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_26 = icmp eq i1 %var_25, false
  store i1 false, ptr %var_27
  br i1 %var_26, label %block_12, label %block_14
block_11:
  %var_13 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_13, ptr %var_12
  br label %block_13
block_12:
  %var_28 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_28, ptr %var_27
  br label %block_14
block_13:
  %var_102 = load i1, ptr %var_12
  br i1 %var_102, label %block_15, label %block_16
block_14:
  %var_82 = load i1, ptr %var_27
  br i1 %var_82, label %block_17, label %block_18
block_15:
  store i64 1, ptr %var_2
  br label %block_29
block_16:
  %var_15 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_17
  br i1 %var_15, label %block_19, label %block_21
block_17:
  store i64 5, ptr %var_2
  br label %block_30
block_18:
  %var_30 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 false, ptr %var_32
  br i1 %var_30, label %block_20, label %block_22
block_19:
  %var_18 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_19 = icmp eq i1 %var_18, false
  store i1 %var_19, ptr %var_17
  br label %block_21
block_20:
  %var_33 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  %var_34 = icmp eq i1 %var_33, false
  store i1 %var_34, ptr %var_32
  br label %block_22
block_21:
  %var_104 = load i1, ptr %var_17
  br i1 %var_104, label %block_23, label %block_24
block_22:
  %var_84 = load i1, ptr %var_32
  br i1 %var_84, label %block_25, label %block_26
block_23:
  store i64 2, ptr %var_2
  br label %block_27
block_24:
  store i64 3, ptr %var_2
  br label %block_27
block_25:
  store i64 6, ptr %var_2
  br label %block_28
block_26:
  store i64 7, ptr %var_2
  br label %block_28
block_27:
  br label %block_29
block_28:
  br label %block_30
block_29:
  br label %block_31
block_30:
  br label %block_32
block_31:
  br label %block_33
block_32:
  br label %block_33
block_33:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 6 to ptr))
  %var_40 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_41 = icmp eq i1 %var_40, false
  br i1 %var_41, label %block_34, label %block_35
block_34:
  %var_42 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_43 = icmp eq i1 %var_42, false
  br i1 %var_43, label %block_36, label %block_37
block_35:
  %var_48 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  %var_49 = icmp eq i1 %var_48, false
  store i1 false, ptr %var_50
  br i1 %var_49, label %block_38, label %block_43
block_36:
  %var_44 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_45 = icmp eq i1 %var_44, false
  br i1 %var_45, label %block_39, label %block_40
block_37:
  %var_46 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_47 = icmp eq i1 %var_46, false
  br i1 %var_47, label %block_41, label %block_42
block_38:
  %var_51 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  store i1 %var_51, ptr %var_50
  br label %block_43
block_39:
  br label %block_44
block_40:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_44
block_41:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_45
block_42:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  br label %block_45
block_43:
  %var_87 = load i1, ptr %var_50
  br i1 %var_87, label %block_46, label %block_47
block_44:
  br label %block_48
block_45:
  br label %block_48
block_46:
  %var_53 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_54 = icmp eq i1 %var_53, false
  br i1 %var_54, label %block_49, label %block_50
block_47:
  %var_59 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  store i1 false, ptr %var_61
  br i1 %var_59, label %block_51, label %block_56
block_48:
  br label %block_82
block_49:
  %var_55 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_56 = icmp eq i1 %var_55, false
  br i1 %var_56, label %block_52, label %block_53
block_50:
  %var_57 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_58 = icmp eq i1 %var_57, false
  br i1 %var_58, label %block_54, label %block_55
block_51:
  %var_62 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_63 = icmp eq i1 %var_62, false
  store i1 %var_63, ptr %var_61
  br label %block_56
block_52:
  br label %block_57
block_53:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_57
block_54:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_58
block_55:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  br label %block_58
block_56:
  %var_89 = load i1, ptr %var_61
  br i1 %var_89, label %block_59, label %block_60
block_57:
  br label %block_61
block_58:
  br label %block_61
block_59:
  %var_64 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_65 = icmp eq i1 %var_64, false
  br i1 %var_65, label %block_62, label %block_63
block_60:
  %var_70 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  %var_71 = icmp eq i1 %var_70, false
  br i1 %var_71, label %block_64, label %block_65
block_61:
  br label %block_81
block_62:
  %var_66 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_67 = icmp eq i1 %var_66, false
  br i1 %var_67, label %block_66, label %block_67
block_63:
  %var_68 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_69 = icmp eq i1 %var_68, false
  br i1 %var_69, label %block_68, label %block_69
block_64:
  %var_72 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_73 = icmp eq i1 %var_72, false
  br i1 %var_73, label %block_70, label %block_71
block_65:
  %var_74 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  %var_75 = icmp eq i1 %var_74, false
  br i1 %var_75, label %block_72, label %block_73
block_66:
  br label %block_74
block_67:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_74
block_68:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_75
block_69:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  br label %block_75
block_70:
  br label %block_76
block_71:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 7 to ptr))
  br label %block_76
block_72:
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__y__body(ptr inttoptr (i64 7 to ptr))
  br label %block_77
block_73:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 7 to ptr))
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
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 7 to ptr))
  %var_77 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @1)
  call void @__quantum__rt__array_record_output(i64 3, ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  %var_90 = load i64, ptr %var_2
  call void @__quantum__rt__int_record_output(i64 %var_90, ptr @6)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @7)
  call void @__quantum__rt__array_record_output(i64 4, ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @9)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @10)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @11)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 6 to ptr), ptr @12)
  call void @__quantum__rt__bool_record_output(i1 %var_77, ptr @13)
  ret i64 0
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
