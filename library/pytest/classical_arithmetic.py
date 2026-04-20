import qsharp
import pytest
import random


# To run:
# cd source/pip && maturin develop
# pytest ../../library/pytest/classical_arithmetic.py


@pytest.mark.parametrize("mode", [0, 1])
def test_increment(mode: int):
    size = 4
    for x in range(0, 2**size):
        op = f"Std.ClassicalArithmetic.Increment(_,{mode})"
        result = qsharp.eval(f"Std.TestUtils.TestUnaryOp({op},{size},{x}L)")
        assert result == ((x + 1) % (2**size))


@pytest.mark.parametrize("mode", [0, 1])
def test_modexp(mode: int):
    size, a, y, n = 4, 7, 6, 13
    op = f"Std.ClassicalArithmetic.ModExp(_,_,{a}L,{n}L,{mode})"
    result = qsharp.eval(f"Std.TestUtils.TestBinaryOp({op},{size},{size},1L,{y}L)")
    assert result == ((a**y) % n, y)


def profile():
    pass


if __name__ == "__main__":
    profile()
