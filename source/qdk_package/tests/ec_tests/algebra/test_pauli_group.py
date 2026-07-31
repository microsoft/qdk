from typing import Sequence
from paulimer import PauliGroup

from qdk.ec.profile.propagation.pauli import Pauli


def test_intersection_of() -> None:
    assert 2 ** (PauliGroup([]) & PauliGroup([])).log2_size == 1

    group1 = PauliGroup([Pauli({0: "X"}), Pauli({1: "Y"})])
    group2 = PauliGroup([Pauli({2: "Z"})])
    assert 2 ** (group1 & group2).log2_size == 1
    group1 = PauliGroup([Pauli({0: "X"}), Pauli({1: "Y"}), Pauli({2: "Z"})])
    group2 = PauliGroup([Pauli({0: "X", 1: "Y", 2: "Z"})])
    intersection = group1 & group2
    assert 2 ** intersection.log2_size > 0
    for pauli in intersection.elements:
        assert pauli in group1 and pauli in group2


def are_all_commuting(paulis: Sequence[Pauli]) -> bool:
    for i, pauli1 in enumerate(paulis):
        for pauli2 in paulis[i + 1 :]:
            if not pauli1.commutes_with(pauli2):
                return False
    return True
