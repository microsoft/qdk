# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Helpers to compare the output samples of Stim and qdk-stim for the same Stim program.

Both simulators sample the program's measurement record many times, and the two
histograms must agree within a statistical tolerance.
"""

from collections import Counter
import math
import os
from textwrap import dedent
from typing import Literal

import pytest
import stim as reference_stim

from qdk import Result
import qdk.stim as qdk_stim

Backend = Literal["stabilizer", "cpu", "gpu"]

BACKENDS = [
    "stabilizer",
    "cpu",
    pytest.param(
        "gpu",
        marks=pytest.mark.skipif(
            not os.environ.get("QDK_GPU_TESTS"), reason="QDK_GPU_TESTS is not enabled"
        ),
    ),
]

NOISY_SHOTS = 50_000

SIGMAS = 6

# Rejects any other result, such as a lost qubit.
_BITS = {Result.Zero: "0", Result.One: "1"}


def sample_reference(source: str, shots: int) -> Counter[str]:
    circuit = reference_stim.Circuit(source)
    samples = circuit.compile_sampler().sample(shots=shots)
    return Counter("".join("1" if bit else "0" for bit in row) for row in samples)


def sample_qdk(source: str, shots: int, backend: Backend) -> Counter[str]:
    results = qdk_stim.run(source, shots=shots, type=backend)
    return Counter("".join(_BITS[bit] for bit in row) for row in results)


def assert_matches_stim(source: str, shots: int, backend: Backend) -> None:
    source = dedent(source).strip()
    reference = sample_reference(source, shots)
    actual = sample_qdk(source, shots, backend)
    try:
        check_histograms_match(actual, reference)
    except AssertionError as error:
        raise AssertionError(f"{error}\nProgram:\n{source}") from None


def check_histograms_match(actual: Counter[str], expected: Counter[str]) -> None:
    """Asserts that each outcome has the same frequency in both histograms, using a
    two-proportion z-test."""
    actual_shots = sum(actual.values())  # n1
    reference_shots = sum(expected.values())  # n2
    for outcome in actual.keys() | expected.keys():
        actual_probability = actual[outcome] / actual_shots  # s1 / n1 = p1
        reference_probability = expected[outcome] / reference_shots  # s2 / n2 = p2

        # this comes from the null hypothesis that the two probabilities are equal
        pooled = (actual[outcome] + expected[outcome]) / (
            actual_shots + reference_shots
        )  # (s1 + s2) / (n1 + n2) = p
        standard_deviation = math.sqrt(
            pooled * (1 - pooled) * (1 / actual_shots + 1 / reference_shots)
        )  # sqrt(p * (1 - p) * (1/n1 + 1/n2))
        tolerance = SIGMAS * standard_deviation
        assert abs(actual_probability - reference_probability) <= tolerance, (
            f"Histograms differ for {outcome!r}: "
            f"{actual_probability:.5f} vs {reference_probability:.5f} "
            f"(tolerance={tolerance:.5f}, {SIGMAS} standard deviations, "
            f"actual_shots={actual_shots}, reference_shots={reference_shots}):\n"
            f"Actual: {dict(actual)}\nReference: {dict(expected)}"
        )
