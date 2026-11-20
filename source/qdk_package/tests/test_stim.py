import math

import pytest

import qdk.stim as stim

from simulator_test_utils import check_histogram, map_result_list_to_str

SIMULATORS = ["stabilizer", "cpu"]

# Every program below is deterministic, so a few shots are enough to see a wrong result.
SHOTS = 20


def test_t_gate_runs_on_clifford_simulator() -> None:
    results = stim.run("H 0\nT 0\nH 0\nM 0", shots=2_000, seed=42, type="stabilizer")

    one_probability = math.sin(math.pi / 8.0) ** 2
    check_histogram(
        [map_result_list_to_str(result) for result in results],
        {"0": 1.0 - one_probability, "1": one_probability},
    )


def test_arbitrary_rotation_runs_on_clifford_simulator() -> None:
    half_turns = 0.4
    results = stim.run(
        f"R_Y({half_turns}) 0\nM 0", shots=2_000, seed=42, type="stabilizer"
    )

    one_probability = math.sin(math.pi * half_turns / 2.0) ** 2
    check_histogram(
        [map_result_list_to_str(result) for result in results],
        {"0": 1.0 - one_probability, "1": one_probability},
    )


def run_to_strs(source: str, sim: str, shots: int = SHOTS) -> list[str]:
    results = stim.run(source, shots=shots, seed=42, type=sim)
    return [map_result_list_to_str(result) for result in results]


def assert_always(source: str, expected: str, sim: str) -> None:
    outcomes = set(run_to_strs(source, sim))
    message = f"program:\n{source}\n\nexpected {expected}, got {outcomes}"
    assert outcomes == {expected}, message


# Resets


RESETS = [
    pytest.param("X 0\nR 0\nM 0", "0", id="R"),
    pytest.param("X 0\nRZ 0\nM 0", "0", id="RZ"),
    pytest.param("X 0\nRX 0\nMX 0", "0", id="RX"),
    pytest.param("X 0\nRY 0\nMY 0", "0", id="RY"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", RESETS)
def test_reset(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Single-qubit gates


SINGLE_QUBIT_GATES = [
    pytest.param("X 0\nI 0\nM 0", "1", id="I"),
    pytest.param("X 0\nM 0", "1", id="X"),
    pytest.param("Y 0\nM 0", "1", id="Y"),
    pytest.param("RX 0\nZ 0\nMX 0", "1", id="Z"),
    pytest.param("X 0\nH 0\nMX 0", "1", id="H"),
    pytest.param("X 0\nH_XZ 0\nMX 0", "1", id="H_XZ"),
    pytest.param("H_XY 0\nM 0", "1", id="H_XY"),
    pytest.param("X 0\nH_YZ 0\nMY 0", "1", id="H_YZ"),
    pytest.param("H_NXY 0\nM 0", "1", id="H_NXY"),
    pytest.param("H_NXZ 0\nMX 0", "1", id="H_NXZ"),
    pytest.param("H_NYZ 0\nMY 0", "1", id="H_NYZ"),
    pytest.param("RY 0\nS 0\nMX 0", "1", id="S"),
    pytest.param("RY 0\nSQRT_Z 0\nMX 0", "1", id="SQRT_Z"),
    pytest.param("RX 0\nS_DAG 0\nMY 0", "1", id="S_DAG"),
    pytest.param("RX 0\nSQRT_Z_DAG 0\nMY 0", "1", id="SQRT_Z_DAG"),
    pytest.param("SQRT_X 0\nMY 0", "1", id="SQRT_X"),
    pytest.param("RY 0\nSQRT_X_DAG 0\nM 0", "1", id="SQRT_X_DAG"),
    pytest.param("RX 0\nSQRT_Y 0\nM 0", "1", id="SQRT_Y"),
    pytest.param("SQRT_Y_DAG 0\nMX 0", "1", id="SQRT_Y_DAG"),
    pytest.param("X 0\nC_XYZ 0\nMX 0", "1", id="C_XYZ"),
    pytest.param("X 0\nC_ZYX 0\nMY 0", "1", id="C_ZYX"),
    pytest.param("C_NXYZ 0\nMX 0", "1", id="C_NXYZ"),
    pytest.param("RY 0\nC_XNYZ 0\nM 0", "1", id="C_XNYZ"),
    pytest.param("C_XYNZ 0\nMX 0", "1", id="C_XYNZ"),
    pytest.param("C_NZYX 0\nMY 0", "1", id="C_NZYX"),
    pytest.param("C_ZNYX 0\nMY 0", "1", id="C_ZNYX"),
    pytest.param("RX 0\nC_ZYNX 0\nM 0", "1", id="C_ZYNX"),
    # T T = S and T_DAG T_DAG = S_DAG, which take |+> to |+i> and |-i>.
    pytest.param("RX 0\nT 0\nT 0\nMY 0", "0", id="T"),
    pytest.param("RX 0\nT_DAG 0\nT_DAG 0\nMY 0", "1", id="T_DAG"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", SINGLE_QUBIT_GATES)
def test_single_qubit_gate(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Two-qubit gates


TWO_QUBIT_GATES = [
    pytest.param("X 0\nII 0 1\nM 0 1", "10", id="II"),
    pytest.param("X 0\nCX 0 1\nM 0 1", "11", id="CX"),
    pytest.param("X 0\nCNOT 0 1\nM 0 1", "11", id="CNOT"),
    pytest.param("X 0\nZCX 0 1\nM 0 1", "11", id="ZCX"),
    pytest.param("X 0\nRX 1\nCY 0 1\nM 0\nMX 1", "11", id="CY"),
    pytest.param("X 0\nRX 1\nZCY 0 1\nM 0\nMX 1", "11", id="ZCY"),
    pytest.param("X 0\nRX 1\nCZ 0 1\nM 0\nMX 1", "11", id="CZ"),
    pytest.param("X 0\nRX 1\nZCZ 0 1\nM 0\nMX 1", "11", id="ZCZ"),
    pytest.param("RX 0\nZ 0\nXCX 0 1\nMX 0\nM 1", "11", id="XCX"),
    pytest.param("RX 0 1\nZ 0\nXCY 0 1\nMX 0 1", "11", id="XCY"),
    pytest.param("RX 0 1\nZ 0\nXCZ 0 1\nMX 0 1", "11", id="XCZ"),
    pytest.param("RY 0\nZ 0\nYCX 0 1\nMY 0\nM 1", "11", id="YCX"),
    pytest.param("RY 0\nRX 1\nZ 0\nYCY 0 1\nMY 0\nMX 1", "11", id="YCY"),
    pytest.param("RY 0\nRX 1\nZ 0\nYCZ 0 1\nMY 0\nMX 1", "11", id="YCZ"),
    pytest.param("X 0\nRY 1\nSWAP 0 1\nMY 0\nM 1", "01", id="SWAP"),
    pytest.param("RY 1\nISWAP 0 1\nMX 0\nM 1", "10", id="ISWAP"),
    pytest.param("RX 1\nISWAP_DAG 0 1\nMY 0\nM 1", "10", id="ISWAP_DAG"),
    pytest.param("X 0 1\nCXSWAP 0 1\nM 0 1", "01", id="CXSWAP"),
    pytest.param("X 1\nSWAPCX 0 1\nM 0 1", "11", id="SWAPCX"),
    pytest.param("X 0\nRX 1\nCZSWAP 0 1\nMX 0\nM 1", "11", id="CZSWAP"),
    pytest.param("X 0\nRX 1\nSWAPCZ 0 1\nMX 0\nM 1", "11", id="SWAPCZ"),
    pytest.param("RX 1\nSQRT_XX 0 1\nMY 0\nMX 1", "10", id="SQRT_XX"),
    pytest.param("RY 0\nRX 1\nSQRT_XX_DAG 0 1\nM 0\nMX 1", "10", id="SQRT_XX_DAG"),
    pytest.param("RX 0\nRY 1\nSQRT_YY 0 1\nM 0\nMY 1", "10", id="SQRT_YY"),
    pytest.param("RY 1\nSQRT_YY_DAG 0 1\nMX 0\nMY 1", "10", id="SQRT_YY_DAG"),
    pytest.param("RY 0\nSQRT_ZZ 0 1\nMX 0\nM 1", "10", id="SQRT_ZZ"),
    pytest.param("RX 0\nSQRT_ZZ_DAG 0 1\nMY 0\nM 1", "10", id="SQRT_ZZ_DAG"),
    pytest.param("X 0 1\nCH 0 1\nM 0\nMX 1", "11", id="CH"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", TWO_QUBIT_GATES)
def test_two_qubit_gate(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Three-qubit gates


THREE_QUBIT_GATES = [
    pytest.param("X 0 1\nCCX 0 1 2\nM 0 1 2", "111", id="CCX"),
    pytest.param("X 0 1\nRX 2\nCCZ 0 1 2\nM 0 1\nMX 2", "111", id="CCZ"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", THREE_QUBIT_GATES)
def test_three_qubit_gate(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Pauli product gates


PAULI_PRODUCT_GATES = [
    pytest.param("RY 0\nSPP Z0\nMX 0", "1", id="SPP"),
    pytest.param("RX 0\nSPP !Z0\nMY 0", "1", id="SPP negated"),
    pytest.param("RX 0\nSPP_DAG Z0\nMY 0", "1", id="SPP_DAG"),
    pytest.param("RX 0\nTPP Z0\nTPP Z0\nMY 0", "0", id="TPP"),
    pytest.param("RX 0\nTPP_DAG Z0\nTPP_DAG Z0\nMY 0", "1", id="TPP_DAG"),
    # Qubit 1 stays in |0>, so Z0*Z1 acts as Z0.
    pytest.param("RY 0\nSPP Z0*Z1\nMX 0\nM 1", "10", id="SPP product"),
    pytest.param("RX 0\nTPP Z0*Z1\nTPP Z0*Z1\nMY 0\nM 1", "00", id="TPP product"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", PAULI_PRODUCT_GATES)
def test_pauli_product_gate(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Rotations, whose angles are in half turns


ROTATIONS = [
    # R_X(0.5) takes |0> to |-i>.
    pytest.param("R_X(0.5) 0\nMY 0", "1", id="R_X"),
    # R_Y(-0.5) takes |0> to |->.
    pytest.param("R_Y(-0.5) 0\nMX 0", "1", id="R_Y"),
    # R_Z(-0.5) takes |+> to |-i>.
    pytest.param("RX 0\nR_Z(-0.5) 0\nMY 0", "1", id="R_Z"),
    pytest.param("R_XX(1) 0 1\nM 0 1", "11", id="R_XX"),
    pytest.param("R_YY(1) 0 1\nM 0 1", "11", id="R_YY"),
    pytest.param("RX 0 1\nR_ZZ(1) 0 1\nMX 0 1", "11", id="R_ZZ"),
    pytest.param("R_PAULI(1) X0*X1\nM 0 1", "11", id="R_PAULI"),
    pytest.param("R_PAULI(0.5) !X0\nMY 0", "0", id="R_PAULI negated"),
    # U3(θ, φ, λ) = R_Z(φ) R_Y(θ) R_Z(λ): R_Y(0.5) takes |0> to |+>, then R_Z(1) to |->.
    pytest.param("U3(0.5, 1, 0) 0\nMX 0", "1", id="U3"),
    # R_Z(1) takes |+> to |->, then R_Y(0.5) to |0>.
    pytest.param("RX 0\nU3(0.5, 0, 1) 0\nM 0", "0", id="U3 lambda"),
    pytest.param("U(1, 0, 0) 0\nM 0", "1", id="U"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", ROTATIONS)
def test_rotation(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Measurements


MEASUREMENTS = [
    pytest.param("X 0\nM 0", "1", id="M"),
    pytest.param("M !0", "1", id="M negated"),
    pytest.param("M(1) 0", "1", id="M readout noise"),
    pytest.param("X 0\nMZ 0", "1", id="MZ"),
    pytest.param("RX 0\nZ 0\nMX 0", "1", id="MX"),
    pytest.param("RX 0\nMX(1) 0", "1", id="MX readout noise"),
    pytest.param("RY 0\nZ 0\nMY 0", "1", id="MY"),
    pytest.param("X 0\nMR 0\nM 0", "10", id="MR"),
    pytest.param("MR !0\nM 0", "10", id="MR negated"),
    pytest.param("X 0\nMRZ 0\nM 0", "10", id="MRZ"),
    pytest.param("RX 0\nZ 0\nMRX 0\nMX 0", "10", id="MRX"),
    pytest.param("RY 0\nZ 0\nMRY 0\nMY 0", "10", id="MRY"),
    pytest.param("RX 0 1\nZ 0\nMXX 0 1", "1", id="MXX"),
    pytest.param("RY 0 1\nZ 0\nMYY 0 1", "1", id="MYY"),
    pytest.param("X 0\nMZZ 0 1", "1", id="MZZ"),
    pytest.param("MZZ !0 1", "1", id="MZZ negated"),
    pytest.param("MZZ(1) 0 1", "1", id="MZZ readout noise"),
    pytest.param("RX 0\nRY 1\nZ 0\nMPP X0*Y1", "1", id="MPP"),
    pytest.param("X 0\nMPP Z0 Z1 Z0*Z1", "101", id="MPP several products"),
    pytest.param("MPP !Z0", "1", id="MPP negated"),
    pytest.param("MPP(1) Z0", "1", id="MPP readout noise"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", MEASUREMENTS)
def test_measurement(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Gates controlled by a measurement record


CLASSICALLY_CONTROLLED_GATES = [
    pytest.param("M 0\nCX rec[-1] 1\nM 1", "00", id="CX not taken"),
    pytest.param("X 0\nM 0\nCX rec[-1] 1\nM 1", "11", id="CX"),
    pytest.param("X 0\nM 0\nCNOT rec[-1] 1\nM 1", "11", id="CNOT"),
    pytest.param("X 0\nM 0\nZCX rec[-1] 1\nM 1", "11", id="ZCX"),
    pytest.param("X 0\nM 0\nRX 1\nCY rec[-1] 1\nMX 1", "11", id="CY"),
    pytest.param("X 0\nM 0\nRX 1\nZCY rec[-1] 1\nMX 1", "11", id="ZCY"),
    pytest.param("X 0\nM 0\nRX 1\nCZ rec[-1] 1\nMX 1", "11", id="CZ"),
    pytest.param("X 0\nM 0\nRX 1\nZCZ rec[-1] 1\nMX 1", "11", id="ZCZ"),
    pytest.param("X 0\nM 0\nRX 1\nCZ 1 rec[-1]\nMX 1", "11", id="CZ record target"),
    pytest.param("X 0\nM 0\nXCZ 1 rec[-1]\nM 1", "11", id="XCZ"),
    pytest.param("X 0\nM 0\nRX 1\nYCZ 1 rec[-1]\nMX 1", "11", id="YCZ"),
    pytest.param("X 0\nM 0 1\nCX rec[-2] 2\nM 2", "101", id="older record"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", CLASSICALLY_CONTROLLED_GATES)
def test_classically_controlled_gate(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Noise, with probabilities 0 and 1


NOISE = [
    pytest.param("X_ERROR(1) 0\nM 0", "1", id="X_ERROR"),
    pytest.param("X_ERROR(0) 0\nM 0", "0", id="X_ERROR never"),
    pytest.param("RX 0\nY_ERROR(1) 0\nMX 0", "1", id="Y_ERROR"),
    pytest.param("RX 0\nZ_ERROR(1) 0\nMX 0", "1", id="Z_ERROR"),
    pytest.param("I_ERROR(0.5) 0\nM 0", "0", id="I_ERROR"),
    pytest.param("II_ERROR(0.5) 0 1\nM 0 1", "00", id="II_ERROR"),
    # Qubit 0 starts in |0> and qubit 1 in |+>, so X flips only qubit 0, Z flips only
    # qubit 1, and Y flips both.
    pytest.param(
        "RX 1\nPAULI_CHANNEL_1(1, 0, 0) 0 1\nM 0\nMX 1", "10", id="PAULI_CHANNEL_1 X"
    ),
    pytest.param(
        "RX 1\nPAULI_CHANNEL_1(0, 1, 0) 0 1\nM 0\nMX 1", "11", id="PAULI_CHANNEL_1 Y"
    ),
    pytest.param(
        "RX 1\nPAULI_CHANNEL_1(0, 0, 1) 0 1\nM 0\nMX 1", "01", id="PAULI_CHANNEL_1 Z"
    ),
    # PAULI_CHANNEL_2 lists IX, IY, IZ, XI, XX, ..., where the first Pauli acts on the
    # first target.
    pytest.param(
        "PAULI_CHANNEL_2(1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1\nM 0 1",
        "01",
        id="PAULI_CHANNEL_2 IX",
    ),
    pytest.param(
        "PAULI_CHANNEL_2(0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0) 0 1\nM 0 1",
        "10",
        id="PAULI_CHANNEL_2 XI",
    ),
    pytest.param("E(1) X0 Y1\nM 0 1", "11", id="E"),
    pytest.param("CORRELATED_ERROR(1) X0 X1\nM 0 1", "11", id="CORRELATED_ERROR"),
    pytest.param(
        "E(0) X0\nELSE_CORRELATED_ERROR(1) X1\nM 0 1",
        "01",
        id="ELSE_CORRELATED_ERROR",
    ),
    pytest.param(
        "E(1) X0\nELSE_CORRELATED_ERROR(1) X1\nM 0 1",
        "10",
        id="ELSE_CORRELATED_ERROR after an error",
    ),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", NOISE)
def test_noise(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# DEPOLARIZE1(p) applies X, Y or Z with probability p/3 each, and X and Y flip M.
@pytest.mark.parametrize("sim", SIMULATORS)
def test_depolarize1(sim: str) -> None:
    results = run_to_strs("DEPOLARIZE1(0.3) 0\nM 0", sim, shots=2_000)
    check_histogram(results, {"0": 0.8, "1": 0.2})


# DEPOLARIZE2(p) applies each of the 15 non-identity Paulis with probability p/15. 8 of
# them flip each qubit, and 4 flip both.
@pytest.mark.parametrize("sim", SIMULATORS)
def test_depolarize2(sim: str) -> None:
    results = run_to_strs("DEPOLARIZE2(0.6) 0 1\nM 0 1", sim, shots=2_000)
    check_histogram(results, {"00": 0.52, "01": 0.16, "10": 0.16, "11": 0.16})


# Loss


LOSS = [
    pytest.param("LOSS_ERROR(1) 0\nM 0", "L", id="LOSS_ERROR"),
    pytest.param("LOSS_ERROR(0) 0\nM 0", "0", id="LOSS_ERROR never"),
    pytest.param("E(1) L0 X1\nM 0 1", "L1", id="E loss"),
    pytest.param("LOSS_ERROR(1) 0\nPEEK_LOSS 0", "1", id="PEEK_LOSS lost"),
    pytest.param("PEEK_LOSS 0", "0", id="PEEK_LOSS not lost"),
    pytest.param("PEEK_LOSS(1) 0", "1", id="PEEK_LOSS readout noise"),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", LOSS)
def test_loss(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)


# Blocks


BLOCKS = [
    pytest.param("REPEAT 3 {\n    X 0\n}\nM 0", "1", id="REPEAT"),
    # REQUIRE restarts the SELECT block until the parity of the records is 0.
    pytest.param(
        "SELECT {\n    H 0\n    M 0\n    REQUIRE rec[-1]\n}", "0", id="REQUIRE"
    ),
    pytest.param(
        "SELECT {\n    H 0\n    M 0\n    REQUIRE !rec[-1]\n}",
        "1",
        id="REQUIRE negated",
    ),
    # Qubit 0 always reads 1, so REQUIRE restarts the block until qubit 1 also reads 1.
    pytest.param(
        "X 0\nSELECT {\n    H 1\n    M 0 1\n    REQUIRE rec[-1] rec[-2]\n}",
        "11",
        id="REQUIRE several records",
    ),
    # NOTLEAKED restarts the SELECT block until none of the records is lost.
    pytest.param(
        "SELECT {\n    LOSS_ERROR(0.5) 0\n    M 0\n    NOTLEAKED rec[-1]\n}",
        "0",
        id="NOTLEAKED",
    ),
]


@pytest.mark.parametrize("sim", SIMULATORS)
@pytest.mark.parametrize("source, expected", BLOCKS)
def test_block(source: str, expected: str, sim: str) -> None:
    assert_always(source, expected, sim)
