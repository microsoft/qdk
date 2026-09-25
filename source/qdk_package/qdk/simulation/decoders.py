"""Decoder factories and contracts for :func:`qdk.simulation.run_qir`.

These utilities require ``qdk[ec]``. Pass a preparation callable as
``run_qir(..., qodec=codec, decoder=prepare_deq_decoder)``. Preparation runs
once per encoded layer and returns a factory for fresh, seeded shot sessions.

``prepare_syndrome_decoder`` is the default minimum-weight Pauli decoder.
``prepare_frame_decoder`` tracks noiseless frames without inferring faults.
``prepare_deq_decoder`` uses deq relay-BP for per-boundary syndrome decoding.

Single-qubit Pauli corrections are tracked in a noiseless Pauli frame: they
sample no gate noise and never lose a qubit. Any other correction operation
runs as a physical gate with its configured noise.

Clifford programs on the default stabilizer backend with one encoded layer,
Pauli gate noise without loss or measurement noise, and no
measurement-dependent feedback run in the native shot loop.
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

Install deq separately with ``pip install deq deq-runtime``. It is not
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
