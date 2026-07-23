"""Tests for MultiControl.qs."""

import random

import pytest
from qdk import Context
from qdk.test_utils import ArithmeticOpTester


@pytest.mark.parametrize("n", [1, 2, 3, 4, 5, 6, 7, 8, 10, 20])
def test_multi_control(context: Context, n: int):
    """Correctness tests for MultiControl."""
    op = "MultiControl.MultiControl"
    op = f"((ctrl, target) => {op}(ctrl, target[0]))"
    values = []
    if n <= 6:
        values = list(range(2**n))
    else:
        values = [2**n - 1] + [random.randint(0, 2**n - 2) for _ in range(10)]
    tester = ArithmeticOpTester(op, [n, 1], context)
    for ctl_val in values:
        target_val = random.randint(0, 1)
        result = tester.run([ctl_val, target_val])
        expected = target_val ^ 1 if (ctl_val == 2**n - 1) else target_val
        assert result == [ctl_val, expected]
