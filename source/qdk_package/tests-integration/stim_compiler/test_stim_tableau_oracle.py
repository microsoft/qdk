# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""White-box differential tests of qdk-stim (the QDK Stim compiler and stabilizer
simulator) against Stim, used as an oracle for the final tableau.

Every test compares the final stabilizer state (and measurement record) that qdk-stim
reaches with the one Stim reaches for the same program:

1. Resets and non-collapsing gates, from every product of R/RX/RY preparations.
2. Collapsing gates: qdk-stim runs the program several times (see `RUNS`), and each
   run is replayed in Stim with every measurement forced to qdk-stim's outcome.
   Each run's record and final state must match Stim's.
3. Gates that depend on the measurement record (classical control).
4. Noise, restricted to probabilities 0 and 1.
5. Algorithms from resources/, with their noise stripped, and random programs.

Each instruction is compared once here; how the compiler lowers it (broadcasting,
REPEAT, annotations, ...) is covered by the compiler's own tests.

qdk-stim numbers qubits in first-use order, so programs first use qubits as 0, 1, 2, ...
"""

from itertools import product
from pathlib import Path
import sys
from textwrap import dedent

import pytest

stim = pytest.importorskip(
    "stim", reason="Stim is not available", exc_type=ImportError
)

from random_program_generator import (
    SINGLE_QUBIT_GATES,
    TWO_QUBIT_GATES,
    random_program,
)
from tableau_utils import (
    QdkStimProgram,
    RunResult,
    replay_in_stim,
    run_qdk_stim,
    run_stim,
)

RESOURCES = Path(__file__).parent / "resources"
SEED = 42

# Number of qdk-stim runs of each program with measurements, enough to see every outcome
# of a few random measurements. Programs with one measurement use fewer, long ones more.
DEFAULT_RUNS = 32
SINGLE_MEASUREMENT_RUNS = 8
LONG_PROGRAM_RUNS = 64

PREPARATION_BASES = ("RZ {q}", "RX {q}", "RY {q}")


def assert_matches_stim_without_measurements(source: str) -> None:
    source = dedent(source).strip()
    _assert_same_outcome(source, run_stim(source, SEED), run_qdk_stim(source, SEED))


def assert_matches_stim_with_measurements(
    source: str, runs: int = DEFAULT_RUNS
) -> None:
    """Asserts that every qdk-stim run of `source` matches Stim replaying that run's
    measurement record: same record, and same final state."""
    source = dedent(source).strip()
    program = QdkStimProgram(source, SEED)
    seen = set()
    for _ in range(runs):
        actual = program.run()
        expected = replay_in_stim(source, actual.record, SEED)
        if actual.record not in seen:
            seen.add(actual.record)
        _assert_same_outcome(source, expected, actual)


def _assert_same_outcome(source: str, expected: RunResult, actual: RunResult) -> None:
    assert actual.outcome() == expected.outcome(), (
        f"program:\n{source}\n\n"
        f"expected (stim):   {expected.outcome()}\n"
        f"actual (qdk-stim): {actual.outcome()}\n\n"
        f"{expected}\n\n{actual}"
    )


def assert_matches_stim_in_every_basis(
    body: str, qubits: list[int], runs: int = DEFAULT_RUNS
) -> None:
    """Runs `body` after every product of R/RX/RY preparations of `qubits`, using the
    assertion that matches whether `body` measures."""
    for preparations in product(PREPARATION_BASES, repeat=len(qubits)):
        program = [p.format(q=q) for p, q in zip(preparations, qubits)]
        program.append(dedent(body).strip())
        source = "\n".join(program)
        if stim.Circuit(source).num_measurements > 0:
            assert_matches_stim_with_measurements(source, runs)
        else:
            assert_matches_stim_without_measurements(source)


# # Step 1: resets and non-collapsing gates

# Resets

RESETS = ["R 0", "RZ 0", "RX 0", "RY 0"]


@pytest.mark.parametrize("reset", RESETS)
def test_reset(reset: str) -> None:
    assert_matches_stim_in_every_basis(reset, [0])


# Single-qubit gates


@pytest.mark.parametrize("gate", SINGLE_QUBIT_GATES)
def test_single_qubit_gate(gate: str) -> None:
    assert_matches_stim_in_every_basis(f"{gate} 0", [0])


# Two-qubit gates


@pytest.mark.parametrize("gate", TWO_QUBIT_GATES)
def test_two_qubit_gate(gate: str) -> None:
    assert_matches_stim_in_every_basis(f"{gate} 0 1", [0, 1])


# Pauli product gates

# fmt: off
PAULI_PRODUCT_GATES = [
    "SPP Z0", "SPP X0*Y1", "SPP !Z0*X1", "SPP Z1*X0",
    "SPP_DAG Y0*Z1", "SPP_DAG !X0",
    "SPP Z0*X1*Z0", "SPP Y0*X1*Z0*Y1",
]
# fmt: on


@pytest.mark.parametrize("gate", PAULI_PRODUCT_GATES)
def test_pauli_product_gate(gate: str) -> None:
    assert_matches_stim_in_every_basis(gate, [0, 1])


# # Step 2: collapsing gates

# Single-qubit measurements

# fmt: off
SINGLE_QUBIT_MEASUREMENTS = [
    "M 0", "MZ 0", "MX 0", "MY 0",
    "M !0", "MZ !0", "MX !0", "MY !0",
    "MR 0", "MRZ 0", "MRX 0", "MRY 0",
    "MR !0", "MRZ !0", "MRX !0", "MRY !0",
]
# fmt: on


@pytest.mark.parametrize("measurement", SINGLE_QUBIT_MEASUREMENTS)
def test_single_qubit_measurement(measurement: str) -> None:
    assert_matches_stim_in_every_basis(measurement, [0], SINGLE_MEASUREMENT_RUNS)


# Pair and Pauli product measurements

# fmt: off
PAIR_MEASUREMENTS = [
    "MXX 0 1", "MYY 0 1", "MZZ 0 1",
    "MXX !0 1", "MYY 0 !1", "MZZ !0 !1",
]

PAULI_PRODUCT_MEASUREMENTS = [
    "MPP Y0", "MPP X0*Y1", "MPP !Z0*X1", "MPP Z1*X0",
    "MPP Z0*X1*Z0", "MPP Y0*X1*Z0*Y1",
]
# fmt: on


@pytest.mark.parametrize("measurement", PAIR_MEASUREMENTS)
def test_pair_measurement(measurement: str) -> None:
    assert_matches_stim_in_every_basis(measurement, [0, 1], SINGLE_MEASUREMENT_RUNS)


@pytest.mark.parametrize("measurement", PAULI_PRODUCT_MEASUREMENTS)
def test_pauli_product_measurement(measurement: str) -> None:
    assert_matches_stim_in_every_basis(measurement, [0, 1], SINGLE_MEASUREMENT_RUNS)


def test_several_products_in_one_instruction() -> None:
    assert_matches_stim_in_every_basis("MPP X0*X1 Y0*Y1 !Z0*Z1", [0, 1])


# # Step 3: gates that depend on the measurement record

# Classically controlled Paulis

# fmt: off
CLASSICALLY_CONTROLLED_PAULIS = [
    "CX rec[-1] 1", "CNOT rec[-1] 1", "ZCX rec[-1] 1",
    "CY rec[-1] 1", "ZCY rec[-1] 1",
    "CZ rec[-1] 1", "ZCZ rec[-1] 1", "CZ 1 rec[-1]", "ZCZ 1 rec[-1]",
    "XCZ 1 rec[-1]", "YCZ 1 rec[-1]",
]
# fmt: on


@pytest.mark.parametrize("gate", CLASSICALLY_CONTROLLED_PAULIS)
def test_classically_controlled_pauli(gate: str) -> None:
    assert_matches_stim_in_every_basis(f"M 0\n{gate}", [0, 1], SINGLE_MEASUREMENT_RUNS)


# Qubits 0 and 1 are measured, and qubit 2 is controlled by the older record.
def test_classically_controlled_pauli_reads_older_record() -> None:
    assert_matches_stim_in_every_basis("M 0\nM 1\nCX rec[-2] 2", [0, 1, 2])


# # Step 4: noise (probabilities 0 and 1 only)

# fmt: off
SINGLE_QUBIT_NOISE = [
    "X_ERROR(1) 0", "Y_ERROR(1) 0", "Z_ERROR(1) 0",
    "X_ERROR(0) 0", "DEPOLARIZE1(0) 0",
    "PAULI_CHANNEL_1(1, 0, 0) 0", "PAULI_CHANNEL_1(0, 1, 0) 0", "PAULI_CHANNEL_1(0, 0, 1) 0",
]

# PAULI_CHANNEL_2 takes the probabilities of the 15 two-qubit Paulis in Stim's order
# (IX, IY, IZ, XI, ...): each case applies exactly one of them with probability 1.
TWO_QUBIT_NOISE = [
    "PAULI_CHANNEL_2(1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1",  # IX
    "PAULI_CHANNEL_2(0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1",  # IY
    "PAULI_CHANNEL_2(0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1",  # IZ
    "PAULI_CHANNEL_2(0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1",  # XI
    "PAULI_CHANNEL_2(0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1",  # XX
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1",  # XY
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0) 0 1",  # XZ
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0) 0 1",  # YI
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0) 0 1",  # YX
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0) 0 1",  # YY
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0) 0 1",  # YZ
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0) 0 1",  # ZI
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0) 0 1",  # ZX
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0) 0 1",  # ZY
    "PAULI_CHANNEL_2(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1) 0 1",  # ZZ
    "DEPOLARIZE2(0) 0 1",
]

CORRELATED_ERRORS = [
    "E(1) X0 Y1", "E(1) Z0 X1", "CORRELATED_ERROR(1) Y0 Z1", "E(0) X0 Y1",
    "E(0) X0\nELSE_CORRELATED_ERROR(1) Y1",           # ELSE fires if no earlier one did
]
# fmt: on


@pytest.mark.parametrize("channel", SINGLE_QUBIT_NOISE)
def test_single_qubit_noise(channel: str) -> None:
    assert_matches_stim_in_every_basis(channel, [0])


@pytest.mark.parametrize("channel", TWO_QUBIT_NOISE)
def test_two_qubit_noise(channel: str) -> None:
    assert_matches_stim_in_every_basis(channel, [0, 1])


@pytest.mark.parametrize("error", CORRELATED_ERRORS)
def test_correlated_error(error: str) -> None:
    assert_matches_stim_in_every_basis(error, [0, 1])


# # Step 5: algorithms and random programs

ALGORITHMS = sorted(path.stem for path in RESOURCES.glob("*.stim"))
RANDOM_PROGRAM_SEEDS = range(30)


def read_resource(program: str) -> str:
    return (RESOURCES / f"{program}.stim").read_text(encoding="utf-8")


@pytest.mark.parametrize("program", ALGORITHMS)
def test_algorithm(program: str) -> None:
    noiseless = stim.Circuit(read_resource(program)).without_noise()
    assert_matches_stim_with_measurements(str(noiseless), LONG_PROGRAM_RUNS)


# Random programs combine every gate, measurement, feedback and deterministic noise.
@pytest.mark.parametrize("seed", RANDOM_PROGRAM_SEEDS)
def test_random_program(seed: int) -> None:
    source = random_program(seed, qubits=8, length=120)
    assert_matches_stim_with_measurements(source, LONG_PROGRAM_RUNS)
