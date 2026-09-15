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
    summary: str
    reason: str


def readout_disagreements(gadget: qc.Gadget) -> list[ReadoutMismatch]:
    count = min(observe_count_of(gadget.implements), len(gadget.readouts))
    if not count:
        return []
    analysis = ParityAnalysis(gadget)
    values, unresolved, dependency_error = analysis.resolved
    observables = [
        observable
        for action in gadget.implements.action
        if isinstance(action, qc.actions.Observe)
        for observable in action.observables
    ]
    mismatches = []
    for position in range(count):
        required = f"logical {observables[position]} measurement"
        if dependency_error or position in unresolved:
            problem = (
                "inconsistent readout equations"
                if dependency_error
                else "an undetermined readout"
            )
            explanation = (
                "The defining equations cannot all hold for every noiseless circuit result.\n"
                + dependency_error
                if dependency_error
                else "The defining equations allow both 0 and 1 for this readout; its value is not uniquely determined."
            )
            mismatches.append(
                ReadoutMismatch(
                    position,
                    None,
                    f"readouts[{position}] cannot report the required {required}: {problem}",
                    explanation,
                )
            )
            continue
        expected = analysis.expected[position]
        difference = values[position] ^ expected
        if difference.is_zero:
            continue
        candidate = analysis.candidate(expected)
        summary = f"readouts[{position}] does not report the required {required}"
        if candidate is not None:
            reason = ""
        else:
            summary = f"The circuit does not provide the required {required} result for readouts[{position}]"
            reason = "No readout formula using circuit bits and incoming frame signs can recover this result."
        mismatches.append(ReadoutMismatch(position, candidate, summary, reason))
    return mismatches


__all__ = ["ReadoutMismatch", "readout_disagreements"]
