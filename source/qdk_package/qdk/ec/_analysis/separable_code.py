"""Internal tensor-product code view used by action profiling."""

from __future__ import annotations

from itertools import chain
from typing import Mapping

from .propagation.pauli import Pauli, identity
from .code_algebra import SubsystemCode
from .stabilizer_code import StabilizerCode


class SeparableCode(SubsystemCode):
    @staticmethod
    def by_stacking(*codes: SubsystemCode) -> "SeparableCode":
        blocks = []
        offset = 0
        for code in codes:
            mapping = {
                qubit: offset + index
                for index, qubit in enumerate(sorted(code.support))
            }
            blocks.append(_relocate(code, by=mapping))
            offset += len(code.support)
        return SeparableCode(*blocks)

    def __init__(self, *blocks: SubsystemCode):
        if not _are_disjoint(*blocks):
            raise ValueError("Code blocks are not disjoint.")
        self._blocks = blocks
        super().__init__(
            tuple(chain(*(code.stabilizers for code in blocks))),
            tuple(chain(*(code.logical_basis for code in blocks))),
        )

    @property
    def blocks(self) -> tuple[SubsystemCode, ...]:
        return self._blocks

    def __add__(self, addend: SubsystemCode) -> "SeparableCode":
        add_blocks = addend.blocks if isinstance(addend, SeparableCode) else (addend,)
        return SeparableCode(*(tuple(self.blocks) + tuple(add_blocks)))

    def __iadd__(self, addend: SubsystemCode) -> "SeparableCode":
        return self + addend

    def __sub__(self, subtrahend: SubsystemCode) -> "SeparableCode":
        sub_blocks = (
            set(subtrahend.blocks)
            if isinstance(subtrahend, SeparableCode)
            else {subtrahend}
        )
        return SeparableCode(*(set(self.blocks) - sub_blocks))

    def __isub__(self, subtrahend: SubsystemCode) -> "SeparableCode":
        return self - subtrahend


def _are_disjoint(*blocks: SubsystemCode) -> bool:
    supports = [block.support for block in blocks]
    support = set(chain.from_iterable(supports))
    return len(support) == sum(map(len, supports))


def _remap_pauli(pauli: Pauli, mapping: Mapping[int, int]) -> Pauli:
    return Pauli(
        {mapping.get(qubit, qubit): pauli[qubit] for qubit in pauli.support}
    ) * identity(pauli.phase)


def _relocate(code: SubsystemCode, *, by: Mapping[int, int]) -> SubsystemCode:
    generators = tuple(_remap_pauli(generator, by) for generator in code.stabilizers)
    logicals = tuple(_remap_pauli(generator, by) for generator in code.logical_basis)
    return StabilizerCode(generators, logical_basis=logicals)


__all__ = ["SeparableCode"]
