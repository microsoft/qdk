"""Verify complete readout equations against noiseless encoded execution."""

from __future__ import annotations

from dataclasses import dataclass
import qodec as qc

from .._readouts import observe_count_of
from ._parity import ParityAnalysis


@dataclass(frozen=True)
class ReadoutMismatch:
    position: int
    expected_equation: tuple[str, ...] | None
    reason: str


def readout_disagreements(gadget: qc.Gadget) -> list[ReadoutMismatch]:
    count = min(observe_count_of(gadget.implements), len(gadget.readouts))
    if not count:
        return []
    analysis = ParityAnalysis(gadget)
    values, unresolved, dependency_error = analysis.resolved
    mismatches = []
    for position in range(count):
        if dependency_error or position in unresolved:
            mismatches.append(
                ReadoutMismatch(
                    position,
                    None,
                    dependency_error
                    or f"readouts[{position}] is not uniquely determined.",
                )
            )
            continue
        expected = analysis.expected[position]
        difference = values[position] ^ expected
        if difference.is_zero:
            continue
        candidate = analysis.candidate(expected)
        witness_terms = (*analysis.readouts[position], *(candidate or ()))
        reason = (
            "Readout differs on noiseless execution with arbitrary incoming frames.\n"
            f"Witness: {analysis.witness(witness_terms, difference, actual=values[position], expected=expected)}"
        )
        if candidate is None:
            reason += "\nNo equivalent reference equation was derived."
        mismatches.append(ReadoutMismatch(position, candidate, reason))
    return mismatches


__all__ = ["ReadoutMismatch", "readout_disagreements"]
