%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double -0.6579630871775028, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double -0.060868078845781784, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.20724538589718786, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rzz__body(double 0.4144907717943757, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__rx__body(double 0.10362269294859393, %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__rt__array_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rx__body(double, %Qubit*)

declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="4" "required_num_results"="4" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
