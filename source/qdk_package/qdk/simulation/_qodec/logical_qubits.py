from qodec.instructions import InstructionCall

from .call_binding import BoundCall
from .instruction_set import InstructionSet
from .protocols import ExecutionUnresolved, Readouts, Request, Requests, Resources
from .quantum_operations import LogicalSlot, Operation, RestoreMeasured


class LogicalQubits:
    """Keeps program qubits usable after destructive logical measurements.

    ``instructions`` is the top instruction set that program instruction calls
    name. A call that consumes a block and measures it in Z records the
    outcome like a logical measurement does, so a later use restores it.

    With ``instructions``, a program qubit's initial preparation waits for the
    qubit's first use, and an instruction call that creates the qubit's block
    replaces it.
    """

    def __init__(self, instructions: InstructionSet | None = None) -> None:
        self.instructions = instructions
        self.measured: dict[int | str | LogicalSlot, bool] = {}
        self.touched: set[int | str | LogicalSlot] = set()
        self.unprepared: dict[int | str | LogicalSlot, Operation] = {}

    def required_resources(self, upper: Resources) -> Resources:
        return upper

    def start(self, resources: Resources) -> None:
        self.measured.clear()
        self.touched.clear()
        self.unprepared.clear()

    def handle(self, request: Request) -> Requests[Readouts]:
        bound: tuple[InstructionSet, BoundCall] | None = None
        if isinstance(request, Operation):
            if (
                self.instructions is not None
                and request.name == "prepare"
                and self.touched.isdisjoint(request.targets)
            ):
                for target in request.targets:
                    self.touched.add(target)
                    self.unprepared[target] = Operation("prepare", (target,))
                return ()
            for target in request.targets:
                self.touched.add(target)
                if request.name in ("prepare", "discard"):
                    self.unprepared.pop(target, None)
                    self.measured.pop(target, None)
                else:
                    yield from self._prepare(target)
                    yield from self._restore(target)
        elif isinstance(request, InstructionCall):
            bound = self._bind(request)
            inputs = {operand.label for operand in bound[1].inputs}
            for label in request.operands:
                self.touched.add(label)
                if label in inputs:
                    yield from self._prepare(label)
                    yield from self._restore(label)
                else:
                    self.unprepared.pop(label, None)
                    self.measured.pop(label, None)
        readouts = yield request
        if readouts is None:
            raise TypeError("Quantum execution must return a readout tuple")
        if isinstance(request, Operation) and request.name == "measure":
            (value,) = readouts
            if value is None:
                raise ExecutionUnresolved("Logical measurement could not be decoded")
            self.measured[request.targets[0]] = value
        elif bound is not None:
            self._record_consumed(*bound, readouts)
        return readouts

    def _bind(self, call: InstructionCall) -> tuple[InstructionSet, BoundCall]:
        if self.instructions is None:
            raise TypeError("Instruction calls require the program's instruction set")
        binding = self.instructions.bindings.get(call.mnemonic)
        if binding is None:
            raise ValueError(f"Unknown instruction {call.mnemonic!r}")
        return self.instructions, binding.bind(call.operands)

    def _record_consumed(
        self, instructions: InstructionSet, bound: BoundCall, readouts: Readouts
    ) -> None:
        outcomes = readouts[: bound.instruction.observe_count]
        if any(value is None for value in outcomes):
            raise ExecutionUnresolved("Logical measurement could not be decoded")
        outputs = {operand.label for operand in bound.outputs}
        for operand in bound.inputs:
            if operand.label in outputs:
                continue
            index = instructions.outcome_index(
                bound.instruction.mnemonic, operand.offset
            )
            if index is not None:
                self.measured[operand.label] = bool(outcomes[index])

    def _prepare(self, target: int | str | LogicalSlot) -> Requests[None]:
        preparation = self.unprepared.pop(target, None)
        if preparation is not None:
            yield preparation

    def _restore(self, target: int | str | LogicalSlot) -> Requests[None]:
        if target in self.measured:
            value = self.measured.pop(target)
            yield RestoreMeasured(target, value)

    def close(self) -> None:
        self.measured.clear()
        self.touched.clear()
        self.unprepared.clear()
