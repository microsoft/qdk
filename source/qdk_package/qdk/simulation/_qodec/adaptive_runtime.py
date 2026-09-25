from __future__ import annotations
from collections.abc import Sequence
from typing import cast

from qodec.instructions import InstructionCall

from qdk import Result
from ..._adaptive_pass import AdaptiveProgram
from ._interpreter import OutputRecordValue, _Interpreter
from .protocols import Request, Requests, Resources
from .quantum_operations import Operation

DEFAULT_MAX_STEPS = 10_000_000


class AdaptiveRuntime:
    def __init__(self, *, initialize: bool = True) -> None:
        self.initialize = initialize
        self.results: dict[int, Result] = {}
        self._pending: list[tuple[Request, tuple[int, ...]]] = []

    def required_resources(self, bytecode: AdaptiveProgram) -> Resources:
        return Resources(qubits=bytecode.num_qubits)

    def run(self, bytecode: AdaptiveProgram) -> Requests[list[OutputRecordValue]]:
        self.results.clear()
        self._pending.clear()
        interpreter = _Interpreter(bytecode, self)
        if self.initialize:
            for target in range(bytecode.num_qubits):
                yield Operation("prepare", (target,))
        try:
            for _ in range(DEFAULT_MAX_STEPS):
                if interpreter.step():
                    return interpreter.records
                for request, result_ids in self._pending:
                    readouts = yield request
                    if not result_ids:
                        continue
                    if readouts is None:
                        raise TypeError(
                            "Measurement execution must return a readout tuple"
                        )
                    for result_id, value in zip(result_ids, readouts):
                        if value is None:
                            raise ValueError("Measurement reply is unresolved")
                        self.results[result_id] = cast(
                            Result, Result.One if value else Result.Zero
                        )
                self._pending.clear()
            raise RuntimeError(
                f"Program did not terminate within {DEFAULT_MAX_STEPS} bytecode steps"
            )
        finally:
            self._pending.clear()

    def _apply(
        self, operation: str, targets: tuple[int, ...], angle: float | None = None
    ) -> None:
        self._pending.append((Operation(operation, targets, angle), ()))

    def instruction(self, call: InstructionCall, results: Sequence[int]) -> None:
        self._pending.append((call, tuple(results)))

    def x(self, target: int) -> None:
        self._apply("x", (target,))

    def y(self, target: int) -> None:
        self._apply("y", (target,))

    def z(self, target: int) -> None:
        self._apply("z", (target,))

    def h(self, target: int) -> None:
        self._apply("h", (target,))

    def s(self, target: int) -> None:
        self._apply("s", (target,))

    def s_adj(self, target: int) -> None:
        self._apply("s_adj", (target,))

    def t(self, target: int) -> None:
        self._apply("t", (target,))

    def t_adj(self, target: int) -> None:
        self._apply("t_adj", (target,))

    def sx(self, target: int) -> None:
        self._apply("sx", (target,))

    def sx_adj(self, target: int) -> None:
        self._apply("sx_adj", (target,))

    def rx(self, angle: float, target: int) -> None:
        self._apply("rx", (target,), angle)

    def ry(self, angle: float, target: int) -> None:
        self._apply("ry", (target,), angle)

    def rz(self, angle: float, target: int) -> None:
        self._apply("rz", (target,), angle)

    def cx(self, control: int, target: int) -> None:
        self._apply("cx", (control, target))

    def cy(self, control: int, target: int) -> None:
        self._apply("cy", (control, target))

    def cz(self, control: int, target: int) -> None:
        self._apply("cz", (control, target))

    def swap(self, q1: int, q2: int) -> None:
        self._apply("swap", (q1, q2))

    def rxx(self, angle: float, q1: int, q2: int) -> None:
        self._apply("rxx", (q1, q2), angle)

    def ryy(self, angle: float, q1: int, q2: int) -> None:
        self._apply("ryy", (q1, q2), angle)

    def rzz(self, angle: float, q1: int, q2: int) -> None:
        self._apply("rzz", (q1, q2), angle)

    def mov(self, target: int) -> None:
        self._apply("mov", (target,))

    def mz(self, target: int, result_id: int) -> None:
        self._pending.append((Operation("measure", (target,)), (result_id,)))

    def mresetz(self, target: int, result_id: int) -> None:
        self.mz(target, result_id)
        self.resetz(target)

    def resetz(self, target: int) -> None:
        self._apply("prepare", (target,))

    def result(self, result_id: int) -> Result:
        return self.results.get(result_id, cast(Result, Result.Zero))

    def peek_loss(self, target: int, result_id: int) -> None:
        raise NotImplementedError("QIR loss queries are not supported")

    def apply_readout_noise(
        self, p_zero_as_one: float, p_one_as_zero: float, result_id: int
    ) -> None:
        raise NotImplementedError("QIR readout-noise instructions are not supported")

    def correlated_noise_intrinsic(
        self, intrinsic_id: int, targets: Sequence[int]
    ) -> None:
        raise NotImplementedError("QIR noise intrinsics are not supported")
