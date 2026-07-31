"""Relocation compilers.

`Relocate` and `AutoRelocate` rewrite qubit labels in a `Program`.
They are intended to follow `RecursiveLowering`, which always emits
namespaced labels of the form ``"<block_name>.<index>"``.

Relocation operates on a flat program: it walks every call's
``inputs`` and ``outputs``, splits each value into whitespace-separated
qubit-label tokens, and rewrites each token through a label-to-label
map. Tokens that don't appear in the map pass through unchanged.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Hashable

from ..._typed_ir import value_to_string as _value_to_string
from ..._typed_ir import value_tokens as _value_tokens

import qodec
from qodec.circuits import Program

from .compiler import CompileResult


class Relocate:
    """Rewrite qubit labels using an explicit label → label map.

    Useful for assigning concrete physical qubit indices to namespaced
    labels produced by `RecursiveLowering`. The map can use either
    namespaced source labels (``"alice.0"``) or per-block prefix-style
    expansions (see `Relocate.from_block_placement`).

    Tokens not in the map pass through unchanged.
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

    Walks the program once to collect every distinct qubit-label token,
    then assigns each label an integer index starting from ``start``.
    """

    def __init__(self, *, start: int = 0) -> None:
        self._start = start

    def compile(self, program: Program) -> CompileResult:
        labels: list[str] = []
        seen: set[str] = set()
        for call in program.instructions:
            for value in (*call.inputs.values(), *call.outputs.values()):
                for token in _value_tokens(value):
                    if token in seen:
                        continue
                    if _is_int_token(token):
                        # Pure-integer tokens don't need re-mapping if we want
                        # them to keep their numeric meaning. But for "renumber
                        # in first-seen order" we treat all labels uniformly.
                        pass
                    seen.add(token)
                    labels.append(token)
        label_map = {label: str(self._start + i) for i, label in enumerate(labels)}
        return CompileResult(program=_remap_program(program, label_map))


def _remap_program(program: Program, label_map: Mapping[str, str]) -> Program:
    new_calls: list[qodec.instructions.InstructionCall] = []
    for call in program.instructions:
        new_inputs = {n: _remap_value(v, label_map) for n, v in call.inputs.items()}
        new_outputs = {n: _remap_value(v, label_map) for n, v in call.outputs.items()}
        new_calls.append(
            qodec.instructions.InstructionCall(
                call.mnemonic,
                inputs=new_inputs,
                outputs=new_outputs,
                parameters=call.parameters,
            )
        )
    return Program(new_calls, program.isa)


def _remap_value(value: object, label_map: Mapping[str, str]) -> str:
    tokens = _value_tokens(value)
    if not tokens:
        return _value_to_string(value)
    return " ".join(label_map.get(token, token) for token in tokens)


def _is_int_token(token: str) -> bool:
    try:
        int(token)
        return True
    except ValueError:
        return False
