# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Compares qdk-stim with Stim by sampling noisy programs many times on both and checking
that their histograms of measurement records match.
"""

from pathlib import Path
import sys

import pytest

pytest.importorskip("stim", reason="Stim is not available", exc_type=ImportError)

from random_program_generator import random_program
from sampling_utils import BACKENDS, NOISY_SHOTS, Backend, assert_matches_stim

RESOURCES = Path(__file__).parent / "resources"

# color_d3 takes most of the suite's time on the CPU backend and catches few noise
# bugs, so it only runs in test_stim_tableau_oracle.py.
SKIPPED_ALGORITHMS = {"color_d3"}
ALGORITHMS = sorted(
    path.stem
    for path in RESOURCES.glob("*.stim")
    if path.stem not in SKIPPED_ALGORITHMS
)
RANDOM_PROGRAM_SEEDS = range(20)
RANDOM_PROGRAM_NOISE = 0.05


def read_resource(program: str) -> str:
    return (RESOURCES / f"{program}.stim").read_text(encoding="utf-8")


@pytest.mark.parametrize("backend", BACKENDS)
@pytest.mark.parametrize("program", ALGORITHMS)
def test_algorithm(program: str, backend: Backend) -> None:
    assert_matches_stim(read_resource(program), NOISY_SHOTS, backend)


# Small programs keep the measurement record short, so the histograms stay sensitive.
@pytest.mark.parametrize("backend", BACKENDS)
@pytest.mark.parametrize("seed", RANDOM_PROGRAM_SEEDS)
def test_random_program(seed: int, backend: Backend) -> None:
    source = random_program(
        seed, qubits=4, length=40, noise=RANDOM_PROGRAM_NOISE, measure_at_end=True
    )
    assert_matches_stim(source, NOISY_SHOTS, backend)
