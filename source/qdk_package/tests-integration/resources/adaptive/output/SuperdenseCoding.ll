@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0t\00"
@2 = internal constant [8 x i8] c"2_t0t0b\00"
@3 = internal constant [8 x i8] c"3_t0t1b\00"
@4 = internal constant [6 x i8] c"4_t1t\00"
@5 = internal constant [8 x i8] c"5_t1t0b\00"
@6 = internal constant [8 x i8] c"6_t1t1b\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_15 = alloca i1
  %var_21 = alloca i1
  %var_22 = alloca i1
  %var_23 = alloca i1
  %var_40 = alloca i1
  %var_41 = alloca i1
  %var_42 = alloca i64
  %var_48 = alloca i1
  %var_49 = alloca i1
  %var_50 = alloca i1
  %var_51 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @CreateEntangledPair(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_12 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_12, ptr %var_15
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  %var_18 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 %var_18, ptr %var_21
  %var_54 = load i1, ptr %var_15
  store i1 %var_54, ptr %var_22
  %var_56 = load i1, ptr %var_21
  store i1 %var_56, ptr %var_23
  %var_58 = load i1, ptr %var_22
  %var_59 = load i1, ptr %var_23
  call void @SuperdenseEncode(i1 zeroext %var_58, i1 zeroext %var_59, ptr inttoptr (i64 0 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_31 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  %var_35 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  store i1 %var_31, ptr %var_40
  store i1 %var_35, ptr %var_41
  store i64 0, ptr %var_42
  br label %block_1
block_1:
  %var_63 = load i64, ptr %var_42
  %var_43 = icmp slt i64 %var_63, 2
  br i1 %var_43, label %block_2, label %block_3
block_2:
  %var_76 = load i64, ptr %var_42
  %var_77_offset_chk = icmp slt i64 %var_76, 0
  %var_77_offset = select i1 %var_77_offset_chk, i64 1, i64 0
  %var_77 = getelementptr [2 x ptr], ptr @array0, i64 %var_77_offset, i64 %var_76
  %var_44 = load ptr, ptr %var_77
  call void @Reset(ptr %var_44)
  %var_47 = add i64 %var_76, 1
  store i64 %var_47, ptr %var_42
  br label %block_1
block_3:
  %var_64 = load i1, ptr %var_15
  store i1 %var_64, ptr %var_48
  %var_66 = load i1, ptr %var_21
  store i1 %var_66, ptr %var_49
  %var_68 = load i1, ptr %var_40
  store i1 %var_68, ptr %var_50
  %var_70 = load i1, ptr %var_41
  store i1 %var_70, ptr %var_51
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @1)
  %var_72 = load i1, ptr %var_48
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_72, ptr @2)
  %var_73 = load i1, ptr %var_49
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_73, ptr @3)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @4)
  %var_74 = load i1, ptr %var_50
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_74, ptr @5)
  %var_75 = load i1, ptr %var_51
  call void @__quantum__rt__bool_record_output(i1 zeroext %var_75, ptr @6)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @CreateEntangledPair(ptr %var_1, ptr %var_2) {
block_4:
  call void @H(ptr %var_1)
  call void @CNOT(ptr %var_1, ptr %var_2)
  ret void
}

define internal void @H(ptr %var_3) {
block_5:
  call void @__quantum__qis__h__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @CNOT(ptr %var_6, ptr %var_7) {
block_6:
  call void @__quantum__qis__cx__body(ptr %var_6, ptr %var_7)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

define internal void @SuperdenseEncode(i1 zeroext %var_24, i1 zeroext %var_25, ptr %var_26) {
block_7:
  br i1 %var_24, label %block_8, label %block_9
block_8:
  call void @Z(ptr %var_26)
  br label %block_9
block_9:
  br i1 %var_25, label %block_10, label %block_11
block_10:
  call void @X(ptr %var_26)
  br label %block_11
block_11:
  ret void
}

define internal void @Z(ptr %var_27) {
block_12:
  call void @__quantum__qis__z__body(ptr %var_27)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define internal void @X(ptr %var_28) {
block_13:
  call void @__quantum__qis__x__body(ptr %var_28)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @H__Adj(ptr %var_30) {
block_14:
  call void @__quantum__qis__h__body(ptr %var_30)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

define internal void @Reset(ptr %var_46) {
block_15:
  call void @__quantum__qis__reset__body(ptr %var_46)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="4" }
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
