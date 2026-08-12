"""Code profiling accepts qodec's canonical code type."""
import qodec
from paulimer import SparsePauli

from qdk.ec.code import syndrome_of
from qdk.ec.distance import code_distance_of


def repetition_code() -> qodec.Code:
    return qodec.Code(
        "repetition_2",
        stabilizers=["Z_0 Z_1"],
        x=["X_0 X_1"],
        z=["Z_0"],
    )


def test_syndrome_of_accepts_qodec_code() -> None:
    assert syndrome_of(repetition_code(), SparsePauli({0: "X"})) == {0}


def test_code_distance_of_accepts_qodec_code() -> None:
    distance, witness = code_distance_of(repetition_code(), errors="X")
    assert distance == 2
    assert len(witness) == 2
