# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import QirModuleVisitor, is_entry_point, qubit_id, required_num_qubits


class ValidateBeginEndParallel(QirModuleVisitor):
    """
    Ensure that only one parallel section is active at a time and that they all begin and end in the same block.
    """

    def _on_block(self, block):
        self.parallel = False
        super()._on_block(block)
        if self.parallel:
            raise ValueError("Unmatched __quantum__rt__begin_parallel at end of block")

    def _on_call_instr(self, call):
        if call.callee.name == "__quantum__rt__begin_parallel":
            if self.parallel:
                raise ValueError(
                    "Nested __quantum__rt__begin_parallel in parallel section"
                )
            self.parallel = True
        elif call.callee.name == "__quantum__rt__end_parallel":
            if not self.parallel:
                raise ValueError("Unmatched __quantum__rt__end_parallel")
            self.parallel = False


class PerQubitOrdering(QirModuleVisitor):
    """
    Get the ordering of instructions on each qubit as a data structure.
    """

    qubit_instructions: list[list[str]]

    def _on_function(self, function):
        if is_entry_point(function):
            num_qubits = required_num_qubits(function)
            if num_qubits is None:
                raise ValueError("Entry function must have a known number of qubits")
            self.qubit_instructions = [[] for _ in range(num_qubits)]
            if len(function.basic_blocks) > 1:
                raise ValueError(
                    "Entry function must have a single basic block for per-qubit ordering analysis"
                )
            super()._on_function(function)

    def _on_call_instr(self, call):
        if call.callee.name == "__quantum__qis__sx__body":
            self._on_qis_sx(call, call.args[0])
        else:
            super()._on_call_instr(call)

    def _on_qis_cz(self, call, ctrl, target):
        ctrl_id = qubit_id(ctrl)
        target_id = qubit_id(target)
        assert (
            ctrl_id is not None and target_id is not None
        ), "Qubit ids should be known"
        self.qubit_instructions[ctrl_id].append(str(call))
        self.qubit_instructions[target_id].append(str(call))

    def _on_qis_sx(self, call, target):
        target_id = qubit_id(target)
        assert target_id is not None, "Qubit id should be known"
        self.qubit_instructions[target_id].append(str(call))

    def _on_qis_rz(self, call, angle, target):
        target_id = qubit_id(target)
        assert target_id is not None, "Qubit id should be known"
        self.qubit_instructions[target_id].append(str(call))

    def _on_qis_mresetz(self, call, target, result):
        target_id = qubit_id(target)
        assert target_id is not None, "Qubit id should be known"
        self.qubit_instructions[target_id].append(str(call))


def check_qubit_ordering_unchanged(
    after: PerQubitOrdering, before: PerQubitOrdering
) -> None:
    for q, (after_instrs, before_instrs) in enumerate(
        zip(after.qubit_instructions, before.qubit_instructions)
    ):
        if before_instrs != after_instrs:
            print("Reordering changed the per-qubit instruction order:")
            print(f"Qubit {q}:")
            print("  Before:")
            for instr in before_instrs:
                print(f"    {instr}")
            print("  After:")
            for instr in after_instrs:
                print(f"    {instr}")
            raise RuntimeError("Reordering changed the per-qubit instruction order")
