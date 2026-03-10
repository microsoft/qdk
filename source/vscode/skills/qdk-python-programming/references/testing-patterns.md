# Testing Q# from Python (pytest)

Use pytest to write unit tests for Q# operations, verifying quantum states, operation matrices, classical return values, and measurement results.

## Setup

```python
import pytest
import qsharp

@pytest.fixture(autouse=True)
def setup():
    qsharp.init(project_root=".")
    yield
```

## Test Quantum States

```python
# Exact state comparison
def test_bell_state():
    qsharp.eval("use qs = Qubit[2]; H(qs[0]); CNOT(qs[0], qs[1]);")
    state = qsharp.dump_machine().as_dense_state()
    assert state == pytest.approx([0.707107, 0, 0, 0.707107])

# State comparison ignoring global phase
def test_state_up_to_phase():
    qsharp.eval("use qs = Qubit[2]; PrepareState(qs);")
    state = qsharp.dump_machine()
    expected = [0.5, 0.5j, -0.5, -0.5j]
    assert state.check_eq(expected)  # tolerates global phase
```

## Test Operation Matrices

```python
from qsharp.utils import dump_operation

def test_identity():
    assert dump_operation("qs => ()", 1) == [[1, 0], [0, 1]]

def test_not_gate():
    assert dump_operation("qs => X(qs[0])", 1) == [[0, 1], [1, 0]]

def test_hadamard():
    assert dump_operation("qs => H(qs[0])", 1) == [
        [0.707107, 0.707107],
        [0.707107, -0.707107],
    ]

def test_cnot():
    assert dump_operation("qs => CNOT(qs[0], qs[1])", 2) == [
        [1, 0, 0, 0],
        [0, 1, 0, 0],
        [0, 0, 0, 1],
        [0, 0, 1, 0],
    ]

# Test custom operations defined in Q# files
def test_custom_swap():
    qsharp.eval(
        "operation ApplySWAP(qs : Qubit[]) : Unit is Ctl + Adj { SWAP(qs[0], qs[1]); }"
    )
    assert dump_operation("ApplySWAP", 2) == [
        [1, 0, 0, 0],
        [0, 0, 1, 0],
        [0, 1, 0, 0],
        [0, 0, 0, 1],
    ]

# Test operations with parameters
def test_parameterized_operation():
    res = dump_operation("BellState.AllBellStates(_, 0)", 2)
    assert res[0] == pytest.approx([0.707107, 0.0, 0.707107, 0.0])
```

## Test Classical Return Values

```python
def test_square():
    for x in range(-10, 11):
        result = qsharp.eval(f"MyMath.Square({x})")
        assert result == x ** 2

# Can also test using Q# test code
def test_classical_via_qsharp():
    qsharp.eval("TestCode.TestSquare()")
```

## Test Measurement Results

```python
def test_measurement():
    bits = [True, False, True]
    # Python True/False must be lowercased for Q#
    results = qsharp.eval(f"MeasureBasisState({str(bits).lower()})")
    for i, bit in enumerate(bits):
        assert (results[i] == qsharp.Result.One) == bit
```

## Test Operation Equivalence

```python
# Use Q#'s built-in operation equivalence checking
def test_equivalence():
    qsharp.eval("OperationEquivalence.TestEquivalence()")
```
