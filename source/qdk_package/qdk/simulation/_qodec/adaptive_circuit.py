from contextlib import closing
from dataclasses import dataclass

from pyqir import Context, Module
from qdk import Result
from ..._adaptive_pass import AdaptiveProgram
from qodec.gadgets import Circuit

from .adaptive_runtime import AdaptiveRuntime
from .bytecode import compile
from .logical_qubits import LogicalQubits
from .protocols import Invocation, PreparedCircuit, Readouts, Requests, Resources
from .quantum_operations import LogicalSlot, Operation, local_indices


def prepare_qir(circuit: Circuit) -> PreparedCircuit:
    program = compile(Module.from_ir(Context(), circuit.source))
    blocks = circuit.instruction_set.blocks
    if len(blocks) != 1 or blocks[0].encodes <= 0:
        raise NotImplementedError(
            "Adaptive gadget source requires one nonempty target block type"
        )
    block = blocks[0]
    count = (program.num_qubits + block.encodes - 1) // block.encodes
    labels = tuple(str(index) for index in range(count))
    return PreparedCircuit(
        labels,
        lambda: AdaptiveCircuitRuntime(program, block.name, block.encodes),
        dict.fromkeys(labels, block.name),
    )


def prepare_openqasm(circuit: Circuit) -> PreparedCircuit:
    import qdk
    import qdk.openqasm as qasm
    from qdk.openqasm.parser import IntegerLiteral, QubitDeclaration
    from qdk.simulation._simulation import preprocess_simulation_input

    parsed = qasm.parse(circuit.source)
    if parsed.has_errors:
        raise ValueError(f"Invalid OpenQASM gadget: {parsed.diagnostics}")
    labels = []
    for statement in parsed.program.statements:
        if isinstance(statement, QubitDeclaration):
            if statement.size is None:
                labels.append(statement.qubit.name)
            elif isinstance(statement.size, IntegerLiteral):
                labels.extend(
                    f"{statement.qubit.name}[{index}]"
                    for index in range(statement.size.value)
                )
            else:
                raise NotImplementedError(
                    "OpenQASM gadget registers currently require literal sizes"
                )
    qir = qasm.compile(circuit.source, target_profile=qdk.TargetProfile.Adaptive)
    module, _, _, _ = preprocess_simulation_input(qir)
    program = compile(module)
    blocks = circuit.instruction_set.blocks
    if len(blocks) != 1 or blocks[0].encodes != 1 or program.num_qubits != len(labels):
        raise NotImplementedError(
            "OpenQASM gadget qubits require an explicit one-qubit register layout"
        )
    block_type = blocks[0].name
    names = tuple(labels)
    return PreparedCircuit(
        names,
        lambda: AdaptiveCircuitRuntime(program, block_type, 1, names),
        dict.fromkeys(names, block_type),
    )


@dataclass(frozen=True)
class AdaptiveCircuitRuntime:
    program: AdaptiveProgram
    block_type: str
    capacity: int
    labels: tuple[str, ...] = ()

    def required_resources(self, invocation: Invocation) -> Resources:
        count = (self.program.num_qubits + self.capacity - 1) // self.capacity
        return (
            Resources(qubits=count)
            if self.capacity == 1
            else Resources(blocks={self.block_type: count})
        )

    def run(self, invocation: Invocation) -> Requests[Readouts]:
        if invocation.gadget.parameter_bindings:
            raise NotImplementedError(
                "Adaptive QIR gadget parameters must be bound in the source before compilation"
            )
        runtime = AdaptiveRuntime(initialize=False)
        with closing(LogicalQubits()) as logical, closing(
            runtime.run(self.program)
        ) as requests:
            reply = None
            while True:
                try:
                    request = requests.send(reply)
                except StopIteration as completed:
                    records = []
                    for value in completed.value:
                        if isinstance(value, bool):
                            records.append(value)
                        elif isinstance(value, Result):
                            records.append(
                                None if value == Result.Loss else value == Result.One
                            )
                        else:
                            raise TypeError(
                                "Adaptive gadget output records must be bits"
                            )
                    return tuple(records)
                if not isinstance(request, Operation):
                    raise TypeError("Adaptive VM requests must be quantum operations")
                targets = local_indices(request)
                if self.labels:
                    request = Operation(
                        request.name,
                        tuple(
                            LogicalSlot(self.labels[target], 0, self.block_type)
                            for target in targets
                        ),
                        request.angle,
                    )
                elif self.capacity != 1:
                    request = Operation(
                        request.name,
                        tuple(
                            LogicalSlot(
                                target // self.capacity,
                                target % self.capacity,
                                self.block_type,
                            )
                            for target in targets
                        ),
                        request.angle,
                    )
                with closing(logical.handle(request)) as restored:
                    reply = None
                    while True:
                        try:
                            pending = restored.send(reply)
                        except StopIteration as completed:
                            reply = completed.value
                            break
                        reply = yield pending
