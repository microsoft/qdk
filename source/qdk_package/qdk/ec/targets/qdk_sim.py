"""QdkSampler: lower a qodec program to a physical stim circuit and sample it on the QDK.

The pipeline is short: build the stim circuit with
:class:`~qdk.ec.targets.StimEmitter` (carrying the codec's noise model), strip
the ``DETECTOR`` / ``OBSERVABLE_INCLUDE`` / ``MPAD`` directives the QDK does not
act on (see :func:`_physical`), optionally annotate the remainder with the QDK's
``#!preselect`` directives, hand the stim source to :func:`qdk.stim.run`, and
return the per-shot physical measurement records as a
:class:`~qdk.ec.targets.Batch`.

The QDK samples the *physical* circuit only — it does not resolve checks across
gadget boundaries. The emitter's ``DETECTOR`` directives, and the ``MPAD``
placeholder records they reference, are a separate deq-style concern (an input
boundary stabilizer is resolved by a previous gadget's *output* boundary
stabilizer — the two XORed give a real parity check) that qdk.ec does not
duplicate here, so they are dropped before the circuit reaches the QDK. The Batch
is the raw physical measurement records in stim's order.

Preselection — keeping only shots whose flag records are ``0`` — is available two
ways: post-hoc on a sampled Batch via :func:`preselect_on_flags`, or up front by
passing ``preselect=<flag records>`` to :meth:`QdkSampler.execute`, which annotates
the source with ``#!preselect`` (see :func:`_preselect_source`) so the QDK
rejection-samples internally and returns exactly ``shots`` accepted shots.
"""

from __future__ import annotations

from collections.abc import Sequence

import numpy as np
import numpy.typing as npt

import stim

import qodec
from .results import Batch
from .base import Target
from .stim import StimEmitter

#: Stim measurement gates that append one record per qubit target.
_MEASUREMENT_GATES = frozenset({"M", "MZ", "MX", "MY", "MR", "MRZ", "MRX", "MRY"})


def _result_to_bit(result: object) -> bool:
    """Map a QDK ``Result`` (``One`` / ``Zero``) to a Python ``bool``."""
    return str(result) == "One"


def _physical(circuit: stim.Circuit) -> stim.Circuit:
    """Strip the directives the QDK does not act on, leaving the bare physical
    circuit (gates, noise, and real measurements).

    The emitter appends ``DETECTOR`` / ``OBSERVABLE_INCLUDE`` directives and
    ``MPAD`` placeholder records to resolve checks across gadget boundaries — the
    deq-style concern qdk.ec does not duplicate in the QDK path. The QDK does
    not act on detectors and drops ``MPAD`` pads, so they are removed here and
    the QDK sees only the physical circuit it actually simulates.
    """
    physical = stim.Circuit()
    for instruction in circuit:
        if isinstance(instruction, stim.CircuitRepeatBlock):
            raise NotImplementedError(
                "QdkSampler does not support REPEAT blocks; flatten the circuit"
            )
        if instruction.name not in ("DETECTOR", "OBSERVABLE_INCLUDE", "MPAD"):
            physical.append(instruction)
    return physical


def _qdk_run(
    source: str,
    *,
    shots: int,
    seed: int | None,
) -> Sequence[Sequence[object]]:
    """Compile stim ``source`` to QIR via the QDK Stim front-end and simulate it.

    Returns the QDK's per-shot list of ``Result`` outcomes, one per physical
    measurement record. This is the single point that calls into the optional
    ``qdk`` package.
    """
    from qdk import stim as qdk_stim

    results: Sequence[Sequence[object]] = qdk_stim.run(
        source, shots=shots, noise=None, seed=seed, type="clifford"
    )
    return results


def _preselect_source(circuit: stim.Circuit, flag_records: Sequence[int]) -> str:
    """Annotate the physical ``circuit`` for native QDK preselection on
    ``flag_records``.

    Wraps the circuit in the QDK's ``#!preselect_begin`` / ``#!preselect_expect``
    checkpoint annotations so the simulator rejection-samples internally, redoing
    a region whenever its flag record is not ``0``. Each flag gets its own
    ``begin`` / ``expect`` region (multiple ``expect`` statements under one
    ``begin`` do not compile). ``flag_records`` index the physical
    measurement-record stream.
    """
    flags = set(flag_records)
    remaining = len(flags)
    lines = ["#!preselect_begin"]
    record_index = 0
    for instruction in circuit:
        if isinstance(instruction, stim.CircuitRepeatBlock):
            continue
        lines.append(str(instruction))
        if instruction.name in _MEASUREMENT_GATES:
            for target in instruction.targets_copy():
                if not target.is_qubit_target:
                    continue
                if record_index in flags:
                    lines.append(f"#!preselect_expect {record_index} 0")
                    remaining -= 1
                    if remaining:
                        lines.append("#!preselect_begin")
                record_index += 1
    return "\n".join(lines) + "\n"


class QdkSampler(Target[Batch]):
    """Sample programs on the QDK simulator (via direct Stim support), returning
    a Batch of the physical measurement records.

    The QDK runs the bare physical circuit — the emitter's cross-gadget
    ``DETECTOR`` / ``OBSERVABLE_INCLUDE`` / ``MPAD`` scaffolding is stripped (see
    :func:`_physical`) — so the Batch is the raw physical measurements in stim's
    record order. For codecs whose gadgets need no ``MPAD`` virtual-input pads it
    matches a `StimSampler` Batch column-for-column; resolving checks across
    gadget boundaries for decoding is left to a deq-style layer.

    Parameters
    ----------
    codec:
        The qodec to bind.
    noise:
        Stim gate-noise model, forwarded to :class:`StimEmitter` (e.g.
        ``{"p_data": 0.01, "p_meas": 0.01}``). The emitted circuit's noise
        instructions are what the QDK compiles and simulates, so the simulated
        noise matches the emitter's DEM exactly. ``None`` runs noiseless.
    seed:
        RNG seed passed to the QDK simulator. Reproducibility is best-effort:
        the QDK's Stim simulator only honours the seed deterministically for
        small circuits, so repeated runs of a real gadget may differ bit-for-bit
        (the sampling *distribution* is unaffected).
    emitter:
        Optional pre-built :class:`StimEmitter`. Mutually exclusive with the
        ``noise`` kwarg.
    """

    def __init__(
        self,
        codec: qodec.Qodec,
        *,
        noise: dict[str, float] | None = None,
        seed: int | None = None,
        emitter: StimEmitter | None = None,
    ) -> None:
        super().__init__(codec)
        if emitter is None:
            emitter = StimEmitter(codec, noise=noise)
        elif noise is not None:
            raise ValueError(
                "QdkSampler(emitter=…) is mutually exclusive with the noise "
                "kwarg; pass noise to StimEmitter directly"
            )
        elif emitter.codec is not codec:
            raise ValueError(
                "QdkSampler(codec, emitter=…): emitter is bound to a different " "codec"
            )
        self._emitter = emitter
        self._seed = seed

    @property
    def emitter(self) -> StimEmitter:
        """The :class:`StimEmitter` that lowers programs to physical circuits."""
        return self._emitter

    def stim_source(
        self, program: object, *, preselect: Sequence[int] | None = None
    ) -> str:
        """Return the stim source :meth:`execute` hands to the QDK for ``program``.

        Without ``preselect`` this is the plain physical stim circuit (the
        emitted :class:`stim.Circuit` as text). With ``preselect`` — a sequence
        of flag record indices (the same indices :func:`preselect_on_flags`
        accepts) — it is that circuit annotated with the QDK's native
        ``#!preselect_begin`` / ``#!preselect_expect`` directives (see
        :func:`_preselect_source`). Useful for reviewing exactly what the QDK
        will run.
        """
        return self._prepare(program, preselect)[0]

    def execute(
        self,
        program: object,
        *,
        shots: int = 1,
        preselect: Sequence[int] | None = None,
    ) -> Batch:
        """Sample ``program`` for ``shots`` shots.

        With ``preselect=None`` (default) every shot is returned. Pass
        ``preselect`` as a sequence of flag record indices to instead return
        exactly ``shots`` *accepted* shots — those for which every listed flag
        record is ``0``. The source is annotated with the QDK's ``#!preselect``
        directives so the simulator rejection-samples internally; printing
        ``stim_source(program, preselect=…)`` shows exactly what runs.
        """
        if shots < 1:
            raise ValueError(f"shots must be >= 1; got {shots}")
        source, flags = self._prepare(program, preselect)
        results = _qdk_run(source, shots=shots, seed=self._seed)
        batch: Batch = [[_result_to_bit(o) for o in shot] for shot in results]
        if flags and any(any(row[i] for i in flags) for row in batch):
            raise RuntimeError(
                "the QDK did not honour the #!preselect annotations: flagged "
                "records still fired in the returned shots. Sample without "
                "preselect and filter with preselect_on_flags instead."
            )
        return batch

    def _prepare(
        self, program: object, preselect: Sequence[int] | None
    ) -> tuple[str, list[int]]:
        """Lower ``program`` to the physical stim source the QDK runs.

        Returns the stim source string (plain, or annotated with ``#!preselect``
        when ``preselect`` is given) and the validated flag record list. Shared
        by :meth:`stim_source` and :meth:`execute` so the two stay in lock-step.
        """
        circuit = _physical(self._emitter.build_circuit(program))
        flags = list(preselect or [])
        width = circuit.num_measurements
        for index in flags:
            if not 0 <= index < width:
                raise ValueError(
                    f"preselect flag record {index} is out of range for a "
                    f"{width}-record Batch"
                )
        source = _preselect_source(circuit, flags) if flags else str(circuit)
        return source, flags


def preselect_on_flags(
    sample: Batch,
    flag_columns: Sequence[int],
) -> npt.NDArray[np.bool_]:
    """Per-shot acceptance mask that preselects on a set of flag records.

    A shot is *accepted* (``True``) when every flag record in ``flag_columns``
    is ``0`` for that shot — the fault-tolerant-preparation preselection rule.
    Returns a boolean array of shape ``(len(sample),)``.
    """
    if not sample:
        return np.zeros((0,), dtype=np.bool_)
    matrix = np.asarray(sample, dtype=np.bool_)
    if not flag_columns:
        return np.ones((matrix.shape[0],), dtype=np.bool_)
    fired = matrix[:, list(flag_columns)].any(axis=1)
    return np.asarray(~fired, dtype=np.bool_)
