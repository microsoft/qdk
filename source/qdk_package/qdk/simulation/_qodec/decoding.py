from __future__ import annotations

from collections.abc import Callable, Sequence
from typing import TYPE_CHECKING

import numpy as np
from paulimer import DensePauli
from qodec import Code, Gadget, Layer
from qodec.instructions import InstructionCall
from scipy.optimize import Bounds, LinearConstraint, milp

from .instruction_set import pauli
from .protocols import (
    BatchDecoderSession,
    BlockReference,
    Correction,
    Corrections,
    Decoded,
    DecoderFactory,
    ExecutionUnresolved,
    Invocation,
    Readouts,
    ReadoutBatch,
    ReadoutTable,
)
from .quantum_operations import Operation
from .readout_equations import (
    BinarySystem,
    InconsistentParity,
    Parity,
    expression,
    prepare_frames,
)

if TYPE_CHECKING:
    from deq.runtime import Runtime


class CodeDecoder:
    def __init__(self, code: Code) -> None:
        self.width = code.physical_qubit_count
        self.operators = {
            property_name: tuple(
                pauli(expression, self.width)
                for expression in getattr(code, property_name)
            )
            for property_name in ("stabilizers", "x", "z")
        }
        self.stabilizers = self.operators["stabilizers"]
        self.faults = tuple(
            pauli(f"{basis}_{target}", self.width)
            for target in range(self.width)
            for basis in "XYZ"
        )
        self.syndromes = np.array(
            [
                [not fault.commutes_with(stabilizer) for fault in self.faults]
                for stabilizer in self.stabilizers
            ],
            dtype=float,
        ).reshape(len(self.stabilizers), len(self.faults))
        self.corrections: dict[Readouts, DensePauli] = {}

    def correct(self, syndrome: Readouts) -> DensePauli:
        if len(syndrome) != len(self.stabilizers):
            raise ValueError("Syndrome positions must match the code stabilizers")
        if syndrome in self.corrections:
            return self.corrections[syndrome].copy()
        correction = DensePauli.identity(self.width)
        if any(syndrome):
            observed = tuple(
                index for index, value in enumerate(syndrome) if value is not None
            )
            observed_values = np.asarray(
                [syndrome[index] for index in observed], dtype=float
            )
            fault_count = len(self.faults)
            check_count = len(observed)
            parity = np.hstack(
                (self.syndromes[list(observed)], -2 * np.eye(check_count))
            )
            exclusive = np.zeros((self.width, fault_count + check_count))
            for target in range(self.width):
                exclusive[target, 3 * target : 3 * target + 3] = 1
            solution = milp(
                c=np.r_[np.ones(fault_count), np.zeros(check_count)],
                integrality=np.ones(fault_count + check_count),
                bounds=Bounds(
                    0, np.r_[np.ones(fault_count), np.full(check_count, self.width)]
                ),
                constraints=(
                    *(
                        LinearConstraint(row, float(value), float(value))
                        for row, value in zip(parity, observed_values)
                    ),
                    LinearConstraint(exclusive, 0, 1),
                ),
            )
            if not solution.success:
                # Dependent stabilizers make some syndromes unreachable by any
                # Pauli, e.g. after a readout fault; the shot is inconsistent.
                raise InconsistentParity(
                    f"No Pauli correction matches syndrome {syndrome}"
                )
            for fault, selected in zip(self.faults, solution.x[:fault_count]):
                if selected > 0.5:
                    correction *= fault
        self.corrections[syndrome] = correction.copy()
        return correction


def _sign(boundary: str, entry: int, property_name: str, index: int) -> Parity:
    return Parity(frozenset({("encoding", boundary, entry, property_name, index)}))


def prepare_syndrome_decoder(layer: Layer) -> DecoderFactory:
    return SyndromeModel(layer)


def prepare_deq_decoder(
    layer: Layer, *, runtime_factory: Callable[[int], Runtime] | None = None
) -> DecoderFactory:
    """Prepare a circuit-level deq decoder for a Qodec layer.

    Requires ``pip install deq deq-runtime``. The decoder composes bounded
    groups of local Clifford gadgets using the ``run_qir`` noise model and
    deq's window coordinator with relay-BP decoding by default.

    ``runtime_factory(seed)`` may instead construct a fresh ``deq.runtime.Runtime``
    with caller-selected decoder/coordinator settings and ``controller="jit"``.
    It is called once when a nonempty run needs decoding, inside the worker's
    asyncio event loop, not during layer preparation or for each shot. The integer
    argument is that run's seed; the factory decides how to configure decoder
    seeding. QDK enters and closes the returned runtime, including on decoding
    errors. Do not return a shared runtime or reuse one between calls.
    With no factory, the existing seeded, single-worker relay-BP runtime and
    radius-one window settings are retained.

    Gadget models are converted independently of the program trace. The trace
    supplies connected top-level ISA calls and native physical samples; deq
    owns frame propagation through explicit PROPAGATE statements derived from
    the instruction's declared ChannelAction. Conditional Paulis are supported;
    other conditional action kinds are rejected.
    The execution trace must be measurement-independent, but individual
    measurements may be random. Execution requires
    one encoded layer, the stabilizer backend, and supported Pauli channels
    without loss. QIR compilation resolves quantum calls against the top-level
    ISA and rejects undeclared calls. Top-level parameters may specialize the
    action when the physical gadget circuit is fixed; check and readout
    definitions do not depend on them. Flags may use only
    local measurements, constants, and aliases of those values. They receive
    neither incoming-frame adjustments nor inferred error corrections. A known
    nonzero incoming syndrome can therefore raise a flag without a new fault.
    It does not use QDK's syndrome decoder. The retry policy is unsupported.
    A worker thread allows synchronous simulation
    inside a running asyncio event loop, including notebooks.
    Circuit fault probabilities are passed to deq without complementing values
    above one half. deq 0.5.7 can miss corrections for such faults at zero syndrome.
    """
    if runtime_factory is not None and not callable(runtime_factory):
        raise TypeError(
            "runtime_factory must be callable and return a fresh deq Runtime"
        )
    try:
        from .deq_decoding import DeqModel
    except ModuleNotFoundError as error:
        if error.name and error.name.split(".")[0] in ("deq", "deq_runtime"):
            raise ImportError(
                "The deq decoder requires deq and deq-runtime. "
                "Install them with: pip install deq deq-runtime"
            ) from error
        raise
    return DeqModel(layer, runtime_factory=runtime_factory)


class SyndromeModel:
    def __init__(self, layer: Layer) -> None:
        self.gadgets = {}
        self.frames = {}
        self.framed: dict[str, frozenset[tuple[int, str, int]]] = {}
        self.systems: dict[str, BinarySystem] = {}
        self.readout_keys: dict[str, tuple[Parity, ...]] = {}
        self._readout_tables: dict[tuple[str, int], tuple[Readouts, ...] | None] = {}
        for name, gadget in layer.gadgets.items():
            self.frames[name] = prepare_frames(gadget)
            self.framed[name] = frozenset(
                (frame.output, frame.basis, frame.logical)
                for frame in self.frames[name]
            )
            codes = {
                (boundary, entry): CodeDecoder(encoding.code)
                for boundary, encodings in (
                    ("in", gadget.inputs),
                    ("out", gadget.outputs),
                )
                for entry, encoding in enumerate(encodings)
            }
            checks = tuple(expression(check) for check in gadget.checks)
            readouts = tuple(
                expression(readout.equation) for readout in gadget.readouts
            )
            self.gadgets[name] = (codes, checks, readouts)
            keys = tuple(
                Parity(frozenset({("readout", None, None, None, index)}))
                for index in range(len(readouts))
            )
            self.readout_keys[name] = keys
            self.systems[name] = BinarySystem(
                (*checks, *(key ^ equation for key, equation in zip(keys, readouts)))
            )

    def __call__(self, seed: int | None = None) -> SyndromeSession:
        return SyndromeSession(self)

    def prepare_batch(self) -> BatchDecoderSession:
        return _SyndromeBatchSession(self)

    def readout_table(
        self, gadget: Gadget, record_count: int
    ) -> tuple[Readouts, ...] | None:
        name = gadget.implements.mnemonic
        key = (name, record_count)
        if key not in self._readout_tables:
            self._readout_tables[key] = self._prepare_readout_table(
                gadget, record_count
            )
        return self._readout_tables[key]

    def _terminal_invocation(
        self, gadget: Gadget, record_count: int
    ) -> Invocation | None:
        if gadget.outputs or not 0 <= record_count <= 10:
            return None
        name = gadget.implements.mnemonic
        codes, checks, equations = self.gadgets[name]
        system = self.systems[name].copy()
        try:
            for index in range(record_count):
                system.add(
                    Parity(frozenset({("circuit_readout", None, None, None, index)}))
                )
        except InconsistentParity:
            return None
        referenced = set().union(
            *(parity.variables for parity in (*checks, *equations))
        )
        for entry in range(len(gadget.inputs)):
            observed = any(
                system.value(_sign("in", entry, "stabilizers", index)) is not None
                for index in range(len(codes[("in", entry)].stabilizers))
            )
            if not observed and any(
                isinstance(variable, tuple)
                and variable[:3] == ("encoding", "in", entry)
                for variable in referenced
            ):
                return None
        return Invocation(
            0,
            gadget,
            InstructionCall(name, operands=list(range(len(gadget.inputs)))),
            tuple(
                BlockReference(entry, 0, operand.block)
                for entry, operand in enumerate(gadget.implements.inputs)
            ),
            (),
        )

    def _prepare_readout_table(
        self, gadget: Gadget, record_count: int
    ) -> tuple[Readouts, ...] | None:
        invocation = self._terminal_invocation(gadget, record_count)
        if invocation is None:
            return None
        table = []
        session = self()
        try:
            for pattern in range(1 << record_count):
                records = tuple(
                    bool(pattern & (1 << index)) for index in range(record_count)
                )
                corrections = session.decode(invocation, records)
                try:
                    next(corrections)
                    return None
                except StopIteration as completed:
                    decoded: Decoded = completed.value
                    if any(value is None for value in decoded.readouts):
                        return None
                    table.append(decoded.readouts)
                except (InconsistentParity, ExecutionUnresolved):
                    return None
                finally:
                    corrections.close()
        finally:
            session.close()
        return tuple(table)


class SyndromeSession:
    def __init__(self, model: SyndromeModel) -> None:
        self.model = model
        self.closed = False
        self.boundaries: dict[BlockReference, dict[tuple[str, int], bool]] = {}

    def decode(
        self, invocation: Invocation, readouts: Readouts
    ) -> Corrections[Decoded]:
        if self.closed:
            raise RuntimeError("Decoder session is closed")
        gadget = invocation.gadget
        name = gadget.implements.mnemonic
        codes, _, _ = self.model.gadgets[name]
        values: dict[tuple[str, str | None, int | None, str | None, int], bool] = {
            ("circuit_readout", None, None, None, index): bool(value)
            for index, value in enumerate(readouts)
            if value is not None
        }
        readout_keys = self.model.readout_keys[name]
        system = self.model.systems[name].copy()
        for key, value in values.items():
            system.add(Parity(frozenset({key}), value))
        for entry, block in enumerate(invocation.inputs):
            decoder = codes[("in", entry)]
            observed = any(
                system.value(_sign("in", entry, "stabilizers", index)) is not None
                for index in range(len(decoder.stabilizers))
            )
            if not observed:
                for (property_name, index), value in self.boundaries.get(
                    block, {}
                ).items():
                    sign = _sign("in", entry, property_name, index)
                    if system.value(sign) is None:
                        system.add(sign ^ Parity(constant=value))
                        values[("encoding", "in", entry, property_name, index)] = value
        pending = {block: {} for block in invocation.outputs}
        framed = self.model.framed[name]
        for (boundary, entry), decoder in codes.items():
            syndrome = tuple(
                system.value(_sign(boundary, entry, "stabilizers", index))
                for index in range(len(decoder.stabilizers))
            )
            if syndrome and all(value is None for value in syndrome):
                continue
            correction = decoder.correct(syndrome)
            for basis in ("x", "z"):
                for index, operator in enumerate(decoder.operators[basis]):
                    known = system.value(_sign(boundary, entry, basis, index))
                    if (
                        known is None
                        and boundary == "out"
                        and not invocation.inputs
                        and (entry, basis, index) in framed
                    ):
                        # A preparation's framed logical carries its sign in the
                        # frame, so the stabilizer correction must preserve it.
                        known = False
                    if known is not None and known != (
                        not correction.commutes_with(operator)
                    ):
                        correction *= decoder.operators["z" if basis == "x" else "x"][
                            index
                        ]
            for property_name, operators in decoder.operators.items():
                for index, operator in enumerate(operators):
                    known = system.value(_sign(boundary, entry, property_name, index))
                    if known is None and property_name == "stabilizers":
                        continue
                    value = (
                        known
                        if known is not None
                        else not correction.commutes_with(operator)
                    )
                    values[("encoding", boundary, entry, property_name, index)] = value
                    if boundary == "out":
                        pending[invocation.outputs[entry]][(property_name, index)] = (
                            value ^ (not correction.commutes_with(operator))
                        )
            if boundary == "out":
                for target in correction.support:
                    yield Correction(
                        (invocation.outputs[entry],),
                        Operation(correction[target].lower(), (target,)),
                    )

        system = self.model.systems[name].copy()
        for key, value in values.items():
            system.add(Parity(frozenset({key}), value))
        decoded = tuple(system.value(key) for key in readout_keys)
        for frame in self.model.frames[gadget.implements.mnemonic]:
            value = system.value(frame.parity)
            if value is None:
                raise ExecutionUnresolved(
                    "Frame correction requires unavailable circuit readouts"
                )
            if value:
                operators = codes[("out", frame.output)].operators
                operator = operators["x" if frame.basis == "z" else "z"][frame.logical]
                for target in operator.support:
                    yield Correction(
                        (invocation.outputs[frame.output],),
                        Operation(operator[target].lower(), (target,)),
                    )
        outcomes = gadget.implements.observe_count
        self.boundaries.update(pending)
        return Decoded(tuple(decoded[:outcomes]), tuple(decoded[outcomes:]))

    def discarded(self, blocks: Sequence[BlockReference]) -> None:
        for block in blocks:
            self.boundaries.pop(block, None)

    def close(self) -> None:
        self.closed = True
        self.boundaries.clear()


class _SyndromeBatchSession(SyndromeSession):
    def prepare_readouts(
        self, invocation: Invocation, record_count: int, /
    ) -> ReadoutBatch | None:
        table = self.model.readout_table(invocation.gadget, record_count)
        return None if table is None else ReadoutTable(table)
