# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""C4 walkthrough support: sample QIR with deq and plot shot counts."""

from __future__ import annotations

from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from functools import partial
from itertools import cycle, islice
import multiprocessing
from typing import TYPE_CHECKING

import qodec
from qdk import Result
from qdk.simulation import NoiseConfig, run_qir
from qdk.simulation.decoders import prepare_deq_decoder

if TYPE_CHECKING:
    from deq.runtime import Runtime
    from matplotlib.axes import Axes

_SHOTS_PER_CHUNK = 1_000
_codec: qodec.Qodec | None = None
_Sweep = dict[str, list[tuple[float, Counter[str]]]]
_Pooled = dict[str, dict[float, Counter[str]]]


def sample_sweep(
    codec: qodec.Qodec,
    benchmarks: dict[str, str],
    probabilities: list[float],
    *,
    shots: int,
    workers: int,
    max_readout_score: float | None = None,
) -> _Sweep:
    """Return benchmark -> (probability, attempted/accepted/wrong counts) pairs.

    Fixed chunks and seeds are independent of worker count. Reject flagged shots
    and, optionally, shots with high readout scores. Failures abort the whole sweep.
    """
    if shots < 1 or workers < 1 or not benchmarks or not probabilities:
        raise ValueError("Use positive shots and workers, and a nonempty sweep.")
    full_chunks, remainder = divmod(shots, _SHOTS_PER_CHUNK)
    sizes = [_SHOTS_PER_CHUNK] * full_chunks + ([remainder] if remainder else [])
    cases = [(name, p) for p in probabilities for name in benchmarks]
    jobs = [
        (benchmarks[name], p, size, 42 + i + j * len(cases), max_readout_score)
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

    if not sweep or any(not points for points in sweep.values()):
        raise ValueError("Cannot plot an empty sweep.")
    pooled: _Pooled = {}
    for name, points in sweep.items():
        totals = pooled.setdefault(name.removesuffix(" X").removesuffix(" Z"), {})
        for probability, counts in points:
            totals.setdefault(probability, Counter()).update(counts)
    for points in pooled.values():
        for counts in points.values():
            counts["rejected"] = counts["attempted"] - counts["accepted"]
    fig, axes = plt.subplots(1, 2, figsize=(13, 4.5), layout="constrained")
    _plot_rates(axes[0], pooled, ("wrong", "accepted"))
    _plot_rates(axes[1], pooled, ("rejected", "attempted"))
    probabilities = list(next(iter(pooled.values())))
    (line,) = axes[0].plot(
        probabilities, probabilities, "k--", linewidth=1, label="Break-even"
    )
    axes[0].legend(handles=[line], loc="lower left", fontsize="small")
    axes[0].set_title("Conditional logical error rate\nArrows: 95% upper bounds")
    axes[1].set(title="Rejection rate\nArrows: 95% upper bounds", ylim=(None, 1))
    axes[1].legend(loc="lower right", fontsize="small")
    fig.suptitle(title)
    plt.show()


def _plot_rates(axis: Axes, pooled: _Pooled, ratio: tuple[str, str]) -> None:
    import numpy as np
    from scipy.stats import binomtest

    numerator, denominator = ratio
    for (name, points), marker in zip(pooled.items(), cycle("os^vDPX*")):
        tests = [binomtest(c[numerator], c[denominator]) for c in points.values()]
        rates = np.array([test.statistic for test in tests])
        bounds = np.array([test.proportion_ci(method="wilson") for test in tests]).T
        zeros = rates == 0
        axis.errorbar(
            list(points),
            np.where(zeros, bounds[1], rates),
            yerr=np.where(zeros, 0, [rates - bounds[0], bounds[1] - rates]),
            uplims=zeros,
            fmt=marker,
            capsize=3,
            label=name,
        )
    axis.set(
        xscale="log",
        yscale="log",
        xlabel="Physical fault probability per location",
        ylabel=f"{numerator.capitalize()} / {denominator}",
    )
    axis.grid(True, which="both", alpha=0.25)


def _sample_chunk(chunk: tuple[str, float, int, int, float | None]) -> Counter[str]:
    program, probability, shots, seed, max_readout_score = chunk
    if _codec is None:
        raise RuntimeError("Initialize the worker with a qodec bundle before sampling.")
    noise = NoiseConfig()
    for gate in (noise.h, noise.x, noise.y, noise.z, noise.cx):
        gate.set_depolarizing(probability)
    noise.mresetz.x = probability
    rows = run_qir(
        program,
        qodec=_codec,
        decoder=partial(
            prepare_deq_decoder,
            runtime_factory=partial(_runtime, forced_gap=max_readout_score is not None),
            max_readout_score=max_readout_score,
        ),
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


def _runtime(seed: int, *, forced_gap: bool) -> Runtime:
    from deq.runtime import Runtime

    return Runtime(
        decoder="black-box-relay-bp",
        decoder_config={"parallel": 1, "seed": seed},
        coordinator="monolithic",
        coordinator_config={"forced_gap": forced_gap},
        controller="jit",
    )


def _initialize_worker(bundle: str) -> None:
    global _codec
    _codec = qodec.Qodec.loads(bundle)
