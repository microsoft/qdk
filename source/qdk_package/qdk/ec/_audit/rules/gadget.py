"""Per-gadget audit rules."""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from dataclasses import dataclass
import json

import qodec as qc

from ..._readouts import flag_slots, observable_slots, observe_count_of, readout_slots
from ..._layout import ProgramLayout
from ..._references import (
    Atom,
    LogicalSign,
    Outcome,
    StabilizerSign,
    parse_equations,
    stabilizer_signs_of,
)
from ..._analysis.channel_action import (
    declared_action_of,
    input_qubits_of,
    realized_action_of,
)
from ..._analysis.check_discovery import _output_relations_of
from ..._analysis.propagation.interpreter import program_of
from ..._analysis.propagation.pauli_remap import encoding_qubit_relocation
from ..._analysis.declaration_issues import declaration_issues
from .._diagnostic import Diagnostic, Phase, Severity
from .._readout_check import readout_disagreements
from .._rule import Rule


def _where(gadget: qc.Gadget) -> str:
    return f"gadget[{gadget.implements.mnemonic!r}]"


def _gadget(target: object) -> qc.Gadget:
    if not isinstance(target, qc.Gadget):
        raise TypeError(f"expected qodec.Gadget, got {type(target).__name__}")
    return target


def _equation(terms: Iterable[object]) -> str:
    return json.dumps([str(term) for term in terms])


def _observable(gadget: qc.Gadget, position: int) -> str:
    return str(
        [
            observable
            for action in gadget.implements.action
            if isinstance(action, qc.actions.Observe)
            for observable in action.observables
        ][position]
    )


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
                f"readouts[{missing}] is unbound (logical {_observable(gadget, int(missing))})",
                _where(gadget),
                f"Expected: {observe_count_of(gadget.implements)} observable bindings; "
                f"declared: {len(observable_slots(gadget))}.",
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
            position = observe_count_of(gadget.implements) + list(
                gadget.implements.flags
            ).index(missing)
            yield Diagnostic(
                self.name,
                self.severity,
                f"readouts[{position}] is unbound (flag {missing!r})",
                _where(gadget),
                f"Expected: {len(gadget.implements.flags)} flag bindings; "
                f"declared: {len(flag_slots(gadget))}.",
            )


@dataclass(frozen=True)
class UnsupportedActionAtomRule:
    name: str = "gadget/unsupported-action-atom"
    severity: Severity = Severity.WARNING
    phase: Phase = Phase.STRUCTURAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        unsupported = set(declaration_issues(gadget).unsupported_atoms)
        for index, action in enumerate(gadget.implements.action):
            atom_name = type(action).__name__
            if atom_name not in unsupported:
                continue
            if (
                isinstance(action, (qc.actions.Pauli, qc.actions.Clifford))
                and action.condition is None
            ):
                continue
            yield Diagnostic(
                self.name,
                self.severity,
                f"implements.action[{index}] ({atom_name}) is not supported by the action verifier",
                _where(gadget),
                "Logical action not checked.",
            )


@dataclass(frozen=True)
class PreparedInputRule:
    name: str = "gadget/prepared-input"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        declared = {
            qubit
            for encoding in gadget.inputs
            for qubit in encoding_qubit_relocation(encoding).values()
        }
        if not declared:
            return
        program = program_of(gadget)
        try:
            prepared = set(range(ProgramLayout.of(program).total_qubits)) - set(
                input_qubits_of(program)
            )
        except (KeyError, TypeError, ValueError):
            return
        overlap = declared & prepared
        if overlap:
            encodings = [
                f"in[{entry}] ({encoding.code.name}): "
                f"circuit qubits {sorted(overlap & set(encoding_qubit_relocation(encoding).values()))}"
                for entry, encoding in enumerate(gadget.inputs)
                if overlap & set(encoding_qubit_relocation(encoding).values())
            ]
            yield Diagnostic(
                self.name,
                self.severity,
                "circuit prepares qubits declared as encoded inputs",
                _where(gadget),
                "\n".join(encodings),
            )


@dataclass(frozen=True)
class FlagContentRule:
    name: str = "gadget/flag-content-not-checked"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for slot in flag_slots(gadget):
            yield Diagnostic(
                self.name,
                self.severity,
                f"readouts[{slot.position}] (flag {slot.name!r}): binding present; meaning not checked",
                _where(gadget),
                f"Declared: {_equation(gadget.readouts[slot.position].equation)}",
            )


@dataclass(frozen=True)
class ActionMismatchRule:
    name: str = "gadget/action-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        if declaration_issues(gadget).unsupported_atoms:
            return
        try:
            expected = declared_action_of(gadget)
            actual = realized_action_of(gadget)
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            if not gadget.inputs and gadget.outputs:
                yield Diagnostic(
                    self.name,
                    Severity.INFO,
                    "Logical action not checked: preparation has no input encoding",
                    _where(gadget),
                    f"{type(error).__name__}: {error}",
                )
                return
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                "Logical action not checked: analysis failed",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        if expected.is_equivalent_to(actual):
            return
        yield Diagnostic(
            self.name,
            self.severity,
            "circuit action differs from implements.action",
            _where(gadget),
            expected.why_not_equivalent_to(actual),
        )


@dataclass(frozen=True)
class ReadoutMismatchRule:
    name: str = "gadget/readout-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        try:
            mismatches = readout_disagreements(gadget)
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                "Readouts not checked: analysis failed",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        for mismatch in mismatches:
            position = int(mismatch.name)
            declared = _equation(gadget.readouts[position].equation)
            detail = [f"Declared: {declared}"]
            if mismatch.expected_positions is not None:
                expected = _equation(
                    Outcome(index) for index in mismatch.expected_positions
                )
                detail.append(f"Verified measurement parity: {expected}")
            elif mismatch.verifiable:
                detail.append("No equivalent measurement parity was derived.")
            if mismatch.verifiable:
                detail.append(
                    "Compared on noiseless codewords; encoding-sign terms not checked."
                )
            else:
                detail.append(mismatch.reason)
            yield Diagnostic(
                self.name,
                self.severity if mismatch.verifiable else Severity.WARNING,
                f"readouts[{position}] (logical {_observable(gadget, position)}): "
                + (
                    "measurement parity mismatch"
                    if mismatch.verifiable
                    else "not verified"
                ),
                _where(gadget),
                "\n".join(detail),
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
                "Output frames not checked: analysis failed",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        if not missing:
            return
        try:
            relations = _output_relations_of(gadget)
            unavailable = "No noiseless relation was derived."
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            relations = []
            unavailable = f"Relation not derived: {type(error).__name__}: {error}"
        for operand, index in sorted(missing):
            sign = StabilizerSign("out", operand, index)
            encoding = gadget.outputs[operand]
            relation = next(
                (
                    (equation, offset)
                    for equation, offset in relations
                    if sign in equation
                ),
                None,
            )
            detail = unavailable
            if relation is not None:
                terms = _equation(
                    (sign, *(term for term in relation[0] if term != sign))
                )
                detail = (
                    f"Relation terms: {terms}\nParity: 1 (not a valid zero-parity check)."
                    if relation[1]
                    else f"Verified relation: {terms}"
                )
            yield Diagnostic(
                self.name,
                self.severity,
                f"No check references {sign} ({encoding.code.stabilizers[index]})",
                _where(gadget),
                f"Code: {encoding.code.name}; circuit support: {list(encoding.support)}.\n{detail}",
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
        return f"Valid {atom.side} entries: [0, {len(encodings)}) (stop exclusive)."
    code = encodings[atom.entry].code
    operators = (
        code.stabilizers
        if basis == "stabilizers"
        else code.x if basis == "x" else code.z
    )
    bound = len(list(operators))
    if atom.index >= bound:
        return (
            f"Code {code.name!r}: valid {basis} indices [0, {bound}) (stop exclusive)."
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
            (f"checks[{index}]", check)
            for index, check in enumerate(parse_equations(gadget.checks))
        ] + [
            (f"readouts[{slot.position}]", slot.equation)
            for slot in readout_slots(gadget)
        ]
        for label, equation in equations:
            for atom in equation:
                violation = _encoding_atom_violation(gadget, atom)
                if violation is not None:
                    yield Diagnostic(
                        self.name,
                        self.severity,
                        f"{label}: {atom} is out of bounds",
                        _where(gadget),
                        violation,
                    )


RULES: tuple[Rule, ...] = (
    ReferenceOutOfBoundsRule(),
    MissingObservableRule(),
    MissingFlagRule(),
    UnsupportedActionAtomRule(),
    PreparedInputRule(),
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
    "PreparedInputRule",
    "ReferenceOutOfBoundsRule",
    "ReadoutMismatchRule",
    "RULES",
    "UnsupportedActionAtomRule",
]
