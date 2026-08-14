"""Target result carriers."""

import pytest

from qdk.ec.targets import AnnotatedBatch, leaks_of, probabilities_of


def test_annotated_batch_is_sequence_with_probabilities() -> None:
    batch = AnnotatedBatch([[True, False]], probabilities=[[0.1, 0.2]])
    assert len(batch) == 1
    assert batch[0] == [True, False]
    assert batch.probabilities is not None
    assert batch.probabilities[0] == [0.1, 0.2]


def test_annotated_batch_carries_leaks() -> None:
    batch = AnnotatedBatch([[True, False]], leaks=[[False, True]])
    assert batch.leaks is not None
    assert batch.leaks[0] == [False, True]


def test_annotated_batch_carries_both_channels_at_once() -> None:
    batch = AnnotatedBatch(
        [[True, False]], probabilities=[[0.1, 0.2]], leaks=[[False, True]]
    )
    assert probabilities_of(batch) == [[0.1, 0.2]]
    assert leaks_of(batch) == [[False, True]]


def test_a_plain_batch_carries_no_channels() -> None:
    assert probabilities_of([[True, False]]) is None
    assert leaks_of([[True, False]]) is None


def test_channel_shot_count_must_match() -> None:
    with pytest.raises(ValueError, match="probabilities shots"):
        AnnotatedBatch([[True], [False]], probabilities=[[0.1]])
    with pytest.raises(ValueError, match="leaks shots"):
        AnnotatedBatch([[True], [False]], leaks=[[False]])
