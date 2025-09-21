# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._utils import as_qis_gate, get_used_values, uses_any_value
from pyqir import (
    Call,
    Instruction,
    Function,
    QirModuleVisitor,
    is_entry_point,
    qubit_id,
    required_num_qubits,
)


def is_output_recording(instr: Instruction):
    if isinstance(instr, Call):
        return instr.callee.name.endswith("_record_output")
    return False


def is_irreversible(instr: Instruction):
    if isinstance(instr, Call) and isinstance(instr.callee, Function):
        return "irreversible" in instr.callee.attributes.func
    return False


# Key function for sorting instructions. Instructions are sorted by gate type first and then by qubit arguments.
def instr_key(instr: Instruction):
    gate = as_qis_gate(instr)
    if gate != {}:
        qubits = sorted(gate["qubit_args"])
        if len(qubits) == 2:
            return (gate["gate"], qubits[0], qubits[1])
        if len(gate["result_args"]) > 0:
            return (gate["gate"], gate["result_args"][0])
        return (gate["gate"], qubits[0])
    return ("",)


class Reorder(QirModuleVisitor):
    """
    Reorder instructions within a block to find contiguous sequences of the same gate on
    different qubits. This enables both layout and scheduling at a later stage.
    """

    def _on_block(self, block):
        # The instructions are collected into an ordered list of steps, where each step
        # contains instructions of the same type that do not depend on each other.
        steps = []

        # A list of all values used in the current step. This is used to determine if an instruction
        # can be added to the current step or if it needs to go into a new step by checking dependencies.
        values_used_in_step = []

        # Output recording instructions and terminator must be treated separately, as those
        # must be at the end of the block.
        output_recording = []
        terminator = block.terminator
        terminator.remove()

        for instr in block.instructions:
            # Remove the instruction from the block, which keeps it alive in the module
            # and available for later insertion.
            instr.remove()
            if is_output_recording(instr):
                # Gather output recording instructions to be placed at the end of the block just before
                # the terminator.
                output_recording.append(instr)
            elif is_irreversible(instr):
                used_values = get_used_values(instr)
                # Irreversible instructions must be placed in their own step. Only add
                # them to the last step if it is also for irreversible instructions.
                if (
                    len(steps) > 0
                    and any(is_irreversible(s) for s in steps[-1])
                    and not uses_any_value(used_values, values_used_in_step[-1])
                ):
                    steps[-1].append(instr)
                    values_used_in_step[-1].update(used_values)
                else:
                    steps.append([instr])
                    values_used_in_step.append(set(used_values))
            else:
                # Find the last step that contains instructions that the current instruction
                # depends on. We want to insert the current instruction on the earliest possible
                # step without violating dependencies.
                last_dependent_step_idx = len(steps) - 1
                used_values = get_used_values(instr)
                while last_dependent_step_idx >= 0:
                    if uses_any_value(
                        used_values, values_used_in_step[last_dependent_step_idx]
                    ):
                        break
                    last_dependent_step_idx -= 1

                if last_dependent_step_idx == len(steps) - 1:
                    # The current instruction depends on the last step, so add it to a new step at the end.
                    steps.append([instr])
                    values_used_in_step.append(set(used_values))
                else:
                    # The last dependent step is before the end, so add the current instruction to the
                    # step after it.
                    steps[last_dependent_step_idx + 1].append(instr)
                    values_used_in_step[last_dependent_step_idx + 1].update(used_values)

        # Insert the instructions back into the block in the correct order.
        self.builder.insert_at_end(block)
        for step in steps:
            for instr in sorted(step, key=instr_key):
                self.builder.instr(instr)
        # Add output recording instructions and terminator at the end of the block.
        for instr in output_recording:
            self.builder.instr(instr)
        self.builder.instr(terminator)


class PerQubitOrdering(QirModuleVisitor):
    """
    Get the ordering of instructions on each qubit as a data structure.
    """

    qubit_instructions: list[list[str]]

    def _on_function(self, function):
        if is_entry_point(function):
            self.qubit_instructions = [[] for _ in range(required_num_qubits(function))]
            super()._on_function(function)

    def _on_call_instr(self, call):
        if call.callee.name == "__quantum__qis__sx__body":
            self._on_qis_sx(call, call.args[0])
        else:
            super()._on_call_instr(call)

    def _on_qis_cz(self, call, ctrl, target):
        self.qubit_instructions[qubit_id(ctrl)].append(str(call))
        self.qubit_instructions[qubit_id(target)].append(str(call))

    def _on_qis_sx(self, call, target):
        self.qubit_instructions[qubit_id(target)].append(str(call))

    def _on_qis_rz(self, call, angle, target):
        self.qubit_instructions[qubit_id(target)].append(str(call))

    def _on_qis_mresetz(self, call, target, result):
        self.qubit_instructions[qubit_id(target)].append(str(call))
