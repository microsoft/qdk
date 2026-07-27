import random

import pytest
from qdk.test_utils import ArithmeticOpTester

from test.test_utils import get_qdk_context


def test_compress_garbage() -> None:
    """Tests that the compression circuit always leaves last qubit in 0 state."""
    context = get_qdk_context()

    def _is_valid_input(x: int) -> bool:
        for i in range(3):
            b0 = (x >> (2 * i)) & 1
            b1 = (x >> (2 * i + 1)) & 1
            if b0 == 0 and b1 == 1:
                return False
        return True

    tester = ArithmeticOpTester("ModDiv.CompressGarbage", [6], context)
    for x in range(2**6):
        if _is_valid_input(x):
            result = tester.run([x])[0]
            assert 0 <= result < 32


@pytest.mark.parametrize("num_bits,modulus", [(3, 5), (4, 13), (5, 31), (6, 61)])
def test_mod_mul(num_bits: int, modulus: int) -> None:
    context = get_qdk_context()
    op = f"ModDiv.ModMul(_,_,{modulus}L)"
    tester = ArithmeticOpTester(op, [num_bits, num_bits], context)
    for _ in range(5):
        x = random.randint(1, modulus - 1)
        y = random.randint(0, modulus - 1)
        result = tester.run([x, y])
        assert result == [x, (y * x) % modulus]


@pytest.mark.parametrize("num_bits,modulus", [(3, 5), (4, 13), (5, 31), (6, 61)])
def test_safe_mod_mul(num_bits: int, modulus: int) -> None:
    context = get_qdk_context()
    op = f"ModDiv.SafeModMul(_,_,{modulus}L)"
    tester = ArithmeticOpTester(op, [num_bits, num_bits], context)

    # Special branch: x == 0 should leave y unchanged.
    for _ in range(5):
        y = random.randint(0, modulus - 1)
        result = tester.run([0, y])
        assert result == [0, y]

    # Regular branch: x > 0 should behave as modular multiplication.
    for _ in range(5):
        x = random.randint(1, modulus - 1)
        y = random.randint(0, modulus - 1)
        result = tester.run([x, y])
        assert result == [x, (y * x) % modulus]


@pytest.mark.parametrize("num_bits,modulus", [(3, 5), (4, 13), (5, 31), (6, 61)])
def test_mod_div(num_bits: int, modulus: int) -> None:
    context = get_qdk_context()
    op = f"ModDiv.ModDiv(_,_,{modulus}L)"
    tester = ArithmeticOpTester(op, [num_bits, num_bits], context)
    for _ in range(5):
        x = random.randint(1, modulus - 1)
        y = random.randint(0, modulus - 1)
        result = tester.run([x, y])
        assert result == [x, (y * pow(x, -1, modulus)) % modulus]


@pytest.mark.parametrize("num_bits,modulus", [(3, 5), (4, 13), (5, 31), (6, 61)])
def test_safe_mod_div(num_bits: int, modulus: int) -> None:
    context = get_qdk_context()
    op = f"ModDiv.SafeModDiv(_,_,{modulus}L)"
    tester = ArithmeticOpTester(op, [num_bits, num_bits], context)

    # Special branch: x == 0 should leave y unchanged.
    for _ in range(5):
        y = random.randint(0, modulus - 1)
        result = tester.run([0, y])
        assert result == [0, y]

    # Regular branch: x > 0 should behave as modular division.
    for _ in range(5):
        x = random.randint(1, modulus - 1)
        y = random.randint(0, modulus - 1)
        result = tester.run([x, y])
        assert result == [x, (y * pow(x, -1, modulus)) % modulus]


@pytest.mark.parametrize("num_bits,modulus", [(3, 5), (4, 13), (5, 31), (6, 61)])
def test_mod_inv(num_bits: int, modulus: int) -> None:
    context = get_qdk_context()
    op = f"ModDiv.ModInv(_,_,{modulus}L)"
    tester = ArithmeticOpTester(op, [num_bits, num_bits], context)
    for _ in range(5):
        x = random.randint(1, modulus - 1)
        result = tester.run([x, 0])
        assert result == [x, pow(x, -1, modulus)]
