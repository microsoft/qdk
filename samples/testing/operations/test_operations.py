# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.

from pathlib import Path
import pytest
from qdk import qsharp
from qdk.qsharp import dump_operation


@pytest.fixture(autouse=True)
def setup():
    """Fixture to execute before a test is run"""
    # Setting the project root to current folder.
    this_dir = Path(__file__).parent
    qsharp.init(project_root=this_dir)
    yield  # this is where the testing happens


def test_empty_operation() -> None:
    res = dump_operation("qs => ()", 1)
    assert res == [
        [1, 0],
        [0, 1],
    ]


def test_single_qubit_not_gate() -> None:
    res = dump_operation("qs => X(qs[0])", 1)
    assert res == [
        [0, 1],
        [1, 0],
    ]


def test_single_qubit_hadamard_gate() -> None:
    res = dump_operation("qs => H(qs[0])", 1)
    assert res == [
        [0.707107, 0.707107],
        [0.707107, -0.707107],
    ]


def test_two_qubit_cnot_gate() -> None:
    res = dump_operation("qs => CNOT(qs[0], qs[1])", 2)
    assert res == [
        [1, 0, 0, 0],
        [0, 1, 0, 0],
        [0, 0, 0, 1],
        [0, 0, 1, 0],
    ]


def test_custom_operation() -> None:
    qsharp.eval(
        "operation ApplySWAP(qs : Qubit[]) : Unit is Ctl + Adj { SWAP(qs[0], qs[1]); }"
    )

    res = dump_operation("ApplySWAP", 2)
    assert res == [
        [1, 0, 0, 0],
        [0, 0, 1, 0],
        [0, 1, 0, 0],
        [0, 0, 0, 1],
    ]


def test_operation_no_args_in_qsharp_file() -> None:
    res = dump_operation("qs => CustomOperation.ApplySWAP(qs[0], qs[1])", 2)
    assert res == [
        [1, 0, 0, 0],
        [0, 0, 1, 0],
        [0, 1, 0, 0],
        [0, 0, 0, 1],
    ]


def test_operation_with_args_in_qsharp_file() -> None:
    res0 = dump_operation("BellState.AllBellStates(_, 0)", 2)

    assert res0 == [
        [0.707107, 0.0, 0.707107, 0.0],
        [0.0, 0.707107, 0.0, 0.707107],
        [0.0, 0.707107, 0.0, -0.707107],
        [0.707107, 0.0, -0.707107, 0.0],
    ]

    res1 = dump_operation("BellState.AllBellStates(_, 1)", 2)

    assert res1 == [
        [0.707107, 0.0, 0.707107, 0.0],
        [0.0, 0.707107, 0.0, 0.707107],
        [0.0, -0.707107, 0.0, 0.707107],
        [-0.707107, 0.0, 0.707107, 0.0],
    ]

    res2 = dump_operation("BellState.AllBellStates(_, 2)", 2)

    assert res2 == [
        [0.0, 0.707107, 0.0, 0.707107],
        [0.707107, 0.0, 0.707107, 0.0],
        [0.707107, 0.0, -0.707107, 0.0],
        [0.0, 0.707107, 0.0, -0.707107],
    ]

    res3 = dump_operation("BellState.AllBellStates(_, 3)", 2)

    assert res3 == [
        [0.0, 0.707107, 0.0, 0.707107],
        [0.707107, 0.0, 0.707107, 0.0],
        [-0.707107, 0.0, 0.707107, 0.0],
        [0.0, -0.707107, 0.0, 0.707107],
    ]


def test_operation_equivalence_using_fact() -> None:
    qsharp.eval("OperationEquivalence.TestEquivalence()")
