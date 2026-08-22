"""Per-gadget audit rules."""

from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass

import qodec as qc

from ..._readouts import flag_slots, observable_slots, readout_slots
from ..._references import (
    Atom,
    LogicalSign,
    StabilizerSign,
    parse_equations,
    stabilizer_signs_of,
)
from ..._analysis.circuit_action import (
    declared_action_of,
    realized_action_of,
)
from ..._analysis.declaration_issues import declaration_issues
from ...lint._diagnostic import Diagnostic, Phase
from ...lint._readout_check import readout_disagreements
from ...lint._rule import Rule
from ...lint._severity import Severity


def _where(gadget: qc.Gadget) -> str:
    return f"gadget[{gadget.implements.mnemonic!r}]"


def _gadget(target: object) -> qc.Gadget:
    if not isinstance(target, qc.Gadget):
        raise TypeError(f"expected qodec.Gadget, got {type(target).__name__}")
    return target


@dataclass(frozen=True)
class MissingObservableRule:
    name: str = "gadget/missing-observable"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for missing in declaration_issues(gadget).missing_observables:
            yield Diagnostic(
                self.name,
                self.severity,
                f"instruction declares observable {missing!r}, circuit does not emit it",
                _where(gadget),
                f"realized observables: "
                f"{sorted(slot.name for slot in observable_slots(gadget))}",
            )


@dataclass(frozen=True)
class MissingFlagRule:
    name: str = "gadget/missing-flag"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for missing in declaration_issues(gadget).missing_flags:
            yield Diagnostic(
                self.name,
                self.severity,
                f"instruction declares flag {missing!r}, circuit does not bind it",
                _where(gadget),
                f"instruction flags: {list(gadget.implements.flags)}; bound "
                f"readout slots: {len(flag_slots(gadget))}",
            )


@dataclass(frozen=True)
class UnsupportedActionAtomRule:
    name: str = "gadget/unsupported-action-atom"
    severity: Severity = Severity.WARNING
    phase: Phase = Phase.STRUCTURAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for atom_name in declaration_issues(gadget).unsupported_atoms:
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
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for flag_name in declaration_issues(gadget).bound_flags:
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
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        mnemonic = gadget.implements.mnemonic
        try:
            expected = declared_action_of(gadget)
            actual = realized_action_of(gadget)
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
            f"realized logical action does not match the action of "
            f"instruction {mnemonic!r}"
            + (" (matches up to Pauli signs only)" if modulo_paulis else ""),
            _where(gadget),
        )


@dataclass(frozen=True)
class ReadoutMismatchRule:
    name: str = "gadget/readout-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
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
                f"{verbiage} the circuit's discovered signature",
                _where(gadget),
                f"declared positions: {list(mismatch.declared_positions)}; "
                f"{mismatch.reason}",
            )


def _declared_out_frames(gadget: qc.Gadget) -> set[tuple[int, int]]:
    return {
        sign.key
        for check in parse_equations(gadget.checks)
        for sign in stabilizer_signs_of(check, side="out")
    }


def _required_out_frames(gadget: qc.Gadget) -> set[tuple[int, int]]:
    return {
        (entry, index)
        for entry, encoding in enumerate(gadget.outputs)
        for index in range(len(list(encoding.code.stabilizers)))
    }


@dataclass(frozen=True)
class IncompleteOutputFrameRule:
    name: str = "gadget/incomplete-output-frame"
    severity: Severity = Severity.WARNING
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
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


def _encoding_atom_violation(gadget: qc.Gadget, atom: Atom) -> str | None:
    if isinstance(atom, StabilizerSign):
        basis = "stabilizers"
    elif isinstance(atom, LogicalSign):
        basis = atom.basis
    else:
        return None
    encodings = gadget.inputs if atom.side == "in" else gadget.outputs
    if atom.entry >= len(encodings):
        return (
            f"{atom.side}[{atom.entry}], but the gadget declares "
            f"{len(encodings)} {atom.side} encoding(s)"
        )
    code = encodings[atom.entry].code
    operators = (
        code.stabilizers
        if basis == "stabilizers"
        else code.x if basis == "x" else code.z
    )
    bound = len(list(operators))
    if atom.index >= bound:
        return (
            f"{atom.side}[{atom.entry}].{basis}[{atom.index}], "
            f"but that code has {bound} {basis} operator(s)"
        )
    return None


@dataclass(frozen=True)
class ReferenceOutOfBoundsRule:
    name: str = "gadget/reference-out-of-bounds"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        equations = [
            (f"check[{index}]", check)
            for index, check in enumerate(parse_equations(gadget.checks))
        ] + [
            (f"readout[{slot.position}]", slot.equation)
            for slot in readout_slots(gadget)
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
