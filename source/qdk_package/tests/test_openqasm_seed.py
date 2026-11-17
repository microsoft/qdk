# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from qdk.openqasm import run

SOURCE = 'include "stdgates.inc"; qubit[16] q; h q; output bit[16] c; c = measure q;'


@pytest.mark.parametrize(
    "seed, error",
    [(1.5, TypeError), (-1, OverflowError), (2**64, OverflowError)],
)
def test_run_rejects_invalid_seed(seed, error) -> None:
    with pytest.raises(error):
        run(SOURCE, shots=1, seed=seed)


@pytest.mark.parametrize("seed", [0, 1, 2**64 - 1])
def test_run_with_seed_is_reproducible(seed) -> None:
    assert run(SOURCE, shots=1, seed=seed) == run(SOURCE, shots=1, seed=seed)


def test_run_accepts_optional_seed() -> None:
    assert len(run(SOURCE, shots=1)) == 1
    assert len(run(SOURCE, shots=1, seed=None)) == 1
