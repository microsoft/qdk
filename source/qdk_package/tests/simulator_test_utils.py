# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from collections import Counter
from collections.abc import Mapping, Sequence

def check_histogram(
	results: Sequence[str] | Counter[str],
	expected_probs: Mapping[str, float],
	tolerance: float = 0.05,
) -> None:
	"""
    Assert that the probability distribution of *results* matches
    *expected_probs* (a dict mapping str keys to float probabilities)
    within *tolerance*.
    """
	hist = results if isinstance(results, Counter) else Counter(results)
	n = sum(hist.values())
	assert n > 0, "No results to check"
	all_keys = set(expected_probs.keys()) | set(hist.keys())
	for key in all_keys:
		actual_prob = hist.get(key, 0) / n
		expected_prob = expected_probs.get(key, 0.0)
		assert abs(actual_prob - expected_prob) <= tolerance, (
			f"Key '{key}': expected ~{expected_prob:.2f}, got {actual_prob:.3f} "
			f"({hist.get(key, 0)}/{n})"
		)
