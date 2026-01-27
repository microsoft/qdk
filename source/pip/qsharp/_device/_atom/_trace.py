# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import QirModuleVisitor, qubit_id, required_num_qubits
from .._device import Device


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

    def _next_step(self):
        self.trace["steps"].append({"id": len(self.trace["steps"]), "ops": []})

    def _on_function(self, function):
        num_qubits = required_num_qubits(function)
        if num_qubits:
            self.trace["qubits"] = self.trace["qubits"][:num_qubits]
        super()._on_function(function)

    def _on_call_instr(self, call):
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

    def _on_qis_move(self, call, qubit, row, col):
        if not self.in_parallel:
            self._next_step()
        q = qubit_id(qubit)
        self.q_cols[q] = col.value
        self.trace["steps"][-1]["ops"].append(f"move({row.value}, {col.value}) {q}")

    def _on_qis_sx(self, call, qubit):
        if not self.in_parallel:
            self._next_step()
        q = qubit_id(qubit)
        self.trace["steps"][-1]["ops"].append(f"sx {q}")

    def _on_qis_rz(self, call, angle, qubit):
        if not self.in_parallel:
            self._next_step()
        q = qubit_id(qubit)
        self.trace["steps"][-1]["ops"].append(f"rz({angle.value}) {q}")

    def _on_qis_cz(self, call, qubit1, qubit2):
        if not self.in_parallel:
            self._next_step()
        q1 = qubit_id(qubit1)
        q2 = qubit_id(qubit2)
        if self.q_cols.get(q1, -1) > self.q_cols.get(q2, -1):
            q1, q2 = q2, q1
        self.trace["steps"][-1]["ops"].append(f"cz {q1}, {q2}")

    def _on_qis_mresetz(self, call, target, result):
        if not self.in_parallel:
            self._next_step()
        q = qubit_id(target)
        self.trace["steps"][-1]["ops"].append(f"mz {q}")
