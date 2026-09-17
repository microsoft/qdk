from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from collections import Counter

from qodec import Instruction
from qodec.gadgets import Circuit, Reference
from qodec.instructions import InstructionCall, Parameter

from .call_binding import bind_operands, validate_arguments
from .protocols import Invocation, PreparedCircuit, Readouts, Requests, Resources
from .selection import Selection, prepare_selection


def prepare_circuit(circuit: Circuit) -> PreparedCircuit:
    source_format = circuit.effective_format.lstrip(".")
    if source_format == "qir":
        from .adaptive_circuit import prepare_qir

        return prepare_qir(circuit)
    if source_format in ("openqasm", "qasm"):
        from .adaptive_circuit import prepare_openqasm

        return prepare_openqasm(circuit)
    return prepare_call_list(circuit)


@dataclass(frozen=True)
class PreparedCall:
    call: InstructionCall
    declaration: Instruction
    record_count: int
    selection: Selection


def prepare_call_list(circuit: Circuit) -> PreparedCircuit:
    calls = tuple(circuit.calls())
    labels = tuple(
        dict.fromkeys(str(operand) for call in calls for operand in call.operands)
    )
    block_types = {}
    prepared = []
    offset = 0
    for call in calls:
        binding = bind_operands(circuit.instruction_set, call.mnemonic, call.operands)
        declaration = binding.instruction
        parameters = {
            parameter.name: parameter.kind for parameter in declaration.parameters
        }
        for name, argument in call.arguments.items():
            if name not in parameters:
                raise ValueError(f"Unknown parameter {name!r} for {call.mnemonic!r}")
            if isinstance(argument, str) and argument.startswith("circuit.readouts["):
                reference = Reference(argument)
                if (
                    parameters[name] != Parameter.Kind.BIT
                    or len(reference.expand()) != 1
                    or reference.index >= offset
                ):
                    raise ValueError(
                        "Readout arguments require a bit parameter and an earlier circuit record"
                    )
        count = declaration.observe_count + len(declaration.flags)
        prepared.append(
            PreparedCall(
                call,
                declaration,
                count,
                prepare_selection(declaration.flags, call.select),
            )
        )
        offset += count
        block_types.update(
            (str(operand.label), operand.block_type)
            for operand in (*binding.inputs, *binding.outputs)
        )
    blocks = circuit.instruction_set.blocks
    resources = (
        Resources(qubits=len(labels))
        if len(blocks) == 1 and blocks[0].encodes == 1
        else Resources(blocks=Counter(block_types.values()))
    )
    return PreparedCircuit(
        labels, lambda: CallListRuntime(tuple(prepared), resources), block_types
    )


@dataclass(frozen=True)
class CallListRuntime:
    calls: tuple[PreparedCall, ...]
    resources: Resources

    def required_resources(self, invocation: Invocation) -> Resources:
        return self.resources

    def run(self, invocation: Invocation) -> Requests[Readouts]:
        arguments = invocation.call.arguments
        validate_arguments(invocation.gadget.implements, arguments)
        parameters = {}
        for name, destination in invocation.gadget.parameter_bindings.items():
            parameters[destination.removeprefix("circuit.source.")] = arguments[name]
        readouts: list[bool | None] = []
        for prepared in self.calls:
            call = prepared.call
            bound = {
                name: _argument(value, parameters, readouts)
                for name, value in call.arguments.items()
            }
            validate_arguments(prepared.declaration, bound)
            reply = yield InstructionCall(
                call.mnemonic,
                operands=list(call.operands),
                arguments=bound,
                select=call.select,
            )
            if reply is None:
                raise TypeError("Instruction execution must return a readout tuple")
            if len(reply) != prepared.record_count:
                raise ValueError(
                    f"Instruction {call.mnemonic!r} returned {len(reply)} records, expected {prepared.record_count}"
                )
            prepared.selection.require(reply[prepared.declaration.observe_count :])
            readouts.extend(reply)
        return tuple(readouts)


def _argument(
    value: InstructionCall.Argument,
    parameters: Mapping[str, InstructionCall.Argument],
    readouts: Sequence[bool | None],
) -> InstructionCall.Argument:
    if not isinstance(value, str):
        return value
    if value in parameters:
        return parameters[value]
    if value.startswith("circuit.readouts["):
        readout = readouts[Reference(value).index]
        if readout is None:
            raise TypeError("An unresolved readout cannot be an instruction argument")
        return readout
    return value
