@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [8 x i8] c"4_t0a2r\00"
@5 = internal constant [8 x i8] c"5_t0a3r\00"
@6 = internal constant [6 x i8] c"6_t1a\00"
@7 = internal constant [8 x i8] c"7_t1a0r\00"
@8 = internal constant [8 x i8] c"8_t1a1r\00"
@9 = internal constant [8 x i8] c"9_t1a2r\00"
@10 = internal constant [9 x i8] c"10_t1a3r\00"
@array0 = internal constant [4 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_4 = alloca i64
  %var_5 = alloca i64
  %var_9 = alloca i64
  %var_15 = alloca i64
  %var_16 = alloca i64
  %var_21 = alloca i64
  %var_24 = alloca i64
  %var_25 = alloca i64
  %var_30 = alloca i64
  %var_34 = alloca i64
  %var_35 = alloca i64
  %var_39 = alloca i64
  %var_41 = alloca i64
  %var_42 = alloca i64
  %var_46 = alloca i64
  %var_49 = alloca i64
  %var_50 = alloca i64
  %var_54 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 4 to ptr))
  call void @H(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_4
  br label %block_1
block_1:
  %var_57 = load i64, ptr %var_4
  store i64 %var_57, ptr %var_5
  %var_59 = load i64, ptr %var_5
  %var_6 = icmp slt i64 %var_59, 4
  br i1 %var_6, label %block_2, label %block_3
block_2:
  %var_105 = load i64, ptr %var_4
  %var_106_offset_chk = icmp slt i64 %var_105, 0
  %var_106_offset = select i1 %var_106_offset_chk, i64 1, i64 0
  %var_106 = getelementptr [4 x ptr], ptr @array0, i64 %var_106_offset, i64 %var_105
  %var_7 = load ptr, ptr %var_106
  call void @H(ptr %var_7)
  store i64 %var_105, ptr %var_9
  %var_108 = load i64, ptr %var_9
  %var_10 = add i64 %var_108, 1
  store i64 %var_10, ptr %var_4
  br label %block_1
block_3:
  call void @CX(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_15
  br label %block_4
block_4:
  %var_61 = load i64, ptr %var_15
  store i64 %var_61, ptr %var_16
  %var_63 = load i64, ptr %var_16
  %var_17 = icmp sge i64 %var_63, 0
  br i1 %var_17, label %block_5, label %block_6
block_5:
  %var_100 = load i64, ptr %var_15
  %var_101_offset_chk = icmp slt i64 %var_100, 0
  %var_101_offset = select i1 %var_101_offset_chk, i64 1, i64 0
  %var_101 = getelementptr [4 x ptr], ptr @array0, i64 %var_101_offset, i64 %var_100
  %var_18 = load ptr, ptr %var_101
  call void @H__Adj(ptr %var_18)
  store i64 %var_100, ptr %var_21
  %var_103 = load i64, ptr %var_21
  %var_22 = add i64 %var_103, -1
  store i64 %var_22, ptr %var_15
  br label %block_4
block_6:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_24
  br label %block_7
block_7:
  %var_65 = load i64, ptr %var_24
  store i64 %var_65, ptr %var_25
  %var_67 = load i64, ptr %var_25
  %var_26 = icmp slt i64 %var_67, 4
  br i1 %var_26, label %block_8, label %block_9
block_8:
  %var_95 = load i64, ptr %var_24
  %var_96_offset_chk = icmp slt i64 %var_95, 0
  %var_96_offset = select i1 %var_96_offset_chk, i64 1, i64 0
  %var_96 = getelementptr [4 x ptr], ptr @array0, i64 %var_96_offset, i64 %var_95
  %var_27 = load ptr, ptr %var_96
  call void @Reset(ptr %var_27)
  store i64 %var_95, ptr %var_30
  %var_98 = load i64, ptr %var_30
  %var_31 = add i64 %var_98, 1
  store i64 %var_31, ptr %var_24
  br label %block_7
block_9:
  call void @Reset(ptr inttoptr (i64 4 to ptr))
  call void @X(ptr inttoptr (i64 4 to ptr))
  call void @H(ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_34
  br label %block_10
block_10:
  %var_69 = load i64, ptr %var_34
  store i64 %var_69, ptr %var_35
  %var_71 = load i64, ptr %var_35
  %var_36 = icmp slt i64 %var_71, 4
  br i1 %var_36, label %block_11, label %block_12
block_11:
  %var_90 = load i64, ptr %var_34
  %var_91_offset_chk = icmp slt i64 %var_90, 0
  %var_91_offset = select i1 %var_91_offset_chk, i64 1, i64 0
  %var_91 = getelementptr [4 x ptr], ptr @array0, i64 %var_91_offset, i64 %var_90
  %var_37 = load ptr, ptr %var_91
  call void @H(ptr %var_37)
  store i64 %var_90, ptr %var_39
  %var_93 = load i64, ptr %var_39
  %var_40 = add i64 %var_93, 1
  store i64 %var_40, ptr %var_34
  br label %block_10
block_12:
  call void @X(ptr inttoptr (i64 4 to ptr))
  store i64 3, ptr %var_41
  br label %block_13
block_13:
  %var_73 = load i64, ptr %var_41
  store i64 %var_73, ptr %var_42
  %var_75 = load i64, ptr %var_42
  %var_43 = icmp sge i64 %var_75, 0
  br i1 %var_43, label %block_14, label %block_15
block_14:
  %var_85 = load i64, ptr %var_41
  %var_86_offset_chk = icmp slt i64 %var_85, 0
  %var_86_offset = select i1 %var_86_offset_chk, i64 1, i64 0
  %var_86 = getelementptr [4 x ptr], ptr @array0, i64 %var_86_offset, i64 %var_85
  %var_44 = load ptr, ptr %var_86
  call void @H__Adj(ptr %var_44)
  store i64 %var_85, ptr %var_46
  %var_88 = load i64, ptr %var_46
  %var_47 = add i64 %var_88, -1
  store i64 %var_47, ptr %var_41
  br label %block_13
block_15:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  store i64 0, ptr %var_49
  br label %block_16
block_16:
  %var_77 = load i64, ptr %var_49
  store i64 %var_77, ptr %var_50
  %var_79 = load i64, ptr %var_50
  %var_51 = icmp slt i64 %var_79, 4
  br i1 %var_51, label %block_17, label %block_18
block_17:
  %var_80 = load i64, ptr %var_49
  %var_81_offset_chk = icmp slt i64 %var_80, 0
  %var_81_offset = select i1 %var_81_offset_chk, i64 1, i64 0
  %var_81 = getelementptr [4 x ptr], ptr @array0, i64 %var_81_offset, i64 %var_80
  %var_52 = load ptr, ptr %var_81
  call void @Reset(ptr %var_52)
  store i64 %var_80, ptr %var_54
  %var_83 = load i64, ptr %var_54
  %var_55 = add i64 %var_83, 1
  store i64 %var_55, ptr %var_49
  br label %block_16
block_18:
  call void @Reset(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__array_record_output(i64 4, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @5)
  call void @__quantum__rt__array_record_output(i64 4, ptr @6)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @7)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 6 to ptr), ptr @9)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 7 to ptr), ptr @10)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_2) {
block_19:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @H(ptr %var_3) {
block_20:
  call void @__quantum__qis__h__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @CX(ptr %var_11, ptr %var_12) {
block_21:
  call void @__quantum__qis__cx__body(ptr %var_11, ptr %var_12)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define internal void @H__Adj(ptr %var_20) {
block_22:
  call void @__quantum__qis__h__body(ptr %var_20)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_29) {
block_23:
  call void @__quantum__qis__reset__body(ptr %var_29)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="8" }
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
