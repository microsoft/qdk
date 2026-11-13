# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Times the current GPU simulator (`GpuSimulator`, its adaptive path) on
the `qir_gpu_resumer` example's workloads, as a Python user calls it. The
like-for-like comparison with the resumer is the example's `compare` mode,
which times both in Rust on the same bytecode; this script's time also
holds the Python and pyo3 conversion of each shot's results.

Usage: python3 bench_current.py bench|rep,<n>,<rounds>,<shots>,<runs>,<p>[,gates]
(bench: the bench program of n qubits; rep: the distance-n repetition
code). Depolarizing noise of rate p is set on every gate and move, and on
the measurements unless `gates` is given. Prints each run's time and the
best after the first (warm-up) run, which compiles the shader; or, if a run
has failed shots, their counts by error code.

The noise is set after the program: `set_program` switches the context to
its adaptive mode, which forgets a noise configuration set before it.
"""

import os
import sys
import time
from collections import Counter

from qdk import TargetProfile, qsharp
from qdk.simulation import NoiseConfig
from qdk.simulation._simulation import GpuSimulator

from workloads import bench_source, rep_source


def main():
    args = sys.argv[1].split(",")
    kind, n, rounds, shots, runs = args[0], *map(int, args[1:5])
    p = float(args[5])
    gates_only = args[6:] == ["gates"]

    qsharp.init(target_profile=TargetProfile.Adaptive_RIF)
    source = bench_source(n, rounds) if kind == "bench" else rep_source(n, rounds)
    qir = qsharp.compile(source)

    names = "x y z h s s_adj t t_adj sx sx_adj rx ry rz cx cy cz rxx ryy rzz swap mov"
    if not gates_only:
        names += " mz mresetz"
    noise = NoiseConfig()
    if p > 0:
        for name in names.split():
            getattr(noise, name).set_depolarizing(p)

    sim = GpuSimulator()
    sim.set_program(qir)
    sim.gpu_context.set_noise(noise)

    times = []
    for _ in range(runs):
        started = time.perf_counter()
        result = sim.run_shots(shots, seed=7)
        times.append(time.perf_counter() - started)
        codes = Counter(result["shot_result_codes"])
        if set(codes) != {0}:
            print(f"FAILED shots by error code: {dict(codes)}", flush=True)
            # GL processes may hang at exit in WSL: end at once.
            os._exit(1)
    print(
        f"CURRENT {kind} n={n} rounds={rounds} shots={shots} p={p:g}"
        + (" gates" if gates_only else "")
        + " times="
        + ",".join(f"{t:.3f}" for t in times)
        + f" best_after_warmup={min(times[1:] or times):.4f}s",
        flush=True,
    )


main()
# GL processes may hang at exit in WSL: end at once.
os._exit(0)
