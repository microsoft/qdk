# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""C4 walkthrough support: add flags, sample QIR with deq, and plot shot counts."""

from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from copy import deepcopy
from functools import partial
from itertools import cycle, islice
import multiprocessing

import qodec
from qdk import Result
from qdk.simulation import NoiseConfig, run_qir
from qdk.simulation.decoders import prepare_deq_decoder

_SHOTS_PER_CHUNK = 1_000
_codec: qodec.Qodec | None = None
_Sweep = dict[str, list[tuple[float, Counter[str]]]]


def add_flags(
    codec: qodec.Qodec, mnemonic: str, equations: dict[str, list[str]]
) -> None:
    """Append raw-readout XOR flags without changing circuits or existing flags."""
    layer = codec.layers[0]
    gadget = layer.gadgets[mnemonic]
    if gadget.implements.flags:
        raise ValueError(f"{mnemonic} already declares flags.")
    gadget.implements.flags = list(equations)
    gadget.readouts = [
        *gadget.readouts,
        *({flag: terms} for flag, terms in equations.items()),
    ]
    layer.gadgets[mnemonic] = gadget
    layer.instruction_set.instructions[mnemonic] = deepcopy(gadget.implements)


def _initialize_worker(bundle: str) -> None:
    global _codec
    _codec = qodec.Qodec.loads(bundle)


def _sample_chunk(chunk: tuple[str, float, int, int]) -> Counter[str]:
    program, probability, shots, seed = chunk
    if _codec is None:
        raise RuntimeError("Initialize the worker with a qodec bundle before sampling.")
    noise = NoiseConfig()
    for gate in (noise.h, noise.x, noise.y, noise.z, noise.cx):
        gate.set_depolarizing(probability)
    noise.mresetz.x = probability
    rows = run_qir(
        program,
        qodec=_codec,
        decoder=partial(prepare_deq_decoder, circuit_level=True),
        noise=noise,
        shots=shots,
        seed=seed,
        type="clifford",
        on_shot_failure="discard",
    )
    accepted = [
        logical for flags, logical in rows if all(bit == Result.Zero for bit in flags)
    ]
    wrong = sum(any(bit != Result.Zero for bit in logical) for logical in accepted)
    return Counter(attempted=shots, accepted=len(accepted), wrong=wrong)


def sample_sweep(
    codec: qodec.Qodec,
    benchmarks: dict[str, str],
    probabilities: list[float],
    *,
    shots: int,
    workers: int,
) -> _Sweep:
    """Return benchmark -> (probability, attempted/accepted/wrong counts) pairs.

    Fixed chunks and seeds do not depend on worker count. Each process loads
    one codec; only counts come back. Failures propagate without partial sweeps.
    """
    if shots < 1 or workers < 1 or not benchmarks or not probabilities:
        raise ValueError("Use positive shots and workers, and a nonempty sweep.")
    sizes = [
        min(_SHOTS_PER_CHUNK, shots - start)
        for start in range(0, shots, _SHOTS_PER_CHUNK)
    ]
    cases = [(name, p) for p in probabilities for name in benchmarks]
    jobs = [
        (benchmarks[name], p, size, 42 + i + j * len(cases))
        for i, (name, p) in enumerate(cases)
        for j, size in enumerate(sizes)
    ]
    sweep: _Sweep = {name: [] for name in benchmarks}
    with ProcessPoolExecutor(
        max_workers=workers,
        mp_context=multiprocessing.get_context("spawn"),
        initializer=_initialize_worker,
        initargs=(codec.dumps(),),
    ) as executor:
        results = executor.map(_sample_chunk, jobs)
        for name, p in cases:
            counts = sum(islice(results, len(sizes)), Counter[str]())
            if not counts["accepted"]:
                raise RuntimeError(f"No {name} shots accepted at p={p:g}.")
            sweep[name].append((p, counts))
    return sweep


def plot_sweep(sweep: _Sweep, *, title: str) -> None:
    """Pool X/Z counts and plot probabilities with approximate 95% Wilson intervals."""
    import matplotlib.pyplot as plt
    import numpy as np
    from scipy.stats import binomtest

    if not sweep or any(not points for points in sweep.values()):
        raise ValueError("Cannot plot an empty sweep.")
    pooled: dict[str, dict[float, Counter[str]]] = {}
    for name, points in sweep.items():
        experiment = name.removesuffix(" X").removesuffix(" Z")
        for probability, counts in points:
            pooled.setdefault(experiment, {}).setdefault(probability, Counter()).update(
                counts
            )
    fig, axes = plt.subplots(1, 2, figsize=(13, 4.5), layout="constrained")
    for ax, numerator, denominator in zip(
        axes, ("wrong", "rejected"), ("accepted", "attempted")
    ):
        for (name, points), marker in zip(pooled.items(), cycle("os^vDPX*")):
            probabilities = np.array(list(points))
            tests = [
                binomtest(
                    (
                        c["wrong"]
                        if numerator == "wrong"
                        else c["attempted"] - c["accepted"]
                    ),
                    c[denominator],
                )
                for c in points.values()
            ]
            rates = np.array([test.statistic for test in tests])
            bounds = np.array([test.proportion_ci(method="wilson") for test in tests]).T
            zeros = rates == 0
            ax.errorbar(
                probabilities,
                np.where(zeros, bounds[1], rates),
                yerr=np.where(zeros, 0, [rates - bounds[0], bounds[1] - rates]),
                uplims=zeros,
                fmt=marker,
                capsize=3,
                label=name,
            )
        ax.set(
            xscale="log",
            yscale="log",
            xlabel="Physical fault probability per location",
            ylabel=f"{numerator.capitalize()} / {denominator}",
        )
        ax.grid(True, which="both", alpha=0.25)
    probabilities = np.array(list(next(iter(pooled.values()))))
    (line,) = axes[0].plot(
        probabilities, probabilities, "k--", linewidth=1, label="Break-even"
    )
    axes[0].legend(handles=[line], loc="lower left", fontsize="small")
    axes[0].set_title("Conditional logical error rate\nArrows: 95% upper bounds")
    axes[1].set(title="Rejection rate\nArrows: 95% upper bounds", ylim=(None, 1))
    axes[1].legend(loc="lower right", fontsize="small")
    fig.suptitle(title)
    plt.show()
