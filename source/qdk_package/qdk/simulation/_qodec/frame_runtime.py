from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass

from qodec import Gadget, Layer

from .action_runtime import prepare_actions
from .frame_transport import circuit_transport
from .protocols import (
    BlockReference,
    Corrections,
    Decoded,
    DecoderFactory,
    ExecutionUnresolved,
    Invocation,
    Readouts,
)
from .quantum_instruments import (
    Allocate,
    CliffordGate,
    Observation,
    PauliGate,
    PauliRotation,
    Stabilization,
    TraceOut,
)
from .readout_equations import (
    BinarySystem,
    FrameDelta,
    Parity,
    expression,
    prepare_frames,
    validate_equations,
)


def _sign(boundary: str, entry: int, basis: str, index: int) -> Parity:
    return Parity(frozenset({("encoding", boundary, entry, basis, index)}))


@dataclass(frozen=True)
class FramePlan:
    gadget: Gadget
    checks: tuple[Parity, ...]
    readouts: tuple[Parity, ...]
    frames: tuple[FrameDelta, ...]
    transport: Mapping[tuple[int, str, int], Parity] | None


def prepare_frame_decoder(layer: Layer) -> DecoderFactory:
    plans = {}
    for name, gadget in layer.gadgets.items():
        validate_equations(gadget)
        plans[name] = FramePlan(
            gadget,
            tuple(expression(check) for check in gadget.checks),
            tuple(expression(readout.equation) for readout in gadget.readouts),
            prepare_frames(gadget),
            circuit_transport(gadget),
        )
    return lambda seed: FrameSession(plans)


class FrameSession:
    def __init__(self, plans: dict[str, FramePlan]) -> None:
        self.plans = plans
        self.boundaries: dict[BlockReference, dict[tuple[str, int], bool]] = {}
        self.closed = False

    def decode(
        self, invocation: Invocation, readouts: Readouts
    ) -> Corrections[Decoded]:
        if self.closed:
            raise RuntimeError("Frame session is closed")
        plan = self.plans[invocation.call.mnemonic]
        validate_equations(plan.gadget, len(readouts))
        system = BinarySystem(plan.checks)
        for index, value in enumerate(readouts):
            if value is not None:
                system.add(
                    Parity(
                        frozenset({("circuit_readout", None, None, None, index)}), value
                    )
                )
        for entry, block in enumerate(invocation.inputs):
            for (basis, index), value in self.boundaries.get(block, {}).items():
                system.add(_sign("in", entry, basis, index) ^ Parity(constant=value))
        keys = tuple(
            Parity(frozenset({("readout", None, None, None, index)}))
            for index in range(len(plan.readouts))
        )
        for key, equation in zip(keys, plan.readouts):
            system.add(key ^ equation)
        transport = (
            plan.transport
            if plan.transport is not None
            else _logical_transport(invocation, system)
        )
        for key, equation in transport.items():
            frame = next(
                (
                    frame.parity
                    for frame in plan.frames
                    if (frame.output, frame.basis, frame.logical) == key
                ),
                Parity(),
            )
            system.add(_sign("out", *key) ^ equation ^ frame)
        pending = {}
        for entry, block in enumerate(invocation.outputs):
            code = invocation.gadget.outputs[entry].code
            values = {}
            for basis in ("stabilizers", "x", "z"):
                for index in range(len(getattr(code, basis))):
                    value = system.value(_sign("out", entry, basis, index))
                    if value is not None:
                        values[(basis, index)] = value
            pending[block] = values
        decoded = tuple(system.value(key) for key in keys)
        yield from ()
        self.boundaries.update(pending)
        count = invocation.gadget.implements.observe_count
        return Decoded(decoded[:count], decoded[count:])

    def discarded(self, blocks: Sequence[BlockReference]) -> None:
        for block in blocks:
            self.boundaries.pop(block, None)

    def close(self) -> None:
        self.closed = True
        self.boundaries.clear()


def _logical_transport(
    invocation: Invocation, system: BinarySystem
) -> dict[tuple[int, str, int], Parity]:
    inputs, outputs = invocation.gadget.inputs, invocation.gadget.outputs
    input_width = sum(len(encoding.code.x) for encoding in inputs)
    output_width = sum(len(encoding.code.x) for encoding in outputs)
    program = prepare_actions(
        invocation.gadget.implements,
        input_width,
        output_width,
        invocation.call.arguments,
    )
    signs = [
        (_sign("in", entry, "z", index), _sign("in", entry, "x", index))
        for entry, encoding in enumerate(inputs)
        for index in range(len(encoding.code.x))
    ]
    signs.extend((Parity(), Parity()) for _ in range(program.num_qubits - input_width))
    for step in program.steps:
        instrument = step.instrument
        if step.guard is not None:
            if any(system.value(part) is not False for pair in signs for part in pair):
                raise ExecutionUnresolved(
                    "Conditional action requires an explicitly realized incoming frame"
                )
            continue
        if isinstance(instrument, CliffordGate):
            width = instrument.operator.qubit_count
            transformed = [(Parity(), Parity()) for _ in range(width)]
            for index in range(width):
                for value, image in (
                    (signs[index][0], instrument.operator.image_x(index)),
                    (signs[index][1], instrument.operator.image_z(index)),
                ):
                    for target in image.support:
                        x_sign, z_sign = transformed[target]
                        if image[target] in ("X", "Y"):
                            x_sign ^= value
                        if image[target] in ("Z", "Y"):
                            z_sign ^= value
                        transformed[target] = (x_sign, z_sign)
            signs[:width] = transformed
        elif isinstance(instrument, Stabilization):
            for operator in instrument.operators:
                if any(
                    system.value(part) is not False
                    for index in operator.support
                    for part in signs[index]
                ):
                    raise ExecutionUnresolved(
                        "Stabilization requires an explicitly realized incoming frame"
                    )
        elif isinstance(instrument, PauliRotation):
            parity = Parity()
            for index in instrument.operator.support:
                if instrument.operator[index] in ("Z", "Y"):
                    parity ^= signs[index][0]
                if instrument.operator[index] in ("X", "Y"):
                    parity ^= signs[index][1]
            if system.value(parity) is not False:
                raise ExecutionUnresolved(
                    "Non-Clifford action requires an explicitly realized incoming frame"
                )
        elif isinstance(instrument, (Allocate, TraceOut, Observation, PauliGate)):
            continue
    transported = {}
    offset = 0
    for entry, encoding in enumerate(outputs):
        for index in range(len(encoding.code.x)):
            transported[(entry, "z", index)], transported[(entry, "x", index)] = signs[
                offset + index
            ]
        offset += len(encoding.code.x)
    return transported
