"""Synthesize a runnable qodec from a bare stabilizer code.

A :class:`qodec.Code` is a *static* object: it says which Pauli operators
stabilize the codespace and which represent the logical qubits, but it says
nothing about how to prepare, preserve, or read out an encoded state. A
:class:`qodec.Qodec` is the *runnable* artifact: a layered pipeline whose
gadgets lower each logical instruction into a concrete circuit.

:func:`qodec_from_code` bridges the two. Given a code, it emits a two-layer
qodec — a synthesized logical ISA over the code's ``k`` logical qubits,
lowering to a physical stim ISA — with a textbook circuit for each logical
instruction:

===============  ===========================================================
instruction      synthesized circuit
===============  ===========================================================
``prepare_z``    reset all data to :math:`|0\\rangle`, then one syndrome round
``prepare_x``    reset all data, Hadamard all, then one syndrome round
``idle``         one syndrome-extraction round
``measure_z``    destructive transversal ``M``
``measure_x``    transversal ``H`` then destructive ``M``
``x{i}``         the code's i-th logical X operator, gate by gate
``z{i}``         the code's i-th logical Z operator, gate by gate
===============  ===========================================================

Syndrome extraction uses one ancilla per stabilizer, in the uniform
controlled-Pauli form: the ancilla is prepared in :math:`|+\\rangle`, a
controlled-``X`` / controlled-``Z`` is applied from it to each qubit in the
stabilizer's support, and it is then rotated back and measured. This single
construction covers CSS and non-CSS codes alike, and touches no data qubit
with a basis-changing gate.

.. warning::

   The synthesized circuits are textbook, **not fault-tolerant**. Extracting a
   stabilizer with a single unflagged ancilla means one fault partway through
   its string of controlled Paulis can propagate onto several data qubits at
   once, so a synthesized gadget typically has circuit-level distance 1 no
   matter how large the code's distance is
   (:func:`~qdk.ec.targets.gadget_distance_of` will show this). Fault-tolerant
   extraction needs flag qubits, Shor- or Steane-style ancilla preparation, or a
   code-specific schedule — design decisions a general synthesizer should not
   make silently. Treat the result as the fastest path to something runnable and
   measurable, and as a baseline to compare a hand-tuned qodec against.

Checks and readouts are *not* hand-derived: each synthesized gadget is a draft
that :func:`~qdk.ec.develop.completion.complete_gadget` finishes by exact
simulation. Every finished gadget is then verified with
:func:`~qdk.ec.profile.action.gadget_action_mismatch`, so an instruction
survives only if its circuit provably realizes the action it declares. See
:ref:`unsupported-instructions` below.

.. _unsupported-instructions:

Instructions that cannot be synthesized
---------------------------------------
Not every logical instruction is available for every code. Some omissions are
mathematical: ``prepare_z`` prepares :math:`|0\\rangle^{\\otimes n}` and projects
into the codespace, which pins the logical state only when the code's logical Z
operators are Z-type. The five-qubit code, as conventionally written, declares a
logical Z with X components, so no ``prepare_z`` (nor transversal ``measure_z``)
exists for that basis — even though an equivalent all-Z representative lives in
the same coset.

Others are limitations of the surrounding tooling rather than of the code. The
observable-discovery pass that completion relies on is sensitive to the choice
of logical basis: the [[4,2,2]] code admits ``measure_z`` when its logical Z
operators are written ``Z_0 Z_2, Z_0 Z_1`` but not when the same code is written
``Z_1 Z_3, Z_2 Z_3``, though the two bases are equally valid.

Rather than guess which case applies, :func:`qodec_from_code` keeps only the
instructions whose gadgets complete *and* verify, and records every omission
with its reason under the returned qodec's
``metadata["qdk.ec"]["synthesis"]["omitted"]`` (see :func:`synthesis_notes`).
Pass ``strict=True`` to turn any omission into an exception instead.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from typing import Optional

import qodec
from qodec.actions import Clifford, Observe, Pauli as PauliAction, Stabilize
from qodec.gadgets import Circuit, Encoding
from qodec.instructions import Block, BlockOperand, Instruction, InstructionSet

from ..profile.action import gadget_action_mismatch
from ..profile.propagation.pauli import Pauli, characters_of
from .completion import complete_gadget

#: Name given to the synthesized physical instruction set.
_PHYSICAL_ISA_NAME = "stim"

#: Key under which synthesis notes are recorded in the qodec's metadata.
_METADATA_KEY = "qdk.ec"


def _characters(text: object) -> dict[int, str]:
    """The ``{qubit: character}`` map of a qodec Pauli string."""
    return dict(characters_of(Pauli(str(text))))


def _qubit_count(code: qodec.Code) -> int:
    """Number of physical qubits the code addresses.

    Derived as one past the highest qubit index mentioned by any stabilizer or
    logical operator, so a code that never touches a trailing qubit reports the
    narrower width.
    """
    highest = -1
    for group in (code.stabilizers, code.x, code.z):
        for text in group:
            for qubit in _characters(text):
                highest = max(highest, qubit)
    return highest + 1


def _reject_y_components(code: qodec.Code) -> None:
    """Raise if any operator has a Y component.

    Y components would need ``S`` / ``S_DAG`` in the physical ISA, whose sign
    conventions are not covered by this synthesizer. Every operator is reported
    at once so a caller sees the full picture rather than the first offender.
    """
    offenders = [
        str(text)
        for group in (code.stabilizers, code.x, code.z)
        for text in group
        if "Y" in set(_characters(text).values())
    ]
    if offenders:
        raise NotImplementedError(
            "qodec_from_code cannot synthesize circuits for operators with Y "
            f"components: {', '.join(sorted(offenders))}. Re-express the code "
            "in an X/Z basis, or author the gadgets by hand."
        )


def _physical_isa() -> InstructionSet:
    """The stim ISA the synthesized gadget circuits target.

    Deliberately small: reset, Hadamard, the two controlled Paulis syndrome
    extraction needs, destructive measurement, and the two Pauli gates logical
    Pauli gadgets need. Each carries the action that makes it simulable by
    :mod:`qdk.ec.profile.propagation`.
    """

    def operand() -> BlockOperand:
        return BlockOperand("qubit")

    return InstructionSet(
        name=_PHYSICAL_ISA_NAME,
        blocks=[Block("qubit", encodes=1)],
        instructions=[
            Instruction(
                "R",
                description="Reset to |0>.",
                outputs=[operand()],
                action=[Stabilize(["Z_0"])],
            ),
            Instruction(
                "H",
                description="Hadamard.",
                inputs=[operand()],
                outputs=[operand()],
                action=[Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
            ),
            Instruction(
                "CX",
                description="Controlled-X.",
                inputs=[operand(), operand()],
                outputs=[operand(), operand()],
                action=[Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
            ),
            Instruction(
                "CZ",
                description="Controlled-Z.",
                inputs=[operand(), operand()],
                outputs=[operand(), operand()],
                action=[Clifford({"X_0": "X_0 Z_1", "X_1": "Z_0 X_1"})],
            ),
            Instruction(
                "M",
                description="Destructive Z-basis measurement.",
                inputs=[operand()],
                action=[Observe(["Z_0"])],
            ),
            Instruction(
                "X",
                description="Pauli X.",
                inputs=[operand()],
                outputs=[operand()],
                action=[PauliAction("X_0")],
            ),
            Instruction(
                "Z",
                description="Pauli Z.",
                inputs=[operand()],
                outputs=[operand()],
                action=[PauliAction("Z_0")],
            ),
        ],
    )


def _targets(qubits: Iterable[int]) -> str:
    return " ".join(str(qubit) for qubit in qubits)


def _syndrome_round(stabilizers: Sequence[object], data_width: int) -> list[str]:
    """Stim lines measuring every stabilizer once, one ancilla each.

    Ancillas occupy ``data_width, data_width + 1, ...``. Each is prepared in
    :math:`|+\\rangle`, used as the control of a controlled-Pauli into every
    qubit of its stabilizer's support, then rotated back and measured — so the
    ancilla's outcome is the stabilizer's eigenvalue and no data qubit is
    disturbed.
    """
    if not stabilizers:
        return []
    ancillas = [data_width + offset for offset in range(len(stabilizers))]
    lines = [f"R {_targets(ancillas)}", f"H {_targets(ancillas)}"]
    for ancilla, stabilizer in zip(ancillas, stabilizers):
        characters = _characters(stabilizer)
        for qubit in sorted(characters):
            gate = "CX" if characters[qubit] == "X" else "CZ"
            lines.append(f"{gate} {ancilla} {qubit}")
    lines.append(f"H {_targets(ancillas)}")
    lines.append(f"M {_targets(ancillas)}")
    return lines


def _pauli_lines(operator: object) -> list[str]:
    """Stim lines applying a Pauli operator gate by gate."""
    characters = _characters(operator)
    x_targets = sorted(q for q, c in characters.items() if c == "X")
    z_targets = sorted(q for q, c in characters.items() if c == "Z")
    lines = []
    if x_targets:
        lines.append(f"X {_targets(x_targets)}")
    if z_targets:
        lines.append(f"Z {_targets(z_targets)}")
    return lines


def _logical_token_map(
    code: qodec.Code,
    block: str,
    logical_count: int,
    physical: InstructionSet,
    data_width: int,
) -> dict[tuple[str, int], int]:
    """Resolve which action token names each of the code's logical qubits.

    A ``pauli: X_<t>`` action names a logical qubit by a token index ``t``.
    That index is *assumed* to be the position of the operator in the code's
    own ``x`` / ``z`` lists, but the declared-action machinery does not always
    agree: for a ``k = 6`` code the observed correspondence is the permutation
    ``[0, 1, 4, 5, 2, 3]``, while for ``k = 2`` it is the identity.

    Rather than encode either convention, this resolves the map by
    verification: for logical qubit ``j`` it emits the circuit that applies the
    code's ``j``-th logical operator and finds the token index whose declared
    action the realized action actually matches. The identity is tried first,
    so a correct convention costs one check per logical qubit and the map is
    the identity if and when the inconsistency is resolved upstream.

    Logical qubits whose token cannot be resolved are absent from the result.
    """
    support = [str(qubit) for qubit in range(data_width)]
    probe_isa = InstructionSet(
        name=f"{block}__probe",
        blocks=[Block(block, encodes=logical_count)],
        instructions=[
            Instruction(
                f"probe_{basis.lower()}{token}",
                inputs=[BlockOperand(block)],
                outputs=[BlockOperand(block)],
                action=[PauliAction(f"{basis}_{token}")],
            )
            for basis in ("X", "Z")
            for token in range(logical_count)
        ],
    )

    def matches(basis: str, token: int, source: str) -> bool:
        probe = qodec.Gadget(
            probe_isa.instruction(f"probe_{basis.lower()}{token}"),
            Circuit(physical, source, format="stim"),
            inputs=[Encoding(code, support=list(support))],
            outputs=[Encoding(code, support=list(support))],
        )
        try:
            return gadget_action_mismatch(probe) is None
        except Exception:  # noqa: BLE001 - an unverifiable probe is not a match
            return False
    resolved: dict[tuple[str, int], int] = {}
    for basis, operators in (("X", list(code.x)), ("Z", list(code.z))):
        taken: set[int] = set()
        for index, operator in enumerate(operators):
            source = "\n".join(_pauli_lines(operator)) + "\n"
            order = [index] + [t for t in range(logical_count) if t != index]
            for token in order:
                if token in taken:
                    continue
                if matches(basis, token, source):
                    resolved[(basis, index)] = token
                    taken.add(token)
                    break
    return resolved


class _Candidate:
    """One logical instruction plus the circuit that is meant to realize it."""

    def __init__(
        self,
        instruction: Instruction,
        source_lines: list[str],
        *,
        takes_input: bool,
        gives_output: bool,
    ) -> None:
        self.instruction = instruction
        self.source = "\n".join(source_lines) + "\n" if source_lines else "\n"
        self.takes_input = takes_input
        self.gives_output = gives_output

    @property
    def mnemonic(self) -> str:
        return self.instruction.mnemonic


def _candidates(
    code: qodec.Code,
    block: str,
    logical_count: int,
    data_width: int,
    tokens: Mapping[tuple[str, int], int],
) -> list[_Candidate]:
    """Every logical instruction this synthesizer knows how to attempt.

    ``tokens`` maps ``(basis, logical index)`` to the action token index that
    names that logical qubit (see :func:`_logical_token_map`).
    """

    def operand() -> BlockOperand:
        return BlockOperand(block)

    def token(basis: str, index: int) -> int:
        return tokens.get((basis, index), index)

    stabilizers = list(code.stabilizers)
    syndrome = _syndrome_round(stabilizers, data_width)
    all_data = _targets(range(data_width))
    order = range(logical_count)

    # Stabilize/Observe list *all* logical qubits, so they name them in
    # resolved-token order: the action's list position is the logical qubit,
    # and the token is whatever names it.
    z_tokens = [f"Z_{token('Z', i)}" for i in order]
    x_tokens = [f"X_{token('X', i)}" for i in order]

    candidates = [
        _Candidate(
            Instruction(
                "prepare_z",
                description=f"Prepare all {logical_count} logical qubit(s) in |0>.",
                outputs=[operand()],
                action=[Stabilize(z_tokens)],
            ),
            [f"R {all_data}", *syndrome],
            takes_input=False,
            gives_output=True,
        ),
        _Candidate(
            Instruction(
                "prepare_x",
                description=f"Prepare all {logical_count} logical qubit(s) in |+>.",
                outputs=[operand()],
                action=[Stabilize(x_tokens)],
            ),
            [f"R {all_data}", f"H {all_data}", *syndrome],
            takes_input=False,
            gives_output=True,
        ),
        _Candidate(
            Instruction(
                "idle",
                description="Hold the encoded state for one syndrome round.",
                inputs=[operand()],
                outputs=[operand()],
            ),
            list(syndrome),
            takes_input=True,
            gives_output=True,
        ),
        _Candidate(
            Instruction(
                "measure_z",
                description="Destructively measure every logical qubit in Z.",
                inputs=[operand()],
                action=[Observe(z_tokens)],
            ),
            [f"M {all_data}"],
            takes_input=True,
            gives_output=False,
        ),
        _Candidate(
            Instruction(
                "measure_x",
                description="Destructively measure every logical qubit in X.",
                inputs=[operand()],
                action=[Observe(x_tokens)],
            ),
            [f"H {all_data}", f"M {all_data}"],
            takes_input=True,
            gives_output=False,
        ),
    ]

    for index, operator in enumerate(code.x):
        candidates.append(
            _Candidate(
                Instruction(
                    f"x{index}",
                    description=f"Logical X on logical qubit {index}.",
                    inputs=[operand()],
                    outputs=[operand()],
                    action=[PauliAction(f"X_{token('X', index)}")],
                ),
                _pauli_lines(operator),
                takes_input=True,
                gives_output=True,
            )
        )
    for index, operator in enumerate(code.z):
        candidates.append(
            _Candidate(
                Instruction(
                    f"z{index}",
                    description=f"Logical Z on logical qubit {index}.",
                    inputs=[operand()],
                    outputs=[operand()],
                    action=[PauliAction(f"Z_{token('Z', index)}")],
                ),
                _pauli_lines(operator),
                takes_input=True,
                gives_output=True,
            )
        )
    return candidates


def _draft(
    candidate: _Candidate,
    instruction: Instruction,
    code: qodec.Code,
    physical: InstructionSet,
    data_width: int,
) -> qodec.Gadget:
    support = [str(qubit) for qubit in range(data_width)]
    return qodec.Gadget(
        instruction,
        Circuit(physical, candidate.source, format="stim"),
        inputs=[Encoding(code, support=list(support))] if candidate.takes_input else [],
        outputs=(
            [Encoding(code, support=list(support))] if candidate.gives_output else []
        ),
    )


def _readout_value(entry: object) -> "list[str] | dict[str, list[str]]":
    if isinstance(entry, Mapping):
        return {
            name: [str(atom) for atom in equation] for name, equation in entry.items()
        }
    return [str(atom) for atom in entry]  # type: ignore[union-attr]


def _rebound(gadget: qodec.Gadget, instruction: Instruction) -> qodec.Gadget:
    """``gadget`` re-pointed at ``instruction``, keeping its completed surface."""
    return qodec.Gadget(
        instruction,
        gadget.circuit,
        inputs=list(gadget.inputs),
        outputs=list(gadget.outputs),
        checks=[[str(atom) for atom in check] for check in gadget.checks],
        readouts=[_readout_value(entry) for entry in gadget.readouts],
        parameters=dict(gadget.parameters),
        metadata=dict(gadget.metadata),
    )


def qodec_from_code(
    code: qodec.Code,
    *,
    name: Optional[str] = None,
    description: Optional[str] = None,
    strict: bool = False,
) -> qodec.Qodec:
    """Synthesize a runnable qodec that implements ``code``.

    Returns a two-layer qodec: a logical ISA over the code's ``k`` logical
    qubits, lowering to a physical stim ISA, with one completed gadget per
    logical instruction. See the module docstring for the instruction menu, the
    circuit used for each, and the fault-tolerance caveat.

    Parameters
    ----------
    code:
        The stabilizer code to build around. Its stabilizers and logical
        operators must be free of Y components.
    name:
        Name for the resulting qodec and its logical ISA. Defaults to the
        code's own name.
    description:
        Description for the resulting qodec. A summary of the code's parameters
        is generated when omitted.
    strict:
        When ``True``, raise if any instruction's gadget fails to complete.
        When ``False`` (the default) such instructions are omitted from the
        logical ISA and recorded in the qodec's metadata.

    Raises
    ------
    NotImplementedError
        If any stabilizer or logical operator has a Y component.
    ValueError
        If the code declares no logical qubits, or — with ``strict=True`` — if
        any instruction could not be synthesized.
    """
    _reject_y_components(code)

    logical_count = len(list(code.x))
    if logical_count == 0:
        raise ValueError(
            f"code {code.name!r} declares no logical qubits; there is nothing "
            "for a qodec to compute with"
        )

    data_width = _qubit_count(code)
    resolved_name = name or code.name
    if not resolved_name:
        raise ValueError("code has no name; pass name= explicitly")

    physical = _physical_isa()
    block = Block(resolved_name, encodes=logical_count)
    tokens = _logical_token_map(
        code, resolved_name, logical_count, physical, data_width
    )
    candidates = _candidates(code, resolved_name, logical_count, data_width, tokens)

    # First pass: draft every candidate against a provisional ISA, then let
    # completion and the declared-vs-realized action check decide which
    # circuits genuinely implement their instruction.
    provisional = InstructionSet(
        name=resolved_name,
        blocks=[block],
        instructions=[candidate.instruction for candidate in candidates],
    )

    completed: list[tuple[_Candidate, qodec.Gadget]] = []
    omitted: dict[str, str] = {}

    def reject(mnemonic: str, reason: str) -> None:
        if strict:
            raise ValueError(
                f"could not synthesize {mnemonic!r} for code "
                f"{resolved_name!r}: {reason}"
            )
        omitted[mnemonic] = reason

    for candidate in candidates:
        draft = _draft(
            candidate,
            provisional.instruction(candidate.mnemonic),
            code,
            physical,
            data_width,
        )
        try:
            gadget = complete_gadget(draft)
        except Exception as error:  # noqa: BLE001 - completion is an arbiter
            reject(candidate.mnemonic, f"{type(error).__name__}: {error}")
            continue
        mismatch = gadget_action_mismatch(gadget)
        if mismatch is not None:
            reject(candidate.mnemonic, f"action mismatch: {mismatch}")
            continue
        completed.append((candidate, gadget))

    if not completed:
        raise ValueError(
            f"no instruction could be synthesized for code {resolved_name!r}; "
            f"reasons: {omitted}"
        )

    # Second pass: rebuild the ISA from the survivors only, so the qodec never
    # advertises an instruction it cannot lower.
    logical = InstructionSet(
        name=resolved_name,
        blocks=[Block(resolved_name, encodes=logical_count)],
        instructions=[candidate.instruction for candidate, _ in completed],
    )
    gadgets = [
        _rebound(gadget, logical.instruction(candidate.mnemonic))
        for candidate, gadget in completed
    ]

    metadata: dict[str, object] = {
        _METADATA_KEY: {
            "synthesis": {
                "source": "qdk.ec.develop.qodec_from_code",
                "code": code.name,
                "physical_qubits": data_width,
                "logical_qubits": logical_count,
                "omitted": omitted,
            }
        }
    }

    return qodec.Qodec(
        [qodec.Layer(logical, gadgets=gadgets), qodec.Layer(physical)],
        name=resolved_name,
        description=(
            description
            if description is not None
            else (
                f"Synthesized from the {code.name!r} stabilizer code "
                f"([[{data_width}, {logical_count}]])."
            )
        ),
        metadata=metadata,
    )


def synthesis_notes(codec: qodec.Qodec) -> dict[str, object]:
    """The synthesis record :func:`qodec_from_code` left on ``codec``.

    Returns an empty mapping for a qodec that was not synthesized.
    """
    section = dict(codec.metadata).get(_METADATA_KEY)
    if not isinstance(section, Mapping):
        return {}
    notes = section.get("synthesis")
    return dict(notes) if isinstance(notes, Mapping) else {}


__all__ = ["qodec_from_code", "synthesis_notes"]
