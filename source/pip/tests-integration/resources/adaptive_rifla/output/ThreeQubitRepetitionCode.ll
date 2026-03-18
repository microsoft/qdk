@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1i\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_10 = alloca i1
  %var_25 = alloca i1
  %var_41 = alloca i1
  %var_57 = alloca i1
  %var_73 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_1
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 1 to ptr))
  store i1 true, ptr %var_10
  %var_11 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_11, label %block_1, label %block_2
block_1:
  %var_13 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_13, label %block_3, label %block_4
block_2:
  %var_15 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_15, label %block_5, label %block_6
block_3:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %block_7
block_4:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_7
block_5:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  br label %block_8
block_6:
  store i1 false, ptr %var_10
  br label %block_8
block_7:
  br label %block_9
block_8:
  br label %block_9
block_9:
  %var_86 = load i1, ptr %var_10
  br i1 %var_86, label %block_10, label %block_11
block_10:
  store i64 1, ptr %var_1
  br label %block_11
block_11:
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 3 to ptr))
  store i1 true, ptr %var_25
  %var_26 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  br i1 %var_26, label %block_12, label %block_13
block_12:
  %var_28 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_28, label %block_14, label %block_15
block_13:
  %var_30 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_30, label %block_16, label %block_17
block_14:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %block_18
block_15:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_18
block_16:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  br label %block_19
block_17:
  store i1 false, ptr %var_25
  br label %block_19
block_18:
  br label %block_20
block_19:
  br label %block_20
block_20:
  %var_89 = load i1, ptr %var_25
  br i1 %var_89, label %block_21, label %block_22
block_21:
  %var_106 = load i64, ptr %var_1
  %var_33 = add i64 %var_106, 1
  store i64 %var_33, ptr %var_1
  br label %block_22
block_22:
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr))
  store i1 true, ptr %var_41
  %var_42 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  br i1 %var_42, label %block_23, label %block_24
block_23:
  %var_44 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  br i1 %var_44, label %block_25, label %block_26
block_24:
  %var_46 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  br i1 %var_46, label %block_27, label %block_28
block_25:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %block_29
block_26:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_29
block_27:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  br label %block_30
block_28:
  store i1 false, ptr %var_41
  br label %block_30
block_29:
  br label %block_31
block_30:
  br label %block_31
block_31:
  %var_92 = load i1, ptr %var_41
  br i1 %var_92, label %block_32, label %block_33
block_32:
  %var_104 = load i64, ptr %var_1
  %var_49 = add i64 %var_104, 1
  store i64 %var_49, ptr %var_1
  br label %block_33
block_33:
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 7 to ptr))
  store i1 true, ptr %var_57
  %var_58 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 6 to ptr))
  br i1 %var_58, label %block_34, label %block_35
block_34:
  %var_60 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  br i1 %var_60, label %block_36, label %block_37
block_35:
  %var_62 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  br i1 %var_62, label %block_38, label %block_39
block_36:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %block_40
block_37:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_40
block_38:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  br label %block_41
block_39:
  store i1 false, ptr %var_57
  br label %block_41
block_40:
  br label %block_42
block_41:
  br label %block_42
block_42:
  %var_95 = load i1, ptr %var_57
  br i1 %var_95, label %block_43, label %block_44
block_43:
  %var_102 = load i64, ptr %var_1
  %var_65 = add i64 %var_102, 1
  store i64 %var_65, ptr %var_1
  br label %block_44
block_44:
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 8 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 9 to ptr))
  store i1 true, ptr %var_73
  %var_74 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 8 to ptr))
  br i1 %var_74, label %block_45, label %block_46
block_45:
  %var_76 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 9 to ptr))
  br i1 %var_76, label %block_47, label %block_48
block_46:
  %var_78 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 9 to ptr))
  br i1 %var_78, label %block_49, label %block_50
block_47:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
  br label %block_51
block_48:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_51
block_49:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
  br label %block_52
block_50:
  store i1 false, ptr %var_73
  br label %block_52
block_51:
  br label %block_53
block_52:
  br label %block_53
block_53:
  %var_98 = load i1, ptr %var_73
  br i1 %var_98, label %block_54, label %block_55
block_54:
  %var_100 = load i64, ptr %var_1
  %var_81 = add i64 %var_100, 1
  store i64 %var_81, ptr %var_1
  br label %block_55
block_55:
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 10 to ptr))
  %var_82 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 10 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__bool_record_output(i1 %var_82, ptr @1)
  %var_99 = load i64, ptr %var_1
  call void @__quantum__rt__int_record_output(i64 %var_99, ptr @2)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="11" }
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
