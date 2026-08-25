import math

import qdk.stim as stim

from simulator_test_utils import check_histogram


def test_t_gate_runs_on_clifford_simulator() -> None:
    results = stim.run("H 0\nT 0\nH 0\nM 0", shots=2_000, seed=42, type="clifford")

    one_probability = math.sin(math.pi / 8.0) ** 2
    check_histogram(results, {"0": 1.0 - one_probability, "1": one_probability})


def test_arbitrary_rotation_runs_on_clifford_simulator() -> None:
    half_turns = 0.4
    results = stim.run(
        f"R_Y({half_turns}) 0\nM 0", shots=2_000, seed=42, type="clifford"
    )

    one_probability = math.sin(math.pi * half_turns / 2.0) ** 2
    check_histogram(results, {"0": 1.0 - one_probability, "1": one_probability})
