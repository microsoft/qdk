%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*)) 12
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*)) 13
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*)) 15
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*)) 17
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 0 to %Qubit*)) 18
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*)) 19
  call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 0 to %Qubit*)) 20
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 1, !"int_computations", !"i64"}
!5 = !{i32 1, !"float_computations", !"f64"}

; all callables - these can be generated up front, but no
; guarantee they'll all be referenced by actual DILocations
!6 = subprogram Main scope
!7 = subprogram Bar scope
!8 = subprogram Foo scope

; entrypoint lines don't get inlinedAt metadata
; dump one DILocation per instruction we generate as we partial-eval
!9 = DILocation line 5 scope !6
!10 = DILocation line 6 scope !6
!11 = DILocation line 7 scope !6

; these get generated as we execute real function calls
; inlinedAt is going to be the caller location
!12 = DILocation line 19 scope !8 inlinedAt !9
!13 = DILocation line 19 scope !8 inlinedAt !10
!14 = DILocation line 11 scope !7 inlinedAt !11
!15 = DILocation line 19 scope !8 inlinedAt !14

; loops are always going to be LexicalBlockFiles
; iteration 1
!16 = lexicalblockfile line 12 scope !7 discriminator 1
!17 = DILocation line 13 scope !16
!18 = DILocation line 14 scope !16
; iteration 2
!18 = lexicalblockfile line 12 scope !7 discriminator 2
!19 = DILocation line 13 scope !18
!20 = DILocation line 14 scope !18
