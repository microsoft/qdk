from .protocols import ExecutionUnresolved, Readouts, Request, Requests, Resources
from .quantum_operations import LogicalSlot, Operation, RestoreMeasured


class LogicalQubits:
    def __init__(self) -> None:
        self.measured: dict[int | LogicalSlot, bool] = {}

    def required_resources(self, upper: Resources) -> Resources:
        return upper

    def start(self, resources: Resources) -> None:
        self.measured.clear()

    def handle(self, request: Request) -> Requests[Readouts]:
        if isinstance(request, Operation):
            for target in request.targets:
                if request.name in ("prepare", "discard"):
                    self.measured.pop(target, None)
                else:
                    yield from self._restore(target)
        readouts = yield request
        if readouts is None:
            raise TypeError("Quantum execution must return a readout tuple")
        if isinstance(request, Operation) and request.name == "measure":
            (value,) = readouts
            if value is None:
                raise ExecutionUnresolved("Logical measurement could not be decoded")
            self.measured[request.targets[0]] = value
        return readouts

    def _restore(self, target: int | LogicalSlot) -> Requests[None]:
        if target in self.measured:
            value = self.measured.pop(target)
            yield RestoreMeasured(target, value)

    def close(self) -> None:
        self.measured.clear()
