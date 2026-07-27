import random

import pytest
from qdk import Context
from qdk.test_utils import ArithmeticOpTester


@pytest.mark.parametrize("context", ["min_gates", "min_qubits"], indirect=True)
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add(n: int, context: Context):
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)
        op = f"Modular.ModAdd.ModAdd(_,_,{modulus}L)"
        result = ArithmeticOpTester.run_op(op, [n, n], [x, y], context)
        assert result == [x, (x + y) % modulus]


@pytest.mark.parametrize("context", ["min_gates", "min_qubits"], indirect=True)
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add_controlled(n: int, context: Context):
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)

        # Test with control = 1 (should apply ModAdd)
        op = f"((c,x,y)=>Controlled Modular.ModAdd.ModAdd(c,(x,y,{modulus}L)))"
        tester = ArithmeticOpTester(op, [1, n, n], context)
        result = tester.run([1, x, y])
        assert result == [1, x, (x + y) % modulus]

        # Test with control = 0 (should not apply)
        result = tester.run([0, x, y])
        assert result == [0, x, y]


@pytest.mark.parametrize("context", ["min_gates", "min_qubits"], indirect=True)
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add_adjoint(n: int, context: Context):
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)

        # Adjoint ModAdd should compute (x, (y - x) % modulus)
        op = f"(Adjoint Modular.ModAdd.ModAdd(_,_,{modulus}L))"
        result = ArithmeticOpTester.run_op(op, [n, n], [x, y], context)
        assert result == [x, (y - x) % modulus]


@pytest.mark.parametrize("context", ["min_gates", "min_qubits"], indirect=True)
@pytest.mark.parametrize("n", [2, 5, 20])
def test_mod_add_controlled_adjoint(n: int, context: Context):
    for _ in range(10):
        modulus = random.randint(2, 2**n - 1)
        x = random.randint(0, modulus - 1)
        y = random.randint(0, modulus - 1)

        # Test with control = 1 (should apply Adjoint ModAdd)
        op = f"((c,x,y)=>Adjoint Controlled Modular.ModAdd.ModAdd(c,(x,y,{modulus}L)))"
        tester = ArithmeticOpTester(op, [1, n, n], context)
        result = tester.run([1, x, y])
        assert result == [1, x, (y - x) % modulus]

        # Test with control = 0 (should not apply)
        result = tester.run([0, x, y])
        assert result == [0, x, y]
