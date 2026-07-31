"""PaulimerSampler: codec-bound Sampler backed by `paulimer.FaultySimulation`.

Operates at the **logical** level: each block instance maps to a
contiguous range of qubits (one per logical qubit the block encodes),
and `Program` action atoms are dispatched as `FaultySimulation`
circuit-builder calls.

This is the noiseless logical-semantics reference. Use it to:

* verify a Program's ideal behaviour independently of a codec's
  physical realisation;
* regression-test decoders (zero noise → zero detection events →
  zero predictions);
* cross-check against `StimSampler` at zero noise.

`Readouts.bits` carries one column per program-level
:class:`~qodec.actions.Observe` observable, in program order. At the
logical level there are no syndrome checks, so these are also the
"raw bits" callers care about — internal reset measurements are
discarded.

Noise can be added later via :meth:`apply_fault` hooks; for now the
sampler is noiseless. ``paulimer`` is a required dependency.

Supported action atoms (same surface as :func:`qodec.circuits.to_stim`):

* :class:`~qodec.actions.Stabilize` — measure-and-correct reset, then
  basis rotation (H for X-basis, ``H; S`` for Y-basis). Single-Pauli
  operators only.
* :class:`~qodec.actions.Pauli` — ``apply_pauli``.
* :class:`~qodec.actions.Observe` — ``measure(Pauli)`` per
  observable.
* :class:`~qodec.actions.Clifford` — transversal CX patterns →
  ``ControlledX``.
"""

from __future__ import annotations

from typing import Any, cast

import numpy as np
import numpy.typing as npt

import paulimer
import qodec
from qodec.actions import Clifford, Observe, Pauli as PauliAction, Stabilize
from qodec.circuits._common import (
    BlockLayout,
    ObservableTerm,
    parse_observable,
    transversal_cx_pairs,
)

from ..profile.propagation.pauli import Pauli
from ._coerce import coerce_program
from .results import Batch


class PaulimerSampler:
    """Logical-level noiseless Sampler backed by `paulimer.FaultySimulation`.

    Implements the `Sampler` Protocol: ``codec`` property + ``execute``.
    No detector events are emitted (logical level has no checks).
    """

    def __init__(self, codec: qodec.Codec) -> None:
        self._codec = codec

    @property
    def codec(self) -> qodec.Codec:
        return self._codec

    def execute(self, program: object, *, shots: int) -> Batch:
        coerced = coerce_program(program, self._codec.layers[0].isa)
        layout = BlockLayout.of(coerced)

        sim = paulimer.FaultySimulation(qubit_count=layout.total_qubits)
        observable_indices: list[int] = []

        for call in coerced.instructions:
            instr = coerced.lookup(call.mnemonic)
            for atom in instr.action:
                _check_unconditional(atom, call.mnemonic)
                if isinstance(atom, Stabilize):
                    _emit_stabilize(sim, atom, call, layout)
                elif isinstance(atom, PauliAction):
                    _emit_pauli(sim, atom, call, layout)
                elif isinstance(atom, Observe):
                    _emit_observe(sim, atom, call, layout, observable_indices)
                elif isinstance(atom, Clifford):
                    _emit_clifford(sim, atom, call, layout)
                else:
                    raise NotImplementedError(
                        f"call {call.mnemonic!r}: unsupported action atom "
                        f"of type {type(atom).__name__}"
                    )

        if not observable_indices:
            bits = np.zeros((shots, 0), dtype=np.bool_)
        else:
            all_outcomes = _bitmatrix_to_ndarray(sim.sample(shots))
            # Project to observable columns — at the logical level there
            # are no checks, so the "raw bits" the user cares about are
            # the program's Observe outcomes. The reset-measurement bits
            # are internal mechanics.
            bits = all_outcomes[:, observable_indices]

        return bits.tolist()


# ---------------------------------------------------------------------------
# Action atom dispatch
# ---------------------------------------------------------------------------


def _emit_stabilize(
    sim: paulimer.FaultySimulation,
    atom: Stabilize,
    call: qodec.InstructionCall,
    layout: BlockLayout,
) -> None:
    """Reset (measure + conditional-X) then optionally rotate."""
    for operator in atom.operators:
        terms = parse_observable(operator)
        if len(terms) != 1:
            raise NotImplementedError(
                f"call {call.mnemonic!r}: Stabilize over multi-term Pauli "
                f"product ({operator!r}) requires ancilla-based prep not "
                f"yet emitted by PaulimerSampler"
            )
        term = terms[0]
        q = layout.qubit_of(call, term)
        # Measure Z, then conditionally flip — equivalent to active reset.
        outcome = sim.measure(_single_qubit_pauli("Z", q))
        sim.apply_conditional_pauli(_single_qubit_pauli("X", q), [outcome], parity=True)
        if term.basis == "X":
            sim.apply_unitary(paulimer.UnitaryOpcode.Hadamard, [q])
        elif term.basis == "Y":
            # |+i> = S H |0>
            sim.apply_unitary(paulimer.UnitaryOpcode.Hadamard, [q])
            sim.apply_unitary(paulimer.UnitaryOpcode.SqrtZ, [q])


def _emit_pauli(
    sim: paulimer.FaultySimulation,
    atom: PauliAction,
    call: qodec.InstructionCall,
    layout: BlockLayout,
) -> None:
    sim.apply_pauli(_pauli_from_terms(parse_observable(atom.operator), layout, call))


def _emit_observe(
    sim: paulimer.FaultySimulation,
    atom: Observe,
    call: qodec.InstructionCall,
    layout: BlockLayout,
    indices_collected: list[int],
) -> None:
    for observable in atom.observables:
        pauli = observable.pauli
        if pauli is None:
            raise ValueError(
                f"call {call.mnemonic!r}: Observe of flag observable "
                f"{observable.name!r} (no Pauli) is not transpilable to "
                f"PaulimerSampler"
            )
        terms = parse_observable(pauli)
        outcome_idx = sim.measure(_pauli_from_terms(terms, layout, call))
        indices_collected.append(outcome_idx)


def _emit_clifford(
    sim: paulimer.FaultySimulation,
    atom: Clifford,
    call: qodec.InstructionCall,
    layout: BlockLayout,
) -> None:
    pairs = transversal_cx_pairs(atom.generators, call, layout)
    if pairs is not None:
        for control, target in pairs:
            sim.apply_unitary(paulimer.UnitaryOpcode.ControlledX, [control, target])
        return
    raise NotImplementedError(
        f"call {call.mnemonic!r}: Clifford with generators "
        f"{atom.generators} is not yet recognised by PaulimerSampler"
    )


def _pauli_from_terms(
    terms: list[ObservableTerm],
    layout: BlockLayout,
    call: qodec.InstructionCall,
) -> Pauli:
    """Build a `Pauli` from a list of single-qubit Pauli terms."""
    spec = cast(dict[int, Any], {layout.qubit_of(call, t): t.basis for t in terms})
    return Pauli(spec)


def _single_qubit_pauli(basis: str, qubit: int) -> Pauli:
    return Pauli(cast(dict[int, Any], {qubit: basis}))


def _check_unconditional(atom: object, mnemonic: str) -> None:
    if getattr(atom, "condition", None):
        raise NotImplementedError(
            f"call {mnemonic!r}: conditional action atoms are not yet "
            f"supported by PaulimerSampler ({type(atom).__name__})"
        )


def _bitmatrix_to_ndarray(bitmatrix: object) -> npt.NDArray[np.bool_]:
    """Convert a paulimer `BitMatrix` to a 2-D bool numpy array.

    `BitMatrix` doesn't implement the numpy buffer protocol; iterate
    its `.rows` (each a `BitVector`) and stack.
    """
    return np.array(
        [list(bitmatrix.rows[i]) for i in range(bitmatrix.row_count)],  # type: ignore[attr-defined]
        dtype=np.bool_,
    )
