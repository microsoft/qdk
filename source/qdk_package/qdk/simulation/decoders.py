"""Decoder factories and contracts for :func:`qdk.simulation.run_qir`.

These utilities require ``qdk[ec]``. Pass a preparation callable as
``run_qir(..., qodec=codec, decoder=prepare_deq_decoder)``. Preparation runs
once per encoded layer and returns a factory for fresh, seeded shot sessions.

``prepare_syndrome_decoder`` is the default minimum-weight Pauli decoder.
``prepare_frame_decoder`` tracks noiseless frames without inferring faults.
``prepare_deq_decoder`` uses deq relay-BP for per-boundary syndrome decoding,
or optional circuit-level decoding of complete non-adaptive shots.

Built-in decoders evaluate gadget equations as supplied, without auditing their
correctness or completeness. Use :func:`qdk.ec.audit` to check the declarations
before execution. Invalid declarations can produce incorrect results or fail
during decoding. Contradictory evidence and unresolved readouts retain their
normal decoding failure behavior.

Single-qubit Pauli corrections are tracked in a noiseless Pauli frame: they
sample no gate noise and never lose a qubit. Any other correction operation
runs as a physical gate with its configured noise.

With the default or boundary decoders, Clifford programs on the stabilizer
backend with one encoded layer, Pauli gate and measurement/reset noise without
loss, and no measurement-dependent feedback run in the native shot loop.
Decoding then takes one of two forms. When every measurement is recorded by a
terminal gadget and the prepared factory implements the optional
``BatchDecoderFactory`` capability, its ``prepare_batch()`` session provides
``ReadoutBatch`` evaluators through ``prepare_readouts()``; ``ReadoutTable``
implements one for deterministic finite tables. The supplied syndrome, frame,
and deq factories implement this capability, and wrappers that return them
retain it. Otherwise, including for custom factories without the capability,
each shot replays a fresh, seeded decoder session on its native records, with
its Pauli corrections flipping the records they reach. A correction that is
not a single-qubit Pauli reruns every shot in the interpreter. Other programs,
backends, and the retry policy always use the interpreter. Batched deq
inference preserves the per-shot decoder seeds; only clean-syndrome results
are shared, never stochastic solver answers.

To configure deq's independent Pauli prior::

    from functools import partial
    from qdk.simulation import run_qir
    from qdk.simulation.decoders import PrepareDecoder, prepare_deq_decoder

    decoder: PrepareDecoder = partial(prepare_deq_decoder, error_probability=0.002)
    results = run_qir(qir, shots=100, qodec=codec, decoder=decoder)

To derive a circuit-level deq model from the simulator's noise instead::

    decoder = partial(prepare_deq_decoder, circuit_level=True)
    results = run_qir(qir, shots=1000, qodec=codec, decoder=decoder, noise=noise)

This mode keeps native physical sampling and compiles the entire shot into one
closed deq gadget. It uses the authored record equations in the zero correction
frame, compiles output-sign corrections and declared frame updates, and
propagates circuit faults across invocation boundaries. deq evaluates readouts
and corrects logical outputs; QDK does not replay its syndrome decoder.
Flags retain the declared zero-frame
record parities, not deq-corrected values. Noiseless behavior is preserved, but
noisy logical results and rejection rates can differ from the boundary decoder.
Each run uses a seeded deq stream and isolated shot instances; exact stochastic
answers are not promised to match other decoders or deq versions.

Circuit-level mode requires one encoded layer, measurement-independent Clifford
execution on the stabilizer backend, and no loss. It supports a single Pauli
mechanism per noise table and depolarizing channels with total nonidentity
probability at most 3/4 (one qubit) or 15/16 (two qubits). General Pauli channels
are rejected rather than approximated. Measurement/reset noise follows the
simulator: it acts on the state after measurement/preparation, not on a bit
already recorded; discards remain noiseless. ``error_probability`` and the retry
policy are unsupported in this mode. Unsupported programs raise instead of
silently falling back to the boundary decoder.

The keyword ``circuit_level`` follows this module's circuit-level versus boundary
noise terminology. A separate ``prepare_circuit_deq_decoder`` function would
duplicate the existing preparation entry point, so this is a mode of that
factory, not another exported name. The decoder module keeps its 21 exports.

Install deq separately with ``pip install deq deq-runtime``; circuit-level
decoding is tested with deq and deq-runtime 0.5.7. It is not
required by ``qdk[ec]`` or ``qdk[all]``; missing dependencies are reported
only when this decoder is selected. deq-runtime publishes no Windows ARM64
wheels, and deq requires Stim, which has no Linux aarch64 or Windows ARM64
wheels.
deq 0.5.2 requires a released QDK 1.32.x; when testing a development wheel
versioned 0.0.0, install dependencies first, then reinstall the local QDK
wheel with ``--no-deps`` to avoid replacing it with a released QDK.
"""

from ._qodec.decoding import prepare_deq_decoder as prepare_deq_decoder
from ._qodec.decoding import prepare_syndrome_decoder as prepare_syndrome_decoder
from ._qodec.frame_runtime import prepare_frame_decoder as prepare_frame_decoder
from ._qodec.protocols import (
    BatchDecoderFactory as BatchDecoderFactory,
    BatchDecoderSession as BatchDecoderSession,
    BatchUnsupported as BatchUnsupported,
    BlockReference as BlockReference,
    Correction as Correction,
    Corrections as Corrections,
    Decoded as Decoded,
    DecoderFactory as DecoderFactory,
    DecoderSession as DecoderSession,
    ExecutionRejected as ExecutionRejected,
    ExecutionUnresolved as ExecutionUnresolved,
    Invocation as Invocation,
    PrepareDecoder as PrepareDecoder,
    ReadoutBatch as ReadoutBatch,
    ReadoutTable as ReadoutTable,
    Readouts as Readouts,
)
from ._qodec.quantum_operations import (
    LogicalSlot as LogicalSlot,
    Operation as Operation,
)

__all__ = [
    "BatchDecoderFactory",
    "BatchDecoderSession",
    "BatchUnsupported",
    "BlockReference",
    "Correction",
    "Corrections",
    "Decoded",
    "DecoderFactory",
    "DecoderSession",
    "ExecutionRejected",
    "ExecutionUnresolved",
    "Invocation",
    "LogicalSlot",
    "Operation",
    "PrepareDecoder",
    "ReadoutBatch",
    "ReadoutTable",
    "Readouts",
    "prepare_syndrome_decoder",
    "prepare_frame_decoder",
    "prepare_deq_decoder",
]
