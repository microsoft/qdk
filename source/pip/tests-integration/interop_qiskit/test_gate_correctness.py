# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from qsharp import TargetProfile

from interop_qiskit import QISKIT_AVAILABLE, SKIP_REASON

if QISKIT_AVAILABLE:
    from qiskit import QuantumCircuit
    from qiskit.qasm3 import dumps
    from qsharp.interop.qiskit import QSharpBackend
    from qsharp.openqasm import run as run_qasm, OutputSemantics
    from qsharp import set_quantum_seed, init

from .test_circuits import (
    generate_repro_information,
    random_bit,
    intrinsic_measure,
    intrinsic_hixyz,
    intrinsic_ccnot,
    intrinsic_cnot,
    intrinsic_stswap,
    exercise_reset,
    exercise_rx_ry_rz,
    exercise_tdg_sdg,
    exercise_rxx,
    exercise_ryy,
    exercise_rzz,
    exercise_dcx,
    exercise_ecr,
    exercise_initialize_prepare_state,
    exercise_iswap,
    exercise_barrier_delay,
    exercise_ms,
    exercise_p,
    exercise_pauli,
    exercise_r_rccx_rcccx,
    exercise_rzx,
    exercise_sx_sxdg,
    exercise_u,
    exercise_unitary,
    exercise_ccz,
    exercise_ch,
    exercise_cp,
    exercise_crx_cry_crz,
    exercise_cs_csdg,
    exercise_cswap,
    exercise_csx,
    exercise_cu,
    exercise_cx_cy_cz,
    exercise_mcrx_mcry_mcrz,
    exercise_mcp,
    exercise_mcx,
)


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_random_bit() -> None:
    _test_circuit(*random_bit())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_intrinsic_hixyz() -> None:
    _test_circuit(*intrinsic_hixyz())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_intrinsic_ccnot() -> None:
    _test_circuit(*intrinsic_ccnot())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_intrinsic_cnot() -> None:
    _test_circuit(*intrinsic_cnot())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_intrinsic_measure() -> None:
    _test_circuit(*intrinsic_measure())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_intrinsic_stswap() -> None:
    _test_circuit(*intrinsic_stswap())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_reset() -> None:
    _test_circuit(*exercise_reset())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_rx_ry_rz() -> None:
    _test_circuit(*exercise_rx_ry_rz())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_tdg_sdg() -> None:
    _test_circuit(*exercise_tdg_sdg())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_rxx() -> None:
    _test_circuit(*exercise_rxx())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_ryy() -> None:
    _test_circuit(*exercise_ryy())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_rzz() -> None:
    _test_circuit(*exercise_rzz())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_barrier_delay() -> None:
    _test_circuit(*exercise_barrier_delay(), check_raw_qasm=False)


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_initialize_prepare_state() -> None:
    _test_circuit(*exercise_initialize_prepare_state(), check_raw_qasm=False)


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_dcx() -> None:
    _test_circuit(*exercise_dcx())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_ecr() -> None:
    _test_circuit(*exercise_ecr())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_iswap() -> None:
    _test_circuit(*exercise_iswap())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_ms() -> None:
    _test_circuit(*exercise_ms())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_p() -> None:
    _test_circuit(*exercise_p())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_pauli() -> None:
    _test_circuit(*exercise_pauli(), check_raw_qasm=False)


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_r_rccx_rcccx() -> None:
    _test_circuit(*exercise_r_rccx_rcccx())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_rzx() -> None:
    _test_circuit(*exercise_rzx())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_sx_sxdg() -> None:
    _test_circuit(*exercise_sx_sxdg())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_u() -> None:
    _test_circuit(*exercise_u())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_unitary() -> None:
    _test_circuit(*exercise_unitary())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_ccz() -> None:
    _test_circuit(*exercise_ccz())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_ch() -> None:
    _test_circuit(*exercise_ch())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_cp() -> None:
    _test_circuit(*exercise_cp())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_crx_cry_crz() -> None:
    _test_circuit(*exercise_crx_cry_crz())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_cs_csdg() -> None:
    _test_circuit(*exercise_cs_csdg())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_cswap() -> None:
    _test_circuit(*exercise_cswap())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_csx() -> None:
    _test_circuit(*exercise_csx())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_cu() -> None:
    _test_circuit(*exercise_cu())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_cx_cy_cz() -> None:
    _test_circuit(*exercise_cx_cy_cz())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_mcp() -> None:
    _test_circuit(*exercise_mcp())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_mcrx_mcry_mcrz() -> None:
    _test_circuit(*exercise_mcrx_mcry_mcrz())


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qiskit_qir_exercise_mcx() -> None:
    _test_circuit(*exercise_mcx())


def _test_circuit(
    circuit: "QuantumCircuit",
    peaks,
    results_len=1,
    num_shots=20,
    meas_level=2,
    check_raw_qasm=True,
):
    target_profile = TargetProfile.Base
    seed = 42
    backend = QSharpBackend(
        target_profile=target_profile,
        seed=seed,
        transpile_options={
            "optimization_level": 0  # Use no optimization to get consistent results in simulations
        },
    )
    try:
        job = backend.run(circuit, shots=num_shots)
        result = job.result()
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex

    results = result.results

    assert len(results) == results_len
    assert results[0].shots == num_shots
    assert results[0].success
    assert results[0].meas_level == meas_level
    assert hasattr(results[0].data, "counts")
    assert hasattr(results[0].data, "probabilities")

    counts = result.get_counts()

    # Check if the result is as expected
    assert _check_peaks(counts, peaks)

    if check_raw_qasm:
        # Now test the circuit as exported, untranspiled OpenQASM
        qasm_str = dumps(circuit)
        init()
        set_quantum_seed(seed)
        try:
            result = run_qasm(
                qasm_str,
                shots=num_shots,
                as_bitstring=True,
                output_semantics=OutputSemantics.Qiskit,
            )
        except AssertionError:
            raise
        except Exception as ex:
            raise RuntimeError(f"Failed running QASM:\n{qasm_str}") from ex
        # Create counts dictionary
        counts = {}
        for bitstring in result:
            counts[bitstring] = counts.get(bitstring, 0) + 1

        # Check if the result is as expected
        assert _check_peaks(counts, peaks)


def _check_peaks(counts, peaks) -> bool:
    """
    This function checks if all values in `peaks` are the highest peaks in `counts`.

    Parameters:
    counts (dict): A dictionary where keys are string return types and values are their counts.
    peaks (list): A list of string return types expected to be the peaks.

    Returns:
    bool: True if all values in peaks are the highest peaks in counts, otherwise False.
    """

    # Find the maximum count value in the histogram
    max_count = max(counts.values())

    # Find all the peaks in the histogram that have the maximum count value
    actual_peaks = [key for key, value in counts.items() if value == max_count]

    print("actual peaks:", actual_peaks)
    print("specified peaks", peaks)

    # Check if all actual peaks are in the expected peaks
    return all(peak in peaks for peak in actual_peaks)
