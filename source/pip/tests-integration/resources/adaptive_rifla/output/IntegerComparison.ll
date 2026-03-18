@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1b\00"
@3 = internal constant [6 x i8] c"3_t2b\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_0 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_0
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_2, label %block_1, label %block_2
block_1:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  store i64 1, ptr %var_0
  br label %block_2
block_2:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_4, label %block_3, label %block_4
block_3:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_52 = load i64, ptr %var_0
  %var_6 = add i64 %var_52, 1
  store i64 %var_6, ptr %var_0
  br label %block_4
block_4:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_7 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  br i1 %var_7, label %block_5, label %block_6
block_5:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_50 = load i64, ptr %var_0
  %var_9 = add i64 %var_50, 1
  store i64 %var_9, ptr %var_0
  br label %block_6
block_6:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
  br i1 %var_10, label %block_7, label %block_8
block_7:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_48 = load i64, ptr %var_0
  %var_12 = add i64 %var_48, 1
  store i64 %var_12, ptr %var_0
  br label %block_8
block_8:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 4 to ptr))
  %var_13 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 4 to ptr))
  br i1 %var_13, label %block_9, label %block_10
block_9:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_46 = load i64, ptr %var_0
  %var_15 = add i64 %var_46, 1
  store i64 %var_15, ptr %var_0
  br label %block_10
block_10:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 5 to ptr))
  %var_16 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 5 to ptr))
  br i1 %var_16, label %block_11, label %block_12
block_11:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_44 = load i64, ptr %var_0
  %var_18 = add i64 %var_44, 1
  store i64 %var_18, ptr %var_0
  br label %block_12
block_12:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 6 to ptr))
  %var_19 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 6 to ptr))
  br i1 %var_19, label %block_13, label %block_14
block_13:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_42 = load i64, ptr %var_0
  %var_21 = add i64 %var_42, 1
  store i64 %var_21, ptr %var_0
  br label %block_14
block_14:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 7 to ptr))
  %var_22 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 7 to ptr))
  br i1 %var_22, label %block_15, label %block_16
block_15:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_40 = load i64, ptr %var_0
  %var_24 = add i64 %var_40, 1
  store i64 %var_24, ptr %var_0
  br label %block_16
block_16:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 8 to ptr))
  %var_25 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 8 to ptr))
  br i1 %var_25, label %block_17, label %block_18
block_17:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_38 = load i64, ptr %var_0
  %var_27 = add i64 %var_38, 1
  store i64 %var_27, ptr %var_0
  br label %block_18
block_18:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 9 to ptr))
  %var_28 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 9 to ptr))
  br i1 %var_28, label %block_19, label %block_20
block_19:
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  %var_36 = load i64, ptr %var_0
  %var_30 = add i64 %var_36, 1
  store i64 %var_30, ptr %var_0
  br label %block_20
block_20:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  %var_35 = load i64, ptr %var_0
  %var_31 = icmp sgt i64 %var_35, 5
  %var_32 = icmp slt i64 %var_35, 5
  %var_33 = icmp eq i64 %var_35, 10
  call void @__quantum__rt__tuple_record_output(i64 3, ptr @0)
  call void @__quantum__rt__bool_record_output(i1 %var_31, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 %var_32, ptr @2)
  call void @__quantum__rt__bool_record_output(i1 %var_33, ptr @3)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="10" }
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
