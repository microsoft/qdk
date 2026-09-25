from __future__ import annotations

from collections.abc import Sequence

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
    validate_equations,
)


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
    layer: Layer, *, error_probability: float = 0.001
) -> DecoderFactory:
    """Prepare a deq relay-BP decoder for a Qodec layer.

    Requires ``pip install deq deq-runtime``. The model assigns independent
    X, Y, and Z faults to each code qubit with the given probability, which
    must be between zero and one half. This is a per-boundary syndrome model,
    not a circuit-level or temporal noise model. Simulator noise is not
    inferred. Unknown syndrome entries are omitted rather than treated as zero.

    QDK handles readout equations, frames, and physical corrections. The deq
    runtime uses a private worker so synchronous simulation also works inside
    a running asyncio event loop, including notebooks.
    """
    try:
        from .deq_decoding import DeqModel
    except ModuleNotFoundError as error:
        if error.name and error.name.split(".")[0] in ("deq", "deq_runtime"):
            raise ImportError(
                "The deq decoder requires deq and deq-runtime. "
                "Install them with: pip install deq deq-runtime"
            ) from error
        raise
    if not 0 < error_probability < 0.5:
        raise ValueError("error_probability must be between zero and one half")
    return DeqModel(layer, error_probability)


class SyndromeModel:
    def __init__(self, layer: Layer) -> None:
        self.gadgets = {}
        self.frames = {}
        self.framed: dict[str, frozenset[tuple[int, str, int]]] = {}
        self.systems: dict[str, BinarySystem] = {}
        self.readout_keys: dict[str, tuple[Parity, ...]] = {}
        self._readout_tables: dict[tuple[str, int], tuple[Readouts, ...] | None] = {}
        for name, gadget in layer.gadgets.items():
            validate_equations(gadget)
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

    def new_session(self, seed: int | None = None) -> SyndromeSession:
        return SyndromeSession(self)

    def __call__(self, seed: int | None = None) -> SyndromeSession:
        return self.new_session(seed)

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

    def terminal_invocation(
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
        invocation = self.terminal_invocation(gadget, record_count)
        if invocation is None:
            return None
        table = []
        session = self.new_session()
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

    def correct(self, decoder: CodeDecoder, syndrome: Readouts) -> DensePauli:
        return decoder.correct(syndrome)

    def decode(
        self, invocation: Invocation, readouts: Readouts
    ) -> Corrections[Decoded]:
        if self.closed:
            raise RuntimeError("Decoder session is closed")
        gadget = invocation.gadget
        validate_equations(gadget, len(readouts))
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
            correction = self.correct(decoder, syndrome)
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
