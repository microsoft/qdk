@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@3 = internal constant [6 x i8] c"3_t2i\00"
@4 = internal constant [6 x i8] c"4_t3i\00"
@array0 = internal constant [5 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr)]
@array1 = internal constant [5 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_4 = alloca i64
  %var_6 = alloca i64
  %var_13 = alloca i64
  %var_24 = alloca i64
  %var_30 = alloca i64
  %var_31 = alloca i64
  %var_32 = alloca i64
  %var_33 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  store i64 0, ptr %var_2
  store i64 10, ptr %var_3
  store i64 1, ptr %var_4
  store i64 0, ptr %var_6
  br label %block_1
block_1:
  %var_39 = load i64, ptr %var_6
  %var_7 = icmp slt i64 %var_39, 5
  br i1 %var_7, label %block_2, label %block_3
block_2:
  %var_71 = load i64, ptr %var_6
  %var_72_offset_chk = icmp slt i64 %var_71, 0
  %var_72_offset = select i1 %var_72_offset_chk, i64 1, i64 0
  %var_72 = getelementptr [5 x ptr], ptr @array0, i64 %var_72_offset, i64 %var_71
  %var_8 = load ptr, ptr %var_72
  call void @X(ptr %var_8)
  %var_11 = add i64 %var_71, 1
  store i64 %var_11, ptr %var_6
  br label %block_1
block_3:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  store i64 0, ptr %var_13
  br label %block_4
block_4:
  %var_41 = load i64, ptr %var_13
  %var_14 = icmp slt i64 %var_41, 5
  br i1 %var_14, label %block_5, label %block_6
block_5:
  %var_59 = load i64, ptr %var_13
  %var_60_offset_chk = icmp slt i64 %var_59, 0
  %var_60_offset = select i1 %var_60_offset_chk, i64 1, i64 0
  %var_60 = getelementptr [5 x ptr], ptr @array1, i64 %var_60_offset, i64 %var_59
  %var_15 = load ptr, ptr %var_60
  %var_17 = call i1 @__quantum__rt__read_result(ptr %var_15)
  br i1 %var_17, label %block_7, label %block_9
block_6:
  store i64 0, ptr %var_24
  br label %block_8
block_7:
  %var_63 = load i64, ptr %var_1
  %var_19 = add i64 %var_63, 1
  store i64 %var_19, ptr %var_1
  %var_65 = load i64, ptr %var_2
  %var_20 = add i64 %var_65, 5
  store i64 %var_20, ptr %var_2
  %var_67 = load i64, ptr %var_3
  %var_21 = sub i64 %var_67, 2
  store i64 %var_21, ptr %var_3
  %var_69 = load i64, ptr %var_4
  %var_22 = mul i64 %var_69, 3
  store i64 %var_22, ptr %var_4
  br label %block_9
block_8:
  %var_43 = load i64, ptr %var_24
  %var_25 = icmp slt i64 %var_43, 5
  br i1 %var_25, label %block_10, label %block_11
block_9:
  %var_61 = load i64, ptr %var_13
  %var_23 = add i64 %var_61, 1
  store i64 %var_23, ptr %var_13
  br label %block_4
block_10:
  %var_56 = load i64, ptr %var_24
  %var_57_offset_chk = icmp slt i64 %var_56, 0
  %var_57_offset = select i1 %var_57_offset_chk, i64 1, i64 0
  %var_57 = getelementptr [5 x ptr], ptr @array0, i64 %var_57_offset, i64 %var_56
  %var_26 = load ptr, ptr %var_57
  call void @Reset(ptr %var_26)
  %var_29 = add i64 %var_56, 1
  store i64 %var_29, ptr %var_24
  br label %block_8
block_11:
  %var_44 = load i64, ptr %var_1
  store i64 %var_44, ptr %var_30
  %var_46 = load i64, ptr %var_2
  store i64 %var_46, ptr %var_31
  %var_48 = load i64, ptr %var_3
  store i64 %var_48, ptr %var_32
  %var_50 = load i64, ptr %var_4
  store i64 %var_50, ptr %var_33
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_52 = load i64, ptr %var_30
  call void @__quantum__rt__int_record_output(i64 %var_52, ptr @1)
  %var_53 = load i64, ptr %var_31
  call void @__quantum__rt__int_record_output(i64 %var_53, ptr @2)
  %var_54 = load i64, ptr %var_32
  call void @__quantum__rt__int_record_output(i64 %var_54, ptr @3)
  %var_55 = load i64, ptr %var_33
  call void @__quantum__rt__int_record_output(i64 %var_55, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_10) {
block_12:
  call void @__quantum__qis__x__body(ptr %var_10)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr) #2

define internal void @Reset(ptr %var_28) {
block_13:
  call void @__quantum__qis__reset__body(ptr %var_28)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
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
