# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from expecttest import assert_expected_inline

from qdk import qsharp
from qdk._device._atom import NeutralAtomDevice
from qdk.simulation import NoiseConfig

try:
    import pyqir

    PYQIR_AVAILABLE = True
except ImportError:
    PYQIR_AVAILABLE = False

SKIP_REASON = "PyQIR is not available"

try:
    import qsharp_widgets  # noqa: F401

    QSHARP_WIDGETS_AVAILABLE = True
except ImportError:
    QSHARP_WIDGETS_AVAILABLE = False

WIDGETS_SKIP_REASON = "qsharp-widgets is not available"


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_compile() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile("""
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """)

    device = NeutralAtomDevice()
    compiled = device.compile(qir)
    compiled_qir = str(compiled)

    assert_expected_inline(
        compiled_qir,
        """\

@0 = internal constant [4 x i8] c"0_a\\00"
@1 = internal constant [6 x i8] c"1_a0r\\00"
@2 = internal constant [6 x i8] c"2_a1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, ptr null)
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__sx__body(ptr null)
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, ptr null)
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cz__body(ptr null, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr null, ptr null)
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__array_record_output(i64 2, ptr @0)
  call void @__quantum__rt__result_record_output(ptr null, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  ret i64 0
}

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

declare void @__quantum__qis__sx__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr)

declare void @__quantum__qis__rz__body(double, ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simulate_with_cpu() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile("""
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """)

    device = NeutralAtomDevice()
    compiled = device.compile(qir)
    result = device.simulate(compiled, type="cpu")

    assert result == [[qsharp.Result.Zero, qsharp.Result.Zero]] or result == [
        [qsharp.Result.One, qsharp.Result.One]
    ]


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simlate_with_clifford() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile("""
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """)

    device = NeutralAtomDevice()
    compiled = device.compile(qir)
    result = device.simulate(compiled, type="stabilizer")

    assert result == [[qsharp.Result.Zero, qsharp.Result.Zero]] or result == [
        [qsharp.Result.One, qsharp.Result.One]
    ]


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simulate_z_h_s_h_z_is_sx_adjoint() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    # Z·H·S·H·Z is SX† and Rx(π/2) is SX (up to global phase), so the qubit
    # always ends in |0⟩.
    qir = qsharp.compile("""
        {
            use q = Qubit();
            Z(q);
            H(q);
            S(q);
            H(q);
            Z(q);
            Rx(Std.Math.PI() / 2.0, q);
            MResetZ(q)
        }
        """)

    device = NeutralAtomDevice()
    for sim_type in ["cpu", "stabilizer"]:
        result = device.simulate(qir, shots=10, type=sim_type, seed=1)
        assert result == [qsharp.Result.Zero] * 10, sim_type


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_compile_late_h_s_h_fusion_calls_sx() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    # The h s h on t only appears after the first optimizer run, when the
    # module already declares __quantum__qis__sx__body.
    qir = qsharp.compile("""
        {
            use (c, t) = (Qubit(), Qubit());
            H(t);
            S(t);
            CNOT(c, t);
            H(t);
            Rx(Std.Math.PI() / 2.0, t);
            [MResetZ(c), MResetZ(t)]
        }
        """)

    device = NeutralAtomDevice()
    compiled = str(device.compile(qir))
    assert "__quantum__qis__sx__body." not in compiled
    for sim_type in ["cpu", "stabilizer"]:
        result = device.simulate(qir, shots=10, type=sim_type, seed=1)
        assert result == [[qsharp.Result.Zero, qsharp.Result.One]] * 10, sim_type


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simulate_with_clifford_rejects_tadj() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    # The CZs keep the T† rotation from being folded with the Rz gates of the
    # decomposed H gates, so it reaches the Clifford pass as Rz(-π/4).
    qir = qsharp.compile("""
        {
            use (q, r) = (Qubit(), Qubit());
            H(q);
            CZ(q, r);
            Adjoint T(q);
            CZ(q, r);
            H(q);
            [MResetZ(q), MResetZ(r)]
        }
        """)

    device = NeutralAtomDevice()
    with pytest.raises(
        ValueError, match="used in RZ is not a Clifford compatible rotation angle"
    ):
        device.simulate(qir, type="stabilizer")


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simulate_with_loss() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile("""
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """)

    device = NeutralAtomDevice()
    noise = NoiseConfig()
    noise.mov.loss = 1.0  # Ensure loss occurs
    result = device.simulate(qir, noise=noise, type="cpu")
    result2 = device.simulate(qir, noise=noise, type="stabilizer")

    assert result == [[qsharp.Result.Loss, qsharp.Result.Loss]]
    assert result2 == [[qsharp.Result.Loss, qsharp.Result.Loss]]


def test_s_noise_inherits_from_rz():
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("operation Main() : Result { use q = Qubit(); S(q); MResetZ(q) }")
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    device = NeutralAtomDevice()
    output = device.simulate(ir, 1, noise)
    assert output == [qsharp.Result.One]


def test_z_noise_inherits_from_rz():
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("operation Main() : Result { use q = Qubit(); Z(q); MResetZ(q) }")
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    device = NeutralAtomDevice()
    output = device.simulate(ir, 1, noise)
    assert output == [qsharp.Result.One]


def test_s_adj_noise_inherits_from_rz():
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Main() : Result { use q = Qubit(); Adjoint S(q); MResetZ(q) }"
    )
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    device = NeutralAtomDevice()
    output = device.simulate(ir, 1, noise)
    assert output == [qsharp.Result.One]


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.skipif(not QSHARP_WIDGETS_AVAILABLE, reason=WIDGETS_SKIP_REASON)
def test_show_trace_rejects_branching_control_flow() -> None:
    # Tracing renders the program as a single, straight-line schedule, so it
    # cannot represent control flow. A program that branches on a measurement
    # result must be rejected rather than silently mis-visualized -- even when
    # every gate has constant parameters.
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile("""
        {
            use q = Qubit();
            H(q);
            let r = MResetZ(q);
            if r == One {
                X(q);
            }
            MResetZ(q)
        }
        """)

    device = NeutralAtomDevice()
    with pytest.raises(ValueError, match="programs with branching control flow"):
        device.show_trace(qir)


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.skipif(not QSHARP_WIDGETS_AVAILABLE, reason=WIDGETS_SKIP_REASON)
def test_show_trace_rejects_runtime_computed_values() -> None:
    # A program whose behavior depends on runtime-computed values (here a
    # rotation angle derived from a measurement result) cannot be traced and
    # must surface a clear, actionable error rather than a low-level failure.
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile("""
        {
            use q = Qubit();
            H(q);
            let r = MResetZ(q);
            let angle = if r == One { 1.0 } else { 2.0 };
            Rz(angle, q);
            MResetZ(q)
        }
        """)

    device = NeutralAtomDevice()
    with pytest.raises(ValueError, match="programs with branching control flow"):
        device.show_trace(qir)


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.skipif(not QSHARP_WIDGETS_AVAILABLE, reason=WIDGETS_SKIP_REASON)
def test_show_trace_allows_traceable_adaptive_program() -> None:
    # An Adaptive-profile program with only constant gate parameters and a
    # single execution path can still be traced; `show_trace` must not reject it
    # just because it is compiled to the Adaptive profile.
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile("""
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """)

    device = NeutralAtomDevice()
    # Must not raise: this Adaptive-profile program is fully traceable.
    device.show_trace(qir)


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.skipif(not QSHARP_WIDGETS_AVAILABLE, reason=WIDGETS_SKIP_REASON)
def test_show_trace_rejects_function_calls() -> None:
    # In some Adaptive-profile builds, gate definitions (e.g. `H`) are emitted as
    # separate, non-inlined functions. Tracing renders a single, straight-line
    # schedule and cannot follow a call into another function body, so those
    # operations would be silently dropped from the visualization. Such programs
    # must be rejected rather than mis-visualized.
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qir = qsharp.compile("""
        {
            use q = Qubit();
            H(q);
            MResetZ(q)
        }
        """)

    device = NeutralAtomDevice()
    with pytest.raises(ValueError, match="programs with function calls"):
        device.show_trace(qir)
