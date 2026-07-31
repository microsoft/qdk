"""Per-gadget audit rules."""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from dataclasses import dataclass

import qodec

from ..._qodec_compat import (
    observable_names,
    observe_count,
    parse_encoding_atom,
    parse_stabilizer_atom,
    realization,
)
from ...profile.circuit_action import (
    gadget_objective_action_of,
    gadget_realization_action_of,
)
from ...profile.objective import lift_objective
from ..diagnostic import Diagnostic, Phase
from ..readout_check import readout_disagreements
from ..rule import Rule
from ..severity import Severity


def _where(gadget: qodec.Gadget) -> str:
    return f"gadget[{gadget.implements.mnemonic!r}]"


def _gadget(target: object) -> qodec.Gadget:
    if not isinstance(target, qodec.Gadget):
        raise TypeError(f"expected qodec.Gadget, got {type(target).__name__}")
    return target


@dataclass(frozen=True)
class MissingObservableRule:
    name: str = "gadget/missing-observable"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for missing in lift_objective(gadget).missing_observables:
            yield Diagnostic(
                self.name,
                self.severity,
                f"objective declares observable {missing!r}, realisation does not emit it",
                _where(gadget),
                f"realisation observables: {sorted(observable_names(gadget))}",
            )


@dataclass(frozen=True)
class MissingFlagRule:
    name: str = "gadget/missing-flag"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for missing in lift_objective(gadget).missing_flags:
            yield Diagnostic(
                self.name,
                self.severity,
                f"objective declares flag {missing!r}, realisation does not bind it",
                _where(gadget),
                f"instruction flags: {list(gadget.implements.flags)}; bound "
                f"readout slots: {max(0, len(gadget.readouts) - observe_count(gadget))}",
            )


@dataclass(frozen=True)
class UnsupportedActionAtomRule:
    name: str = "gadget/unsupported-action-atom"
    severity: Severity = Severity.WARNING
    phase: Phase = Phase.STRUCTURAL
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for atom_name in lift_objective(gadget).unsupported_atoms:
            yield Diagnostic(
                self.name,
                self.severity,
                f"implemented instruction contains an action atom of type "
                f"{atom_name!r}, which the verifier does not handle",
                _where(gadget),
                "The instruction's logical action could not be lifted; "
                "gadget/action-mismatch will be skipped.",
            )


@dataclass(frozen=True)
class FlagContentRule:
    name: str = "gadget/flag-content-not-checked"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for flag_name in lift_objective(gadget).bound_flags:
            yield Diagnostic(
                self.name,
                self.severity,
                f"flag {flag_name!r} is bound but its content is decoder-blind; "
                "only structural presence is verified",
                _where(gadget),
            )


@dataclass(frozen=True)
class ActionMismatchRule:
    name: str = "gadget/action-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        mnemonic = gadget.implements.mnemonic
        try:
            expected = gadget_objective_action_of(gadget)
            actual = gadget_realization_action_of(gadget)
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            if not gadget.inputs and gadget.outputs:
                yield Diagnostic(
                    self.name,
                    Severity.INFO,
                    f"{mnemonic!r} prepares from vacuum; no input encoding to "
                    "compare, so its logical action is not action-checked",
                    _where(gadget),
                )
                return
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                f"could not compute logical action for {mnemonic!r}; skipping",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        if expected.is_equivalent_to(actual):
            return
        modulo_paulis = expected.is_equivalent_to(actual, modulo_paulis=True)
        yield Diagnostic(
            self.name,
            self.severity,
            f"realisation's logical action does not match the action of "
            f"instruction {mnemonic!r}"
            + (" (matches up to Pauli signs only)" if modulo_paulis else ""),
            _where(gadget),
        )


@dataclass(frozen=True)
class ReadoutMismatchRule:
    name: str = "gadget/readout-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        mnemonic = gadget.implements.mnemonic
        try:
            mismatches = readout_disagreements(gadget)
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                f"could not check readouts for {mnemonic!r}; skipping",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        for mismatch in mismatches:
            verbiage = (
                "disagrees with"
                if mismatch.verifiable
                else "could not be verified against"
            )
            yield Diagnostic(
                self.name,
                self.severity if mismatch.verifiable else Severity.WARNING,
                f"readout {mismatch.name!r} of {mnemonic!r} XOR pattern "
                f"{verbiage} the realisation's discovered signature",
                _where(gadget),
                f"declared positions: {list(mismatch.declared_positions)}; "
                f"{mismatch.reason}",
            )


def _declared_out_frames(gadget: qodec.Gadget) -> set[tuple[int, int]]:
    declared = set()
    for check in gadget.checks:
        for atom in check:
            parsed = parse_stabilizer_atom(str(atom), side="out")
            if parsed is not None:
                declared.add(parsed)
    return declared


def _required_out_frames(gadget: qodec.Gadget) -> set[tuple[int, int]]:
    return {
        (int(encoding.operand), index)
        for encoding in realization(gadget).encoding_out
        for index in range(len(list(encoding.code.stabilizers)))
    }


@dataclass(frozen=True)
class IncompleteOutputFrameRule:
    name: str = "gadget/incomplete-output-frame"
    severity: Severity = Severity.WARNING
    phase: Phase = Phase.SEMANTIC
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        try:
            missing = _required_out_frames(gadget) - _declared_out_frames(gadget)
        except (KeyError, ValueError, TypeError, AttributeError) as error:
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                f"could not check output frames for "
                f"{gadget.implements.mnemonic!r}; skipping",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        for operand, index in sorted(missing):
            yield Diagnostic(
                self.name,
                self.severity,
                f"{gadget.implements.mnemonic!r} does not declare a sign for "
                f"output stabilizer out[{operand}].stabilizers[{index}]",
                _where(gadget),
                "Every output-encoding stabilizer needs an "
                "out[<entry>].stabilizers[i] declaration.",
            )


def _equation_atoms(
    entry: Sequence[object] | Mapping[str, Sequence[object]],
) -> list[str]:
    if isinstance(entry, Mapping):
        return [str(atom) for atom in next(iter(entry.values()))]
    return [str(atom) for atom in entry]


def _encoding_atom_violation(gadget: qodec.Gadget, atom: str) -> str | None:
    parsed = parse_encoding_atom(atom)
    if parsed is None:
        return None
    encodings = gadget.inputs if parsed.side == "in" else gadget.outputs
    if parsed.entry >= len(encodings):
        return (
            f"{parsed.side}[{parsed.entry}], but the gadget declares "
            f"{len(encodings)} {parsed.side} encoding(s)"
        )
    code = encodings[parsed.entry].code
    operators = (
        code.stabilizers
        if parsed.basis == "stabilizers"
        else code.x if parsed.basis == "x" else code.z
    )
    bound = len(list(operators))
    if parsed.index >= bound:
        return (
            f"{parsed.side}[{parsed.entry}].{parsed.basis}[{parsed.index}], "
            f"but that code has {bound} {parsed.basis} operator(s)"
        )
    return None


@dataclass(frozen=True)
class ReferenceOutOfBoundsRule:
    name: str = "gadget/reference-out-of-bounds"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qodec.Gadget

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        equations = [
            (f"check[{index}]", [str(atom) for atom in check])
            for index, check in enumerate(gadget.checks)
        ] + [
            (f"readout[{index}]", _equation_atoms(readout))
            for index, readout in enumerate(gadget.readouts)
        ]
        for label, equation in equations:
            for atom in equation:
                violation = _encoding_atom_violation(gadget, atom)
                if violation is not None:
                    yield Diagnostic(
                        self.name,
                        self.severity,
                        f"{label} references {violation}",
                        _where(gadget),
                    )


RULES: tuple[Rule, ...] = (
    ReferenceOutOfBoundsRule(),
    MissingObservableRule(),
    MissingFlagRule(),
    UnsupportedActionAtomRule(),
    FlagContentRule(),
    ActionMismatchRule(),
    ReadoutMismatchRule(),
    IncompleteOutputFrameRule(),
)

__all__ = [
    "ActionMismatchRule",
    "FlagContentRule",
    "IncompleteOutputFrameRule",
    "MissingFlagRule",
    "MissingObservableRule",
    "ReferenceOutOfBoundsRule",
    "ReadoutMismatchRule",
    "RULES",
    "UnsupportedActionAtomRule",
]
