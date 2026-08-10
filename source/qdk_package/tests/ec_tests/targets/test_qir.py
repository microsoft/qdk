"""``qdk.ec.targets.qir`` — running a QIR program under a qodec.

The promise of this path is that a program written for physical qubits runs
unchanged on encoded ones, so the tests are organised around that: the same
program, the same result shape, better error rates.
"""

from __future__ import annotations

import pytest
import qodec

from ec_tests.testing.optional import requires_stim
from ec_tests.testing.qodecs import c4

pyqir = pytest.importorskip("pyqir")

from qdk.ec.targets.qir import (  # noqa: E402
    LogicalSlot,
    encodable_gates_of,
    encode_qir,
    stim_noise_from,
)


@pytest.fixture(scope="module")
def codec() -> qodec.Qodec:
    return c4()


def _qir(source: str, profile: str = "Adaptive"):
    """Compile a Q# snippet to QIR under the named target profile."""
    import qdk
    from qdk import qsharp

    qsharp.init(target_profile=getattr(qdk.TargetProfile, profile))
    return qsharp.compile(source)


X_THEN_MEASURE = """
{
    use q = Qubit();
    X(q);
    MResetZ(q)
}
"""

MEASURE_ONLY = """
{
    use q = Qubit();
    MResetZ(q)
}
"""


# ── Gate discovery ──────────────────────────────────────────────────────────


def test_encodable_gates_are_derived_from_the_qodecs_actions(
    codec: qodec.Qodec,
) -> None:
    gates = encodable_gates_of(codec)

    assert {"X", "Z"} <= gates, "c4 implements logical X and Z"
    assert {"M", "MZ", "MResetZ"} <= gates, "c4 implements Z-basis readout"


def test_a_qodec_without_a_gate_does_not_claim_it(codec: qodec.Qodec) -> None:
    # c4 has no logical Hadamard gadget.
    assert "H" not in encodable_gates_of(codec)


# ── Encoding ────────────────────────────────────────────────────────────────


@requires_stim
@pytest.mark.parametrize("profile", ["Base", "Adaptive"])
def test_the_same_program_encodes_identically_under_both_profiles(
    codec: qodec.Qodec, profile: str
) -> None:
    """The Adaptive profile wraps intrinsics in helper functions; inlining
    those must recover exactly the Base-profile gate sequence."""
    from qdk.ec.targets.qir import _extract_gates
    from qdk.simulation._simulation import preprocess_simulation_input

    module, *_ = preprocess_simulation_input(_qir(X_THEN_MEASURE, profile), 1, None, None)
    gates, qubit_count = _extract_gates(module)

    names = [str(gate[0]).rsplit(".", maxsplit=1)[-1] for gate in gates]
    assert qubit_count == 1
    assert names[0] == "X"
    assert names[1] in ("M", "MZ", "MResetZ")


@requires_stim
def test_encoding_opens_with_a_preparation(codec: qodec.Qodec) -> None:
    """QIR starts from |0>; the encoded program must say so explicitly."""
    from qdk.ec.targets.qir import _extract_gates
    from qdk.simulation._simulation import preprocess_simulation_input

    module, *_ = preprocess_simulation_input(_qir(X_THEN_MEASURE), 1, None, None)
    gates, qubit_count = _extract_gates(module)

    encoded = encode_qir(gates, codec, qubit_count=qubit_count)

    mnemonics = [call.mnemonic for call in encoded.program.instructions]
    assert mnemonics == ["prepare_zz", "x0", "measure_zz"]


@requires_stim
def test_encoding_records_where_each_result_came_from(codec: qodec.Qodec) -> None:
    from qdk.ec.targets.qir import _extract_gates
    from qdk.simulation._simulation import preprocess_simulation_input

    module, *_ = preprocess_simulation_input(_qir(X_THEN_MEASURE), 1, None, None)
    gates, qubit_count = _extract_gates(module)

    encoded = encode_qir(gates, codec, qubit_count=qubit_count)

    assert encoded.result_slots == [LogicalSlot(block=0, index=0)]
    assert encoded.measurement_gadgets == ["measure_zz"]


def test_an_unsupported_gate_is_refused_not_silently_dropped(
    codec: qodec.Qodec,
) -> None:
    """Encoding must never substitute an unprotected operation."""
    from qdk._native import QirInstructionId as Id

    with pytest.raises(NotImplementedError, match="cannot encode QIR gate"):
        encode_qir([(Id.H, 0)], codec, qubit_count=1)


def test_too_many_qubits_for_one_block_is_refused(codec: qodec.Qodec) -> None:
    with pytest.raises(NotImplementedError, match="multi-block"):
        encode_qir([], codec, qubit_count=3)


# ── Noise translation ───────────────────────────────────────────────────────


def test_none_noise_stays_noiseless() -> None:
    assert stim_noise_from(None) is None


def test_a_stim_mapping_passes_through_unchanged() -> None:
    model = {"p_data": 0.02, "p_meas": 0.01}

    assert stim_noise_from(model) == model


def test_a_noiseless_noise_config_becomes_none() -> None:
    from qdk.simulation import NoiseConfig

    assert stim_noise_from(NoiseConfig()) is None


def test_a_gate_error_becomes_a_data_error_rate() -> None:
    from qdk.simulation import NoiseConfig

    config = NoiseConfig()
    config.x.x = 0.25

    assert stim_noise_from(config) == {"p_data": 0.25, "p_meas": 0.0}


def test_a_measurement_error_becomes_a_measurement_rate() -> None:
    from qdk.simulation import NoiseConfig

    config = NoiseConfig()
    config.mz.x = 0.125

    assert stim_noise_from(config)["p_meas"] == 0.125


# ── Execution ───────────────────────────────────────────────────────────────


@requires_stim
def test_a_noiseless_encoded_run_reproduces_the_programs_answer(
    codec: qodec.Qodec,
) -> None:
    """X then measure must read One, encoded or not."""
    from qdk.ec.targets.qir import run_qir_encoded

    results = run_qir_encoded(_qir(X_THEN_MEASURE), codec, shots=16)

    assert len(results) == 16, "noiseless: nothing to postselect away"
    assert all(str(shot) == "One" for shot in results)


@requires_stim
def test_a_program_without_gates_reads_zero(codec: qodec.Qodec) -> None:
    from qdk.ec.targets.qir import run_qir_encoded

    results = run_qir_encoded(_qir(MEASURE_ONLY), codec, shots=16)

    assert all(str(shot) == "Zero" for shot in results)


@requires_stim
def test_encoded_results_have_the_same_shape_as_physical_ones(
    codec: qodec.Qodec,
) -> None:
    """The whole point: an encoded run is a drop-in for a physical one."""
    from qdk.ec.targets.qir import run_qir_encoded
    from qdk.simulation import run_qir

    program = _qir(X_THEN_MEASURE)

    physical = run_qir(program, shots=4, type="clifford")
    encoded = run_qir_encoded(program, codec, shots=4)

    assert type(encoded[0]) is type(physical[0])
    assert str(encoded[0]) == str(physical[0])


@requires_stim
def test_postselection_can_be_disabled(codec: qodec.Qodec) -> None:
    from qdk.ec.targets.qir import run_qir_encoded

    kept = run_qir_encoded(
        _qir(X_THEN_MEASURE),
        codec,
        shots=64,
        noise={"p_data": 0.1, "p_meas": 0.1},
        postselect=False,
    )

    assert len(kept) == 64


@requires_stim
def test_postselection_discards_shots_the_code_flagged(codec: qodec.Qodec) -> None:
    from qdk.ec.targets.qir import run_qir_encoded

    program = _qir(X_THEN_MEASURE)
    noise = {"p_data": 0.1, "p_meas": 0.1}

    everything = run_qir_encoded(program, codec, shots=400, noise=noise, postselect=False)
    surviving = run_qir_encoded(program, codec, shots=400, noise=noise, postselect=True)

    assert len(surviving) < len(everything)


@requires_stim
def test_error_detection_improves_the_answer(codec: qodec.Qodec) -> None:
    """The payoff: discarding flagged shots lowers the logical error rate.

    This is what an error-*detecting* code such as [[4,2,2]] buys, and it is the
    claim the demo notebook makes.
    """
    from qdk.ec.targets.qir import run_qir_encoded

    program = _qir(X_THEN_MEASURE)
    noise = {"p_data": 0.05, "p_meas": 0.05}
    shots = 3000

    def wrong_fraction(results) -> float:
        assert results, "expected at least one surviving shot"
        return sum(1 for shot in results if str(shot) != "One") / len(results)

    raw = wrong_fraction(
        run_qir_encoded(program, codec, shots=shots, noise=noise, postselect=False)
    )
    corrected = wrong_fraction(
        run_qir_encoded(program, codec, shots=shots, noise=noise, postselect=True)
    )

    assert corrected < raw / 1.5, (
        f"postselection should substantially cut the error rate; "
        f"got {corrected:.4f} vs {raw:.4f}"
    )


# ── run_qir integration ─────────────────────────────────────────────────────


@requires_stim
def test_run_qir_accepts_a_qodec(codec: qodec.Qodec) -> None:
    """The demo notebook's exact call shape."""
    from qdk.simulation import run_qir

    results = run_qir(_qir(X_THEN_MEASURE), shots=8, type="clifford", qodec=codec)

    assert results
    assert all(str(shot) == "One" for shot in results)


@requires_stim
def test_run_qir_routes_a_noise_config_through_the_encoded_path(
    codec: qodec.Qodec,
) -> None:
    from qdk.simulation import NoiseConfig, run_qir

    noise = NoiseConfig()
    noise.x.x = 0.05

    results = run_qir(
        _qir(X_THEN_MEASURE), shots=64, type="clifford", noise=noise, qodec=codec
    )

    assert len(results) <= 64, "some shots may be postselected away"


@requires_stim
def test_run_qir_without_a_qodec_is_unchanged() -> None:
    """The new parameter must not disturb the existing physical path."""
    from qdk.simulation import run_qir

    results = run_qir(_qir(X_THEN_MEASURE), shots=4, type="clifford")

    assert len(results) == 4
    assert all(str(shot) == "One" for shot in results)
