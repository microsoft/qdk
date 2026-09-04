"""Code profiling accepts qodec's canonical code type."""

import qodec as qc
from paulimer import SparsePauli

from qdk.ec import SubsystemCode
from qdk.ec._distance import code_distance_of


def repetition_code() -> qc.Code:
    return qc.Code(
        "repetition_2",
        stabilizers=["Z_0 Z_1"],
        x=["X_0 X_1"],
        z=["Z_0"],
    )


def test_syndrome_of_accepts_qodec_code() -> None:
    view = SubsystemCode.of(repetition_code())
    assert view.syndrome_of(SparsePauli({0: "X"})) == frozenset({0})


def test_code_distance_of_accepts_qodec_code() -> None:
    distance, witness = code_distance_of(repetition_code(), errors="X")
    assert distance == 2
    assert len(witness) == 2
