import math
import random

import pytest
from qdk import Context
from qdk.test_utils import ArithmeticOpTester


def _random_coprime(modulus: int, attempts: int = 100) -> int:
    for _ in range(attempts):
        ans = random.randint(2, modulus - 1)
        if math.gcd(ans, modulus) == 1:
            return ans
    raise RuntimeError(f"No coprime is found for {modulus} in {attempts} attempts.")


@pytest.mark.parametrize(
    ("target_size", "exp_size"),
    [(2, 2), (5, 3), (6, 3), (7, 4), (10, 10)],
)
def test_window_modular_exp(target_size: int, exp_size: int, context: Context):
    """Tests for WindowModularExp."""
    mul_w = 2
    exp_w = 2

    for _ in range(5):
        mod = 1 + 2 * random.randint(1, 2 ** (target_size - 1) - 1)
        x = random.randint(0, mod - 1)
        y = random.randint(0, 2**exp_size - 1)
        a = _random_coprime(mod, attempts=100)
        op_name = "Modular.WindowModExp.WindowModularExp"
        op = f"{op_name}(_,_,{a}L,{mod}L,{mul_w},{exp_w})"
        result = ArithmeticOpTester.run_op(op, [target_size, exp_size], [x, y], context)
        assert result == [(x * (a**y)) % mod, y]
