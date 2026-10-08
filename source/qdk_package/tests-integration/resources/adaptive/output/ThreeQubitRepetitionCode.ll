@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_4 = alloca i64
  %var_9 = alloca i64
  %var_10 = alloca i64
  %var_13 = alloca i1
  %var_14 = alloca i64
  %var_15 = alloca i64
  %var_18 = alloca i1
  %var_19 = alloca i64
  %var_20 = alloca i64
  %var_29 = alloca i64
  %var_31 = alloca i64
  %var_35 = alloca i1
  %var_43 = alloca i1
  %var_44 = alloca i64
  %var_46 = alloca i64
  %var_54 = alloca i1
  %var_55 = alloca i64
  %var_56 = alloca i64
  %var_61 = alloca i64
  %var_63 = alloca i1
  %var_64 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @H(ptr inttoptr (i64 0 to ptr))
  call void @Z(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_4
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_9
  br label %block_1
block_1:
  %var_67 = load i64, ptr %var_9
  store i64 %var_67, ptr %var_10
  %var_69 = load i64, ptr %var_10
  %var_11 = icmp sle i64 %var_69, 5
  store i1 true, ptr %var_13
  br i1 %var_11, label %block_2, label %block_3
block_2:
  %var_72 = load i1, ptr %var_13
  br i1 %var_72, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_13
  br label %block_2
block_4:
  store i64 1, ptr %var_14
  br label %block_6
block_5:
  call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_52 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_52, ptr %var_54
  store i64 0, ptr %var_55
  br label %block_7
block_6:
  %var_90 = load i64, ptr %var_14
  store i64 %var_90, ptr %var_15
  %var_92 = load i64, ptr %var_15
  %var_16 = icmp sle i64 %var_92, 4
  store i1 true, ptr %var_18
  br i1 %var_16, label %block_8, label %block_9
block_7:
  %var_75 = load i64, ptr %var_55
  store i64 %var_75, ptr %var_56
  %var_77 = load i64, ptr %var_56
  %var_57 = icmp slt i64 %var_77, 2
  br i1 %var_57, label %block_10, label %block_11
block_8:
  %var_95 = load i1, ptr %var_18
  br i1 %var_95, label %block_12, label %block_13
block_9:
  store i1 false, ptr %var_18
  br label %block_8
block_10:
  %var_84 = load i64, ptr %var_55
  %var_85_offset_chk = icmp slt i64 %var_84, 0
  %var_85_offset = select i1 %var_85_offset_chk, i64 1, i64 0
  %var_85 = getelementptr [2 x ptr], ptr @array1, i64 %var_85_offset, i64 %var_84
  %var_58 = load ptr, ptr %var_85
  call void @Reset(ptr %var_58)
  store i64 %var_84, ptr %var_61
  %var_87 = load i64, ptr %var_61
  %var_62 = add i64 %var_87, 1
  store i64 %var_62, ptr %var_55
  br label %block_7
block_11:
  %var_78 = load i1, ptr %var_54
  store i1 %var_78, ptr %var_63
  %var_80 = load i64, ptr %var_4
  store i64 %var_80, ptr %var_64
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  %var_82 = load i1, ptr %var_63
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_82, ptr @1)
  %var_83 = load i64, ptr %var_64
  call void @__quantum__rt__int_record_output(i64 %var_83, ptr @2)
  ret i64 0
block_12:
  store i64 0, ptr %var_19
  br label %block_14
block_13:
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @CNOT(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @CNOT(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 1 to ptr))
  store i1 true, ptr %var_35
  %var_36 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_36, label %block_15, label %block_16
block_14:
  %var_110 = load i64, ptr %var_19
  store i64 %var_110, ptr %var_20
  %var_112 = load i64, ptr %var_20
  %var_21 = icmp slt i64 %var_112, 3
  br i1 %var_21, label %block_17, label %block_18
block_15:
  %var_38 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_38, label %block_19, label %block_20
block_16:
  %var_41 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_41, label %block_21, label %block_22
block_17:
  %var_117 = load i64, ptr %var_19
  %var_118_offset_chk = icmp slt i64 %var_117, 0
  %var_118_offset = select i1 %var_118_offset_chk, i64 1, i64 0
  %var_118 = getelementptr [3 x ptr], ptr @array0, i64 %var_118_offset, i64 %var_117
  %var_22 = load ptr, ptr %var_118
  call void @Rx(double 1.5707963267948966, ptr %var_22)
  store i64 %var_117, ptr %var_29
  %var_120 = load i64, ptr %var_29
  %var_30 = add i64 %var_120, 1
  store i64 %var_30, ptr %var_19
  br label %block_14
block_18:
  %var_113 = load i64, ptr %var_14
  store i64 %var_113, ptr %var_31
  %var_115 = load i64, ptr %var_31
  %var_32 = add i64 %var_115, 1
  store i64 %var_32, ptr %var_14
  br label %block_6
block_19:
  call void @X(ptr inttoptr (i64 1 to ptr))
  br label %block_23
block_20:
  call void @X(ptr inttoptr (i64 0 to ptr))
  br label %block_23
block_21:
  call void @X(ptr inttoptr (i64 2 to ptr))
  br label %block_24
block_22:
  store i1 false, ptr %var_35
  br label %block_24
block_23:
  br label %block_25
block_24:
  br label %block_25
block_25:
  %var_98 = load i1, ptr %var_35
  store i1 %var_98, ptr %var_43
  %var_100 = load i1, ptr %var_43
  br i1 %var_100, label %block_26, label %block_27
block_26:
  %var_105 = load i64, ptr %var_4
  store i64 %var_105, ptr %var_44
  %var_107 = load i64, ptr %var_44
  %var_45 = add i64 %var_107, 1
  store i64 %var_45, ptr %var_4
  br label %block_27
block_27:
  %var_101 = load i64, ptr %var_9
  store i64 %var_101, ptr %var_46
  %var_103 = load i64, ptr %var_46
  %var_47 = add i64 %var_103, 1
  store i64 %var_47, ptr %var_9
  br label %block_1
}

declare void @__quantum__rt__initialize(ptr)

define internal void @H(ptr %var_2) {
block_28:
  call void @__quantum__qis__h__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @Z(ptr %var_3) {
block_29:
  call void @__quantum__qis__z__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define internal void @CNOT(ptr %var_5, ptr %var_6) {
block_30:
  call void @__quantum__qis__cx__body(ptr %var_5, ptr %var_6)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define internal void @Rx(double %var_25, ptr %var_26) {
block_31:
  call void @__quantum__qis__rx__body(double %var_25, ptr %var_26)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @X(ptr %var_40) {
block_32:
  call void @__quantum__qis__x__body(ptr %var_40)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @CNOT__Adj(ptr %var_48, ptr %var_49) {
block_33:
  call void @__quantum__qis__cx__body(ptr %var_48, ptr %var_49)
  ret void
}

define internal void @Reset(ptr %var_60) {
block_34:
  call void @__quantum__qis__reset__body(ptr %var_60)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="3" }
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
