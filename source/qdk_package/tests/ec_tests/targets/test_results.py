"""Target result carriers."""
from collections.abc import Sequence

import pytest

from qdk.ec.targets import HeraldedBatch, SoftBatch


def test_soft_batch_is_sequence_with_probabilities() -> None:
    batch = SoftBatch([[True, False]], [[0.1, 0.2]])
    assert isinstance(batch, Sequence)
    assert list(batch[0]) == [True, False]
    assert batch.probabilities[0] == [0.1, 0.2]


def test_heralded_batch_carries_leaks() -> None:
    batch = HeraldedBatch([[True, False]], [[False, True]])
    assert batch.leaks[0] == [False, True]


def test_result_carriers_validate_shot_count() -> None:
    with pytest.raises(ValueError, match="probabilities shots"):
        SoftBatch([[True], [False]], [[0.1]])
    with pytest.raises(ValueError, match="leaks shots"):
        HeraldedBatch([[True], [False]], [[False]])
