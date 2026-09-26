# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Spawn-safe shot sampling for section 7 of the QEC walkthrough."""

from functools import partial

import qodec
from qdk import Result
from qdk.simulation import NoiseConfig, run_qir
from qdk.simulation.decoders import prepare_deq_decoder

_codec: qodec.Qodec | None = None


def _initialize_worker(bundle: str) -> None:
    global _codec
    _codec = qodec.Qodec.loads(bundle)


def _runtime_noise(probability: float) -> NoiseConfig:
    noise = NoiseConfig()
    for gate in (noise.h, noise.x, noise.y, noise.z, noise.cx):
        gate.set_depolarizing(probability)
    noise.mresetz.x = probability
    return noise


def _sample_chunk(job: tuple[str, float, int, int]) -> list[list[bool]]:
    """Run (QIR, fault probability, attempts, seed); return rows with One as True."""
    if _codec is None:
        raise RuntimeError("Initialize the worker with a qodec bundle before sampling.")
    program, probability, shots, seed = job
    if shots < 1:
        raise ValueError("A shot chunk must contain at least one attempt.")
    results = run_qir(
        program,
        qodec=_codec,
        decoder=partial(prepare_deq_decoder, circuit_level=True),
        noise=_runtime_noise(probability),
        shots=shots,
        seed=seed,
        type="clifford",
        on_shot_failure="discard",
    )
    if any(
        not isinstance(result, list)
        or any(bit not in (Result.Zero, Result.One) for bit in result)
        for result in results
    ):
        raise ValueError(f"Expected QIR arrays of Result values (seed={seed}).")
    # Native Result values are not picklable across process boundaries.
    return [[bit == Result.One for bit in result] for result in results]
