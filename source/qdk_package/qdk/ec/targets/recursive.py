"""RecursiveTarget: execute a layered program through a bottom executor.

A `RecursiveTarget` looks like any other sampler — ``execute(program, *, shots)
→ Batch`` — but it preserves the qodec's abstraction layers instead of
flattening them into one monolithic decode:

* A **bottom** `Sampler` (e.g. `StimSampler`, or a future deq per-shot sampler)
  executes the bottom slice of the qodec under its own noise model and returns
  raw physical readouts. Noise lives entirely on the bottom; the recursive
  target itself is noise-free.
* The bottom slice's physical readouts are lifted to that slice's logical
  readouts via stim's measurement-to-detector conversion.
* Each upper translation is then lifted in turn — bottom-up — by the readout
  parity equations its gadgets declare, until the top program's readouts
  remain.

This is the staged, layer-preserving counterpart to a flat
`DeqLerTarget`/`StimSampler`, which compose every translation into one circuit.
Staging is what lets a *vertically concatenated* qodec (an outer-code block
realised across inner-code blocks) be executed with deq driving only the
physical inner layer — the layer where deq's noise model and decoders are
defined — while the outer code is resolved classically on top.

The default per-layer lift resolves each gadget's logical readouts as the XOR
of the body readouts its analytical surface declares. Richer per-layer
processing — error *detection* (post-selecting on a gadget's checks/flags) or
*correction* (e.g. consuming an erasure herald) — belongs to the deq execution
path; the raw target preserves soft/herald `Batch` carriers and views.
"""

from __future__ import annotations

import numpy as np

import qodec as qc

from .._readouts import observable_slots
from .._references import outcomes_of
from qodec.circuits import Program
from .compilers import RecursiveLowering
from .results import Batch
from ._coerce import coerce_program
from .base import Sampler, Target
from .stim import StimEmitter


def _parity_lift(
    qodec: qc.Qodec,
    level: int,
    upper_program: Program,
    lower: Batch,
) -> Batch:
    """Lift a layer-below `Batch` up one translation by readout parity.

    The layer-below batch carries, per shot, the logical readouts of every
    gadget body in ``upper_program`` order. For each call, its gadget's
    ``readouts`` are parity equations over ``circuit.readouts[i]`` — i.e. over the
    body's own logical outcomes — so each upper readout is the XOR of the
    corresponding columns of the layer-below batch.
    """
    layer = qodec.layers[level]
    below = qodec.layers[level + 1]
    lower_bits = np.asarray(lower, dtype=np.bool_)
    shots = lower_bits.shape[0]

    columns: list[np.ndarray] = []
    offset = 0
    for call in upper_program.instructions:
        gadget = layer.gadgets[call.mnemonic]
        for slot in observable_slots(gadget):
            column = np.zeros(shots, dtype=np.bool_)
            for index in outcomes_of(slot.equation):
                column ^= lower_bits[:, offset + index]
            columns.append(column)
        for body_call in gadget.circuit.instructions:
            body_gadget = below.gadgets.get(body_call.mnemonic)
            if body_gadget is not None:
                offset += len(observable_slots(body_gadget))

    if not columns:
        return [[] for _ in range(shots)]
    stacked: list[list[bool]] = np.stack(columns, axis=1).tolist()
    return stacked


class RecursiveTarget(Target[Batch]):
    """Staged, layer-preserving sampler over a layered qodec.

    Parameters
    ----------
    qodec :
        The full layered qodec.
    bottom :
        A `Sampler` bound to a bottom slice ``qodec.slice(split, n)``. It
        executes that slice (under its own noise) and returns raw physical
        readouts as a `Batch`. The split point is inferred from how many layers
        ``bottom.qodec`` spans.
    """

    def __init__(
        self,
        qodec: qc.Qodec,
        bottom: Sampler,
    ) -> None:
        super().__init__(qodec)
        n_layers = len(qodec.layers)
        split = n_layers - len(bottom.qodec.layers)
        if split < 0 or bottom.qodec.layers[0].isa.name != qodec.layers[split].isa.name:
            raise ValueError(
                "bottom.qodec must be a bottom slice of qodec "
                "(its layers a suffix of qodec.layers)"
            )
        self._bottom = bottom
        self._split = split
        # The bottom slice is sampled raw; gadget flags (verified-prep reject
        # truth tables) are post-processing predicates, not stim observables,
        # so flag emission is suppressed for the readout lift.
        self._bottom_emitter = StimEmitter(bottom.qodec, emit_flags=False)

    @property
    def bottom(self) -> Sampler:
        return self._bottom

    def execute(self, program: object, *, shots: int) -> Batch:
        top = coerce_program(program, self._qodec.layers[0].isa)

        # Lower the program one translation at a time so each upper layer's
        # program is retained for its lift.
        programs: list[Program] = [top]
        for level in range(self._split):
            sub = self._qodec.slice(level, level + 2)
            lowered = RecursiveLowering(sub).compile(programs[-1]).program
            programs.append(lowered)
        bottom_program = programs[self._split]

        # Bottom slice: sample physical readouts, lift to the slice's logical
        # readouts via stim m2d, keeping only the logical (non-flag) columns.
        physical = self._bottom.execute(bottom_program, shots=shots)
        observables = self._bottom_emitter.observable_flips(
            bottom_program, np.asarray(physical, dtype=np.bool_)
        )
        mask = self._bottom_emitter.logical_observable_mask(bottom_program)
        lower: Batch = observables[:, mask].tolist()

        # Fold up, bottom translation first.
        for level in range(self._split - 1, -1, -1):
            lower = _parity_lift(self._qodec, level, programs[level], lower)
        return lower


__all__ = ["RecursiveTarget"]
