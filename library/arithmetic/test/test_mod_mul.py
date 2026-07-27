import random

from qdk.test_utils import ArithmeticOpTester

from test.test_utils import get_qdk_context


def test_mod_double():
    """Tests for ModDouble."""
    context = get_qdk_context()
    n = 20
    for _ in range(10):
        # The modulus is an odd number in the range [3, 2^n-1].
        modulus = random.randint(0, 2 ** (n - 1) - 2) * 2 + 3
        x = random.randint(0, modulus - 1)
        op = f"ModMul.ModDouble(_,{modulus}L)"
        assert ArithmeticOpTester.run_unary_op(op, n, x, context) == (2 * x) % modulus


def test_mod_mul():
    """Tests for ModMul."""
    context = get_qdk_context()
    n = 8
    for _ in range(10):
        # The modulus is an odd number in the range [3, 2^n-1].
        modulus = random.randint(0, 2 ** (n - 1) - 2) * 2 + 3
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)
        z = random.randint(0, modulus - 1)
        op = f"ModMul.ModMul(_,_,_,{modulus}L)"
        result = ArithmeticOpTester.run_op(op, [n, n, n], [x, y, z], context)
        assert result == [x, y, ((z << (n - 1)) + x * y) % modulus]


def test_mod_square():
    """Tests for ModSquare."""
    context = get_qdk_context()
    n = 8
    for _ in range(10):
        # The modulus is an odd number in the range [3, 2^n-1].
        modulus = random.randint(0, 2 ** (n - 1) - 2) * 2 + 3
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)
        op = f"ModMul.ModSquare(_,_,{modulus}L)"
        result = ArithmeticOpTester.run_op(op, [n, n], [x, y], context)
        assert result == [x, ((y << (n - 1)) + (x * x)) % modulus]
