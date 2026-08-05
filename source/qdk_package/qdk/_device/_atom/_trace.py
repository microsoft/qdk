# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import (
    Call,
    FloatConstant,
    Function,
    IntConstant,
    QirModuleVisitor,
    Value,
    ptr_id,
    required_num_qubits,
)
from .._device import Device
from typing import cast


class Trace(QirModuleVisitor):

    def __init__(
        self,
        device: Device,
    ):
        self.in_parallel = False
        self.trace = {
            "qubits": device.home_locs,
            "steps": [],
        }
        self.q_cols = {}
        super().__init__()

    def _next_step(self) -> None:
        self.trace["steps"].append({"id": len(self.trace["steps"]), "ops": []})

    def _on_function(self, function: Function) -> None:
        num_qubits = required_num_qubits(function)
        if num_qubits:
            self.trace["qubits"] = self.trace["qubits"][:num_qubits]
        super()._on_function(function)

    def _on_call_instr(self, call: Call) -> None:
        if call.callee.name == "__quantum__rt__begin_parallel":
            self._next_step()
            self.in_parallel = True
        elif call.callee.name == "__quantum__rt__end_parallel":
            self.in_parallel = False
        elif call.callee.name == "__quantum__qis__move__body":
            self._on_qis_move(call, call.args[0], call.args[1], call.args[2])
        elif call.callee.name == "__quantum__qis__sx__body":
            self._on_qis_sx(call, call.args[0])
        else:
            super()._on_call_instr(call)

    def _on_qis_move(self, call: Call, qubit: Value, row: Value, col: Value) -> None:
        if not self.in_parallel:
            self._next_step()
        q = ptr_id(qubit)
        row_const = cast(IntConstant, row)
        col_const = cast(IntConstant, col)
        self.q_cols[q] = col_const.value
        self.trace["steps"][-1]["ops"].append(
            f"move({row_const.value}, {col_const.value}) {q}"
        )

    def _on_qis_sx(self, call: Call, qubit: Value) -> None:
        if not self.in_parallel:
            self._next_step()
        q = ptr_id(qubit)
        self.trace["steps"][-1]["ops"].append(f"sx {q}")

    def _on_qis_rz(self, call: Call, angle: Value, target: Value) -> None:
        if not self.in_parallel:
            self._next_step()
        q = ptr_id(target)
        angle_const = cast(FloatConstant, angle)
        self.trace["steps"][-1]["ops"].append(f"rz({angle_const.value}) {q}")

    def _on_qis_cz(self, call: Call, ctrl: Value, target: Value) -> None:
        if not self.in_parallel:
            self._next_step()
        q1 = ptr_id(ctrl)
        q2 = ptr_id(target)
        if self.q_cols.get(q1, -1) > self.q_cols.get(q2, -1):
            q1, q2 = q2, q1
        self.trace["steps"][-1]["ops"].append(f"cz {q1}, {q2}")

    def _on_qis_mresetz(self, call: Call, target: Value, result: Value) -> None:
        if not self.in_parallel:
            self._next_step()
        q = ptr_id(target)
        self.trace["steps"][-1]["ops"].append(f"mz {q}")
