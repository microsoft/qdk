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
  %var_4 = alloca i64
  %var_9 = alloca i64
  %var_15 = alloca i64
  %var_20 = alloca i64
  %var_28 = alloca i64
  %var_33 = alloca i64
  %var_38 = alloca i64
  %var_43 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__z__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_4
  br label %block_1
block_1:
  %var_49 = load i64, ptr %var_4
  %var_5 = icmp slt i64 %var_49, 2
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_85 = load i64, ptr %var_4
  %var_6 = getelementptr ptr, ptr @array0, i64 %var_85
  %var_86 = load ptr, ptr %var_6
  call void @__quantum__qis__x__body(ptr %var_86)
  %var_8 = add i64 %var_85, 1
  store i64 %var_8, ptr %var_4
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
  store i64 1, ptr %var_9
  br label %block_4
block_4:
  %var_51 = load i64, ptr %var_9
  %var_10 = icmp sge i64 %var_51, 0
  br i1 %var_10, label %block_5, label %block_6
block_5:
  %var_82 = load i64, ptr %var_9
  %var_11 = getelementptr ptr, ptr @array0, i64 %var_82
  %var_83 = load ptr, ptr %var_11
  call void @__quantum__qis__x__body(ptr %var_83)
  %var_13 = add i64 %var_82, -1
  store i64 %var_13, ptr %var_9
  br label %block_4
block_6:
  store i64 0, ptr %var_15
  br label %block_7
block_7:
  %var_53 = load i64, ptr %var_15
  %var_16 = icmp slt i64 %var_53, 2
  br i1 %var_16, label %block_8, label %block_9
block_8:
  %var_79 = load i64, ptr %var_15
  %var_17 = getelementptr ptr, ptr @array0, i64 %var_79
  %var_80 = load ptr, ptr %var_17
  call void @__quantum__qis__x__body(ptr %var_80)
  %var_19 = add i64 %var_79, 1
  store i64 %var_19, ptr %var_15
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
  store i64 1, ptr %var_20
  br label %block_10
block_10:
  %var_55 = load i64, ptr %var_20
  %var_21 = icmp sge i64 %var_55, 0
  br i1 %var_21, label %block_11, label %block_12
block_11:
  %var_76 = load i64, ptr %var_20
  %var_22 = getelementptr ptr, ptr @array0, i64 %var_76
  %var_77 = load ptr, ptr %var_22
  call void @__quantum__qis__x__body(ptr %var_77)
  %var_24 = add i64 %var_76, -1
  store i64 %var_24, ptr %var_20
  br label %block_10
block_12:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_28
  br label %block_13
block_13:
  %var_57 = load i64, ptr %var_28
  %var_29 = icmp slt i64 %var_57, 2
  br i1 %var_29, label %block_14, label %block_15
block_14:
  %var_73 = load i64, ptr %var_28
  %var_30 = getelementptr ptr, ptr @array0, i64 %var_73
  %var_74 = load ptr, ptr %var_30
  call void @__quantum__qis__reset__body(ptr %var_74)
  %var_32 = add i64 %var_73, 1
  store i64 %var_32, ptr %var_28
  br label %block_13
block_15:
  store i64 0, ptr %var_33
  br label %block_16
block_16:
  %var_59 = load i64, ptr %var_33
  %var_34 = icmp slt i64 %var_59, 2
  br i1 %var_34, label %block_17, label %block_18
block_17:
  %var_70 = load i64, ptr %var_33
  %var_35 = getelementptr ptr, ptr @array1, i64 %var_70
  %var_71 = load ptr, ptr %var_35
  call void @__quantum__qis__reset__body(ptr %var_71)
  %var_37 = add i64 %var_70, 1
  store i64 %var_37, ptr %var_33
  br label %block_16
block_18:
  store i64 0, ptr %var_38
  br label %block_19
block_19:
  %var_61 = load i64, ptr %var_38
  %var_39 = icmp slt i64 %var_61, 2
  br i1 %var_39, label %block_20, label %block_21
block_20:
  %var_67 = load i64, ptr %var_38
  %var_40 = getelementptr ptr, ptr @array2, i64 %var_67
  %var_68 = load ptr, ptr %var_40
  call void @__quantum__qis__reset__body(ptr %var_68)
  %var_42 = add i64 %var_67, 1
  store i64 %var_42, ptr %var_38
  br label %block_19
block_21:
  store i64 0, ptr %var_43
  br label %block_22
block_22:
  %var_63 = load i64, ptr %var_43
  %var_44 = icmp slt i64 %var_63, 2
  br i1 %var_44, label %block_23, label %block_24
block_23:
  %var_64 = load i64, ptr %var_43
  %var_45 = getelementptr ptr, ptr @array3, i64 %var_64
  %var_65 = load ptr, ptr %var_45
  call void @__quantum__qis__reset__body(ptr %var_65)
  %var_47 = add i64 %var_64, 1
  store i64 %var_47, ptr %var_43
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
