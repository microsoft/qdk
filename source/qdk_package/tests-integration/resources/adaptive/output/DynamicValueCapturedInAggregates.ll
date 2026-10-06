@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0i\00"
@2 = internal constant [6 x i8] c"2_t1t\00"
@3 = internal constant [8 x i8] c"3_t1t0i\00"
@4 = internal constant [8 x i8] c"4_t1t1i\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_7 = alloca i64
  %var_8 = alloca i64
  %var_10 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_1
  store i64 0, ptr %var_2
  store i64 3, ptr %var_3
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  %var_4 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_4, label %block_1, label %block_2
block_1:
  store i64 2, ptr %var_1
  store i64 4, ptr %var_2
  br label %block_2
block_2:
  %var_14 = load i64, ptr %var_1
  store i64 %var_14, ptr %var_7
  %var_16 = load i64, ptr %var_2
  store i64 %var_16, ptr %var_8
  store i64 3, ptr %var_2
  %var_19 = load i64, ptr %var_8
  store i64 %var_19, ptr %var_3
  store i64 7, ptr %var_1
  %var_22 = load i64, ptr %var_3
  store i64 %var_22, ptr %var_10
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  %var_24 = load i64, ptr %var_7
  call void @__quantum__rt__int_record_output(i64 %var_24, ptr @1)
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @2)
  call void @__quantum__rt__int_record_output(i64 3, ptr @3)
  %var_25 = load i64, ptr %var_10
  call void @__quantum__rt__int_record_output(i64 %var_25, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_0) {
block_3:
  call void @__quantum__qis__x__body(ptr %var_0)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare zeroext i1 @__quantum__rt__read_result(ptr) #2

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
