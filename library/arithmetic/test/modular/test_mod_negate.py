import random

from qdk import Context
from qdk.test_utils import ArithmeticOpTester


def test_mod_negate(context: Context):
    """Tests for ModNegate."""
    n = 16
    for _ in range(10):
        modulus = random.randint(3, 2**n - 1)
        x = random.randint(0, modulus - 1)
        op = f"Modular.ModNegate.ModNegate(_,{modulus}L)"
        result = ArithmeticOpTester.run_unary_op(op, n, x, context)
        assert result == (-x) % modulus


def test_mod_negate_zero(context: Context):
    """Tests for ModNegate when input is 0."""
    n = 16
    modulus = random.randint(3, 2**n - 1)
    op = f"Modular.ModNegate.ModNegate(_,{modulus}L)"
    assert ArithmeticOpTester.run_unary_op(op, n, 0, context) == 0
