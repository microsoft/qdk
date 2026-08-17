import random

import pytest
from qdk.test_utils import ArithmeticOpTester

from test.test_utils import get_qdk_context


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add(n: int, optimize: str):
    context = get_qdk_context(optimize=optimize)
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)
        op = f"ModAdd.ModAdd(_,_,{modulus}L)"
        result = ArithmeticOpTester.run_op(op, [n, n], [x, y], context)
        assert result == [x, (x + y) % modulus]


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add_controlled(n: int, optimize: str):
    context = get_qdk_context(optimize=optimize)
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)

        # Test with control = 1 (should apply ModAdd)
        op = f"((c,x,y)=>Controlled ModAdd.ModAdd(c,(x,y,{modulus}L)))"
        tester = ArithmeticOpTester(op, [1, n, n], context)
        result = tester.run([1, x, y])
        assert result == [1, x, (x + y) % modulus]

        # Test with control = 0 (should not apply)
        result = tester.run([0, x, y])
        assert result == [0, x, y]


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add_adjoint(n: int, optimize: str):
    context = get_qdk_context(optimize=optimize)
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)

        # Adjoint ModAdd should compute (x, (y - x) % modulus)
        op = f"(Adjoint ModAdd.ModAdd(_,_,{modulus}L))"
        result = ArithmeticOpTester.run_op(op, [n, n], [x, y], context)
        assert result == [x, (y - x) % modulus]


@pytest.mark.parametrize("optimize", ["space", "time"])
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add_controlled_adjoint(n: int, optimize: str):
    context = get_qdk_context(optimize=optimize)
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)

        # Test with control = 1 (should apply Adjoint ModAdd)
        op = f"((c,x,y)=>Adjoint Controlled ModAdd.ModAdd(c,(x,y,{modulus}L)))"
        tester = ArithmeticOpTester(op, [1, n, n], context)
        result = tester.run([1, x, y])
        assert result == [1, x, (y - x) % modulus]

        # Test with control = 0 (should not apply)
        result = tester.run([0, x, y])
        assert result == [0, x, y]
