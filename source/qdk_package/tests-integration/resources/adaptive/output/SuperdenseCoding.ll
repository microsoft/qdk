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
  %var_6 = alloca i1
  %var_12 = alloca i1
  %var_21 = alloca i1
  %var_22 = alloca i1
  %var_23 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_3 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_3, ptr %var_6
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  %var_9 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  store i1 %var_9, ptr %var_12
  %var_30 = load i1, ptr %var_6
  br i1 %var_30, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
  br label %block_2
block_2:
  %var_31 = load i1, ptr %var_12
  br i1 %var_31, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  br label %block_4
block_4:
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_14 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  %var_18 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  store i1 %var_14, ptr %var_21
  store i1 %var_18, ptr %var_22
  store i64 0, ptr %var_23
  br label %block_5
block_5:
  %var_35 = load i64, ptr %var_23
  %var_24 = icmp slt i64 %var_35, 2
  br i1 %var_24, label %block_6, label %block_7
block_6:
  %var_40 = load i64, ptr %var_23
  %var_25 = getelementptr ptr, ptr @array0, i64 %var_40
  %var_41 = load ptr, ptr %var_25
  call void @__quantum__qis__reset__body(ptr %var_41)
  %var_27 = add i64 %var_40, 1
  store i64 %var_27, ptr %var_23
  br label %block_5
block_7:
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @1)
  %var_36 = load i1, ptr %var_6
  call void @__quantum__rt__bool_record_output(i1 %var_36, ptr @2)
  %var_37 = load i1, ptr %var_12
  call void @__quantum__rt__bool_record_output(i1 %var_37, ptr @3)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @4)
  %var_38 = load i1, ptr %var_21
  call void @__quantum__rt__bool_record_output(i1 %var_38, ptr @5)
  %var_39 = load i1, ptr %var_22
  call void @__quantum__rt__bool_record_output(i1 %var_39, ptr @6)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="4" }
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
