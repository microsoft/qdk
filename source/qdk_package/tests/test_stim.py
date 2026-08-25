import math

import qdk.stim as stim
from qdk import Result

from simulator_test_utils import check_histogram


def map_result_list_to_str(results):
    s = ""
    if isinstance(results, (list, tuple)):
        for r in results:
            s += map_result_list_to_str(r)
    else:
        match results:
            case Result.Zero:
                s += "0"
            case Result.One:
                s += "1"
            case Result.Loss:
                s += "L"
    return s


def test_t_gate_runs_on_clifford_simulator() -> None:
    results = stim.run("H 0\nT 0\nH 0\nM 0", shots=2_000, seed=42, type="clifford")

    one_probability = math.sin(math.pi / 8.0) ** 2
    check_histogram(
        [map_result_list_to_str(result) for result in results],
        {"0": 1.0 - one_probability, "1": one_probability},
    )


def test_arbitrary_rotation_runs_on_clifford_simulator() -> None:
    half_turns = 0.4
    results = stim.run(
        f"R_Y({half_turns}) 0\nM 0", shots=2_000, seed=42, type="clifford"
    )

    one_probability = math.sin(math.pi * half_turns / 2.0) ** 2
    check_histogram(
        [map_result_list_to_str(result) for result in results],
        {"0": 1.0 - one_probability, "1": one_probability},
    )
