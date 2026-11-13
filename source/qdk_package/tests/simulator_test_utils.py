# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import math
from collections import Counter
from collections.abc import Iterable, Mapping, Sequence


def statistical_tolerance(shots: int, probability: float, tolerance: float) -> float:
    """
    Returns the deviation to allow between the observed frequency of an outcome over *shots*
    shots and its *probability*: the larger of *tolerance* and five standard deviations.

    A tolerance of one or two standard deviations makes a statistical test pass or fail depending
    on the seed it happens to use, so the test breaks whenever the random numbers change. With
    five, a correct simulator fails a check with probability below 1e-6, whatever the seed.
    """
    sigma = math.sqrt(probability * (1.0 - probability) / shots)
    return max(tolerance, 5.0 * sigma + 1.0 / shots)


def check_histogram(
    results: Sequence[str] | Counter[str],
    expected_probs: Mapping[str, float],
    tolerance: float = 0.05,
) -> None:
    """
    Assert that the probability distribution of *results* matches
    *expected_probs* (a dict mapping str keys to float probabilities)
    within *tolerance*, widened as described in `statistical_tolerance`.
    """
    hist = results if isinstance(results, Counter) else Counter(results)
    n = sum(hist.values())
    assert n > 0, "No results to check"
    all_keys = set(expected_probs.keys()) | set(hist.keys())
    for key in all_keys:
        actual_prob = hist.get(key, 0) / n
        expected_prob = expected_probs.get(key, 0.0)
        allowed = statistical_tolerance(n, expected_prob, tolerance)
        assert abs(actual_prob - expected_prob) <= allowed, (
            f"Key '{key}': expected ~{expected_prob:.2f}, got {actual_prob:.3f} "
            f"({hist.get(key, 0)}/{n}), tolerance {allowed:.3f}"
        )


def expect_distribution(
    results: Iterable[str],
    expected: Mapping[str, float],
    *,
    tolerance: float = 0.01,
) -> None:
    """
    Assert that the frequencies of *results* match the *expected* probabilities within
    *tolerance*, widened as described in `statistical_tolerance`. Outcomes missing from
    *expected* may occur with a frequency of at most *tolerance*.
    """
    histogram = Counter(results)
    shots = sum(histogram.values())
    assert shots > 0, "No results to check"
    actual = {key: count / shots for key, count in histogram.items()}
    for key in set(actual) | set(expected):
        probability = expected.get(key, 0.0)
        allowed = statistical_tolerance(shots, probability, tolerance)
        assert abs(actual.get(key, 0.0) - probability) <= allowed, (
            f"Probability of '{key}' is outside the tolerance of {allowed:.3f}.\n\n"
            f"Expected distribution:\n  {dict(expected)}\n\n"
            f"Actual distribution:\n  {actual}"
        )
