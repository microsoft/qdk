# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path
from typing import Sequence, cast
import math
import os
import random

import pytest
import sys

from qsharp._native import Result

# Skip all tests in this module if QDK_GPU_TESTS is not set
if not os.environ.get("QDK_GPU_TESTS"):
    pytest.skip("Skipping GPU tests (QDK_GPU_TESTS not set)", allow_module_level=True)

SKIP_REASON = "GPU is not available"

gpu_info = "Unknown"

try:
    from qsharp._native import try_create_gpu_adapter

    gpu_info = try_create_gpu_adapter()
    # Printing to stderr so that it is visible if CI run fails
    print(f"*** USING GPU: {gpu_info}", file=sys.stderr)

    GPU_AVAILABLE = True
except OSError as e:
    GPU_AVAILABLE = False
    SKIP_REASON = str(e)


import qsharp
from qsharp import TargetProfile
from qsharp import openqasm

from qsharp._simulation import run_qir_gpu, NoiseConfig

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


def result_array_to_string(results: Sequence[Result]) -> str:
    chars = []
    for value in results:
        if value == Result.Zero:
            chars.append("0")
        elif value == Result.One:
            chars.append("1")
        else:
            chars.append("-")
    return "".join(chars)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_seeding_no_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(
        """
        operation BellTest() : Result[] {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """
    )

    qir = str(qsharp.compile("BellTest()"))

    results = [run_qir_gpu(qir, 1, None, seed)[0] for seed in range(12)]
    print(results)

    # Results will be an array of 12 lists [Result, Result]
    # Each result should be [Zero, Zero] or [One, One]
    # As evident from a manual experiment running with the seeds of 0..11
    # gives 6 results of each. Experiment should be repeatable for fixed seeds.

    # Verify we have 6 of each result
    count_00 = sum(1 for r in results if r == [Result.Zero, Result.Zero])
    count_11 = sum(1 for r in results if r == [Result.One, Result.One])
    assert count_00 == 6
    assert count_11 == 6


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_no_noise():
    """Simple test that GPU simulator works without noise."""
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    output = run_qir_gpu(str(input))
    print(output)
    # Expecting deterministic output, no randomization seed needed.
    assert output == [[Result.Zero] * 25], "Expected result of 0s with pi/2 angles."


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_bitflip_noise():
    """Bitflip noise for GPU simulator."""
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    p_noise = 0.005
    noise = NoiseConfig()
    noise.rx.set_bitflip(p_noise)
    noise.rzz.set_pauli_noise("XX", p_noise)
    noise.mresetz.set_bitflip(p_noise)

    output = run_qir_gpu(str(input), shots=3, noise=noise, seed=17)
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    print(result)
    # Reasonable results obtained from manual run
    assert result == [
        "0000000000011100000000110",
        "0001001100000000000100110",
        "0000000000011000000000000",
    ]


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_mixed_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 4.0, 4)"
    )

    noise = NoiseConfig()
    noise.rz.set_bitflip(0.005)
    noise.rz.loss = 0.003
    noise.rzz.set_depolarizing(0.005)
    noise.rzz.loss = 0.003

    output = run_qir_gpu(str(input), shots=3, noise=noise, seed=53)
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    print(result)
    # Reasonable results obtained from manual run
    assert result == [
        "00000-00000000-0000000000",
        "00100001000-0000000000-00",
        "000000010000000-000000000",
    ]


def build_x_chain_qir(n_instances: int, n_x: int) -> str:
    # Construct multiple instances of x gate chains
    prefix = f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        bit[{n_instances}] c;
        qubit[{n_instances}] q;
    """

    infix = """
        x q;
    """

    suffix = """
        c = measure q;
    """

    src_parallel = prefix + infix * n_x + suffix

    # Compile resulting program
    qsharp.init(target_profile=TargetProfile.Base)
    qir_parallel = openqasm.compile(src_parallel)
    return str(qir_parallel)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "p_noise, n_x, n_instances, n_shots, max_percent",
    [
        (0.0005, 500, 10, 8192, 1.0),
        (0.005, 500, 10, 4096, 2.0),
        (0.0005, 20, 20, 100, 4.0),  # 100 shots is low, so higher error tolerated
    ],
)
def test_gpu_x_chain(
    p_noise: float, n_x: int, n_instances: int, n_shots: int, max_percent: float
):
    """
    Simulate multi-instance X-chain with bitflip noise many times
    Compare result frequencies with analytically computed probabilities
    """
    # Use the GPU simulator with noise
    noise = NoiseConfig()
    noise.x.set_bitflip(p_noise)

    qir = build_x_chain_qir(n_instances, n_x)
    output = run_qir_gpu(qir, shots=n_shots, noise=noise, seed=18)
    histogram = [0 for _ in range(n_instances + 1)]
    for shot in output:
        shot_results = cast(Sequence[Result], shot)
        count_1 = shot_results.count(Result.One)
        histogram[count_1] += 1

    # Probability of obtaining 0 and 1 at the end of the X chain.
    p_0 = ((2.0 * p_noise - 1.0) ** n_x + 1.0) / 2.0
    p_1 = 1.0 - p_0

    # Number of results with k ones that should be there.
    p_N = [
        p_0 ** ((n_instances - k)) * (p_1**k) * math.comb(n_instances, k) * n_shots
        for k in range(n_instances + 1)
    ]

    # Error % for deviation from analytical value
    error_percent = [abs(a - b) * 100.0 / n_shots for (a, b) in zip(histogram, p_N)]
    print(", ".join(f"{a} (Δ≈{b:.1f}%)" for (a, b) in zip(histogram, error_percent)))

    # We tolerate configured percentage error.
    assert all(
        err < max_percent for err in error_percent
    ), f"Error percent too high: {error_percent}"


def generate_op_sequence(
    n_qubits: int, n_ops: int, n_rand: int
) -> list[tuple[int, int]]:
    """Return operation tuples and randomly swap neighboring pairs n_rand times."""
    if n_qubits < 0 or n_ops < 0 or n_rand < 0:
        raise ValueError("Tuple bounds must be non-negative")

    ops = [(q, op) for op in range(n_ops) for q in range(n_qubits)]

    if len(ops) < 2 or n_rand == 0:
        return ops

    max_index = len(ops) - 1
    for _ in range(n_rand):
        idx = random.randrange(max_index)
        left, right = ops[idx], ops[idx + 1]
        if left[0] != right[0]:
            ops[idx], ops[idx + 1] = right, left

    return ops


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize("noisy_gate, noise_number", [(0, 2), (1, 1), (2, 2), (3, 2)])
def test_gpu_permuted_rotations(noisy_gate : int, noise_number : int):
    qsharp.init(target_profile=TargetProfile.Base)

    n_shots = 2000
    n_qubits = 15
    seed = 2026
    p_loss = 0.1
    tolerance_percent = 2.0
    assert n_qubits >= 2, "Need at least two qubits"

    random.seed(seed)
    i1, i2 = random.sample(range(n_qubits), 2)
    prefix = f"""
operation tiny_coeffs() : Result[] {{
    use q = Qubit[{n_qubits}];
    let i1 = {i1};
    let i2 = {i2};
"""

    # The following sequence of rotations is equivalent to identity:
    # 0. H <- could be any rotation
    # 1. Rx(1.123456789)
    # 2. Ry(1.212121212)
    # 3. Rz(1.14856940153986)
    # 4. Ry(-1.41836046203971)
    # 5. Rz(-0.325946593598928)
    # 6. H <- adjoint to step 0
    # We will perform these rotations on every qubit, but randomly intermix sequences for different qubits.
    # This should still result in identity on all qubits as gates on different qubits commute.
    # noise_number = how many times noisy gate appears in sequence.

    n_ops = 7
    ops = generate_op_sequence(n_qubits, n_ops, n_qubits*n_ops*100)
    infix = ""
    for qubit, op in ops:
        match op:
            case 0 | 6:
                infix += f"    H(q[{qubit}]);\n"
            case 1:
                infix += f"    Rx(1.123456789, q[{qubit}]);\n"
            case 2:
                infix += f"    Ry(1.212121212, q[{qubit}]);\n"
            case 3:
                infix += f"    Rz(1.14856940153986, q[{qubit}]);\n"
            case 4:
                infix += f"    Ry(-1.41836046203971, q[{qubit}]);\n"
            case 5:
                infix += f"    Rz(-0.325946593598928, q[{qubit}]);\n"

    suffix = """
    let m1 = M(q[i1]);
    let m2 = M(q[i2]);
    ResetAll(q);
    return [m1, m2];
}
"""

    program = prefix + infix + suffix
    qsharp.eval(program)
    input = qsharp.compile(
        "tiny_coeffs()"
    )

    noise = NoiseConfig()
    p_combined_loss = 1.0 - ((1.0-p_loss)**noise_number)
    match noisy_gate:
        case 0:
            noise.h.loss = p_loss
        case 1:
            noise.rx.loss = p_loss
        case 2:
            noise.ry.loss = p_loss
        case 3:
            noise.rz.loss = p_loss
        case _:
            raise ValueError("Invalid noisy_gate value")

    output = run_qir_gpu(str(input), shots=n_shots, noise=noise, seed=seed)
    result_strings = [
        result_array_to_string(cast(Sequence[Result], shot)) for shot in output
    ]
    assert len(result_strings) == n_shots, f"Shot count mismatch. Actual={len(result_strings)}, Expected={n_shots}"

    p_minus = p_combined_loss
    p_0 = 1.0 - p_minus
    allowed = [
        ("00", n_shots * p_0 * p_0),
        ("0-", n_shots * p_0 * p_minus),
        ("-0", n_shots * p_minus * p_0),
        ("--", n_shots * p_minus * p_minus),
    ]

    counts = {pattern: 0 for pattern, _ in allowed}
    for entry in result_strings:
        assert entry in counts, f"Unexpected measurement string: '{entry}'. Program={program}."
        counts[entry] += 1

    tolerance = tolerance_percent / 100.0 * n_shots
    print(f"Permuted rotations test: n_qubits={n_qubits}, n_shots={n_shots}, seed={seed}, noise#{noise_number}, Δ<={tolerance:.0f} i1={i1}, i2={i2}")
    summary_msg = ", ".join(
        f"'{pattern}': {counts[pattern]} (Δ={abs(counts[pattern] - expected_count):.0f})"
        for pattern, expected_count in allowed
    )
    print(summary_msg)
    for pattern, expected_count in allowed:
        actual_count = counts[pattern]
        assert (
            abs(actual_count - expected_count) <= tolerance
        ), f"Count for {pattern} off by more than {tolerance_percent:.1f}% of shots. Actual={actual_count}, Expected={expected_count:.0f}, noise#{noise_number}, Program={program}."

