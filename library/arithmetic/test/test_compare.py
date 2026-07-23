"""Tests for Compare.qs."""

import random
from collections.abc import Callable

import pytest
from qdk import Context
from qdk.test_utils import ArithmeticOpTester

COMPARE_CASES = [
    ("CompareGT", lambda x, y: x > y),
    ("CompareLT", lambda x, y: x < y),
    ("CompareGE", lambda x, y: x >= y),
    ("CompareLE", lambda x, y: x <= y),
    ("CompareEQ", lambda x, y: x == y),
]


@pytest.mark.parametrize("context", ["min_gates", "min_qubits"], indirect=True)
@pytest.mark.parametrize("n", [1, 2, 5, 20])
@pytest.mark.parametrize(("op_name", "predicate"), COMPARE_CASES)
def test_compare(n: int, op_name: str, predicate: Callable, context: Context):
    """Test compare operations."""
    op = f"((x, y, result) => Compare.{op_name}(x, y, result[0]))"
    tester = ArithmeticOpTester(op, [n, n, 1], context)

    # Explicitly cover x == y corner case.
    ans_bit_eq = random.randint(0, 1)
    x_eq = random.randint(0, 2**n - 1)
    result_eq = tester.run([x_eq, x_eq, ans_bit_eq])
    assert result_eq == [
        x_eq,
        x_eq,
        1 - ans_bit_eq if predicate(x_eq, x_eq) else ans_bit_eq,
    ]

    for _ in range(5):
        ans_bit = random.randint(0, 1)
        x = random.randint(0, 2**n - 1)
        y = random.randint(0, 2**n - 1)

        result = tester.run([x, y, ans_bit])
        assert result == [x, y, 1 - ans_bit if predicate(x, y) else ans_bit]


@pytest.mark.parametrize("context", ["min_gates", "min_qubits"], indirect=True)
@pytest.mark.parametrize("n", [1, 2, 5, 20])
@pytest.mark.parametrize(("op_name", "predicate"), COMPARE_CASES)
def test_compare_controlled(
    n: int, op_name: str, predicate: Callable, context: Context
):
    """Test controlled compare operations."""
    op = f"((ctrl, x, y, result) => (Controlled Compare.{op_name})(ctrl, (x, y, result[0])))"
    tester = ArithmeticOpTester(op, [1, n, n, 1], context)

    # Explicitly cover x == y corner case.
    ans_bit_eq = random.randint(0, 1)
    x_eq = random.randint(0, 2**n - 1)
    result_eq_0 = tester.run([0, x_eq, x_eq, ans_bit_eq])
    assert result_eq_0 == [0, x_eq, x_eq, ans_bit_eq]
    result_eq_1 = tester.run([1, x_eq, x_eq, ans_bit_eq])
    assert result_eq_1 == [
        1,
        x_eq,
        x_eq,
        1 - ans_bit_eq if predicate(x_eq, x_eq) else ans_bit_eq,
    ]

    for _ in range(5):
        ans_bit = random.randint(0, 1)
        x = random.randint(0, 2**n - 1)
        y = random.randint(0, 2**n - 1)

        result_0 = tester.run([0, x, y, ans_bit])
        assert result_0 == [0, x, y, ans_bit]
        result_1 = tester.run([1, x, y, ans_bit])
        assert result_1 == [1, x, y, 1 - ans_bit if predicate(x, y) else ans_bit]
