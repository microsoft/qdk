from pathlib import Path
from random import randint
import pytest
from qdk import qsharp


@pytest.fixture(autouse=True)
def setup():
    """Fixture to execute before a test is run"""
    # Setting the project root to current folder.
    this_dir = Path(__file__).parent
    qsharp.init(project_root=this_dir)
    yield  # this is where the testing happens


def test_classical_computation() -> None:
    """Test that Q# code computes f(x) = x^2 correctly using Python test code."""
    for x in range(-10, 11):
        res = qsharp.eval(f"ClassicalFunction.Square({x})")
        assert res == x**2


def test_classical_computation_qsharp() -> None:
    """Test that Q# code computes f(x) = x^2 correctly using Q# test code."""
    qsharp.eval("TestCode.TestSquare()")


def test_measurement_results() -> None:
    """Test that measuring a basis state returns correct measurement results using Python test code."""
    for _ in range(10):
        n = randint(2, 10)
        bits = [bool(randint(0, 1)) for _ in range(n)]
        # When passing Boolean values to Q#, remember to convert them to lowercase
        # (Python uses True and False, while Q# uses true and false)
        res = qsharp.eval(f"Measurement.MeasureBasisState({str(bits).lower()})")
        for i in range(n):
            assert (res[i] == qsharp.Result.One) == bits[i]


def test_measurement_results_qsharp() -> None:
    """Test that measuring a basis state returns correct measurement results using Q# test code."""
    qsharp.eval("TestCode.TestMeasurement()")
