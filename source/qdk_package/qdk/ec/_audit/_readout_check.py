"""Verify complete readout equations against noiseless encoded execution."""

from __future__ import annotations

from dataclasses import dataclass
import qodec as qc

from .._readouts import observe_count_of
from ._diagnostic import Severity
from ._parity import ParityAnalysis


@dataclass(frozen=True)
class ReadoutMismatch:
    positions: tuple[int, ...]
    expected_equations: dict[int, tuple[str, ...]]
    summary: str
    reason: str
    severity: Severity = Severity.ERROR


def readout_conflicts(analysis: ParityAnalysis) -> list[ReadoutMismatch]:
    conflicts = []
    for positions in analysis.resolution.conflicts:
        labels = ", ".join(f"readouts[{position}].equation" for position in positions)
        verb = "is" if len(positions) == 1 else "are"
        candidates = {}
        for position in positions:
            candidate = analysis.readout_candidate(position)
            if candidate is not None:
                candidates[position] = candidate
        conflicts.append(
            ReadoutMismatch(
                positions,
                candidates,
                f"{labels} {verb} inconsistent",
                "The defining equations cannot all hold for every noiseless circuit result.",
            )
        )
    return conflicts


def readout_disagreements(analysis: ParityAnalysis) -> list[ReadoutMismatch]:
    gadget = analysis.gadget
    count = min(observe_count_of(gadget.implements), len(gadget.readouts))
    if not count:
        return []
    resolution = analysis.resolution
    observables = [
        observable
        for action in gadget.implements.action
        if isinstance(action, qc.actions.Observe)
        for observable in action.observables
    ]
    mismatches = [
        conflict
        for conflict in readout_conflicts(analysis)
        if any(position < count for position in conflict.positions)
    ]
    conflicting = {position for group in resolution.conflicts for position in group}
    for position in range(count):
        if position in conflicting:
            continue
        required = f"logical {observables[position]} measurement"
        if position in resolution.blocked:
            candidate = analysis.readout_candidate(position)
            mismatches.append(
                ReadoutMismatch(
                    (position,),
                    {position: candidate} if candidate is not None else {},
                    f"readouts[{position}]: the required {required} cannot be verified",
                    "This readout depends on inconsistent readout equations.",
                    Severity.WARNING,
                )
            )
            continue
        if position in resolution.unresolved:
            candidate = analysis.readout_candidate(position)
            mismatches.append(
                ReadoutMismatch(
                    (position,),
                    {position: candidate} if candidate is not None else {},
                    f"readouts[{position}] cannot report the required {required}: an undetermined readout",
                    "The defining equations allow both 0 and 1 for this readout; its value is not uniquely determined.",
                )
            )
            continue
        expected = analysis.expected[position]
        difference = resolution.values[position] ^ expected
        if difference.is_zero:
            continue
        candidate = analysis.candidate(expected)
        summary = f"readouts[{position}] does not report the required {required}"
        if candidate is not None:
            reason = ""
        else:
            summary = f"The circuit does not provide the required {required} result for readouts[{position}]"
            reason = "No readout formula using circuit bits and incoming frame signs can recover this result."
        mismatches.append(
            ReadoutMismatch(
                (position,),
                {position: candidate} if candidate is not None else {},
                summary,
                reason,
            )
        )
    return mismatches


__all__ = ["ReadoutMismatch", "readout_conflicts", "readout_disagreements"]
