"""Tests for Add.qs."""

import random

import pytest
from qdk.test_utils import ArithmeticOpTester

from test.test_utils import get_qdk_context


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [8, 16])
def test_add(n: int, optimize: str):
    """Tests AddConst.AddConstant."""
    context = get_qdk_context(optimize=optimize)
    modulus = 2**n
    tester = ArithmeticOpTester("Add.Add", [n, n], context)
    for _ in range(10):
        a = random.randint(0, modulus - 1)
        b = random.randint(0, modulus - 1)
        ans = tester.run([a, b])
        assert ans == [a, (a + b) % modulus]


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [8, 16])
def test_add_resource_estimates(n: int, optimize: str):
    context = get_qdk_context(optimize=optimize)
    tester = ArithmeticOpTester("Add.Add", [n, n], context)
    counts = context.logical_counts(tester.test_callable, [0, 0])
    num_qubits, num_ccz = counts._data["numQubits"], counts._data["cczCount"]
    if optimize == "space":
        assert (num_qubits, num_ccz) == (2 * n, 2 * n - 2)
    elif optimize == "time":
        assert (num_qubits, num_ccz) == (3 * n, n - 1)
