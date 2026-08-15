"""Relocation compilers.

`Relocate` and `AutoRelocate` rewrite qubit labels in a `Program`.
They are intended to follow `RecursiveLowering`, which always emits
namespaced labels of the form ``"<block_name>.<index>"``.

Relocation operates on a flat program: it walks every qubit label of
every call and rewrites it through a label-to-label map. Labels absent
from the map pass through unchanged.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Hashable

from ..._operands import QubitLabel, label_text, map_call_labels, qubit_labels

from qodec.circuits import Program

from .compiler import CompileResult


class Relocate:
    """Rewrite qubit labels using an explicit label → label map.

    Useful for assigning concrete physical qubit indices to namespaced
    labels produced by `RecursiveLowering`. The map can use either
    namespaced source labels (``"alice.0"``) or per-block prefix-style
    expansions (see `Relocate.from_block_placement`).

    Labels not in the map pass through unchanged.
    """

    def __init__(self, label_map: Mapping[str, Hashable]) -> None:
        self._map: dict[str, str] = {k: str(v) for k, v in label_map.items()}

    @property
    def label_map(self) -> dict[str, str]:
        return dict(self._map)

    def compile(self, program: Program) -> CompileResult:
        return CompileResult(program=_remap_program(program, self._map))

    @classmethod
    def from_block_placement(
        cls,
        placement: Mapping[str, list[Hashable]],
    ) -> "Relocate":
        """Build a `Relocate` from a ``{block_name: [physical_labels]}`` map.

        Expands each block's entry into the namespaced labels emitted by
        `RecursiveLowering`: ``placement[name][i]`` becomes the
        replacement for the source label ``f"{name}.{i}"``.
        """
        flat: dict[str, str] = {}
        for block_name, labels in placement.items():
            for i, label in enumerate(labels):
                flat[f"{block_name}.{i}"] = str(label)
        return cls(flat)


class AutoRelocate:
    """Renumber qubit labels to consecutive integers in first-seen order.

    Walks the program once to collect every distinct qubit label, then
    assigns each an integer index starting from ``start``.
    """

    def __init__(self, *, start: int = 0) -> None:
        self._start = start

    def compile(self, program: Program) -> CompileResult:
        labels: list[str] = []
        seen: set[str] = set()
        for call in program.instructions:
            for value in (*call.inputs.values(), *call.outputs.values()):
                for label in qubit_labels(value):
                    text = label_text(label)
                    if text in seen:
                        continue
                    seen.add(text)
                    labels.append(text)
        label_map = {label: str(self._start + i) for i, label in enumerate(labels)}
        return CompileResult(program=_remap_program(program, label_map))


def _remap_program(program: Program, label_map: Mapping[str, str]) -> Program:
    def relabel(label: QubitLabel) -> QubitLabel:
        return label_map.get(label_text(label), label)

    return Program(
        [map_call_labels(call, relabel) for call in program.instructions],
        program.isa,
    )
