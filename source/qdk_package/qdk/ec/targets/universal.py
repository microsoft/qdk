"""UniversalSampler: a minimal, end-to-end sampler over any layered qodec.

The point of this module is *simplicity*. It assembles the smallest parts that
can take a `qodec.Qodec` plus a `Program` and produce logical readout samples,
so it can serve as a proof-of-concept skeleton for more sophisticated machinery
later. Three parts:

* :class:`_PaulimerRuntime` — the **backend**. It lowers the bottom translation
  of the codec all the way to the codec's bottom ISA (whatever that ISA is —
  ``stim`` or otherwise), then *interprets* each bottom instruction's formal
  ``action`` with paulimer's :class:`~paulimer.OutcomeSpecificSimulation`, one
  independent trajectory per shot. It returns the slice's logical readouts,
  trivially decoded from the physical measurement records (see below).

* :class:`_TrivialProcessor` — a **ComposableTarget** for each upper
  translation. It lowers its program one step onto the layer below, delegates
  to that layer, and lifts the result back up by the gadgets' readout parity
  equations. Nothing more.

* :class:`UniversalSampler` — wires the runtime and the processors into a
  :class:`~qdk.ec.targets.base.CompositeTarget`. Its only construction
  parameter is the codec.

The "decoding" here is **trivial**: a gadget's logical readout is the XOR of the
body readouts named by its ``readouts`` parity equation. Syndromes (the gadgets'
``checks``) are *ignored* — there is no correction, and there is no noise model.
This is the noiseless, no-decoder reference: at zero noise every logical readout
is deterministic.

``assume`` assertions *are* enforced: a call's asserted flags are decoded by the
same readout-parity lift, and a violating shot raises :class:`AssumeViolation`
rather than being post-selected away. At zero noise the flags are deterministic,
so this never fires for a well-posed program.

The remaining qodec features are **warned about, not raised** (see
:class:`UnsupportedFeatureWarning`) and simply ignored, so a program using them
still runs: conditional actions (feed-forward), non-Clifford ``Rotate`` (a
stabilizer backend cannot represent them), and multi-term ``Stabilize`` (joint
stabilizer prep). Error correction against ``checks`` is out of scope (there is
no noise to correct), and flags are decoded only to evaluate ``assume`` — they
are not otherwise returned.
"""

from __future__ import annotations

import warnings
from collections.abc import Mapping, Sequence
from typing import Any, cast

import numpy as np
import numpy.typing as npt

import paulimer
import qodec
from qodec.actions import Clifford, Observe, Pauli as PauliAction, Rotate, Stabilize
from qodec.circuits._common import BlockLayout, ObservableTerm, parse_observable

from qodec.circuits import Program

from .._analysis.propagation.pauli import Pauli
from .compilers.recursive_lowering import _build_namespaced_remap, _remap_call
from .._qodec_compat import (
    _readout_equation,
    observe_count,
    outcome_indices,
    realization,
)
from .results import Batch
from ._coerce import coerce_program
from .base import ComposableTarget, CompositeTarget, Target


class UnsupportedFeatureWarning(UserWarning):
    """A qodec feature this proof-of-concept sampler does not model was
    encountered and ignored (rather than raising)."""


class AssumeViolation(RuntimeError):
    """A call's ``assume`` assertion was violated on at least one shot.

    `UniversalSampler` enforces ``assume`` by raising rather than discarding
    shots: at zero noise the asserted flags are deterministic, so a violation
    means the program's stated assumption does not actually hold.
    """

    def __init__(self, mnemonic: str, shot: int) -> None:
        super().__init__(
            f"`assume` assertion for call {mnemonic!r} violated on shot {shot}"
        )
        self.mnemonic = mnemonic
        self.shot = shot


class UniversalSampler(CompositeTarget[Batch]):
    """A from-scratch sampler over any layered qodec.

    Construct it with the codec — nothing else — and call ``execute(program,
    *, shots)`` to draw shots of the top-layer logical readouts. The backend is
    paulimer outcome-specific simulation; the per-layer decoding is the trivial
    readout-parity lift (syndromes ignored, no corrections, no noise model).

    A call's ``assume`` assertion is enforced by raising :class:`AssumeViolation`
    on any violating shot. Other unmodelled features (conditional actions,
    non-Clifford rotations, multi-term stabilizer prep) are warned and ignored;
    see the module docstring.

    Example
    -------
    >>> sampler = UniversalSampler(codec)            # doctest: +SKIP
    >>> batch = sampler.execute(program, shots=1000)  # doctest: +SKIP
    """

    def __init__(self, codec: qodec.Qodec) -> None:
        super().__init__(codec, _PaulimerRuntime, _TrivialProcessor)


class _PaulimerRuntime(Target[Batch]):
    """Bottom-translation backend: lower to the bottom ISA, simulate, decode.

    Bound to a two-layer slice ``[L, bottom-ISA]``. ``execute`` lowers its
    program onto the bottom ISA, simulates it with paulimer (one trajectory per
    shot), and trivially decodes ``L``'s logical readouts from the physical
    records.
    """

    def __init__(self, translation: qodec.Qodec) -> None:
        super().__init__(translation)
        self._translation = translation

    def execute(self, program: object, *, shots: int) -> Batch:
        source = self._translation.layers[0]
        program = coerce_program(program, source.isa)
        lowered, widths = _lower_one(self._translation, program)
        records = _simulate(lowered, shots)
        return _parity_decode(source, program, widths, records)


class _TrivialProcessor(ComposableTarget[Batch, Batch]):
    """Upper-translation processor: lower one step, delegate, lift by parity."""

    def __init__(self, translation: qodec.Qodec) -> None:
        super().__init__(translation)
        self._translation = translation
        self._below: Target[Batch] | None = None

    def compose_with(self, target: Target[Batch]) -> None:
        self._below = target

    def execute(self, program: object, *, shots: int) -> Batch:
        if self._below is None:
            raise RuntimeError("compose_with(...) must precede execute(...)")
        source = self._translation.layers[0]
        program = coerce_program(program, source.isa)
        lowered, widths = _lower_one(self._translation, program)
        below = self._below.execute(lowered, shots=shots)
        return _parity_decode(source, program, widths, below)


# ── lowering ────────────────────────────────────────────────────────────────


def _lower_one(translation: qodec.Qodec, program: Program) -> tuple[Program, list[int]]:
    """Lower ``program`` across one translation of a two-layer ``translation``.

    Substitutes each call's gadget body for the call, namespacing block qubits
    by the call's operands and internal/ancilla qubits per call instance (so
    sibling calls never collide on a shared physical wire). Returns the lowered
    program (in the lower layer's ISA) together with, per source call, the
    number of body readouts it contributes — the width of its block in the
    lower layer's readout stream.
    """
    source = translation.layers[0]
    target = translation.layers[1]
    lowered: list[qodec.instructions.InstructionCall] = []
    widths: list[int] = []
    for call in program.instructions:
        gadget = source.gadgets[call.mnemonic]
        remap = _build_namespaced_remap(
            gadget, call, call.mnemonic, namespace_internal_blocks=True
        )
        width = 0
        for body_call in realization(gadget).instructions:
            lowered.append(_remap_call(body_call, remap))
            width += _readout_width(target, body_call)
        widths.append(width)
    return Program(lowered, target.isa), widths


def _readout_width(layer: qodec.Layer, call: qodec.instructions.InstructionCall) -> int:
    """Number of logical readouts ``call`` produces at ``layer``.

    For a logical layer that has a gadget for the call, that is the gadget's
    ``observe`` count. For the bottom ISA (no gadgets), it is the number of
    ``observe`` outcomes the ISA instruction's action declares — i.e. the
    physical measurement records the instruction emits.
    """
    gadget = layer.gadgets.get(call.mnemonic)
    if gadget is not None:
        return observe_count(gadget)
    instruction = layer.isa.instruction(call.mnemonic)
    return sum(
        len(atom.observables)
        for atom in instruction.action
        if isinstance(atom, Observe)
    )


# ── trivial parity decode ────────────────────────────────────────────────────


def _parity_decode(
    layer: qodec.Layer,
    program: Program,
    widths: Sequence[int],
    below: Batch | npt.NDArray[np.bool_],
) -> Batch:
    """Lift the layer-below readouts up one translation by readout parity.

    ``below`` carries, per shot, the body readouts of every call in ``program``
    order; ``widths[k]`` is the size of call ``k``'s block within that stream.
    Each of a gadget's ``observe`` readout equations is a parity over its body
    readouts (``circuit.readouts[i]``), so the lifted readout is the XOR of the
    addressed columns of ``below``. Checks/syndromes are not consulted. A call
    carrying an ``assume`` assertion is enforced here, decoding its flags by the
    same parity lift and raising :class:`AssumeViolation` on a violating shot.
    """
    bits = np.asarray(below, dtype=np.bool_)
    columns: list[npt.NDArray[np.bool_]] = []
    offset = 0
    for call, width in zip(program.instructions, widths):
        gadget = layer.gadgets[call.mnemonic]
        columns.extend(_readout_columns(gadget, bits, offset))
        if call.assume:
            _check_assume(call, gadget, bits, offset)
        offset += width
    stacked = (
        np.column_stack(columns)
        if columns
        else np.zeros((bits.shape[0], 0), dtype=np.bool_)
    )
    decoded: list[list[bool]] = stacked.tolist()
    return decoded


def _readout_columns(
    gadget: qodec.Gadget, bits: npt.NDArray[np.bool_], offset: int
) -> list[npt.NDArray[np.bool_]]:
    """The XOR-of-records columns for one gadget's ``observe`` readouts.

    Each readout equation is a parity over the gadget's body readouts; the
    addressed records live at ``bits[:, offset + i]``.
    """
    columns: list[npt.NDArray[np.bool_]] = []
    for equation in gadget.readouts[: observe_count(gadget)]:
        column = np.zeros(bits.shape[0], dtype=np.bool_)
        for index in outcome_indices(_readout_equation(equation)):
            column ^= bits[:, offset + index]
        columns.append(column)
    return columns


def _check_assume(
    call: qodec.instructions.InstructionCall,
    gadget: qodec.Gadget,
    bits: npt.NDArray[np.bool_],
    offset: int,
) -> None:
    """Enforce ``call.assume``, raising on the first shot that violates it.

    The asserted flags are the gadget's flag readouts — the entries after its
    ``observe`` outcomes, named positionally by ``implements.flags`` — decoded
    to per-shot bits by the same parity lift as the observables.
    """
    flags = _flag_columns(gadget, bits, offset)
    satisfied = _assume_satisfied(call.assume, flags, bits.shape[0])
    violations = np.flatnonzero(~satisfied)
    if violations.size:
        raise AssumeViolation(call.mnemonic, int(violations[0]))


def _flag_columns(
    gadget: qodec.Gadget, bits: npt.NDArray[np.bool_], offset: int
) -> dict[str, npt.NDArray[np.bool_]]:
    """Decode the gadget's flag readouts to per-shot bit columns, keyed by
    ``implements.flags`` name (flags follow the observables, positionally)."""
    base = observe_count(gadget)
    columns: dict[str, npt.NDArray[np.bool_]] = {}
    for index, name in enumerate(gadget.implements.flags):
        column = np.zeros(bits.shape[0], dtype=np.bool_)
        for record in outcome_indices(_readout_equation(gadget.readouts[base + index])):
            column ^= bits[:, offset + record]
        columns[name] = column
    return columns


def _assume_satisfied(
    assume: Sequence[Mapping[str, int]],
    flags: Mapping[str, npt.NDArray[np.bool_]],
    shots: int,
) -> npt.NDArray[np.bool_]:
    """Per-shot mask of whether observed ``flags`` satisfy ``assume``.

    ``assume`` is an OR-of-AND truth table over flag names: a list of patterns,
    each an AND-conjunction ``{flag: 0|1}``. A shot is satisfied iff some
    pattern matches every flag it names; an empty ``assume`` is vacuous.
    """
    if not assume:
        return np.ones(shots, dtype=np.bool_)
    satisfied = np.zeros(shots, dtype=np.bool_)
    for pattern in assume:
        match = np.ones(shots, dtype=np.bool_)
        for name, bit in pattern.items():
            column = flags.get(name)
            if column is None:
                match = np.zeros(shots, dtype=np.bool_)
                break
            match &= column == bool(bit)
        satisfied |= match
    return satisfied


# ── paulimer interpretation ──────────────────────────────────────────────────


#: Base RNG seed for the backend. Shot ``k`` uses ``_base_seed + k`` so that
#: shots are independent yet the whole run is reproducible.
_base_seed = 0


def _simulate(program: Program, shots: int) -> npt.NDArray[np.bool_]:
    """Run ``program`` on paulimer, one trajectory per shot.

    Each bottom-ISA instruction is interpreted through its formal ``action``;
    every ``observe`` outcome is recorded, in program order, as one physical
    measurement record. Returns a ``(shots, records)`` boolean array.
    """
    layout = BlockLayout.of(program)
    rows: list[list[bool]] = []
    for shot in range(shots):
        sim = paulimer.OutcomeSpecificSimulation.new_with_seeded_random_outcomes(
            layout.total_qubits, _base_seed + shot
        )
        records: list[int] = []
        for call in program.instructions:
            for atom in program.lookup(call.mnemonic).action:
                _apply_atom(sim, atom, call, layout, records)
        outcomes = list(sim.outcome_vector)
        rows.append([bool(outcomes[index]) for index in records])
    if not rows:
        return np.zeros((0, 0), dtype=np.bool_)
    return np.array(rows, dtype=np.bool_)


def _apply_atom(
    sim: paulimer.OutcomeSpecificSimulation,
    atom: object,
    call: qodec.instructions.InstructionCall,
    layout: BlockLayout,
    records: list[int],
) -> None:
    """Dispatch one ISA action atom onto the simulation."""
    if _is_conditional(atom):
        warnings.warn(
            f"call {call.mnemonic!r}: conditional action ignored",
            UnsupportedFeatureWarning,
            stacklevel=2,
        )
        return
    if isinstance(atom, Stabilize):
        for operator in atom.operators:
            _emit_reset(sim, operator, call, layout)
    elif isinstance(atom, PauliAction):
        sim.apply_pauli(_pauli(atom.operator, call, layout))
    elif isinstance(atom, Clifford):
        support_size = _clifford_size(atom.generators)
        support = [
            layout.qubit_of(call, ObservableTerm("X", i)) for i in range(support_size)
        ]
        sim.apply_clifford(_clifford(atom.generators), supported_by=support)
    elif isinstance(atom, Observe):
        for observable in atom.observables:
            terms = parse_observable(observable.pauli)
            if not terms:
                warnings.warn(
                    f"call {call.mnemonic!r}: observe of a non-Pauli observable "
                    f"{observable.pauli!r} ignored",
                    UnsupportedFeatureWarning,
                    stacklevel=2,
                )
                continue
            records.append(sim.measure(_sparse(terms, call, layout)))
    elif isinstance(atom, Rotate):
        warnings.warn(
            f"call {call.mnemonic!r}: non-Clifford rotation ignored",
            UnsupportedFeatureWarning,
            stacklevel=2,
        )
    else:
        warnings.warn(
            f"call {call.mnemonic!r}: unsupported action {type(atom).__name__} "
            "ignored",
            UnsupportedFeatureWarning,
            stacklevel=2,
        )


def _emit_reset(
    sim: paulimer.OutcomeSpecificSimulation,
    operator: str,
    call: qodec.instructions.InstructionCall,
    layout: BlockLayout,
) -> None:
    """Active reset into the ``operator`` eigenbasis (single-Pauli only)."""
    terms = parse_observable(operator)
    if len(terms) != 1:
        warnings.warn(
            f"call {call.mnemonic!r}: multi-term stabilize {operator!r} ignored",
            UnsupportedFeatureWarning,
            stacklevel=2,
        )
        return
    term = terms[0]
    qubit = layout.qubit_of(call, term)
    outcome = sim.measure(_single("Z", qubit))
    sim.apply_conditional_pauli(_single("X", qubit), [outcome], parity=True)
    if term.basis == "X":
        sim.apply_unitary(paulimer.UnitaryOpcode.Hadamard, [qubit])
    elif term.basis == "Y":
        sim.apply_unitary(paulimer.UnitaryOpcode.Hadamard, [qubit])
        sim.apply_unitary(paulimer.UnitaryOpcode.SqrtZ, [qubit])


def _clifford_size(generators: Mapping[str, str]) -> int:
    """Number of qubits the Clifford tableau acts on."""
    size = 0
    for key, value in generators.items():
        for term in (*parse_observable(key), *parse_observable(value)):
            size = max(size, term.index + 1)
    return size


def _clifford(generators: Mapping[str, str]) -> paulimer.CliffordUnitary:
    """Build a paulimer Clifford from a (possibly partial) action tableau.

    A qodec ``Clifford`` lists only the non-trivial generator images; paulimer
    wants a complete tableau, so unlisted generators map to themselves. The
    qodec image format (``"X_0 X_1"``) is exactly paulimer's ``from_string``
    product format, so the tableau string is assembled directly.
    """
    size = _clifford_size(generators)
    parts = [f"X_{i}:{generators.get(f'X_{i}', f'X_{i}')}" for i in range(size)]
    parts += [f"Z_{i}:{generators.get(f'Z_{i}', f'Z_{i}')}" for i in range(size)]
    return paulimer.CliffordUnitary.from_string(", ".join(parts))


def _pauli(
    operator: str,
    call: qodec.instructions.InstructionCall,
    layout: BlockLayout,
) -> Pauli:
    return _sparse(parse_observable(operator), call, layout)


def _sparse(
    terms: Sequence[ObservableTerm],
    call: qodec.instructions.InstructionCall,
    layout: BlockLayout,
) -> Pauli:
    spec = {layout.qubit_of(call, term): term.basis for term in terms}
    return Pauli(cast(dict[int, Any], spec))


def _single(basis: str, qubit: int) -> Pauli:
    return Pauli(cast(dict[int, Any], {qubit: basis}))


def _is_conditional(atom: object) -> bool:
    return getattr(atom, "condition", None) is not None


__all__ = ["AssumeViolation", "UniversalSampler", "UnsupportedFeatureWarning"]
