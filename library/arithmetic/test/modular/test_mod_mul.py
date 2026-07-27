import random

from qdk import Context
from qdk.test_utils import ArithmeticOpTester


def test_mod_double(context: Context):
    """Tests for ModDouble."""
    n = 20
    for _ in range(10):
        # The modulus is an odd number in the range [3, 2^n-1].
        modulus = random.randint(0, 2 ** (n - 1) - 2) * 2 + 3
        x = random.randint(0, modulus - 1)
        op = f"Modular.ModMul.ModDouble(_,{modulus}L)"
        assert ArithmeticOpTester.run_unary_op(op, n, x, context) == (2 * x) % modulus


def test_mod_mul(context: Context):
    """Tests for ModMul."""
    n = 8
    for _ in range(10):
        # The modulus is an odd number in the range [3, 2^n-1].
        modulus = random.randint(0, 2 ** (n - 1) - 2) * 2 + 3
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)
        z = random.randint(0, modulus - 1)
        op = f"Modular.ModMul.ModMul(_,_,_,{modulus}L)"
        result = ArithmeticOpTester.run_op(op, [n, n, n], [x, y, z], context)
        assert result == [x, y, ((z << (n - 1)) + x * y) % modulus]


def test_mod_square(context: Context):
    """Tests for ModSquare."""
    n = 8
    for _ in range(10):
        # The modulus is an odd number in the range [3, 2^n-1].
        modulus = random.randint(0, 2 ** (n - 1) - 2) * 2 + 3
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)
        op = f"Modular.ModMul.ModSquare(_,_,{modulus}L)"
        result = ArithmeticOpTester.run_op(op, [n, n], [x, y], context)
        assert result == [x, ((y << (n - 1)) + (x * x)) % modulus]
