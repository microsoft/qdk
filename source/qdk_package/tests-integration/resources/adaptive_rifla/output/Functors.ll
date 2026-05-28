@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1a\00"
@5 = internal constant [8 x i8] c"5_t1a0r\00"
@6 = internal constant [8 x i8] c"6_t1a1r\00"
@7 = internal constant [6 x i8] c"7_t2a\00"
@8 = internal constant [8 x i8] c"8_t2a0r\00"
@9 = internal constant [8 x i8] c"9_t2a1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array2 = internal constant [2 x ptr] [ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr)]
@array3 = internal constant [2 x ptr] [ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 7 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_3 = alloca i64
  %var_8 = alloca i64
  %var_14 = alloca i64
  %var_19 = alloca i64
  %var_27 = alloca i64
  %var_32 = alloca i64
  %var_37 = alloca i64
  %var_42 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_3
  br label %block_1
block_1:
  %var_48 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_48, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_84 = load i64, ptr %var_3
  %var_5 = getelementptr ptr, ptr @array0, i64 %var_84
  %var_85 = load ptr, ptr %var_5
  call void @__quantum__qis__x__body(ptr %var_85)
  %var_7 = add i64 %var_84, 1
  store i64 %var_7, ptr %var_3
  br label %block_1
block_3:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__s__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__t__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__t__adj(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__s__adj(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 5 to ptr))
  store i64 1, ptr %var_8
  br label %block_4
block_4:
  %var_50 = load i64, ptr %var_8
  %var_9 = icmp sge i64 %var_50, 0
  br i1 %var_9, label %block_5, label %block_6
block_5:
  %var_81 = load i64, ptr %var_8
  %var_10 = getelementptr ptr, ptr @array0, i64 %var_81
  %var_82 = load ptr, ptr %var_10
  call void @__quantum__qis__x__body(ptr %var_82)
  %var_12 = add i64 %var_81, -1
  store i64 %var_12, ptr %var_8
  br label %block_4
block_6:
  store i64 0, ptr %var_14
  br label %block_7
block_7:
  %var_52 = load i64, ptr %var_14
  %var_15 = icmp slt i64 %var_52, 2
  br i1 %var_15, label %block_8, label %block_9
block_8:
  %var_78 = load i64, ptr %var_14
  %var_16 = getelementptr ptr, ptr @array0, i64 %var_78
  %var_79 = load ptr, ptr %var_16
  call void @__quantum__qis__x__body(ptr %var_79)
  %var_18 = add i64 %var_78, 1
  store i64 %var_18, ptr %var_14
  br label %block_7
block_9:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__s__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__t__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__t__adj(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__s__adj(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__s__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__t__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__t__adj(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__s__adj(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  store i64 1, ptr %var_19
  br label %block_10
block_10:
  %var_54 = load i64, ptr %var_19
  %var_20 = icmp sge i64 %var_54, 0
  br i1 %var_20, label %block_11, label %block_12
block_11:
  %var_75 = load i64, ptr %var_19
  %var_21 = getelementptr ptr, ptr @array0, i64 %var_75
  %var_76 = load ptr, ptr %var_21
  call void @__quantum__qis__x__body(ptr %var_76)
  %var_23 = add i64 %var_75, -1
  store i64 %var_23, ptr %var_19
  br label %block_10
block_12:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_27
  br label %block_13
block_13:
  %var_56 = load i64, ptr %var_27
  %var_28 = icmp slt i64 %var_56, 2
  br i1 %var_28, label %block_14, label %block_15
block_14:
  %var_72 = load i64, ptr %var_27
  %var_29 = getelementptr ptr, ptr @array0, i64 %var_72
  %var_73 = load ptr, ptr %var_29
  call void @__quantum__qis__reset__body(ptr %var_73)
  %var_31 = add i64 %var_72, 1
  store i64 %var_31, ptr %var_27
  br label %block_13
block_15:
  store i64 0, ptr %var_32
  br label %block_16
block_16:
  %var_58 = load i64, ptr %var_32
  %var_33 = icmp slt i64 %var_58, 2
  br i1 %var_33, label %block_17, label %block_18
block_17:
  %var_69 = load i64, ptr %var_32
  %var_34 = getelementptr ptr, ptr @array1, i64 %var_69
  %var_70 = load ptr, ptr %var_34
  call void @__quantum__qis__reset__body(ptr %var_70)
  %var_36 = add i64 %var_69, 1
  store i64 %var_36, ptr %var_32
  br label %block_16
block_18:
  store i64 0, ptr %var_37
  br label %block_19
block_19:
  %var_60 = load i64, ptr %var_37
  %var_38 = icmp slt i64 %var_60, 2
  br i1 %var_38, label %block_20, label %block_21
block_20:
  %var_66 = load i64, ptr %var_37
  %var_39 = getelementptr ptr, ptr @array2, i64 %var_66
  %var_67 = load ptr, ptr %var_39
  call void @__quantum__qis__reset__body(ptr %var_67)
  %var_41 = add i64 %var_66, 1
  store i64 %var_41, ptr %var_37
  br label %block_19
block_21:
  store i64 0, ptr %var_42
  br label %block_22
block_22:
  %var_62 = load i64, ptr %var_42
  %var_43 = icmp slt i64 %var_62, 2
  br i1 %var_43, label %block_23, label %block_24
block_23:
  %var_63 = load i64, ptr %var_42
  %var_44 = getelementptr ptr, ptr @array3, i64 %var_63
  %var_64 = load ptr, ptr %var_44
  call void @__quantum__qis__reset__body(ptr %var_64)
  %var_46 = add i64 %var_63, 1
  store i64 %var_46, ptr %var_42
  br label %block_22
block_24:
  call void @__quantum__rt__tuple_record_output(i64 3, ptr @0)
  call void @__quantum__rt__array_record_output(i64 2, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__array_record_output(i64 2, ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @6)
  call void @__quantum__rt__array_record_output(i64 2, ptr @7)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @9)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

declare void @__quantum__qis__s__body(ptr)

declare void @__quantum__qis__t__body(ptr)

declare void @__quantum__qis__t__adj(ptr)

declare void @__quantum__qis__s__adj(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="8" "required_num_results"="6" }
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
