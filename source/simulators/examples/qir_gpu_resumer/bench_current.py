# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Times the current GPU simulator (`GpuSimulator`, its adaptive path) on
the `qir_gpu_resumer` example's workloads, for comparison with the
resumer's `bench` mode. Depolarizing noise of rate p is set on every gate,
move and measurement, as the resumer's `bench` mode does.

Usage: python3 bench_current.py bench|rep,<n>,<rounds>,<shots>,<runs>,<p>
(bench: the bench program of n qubits; rep: the distance-n repetition
code). Prints each run's time and the best after the first (warm-up) run,
which compiles the shader.
"""

import os
import sys
import time

from qdk import TargetProfile, qsharp
from qdk.simulation import NoiseConfig
from qdk.simulation._simulation import GpuSimulator

from workloads import bench_source, rep_source


def main():
    args = sys.argv[1].split(",")
    kind, n, rounds, shots, runs = args[0], *map(int, args[1:5])
    p = float(args[5])

    qsharp.init(target_profile=TargetProfile.Adaptive_RIF)
    source = bench_source(n, rounds) if kind == "bench" else rep_source(n, rounds)
    qir = qsharp.compile(source)

    noise = NoiseConfig()
    if p > 0:
        for name in (
            "x y z h s s_adj t t_adj sx sx_adj rx ry rz cx cy cz rxx ryy rzz swap "
            "mov mz mresetz"
        ).split():
            getattr(noise, name).set_depolarizing(p)

    sim = GpuSimulator()
    sim.gpu_context.set_noise(noise)
    sim.set_program(qir)

    times = []
    for _ in range(runs):
        started = time.perf_counter()
        result = sim.run_shots(shots, seed=7)
        times.append(time.perf_counter() - started)
        codes = set(result["shot_result_codes"])
        assert codes == {0}, codes
    print(
        f"CURRENT {kind} n={n} rounds={rounds} shots={shots} p={p:g} times="
        + ",".join(f"{t:.3f}" for t in times)
        + f" best_after_warmup={min(times[1:] or times):.4f}s",
        flush=True,
    )


main()
# GL processes may hang at exit in WSL: end at once.
os._exit(0)
