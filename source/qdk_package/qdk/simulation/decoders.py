"""Decoder factories and contracts for :func:`qdk.simulation.run_qir`.

These utilities require ``qdk[ec]``. Pass a preparation callable as
``run_qir(..., qodec=codec, decoder=prepare_deq_decoder)``. Preparation runs
once per encoded layer. The syndrome and frame decoders return factories for
fresh, seeded shot sessions; deq prepares a model from the complete circuit trace.

``prepare_syndrome_decoder`` is the default minimum-weight Pauli decoder.
``prepare_frame_decoder`` tracks noiseless frames without inferring faults.
``prepare_deq_decoder`` uses deq relay-BP for circuit-level decoding of complete
measurement-independent shots, with fault probabilities derived from ``noise``.

Built-in decoders evaluate gadget equations as supplied, without auditing their
correctness or completeness. Use :func:`qdk.ec.audit` to check the declarations
before execution. Invalid declarations can produce incorrect results or fail
during decoding. Contradictory evidence and unresolved readouts retain their
normal decoding failure behavior.

Single-qubit Pauli corrections are tracked in a noiseless Pauli frame: they
sample no gate noise and never lose a qubit. Any other correction operation
runs as a physical gate with its configured noise.

With syndrome, frame, or custom session decoders, Clifford programs on the stabilizer
backend with one encoded layer, Pauli gate and measurement/reset noise without
loss, and no measurement-dependent feedback run in the native shot loop.
Decoding then takes one of two forms. When every measurement is recorded by a
terminal gadget and the prepared factory implements the optional
``BatchDecoderFactory`` capability, its ``prepare_batch()`` session provides
``ReadoutBatch`` evaluators through ``prepare_readouts()``; ``ReadoutTable``
implements one for deterministic finite tables. The supplied syndrome and frame
factories implement this capability, and wrappers that return them
retain it. Otherwise, including for custom factories without the capability,
each shot replays a fresh, seeded decoder session on its native records, with
its Pauli corrections flipping the records they reach. A correction that is
not a single-qubit Pauli reruns every shot in the interpreter. Other programs,
backends, and the retry policy use the interpreter with these session decoders.

To decode the physical circuit with deq::

    from qdk.simulation import run_qir
    from qdk.simulation.decoders import prepare_deq_decoder

    results = run_qir(
        qir, shots=1000, qodec=codec, decoder=prepare_deq_decoder, noise=noise
    )

deq decoding keeps native physical sampling and composes consecutive traced Qodec
gadgets into reusable deq types. Groups target at most 1024 model entries,
counting measurements, finished and unfinished checks, errors, and one entry per
source gadget. An individually larger gadget remains intact. Grouping uses no
instruction names or code-specific rules. Instances connect through their encoded
block ports. deq propagates circuit faults across those connections and uses
its window coordinator with buffer and lookahead radii of one composite each.
All instances and outcomes are submitted before waiting for decoded readouts;
discarded and still-live output ports receive explicit terminators.

Authored detection checks are supplied without validation. deq derives only
missing output-port propagation relations from the local circuit, not extra
detection checks. Detection checks must use physical records and stabilizer
ports; logical sign equations describe frame propagation. deq evaluates logical
readouts; QDK does not replay its syndrome decoder. Raw rejection flags are
evaluated in batches from their authored record parities, including declared
frame changes but excluding inferred error corrections. Under ``discard``, shots
that already fail their raw-flag selections are removed before deq decoding.
Explicitly returned, unselected flags are not filtered. Under ``raise``, decoding
still runs before checking selections, preserving failure ordering.
Noiseless behavior is preserved, but noisy logical results and rejection rates
can differ from the default syndrome decoder.
Each run uses a seeded deq stream and isolated shot instances; exact stochastic
answers are not promised to match other decoders or deq versions.
One lowering records the physical circuit, gadget connections, and result maps;
QIR is not rewritten to introduce composite intrinsics. Each gadget specialization
shares one compiled equation/frame contract between raw-flag and deq preparation.
Decoded-bit destinations and check positions are prepared once, not rebuilt per shot.
Explicit ``PROPAGATE`` statements describe logical frame transport before
primitive compilation; compiled matrices are not patched. For unitary Clifford
and Pauli action lists, the declared action determines the logical-input terms,
emitted directly as deq targets without an intermediate QDK parity representation.
The global phase of a propagated correction is ignored. Other action forms retain
deq's inferred transport. deq also supplies physical measurement and input
syndrome contributions. Authored logical-sign equations override inferred rows,
and authored frames supply additional measurement terms and constant flips.
Signs of the intended physical operation are not added as frame corrections.
deq tracks these frames during decoding and compiles bounded ``COMPOSE``
definitions over the primitive models without recompiling them.
The library and repeated composite types are reused within a run. Shot batches
target at most 262144 model entries and 256 shots, with a minimum of one shot.
After a completed batch, deq resets instances and connections while retaining
its type and decoder caches. A connected shot is never reset midway through.
These limits are work estimates, not byte limits: native records for all shots,
the fixed trace, and deq's history within one shot still grow with program size.
Window decoding does not guarantee constant total memory or the same noisy
corrections as whole-component decoding.

deq requires one encoded layer, measurement-independent Clifford
execution on the stabilizer backend, and no loss. Measurements may be random;
only the execution trace must be independent of their values. It supports a
single Pauli mechanism per noise table and depolarizing channels with total
nonidentity probability at most 3/4 (one qubit) or 15/16 (two qubits). General Pauli channels
are rejected rather than approximated. Measurement/reset noise follows the
simulator: it acts on the state after measurement/preparation, not on a bit
already recorded; discards remain noiseless. The retry policy is unsupported.
Unsupported programs raise instead of silently falling back to another decoder.

Circuit fault probabilities are passed to deq without complementing values
above one half. deq 0.5.7 can miss corrections for such faults at zero syndrome;
QDK does not work around this decoder behavior.

Install deq separately with ``pip install deq deq-runtime``; deq
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
