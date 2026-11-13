# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Writes the programs the `qir_gpu_resumer` example runs, as Adaptive
Profile bytecode (`AdaptiveProfilePass(Bytecode.Bit64)`) in JSON, into an
output directory:

- `programs.json`: the programs of the adaptive GPU tests
  (`source/qdk_package/tests/test_adaptive_gpu_quantum_ops.py`) and the
  bench workload `bench_<n>` for n in 4, 12, 16, 20, 22;
- `programs_noise.json`: the distance-d repetition codes `rep_<d>` for d in
  3, 5, 7, 9, 11, and two hand-written programs for the noise checks,
  `readout_branch` (readout noise read by a branch) and `corr_branch` (a
  correlated-noise intrinsic, table 0, then a branch).

Needs the `qdk` Python package built from this tree.
Usage: python3 dump_programs.py <output directory>
"""

import ast
import json
import os
import sys

from qdk import TargetProfile, qsharp
from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
from qdk.simulation._simulation import DecomposeCcxPass, preprocess_simulation_input

from workloads import bench_source, rep_source

TESTS = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    "..",
    "..",
    "qdk_package",
    "tests",
    "test_adaptive_gpu_quantum_ops.py",
)

HEAD = """%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_a\\00"
@1 = internal constant [6 x i8] c"1_a0r\\00"
@2 = internal constant [6 x i8] c"2_a1r\\00"
@3 = internal constant [6 x i8] c"3_a2r\\00"
"""

TAIL = """
declare void @__quantum__rt__initialize(i8*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
declare void @__quantum__rt__readout_noise(double, double, %Result*)
declare void @test_noise_intrinsic(%Qubit*, %Qubit*, %Qubit*) #2
declare void @__quantum__rt__array_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
attributes #1 = { "irreversible" }
attributes #2 = { "qdk_noise" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 1, !"backwards_branching", i2 3}
"""

RECORD = """  call void @__quantum__rt__array_record_output(i64 3, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  ret i64 0
"""

Q = lambda i: f"%Qubit* inttoptr (i64 {i} to %Qubit*)"
R = lambda i: f"%Result* inttoptr (i64 {i} to %Result*)"

# q0 in |+>, q1 in |1>; readout noise on both results; q2 flipped when the
# noisy r0 reads One, then measured. r1 is measured again after readout
# noise (a second mresetz on q1 reads Zero, then noise on the new value).
READOUT = (
    HEAD
    + f"""
define i64 @ENTRYPOINT__main() #0 {{
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body({Q(0)})
  call void @__quantum__qis__x__body({Q(1)})
  call void @__quantum__qis__mresetz__body({Q(0)}, {R(0)})
  call void @__quantum__qis__mresetz__body({Q(1)}, {R(1)})
  call void @__quantum__rt__readout_noise(double 0.2, double 0.1, {R(0)})
  call void @__quantum__rt__readout_noise(double 0.05, double 0.3, {R(1)})
  %c = call i1 @__quantum__rt__read_result({R(0)})
  br i1 %c, label %then, label %cont
then:
  call void @__quantum__qis__x__body({Q(2)})
  br label %cont
cont:
  call void @__quantum__qis__mresetz__body({Q(2)}, {R(2)})
  call void @__quantum__rt__readout_noise(double 0.1, double 0.25, {R(2)})
"""
    + RECORD
    + "}\n"
    + TAIL
)

# q0 in |+>, q1 in |1>, q0-q2 entangled; the correlated intrinsic, then a
# branch on r0 that flips q2 before its measurement.
CORR = (
    HEAD
    + f"""
define i64 @ENTRYPOINT__main() #0 {{
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body({Q(0)})
  call void @__quantum__qis__cx__body({Q(0)}, {Q(2)})
  call void @__quantum__qis__x__body({Q(1)})
  call void @test_noise_intrinsic({Q(0)}, {Q(1)}, {Q(2)})
  call void @__quantum__qis__h__body({Q(0)})
  call void @__quantum__qis__m__body({Q(0)}, {R(0)})
  %c = call i1 @__quantum__rt__read_result({R(0)})
  br i1 %c, label %then, label %cont
then:
  call void @__quantum__qis__x__body({Q(1)})
  br label %cont
cont:
  call void @test_noise_intrinsic({Q(2)}, {Q(1)}, {Q(0)})
  call void @__quantum__qis__m__body({Q(1)}, {R(1)})
  call void @__quantum__qis__m__body({Q(2)}, {R(2)})
"""
    + RECORD
    + "}\n"
    + TAIL
)

TEST_PROGRAMS = (
    "MEASURE_AND_CORRECT_QIR",
    "CONDITIONAL_LOOP_QIR",
    "LOOP_WITH_PHI_QIR",
    "BOOLEAN_COMPUTATION_QIR",
    "TELEPORT_CHAIN_QIR",
    "DYNAMIC_ROTATION_ANGLE_QIR",
)
TELEPORTED_T = ("TeleportedTEcho", "TeleportedTDistribution", "TeleportedTNoiseReporting")


def bytecode(qir, intrinsics=None):
    mod, _, _, _ = preprocess_simulation_input(str(qir), 1, None, 7)
    DecomposeCcxPass().run(mod)
    return AdaptiveProfilePass(Bytecode.Bit64).run(mod, noise_intrinsics=intrinsics).as_dict()


def compile_qsharp(source):
    qsharp.init(target_profile=TargetProfile.Adaptive_RIF)
    return qsharp.compile(source)


def test_constants():
    with open(TESTS) as f:
        tree = ast.parse(f.read())
    consts = {}
    for node in tree.body:
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Constant):
            for t in node.targets:
                if isinstance(t, ast.Name) and isinstance(node.value.value, str):
                    consts[t.id] = node.value.value
    return consts


def main_programs():
    consts = test_constants()
    programs = [{"name": name, "bytecode": bytecode(consts[name])} for name in TEST_PROGRAMS]
    for op in TELEPORTED_T:
        qsharp.init(target_profile=TargetProfile.Adaptive_RIF)
        qsharp.eval(consts["TELEPORTED_T_QSHARP"])
        programs.append({"name": op, "bytecode": bytecode(qsharp.compile(f"{op}()"))})
    for n in (4, 12, 16, 20, 22):
        programs.append({"name": f"bench_{n}", "bytecode": bytecode(compile_qsharp(bench_source(n, 8)))})
    return programs


def noise_programs():
    programs = [
        {"name": f"rep_{d}", "bytecode": bytecode(compile_qsharp(rep_source(d, 8)))}
        for d in (3, 5, 7, 9, 11)
    ]
    programs.append({"name": "readout_branch", "bytecode": bytecode(READOUT)})
    programs.append(
        {"name": "corr_branch", "bytecode": bytecode(CORR, {"test_noise_intrinsic": 0})}
    )
    return programs


def main():
    out = sys.argv[1]
    for file, make in (("programs.json", main_programs), ("programs_noise.json", noise_programs)):
        programs = make()
        with open(os.path.join(out, file), "w") as f:
            json.dump({"programs": programs}, f)
        print(f"wrote {len(programs)} programs to {file}:", " ".join(p["name"] for p in programs))


if __name__ == "__main__":
    main()
