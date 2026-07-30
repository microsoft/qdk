"""Tests for AddConst.qs."""

import random

import pytest
from qdk.test_utils import ArithmeticOpTester

from test.test_utils import get_qdk_context


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [1, 2, 3, 8, 16])
def test_add_constant(n: int, optimize: str):
    """Tests AddConst.AddConstant."""
    context = get_qdk_context(optimize=optimize)
    modulus = 2**n
    for _ in range(10):
        a = random.randint(0, modulus - 1)
        b = random.randint(0, modulus - 1)
        op = f"AddConst.AddConstant({a}L,_)"
        ans = ArithmeticOpTester.run_unary_op(op, n, b, context)
        assert ans == (a + b) % modulus


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [1, 2, 3, 8, 16])
def test_add_constant_controlled(n: int, optimize: str):
    """Tests controlled AddConst.AddConstant."""
    context = get_qdk_context(optimize=optimize)
    modulus = 2**n
    for _ in range(10):
        ctrl = random.randint(0, 1)
        a = random.randint(0, modulus - 1)
        b = random.randint(0, modulus - 1)
        op = f"((ctrl, x) => (Controlled AddConst.AddConstant)(ctrl, ({a}L, x)))"
        ans = ArithmeticOpTester.run_op(op, [1, n], [ctrl, b], context)
        expected = (a + b) % modulus if ctrl == 1 else b
        assert ans == [ctrl, expected]


