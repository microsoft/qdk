# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._utils import as_qis_gate, get_used_values, uses_any_value
from pyqir import (
    Call,
    Instruction,
    Function,
    QirModuleVisitor,
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
        return qubits[0]
    # Any non-gate, non-measurement instructions should get sorted first using a single-arity tuple of "0."
    return 0


class Reorder(QirModuleVisitor):
    """
    Reorder instructions within a block to find contiguous sequences of the same gate on
    different qubits. This enables both layout and scheduling at a later stage.
    """

    def _on_block(self, block):
        # The instructions are collected into an ordered list of steps, where each step
        # contains instructions of the same type that do not depend on each other.
        steps = []

        # A list of all values or resultsused in the current step. This is used to determine if an instruction
        # can be added to the current step or if it needs to go into a new step by checking dependencies.
        values_used_in_step = []
        results_used_in_step = []

        # Output recording instructions and terminator must be treated separately, as those
        # must be at the end of the block.
        output_recording = []
        terminator = block.terminator
        if terminator:
            terminator.remove()

        for instr in block.instructions:
            # Remove the instruction from the block, which keeps it alive in the module
            # and available for later insertion.
            instr.remove()
            if is_output_recording(instr):
                # Gather output recording instructions to be placed at the end of the block just before
                # the terminator.
                output_recording.append(instr)
            else:
                # Find the last step that contains instructions that the current instruction
                # depends on. We want to insert the current instruction on the earliest possible
                # step without violating dependencies.
                last_dependent_step_idx = len(steps) - 1
                (used_values, used_results) = get_used_values(instr)
                while last_dependent_step_idx >= 0:
                    if uses_any_value(
                        used_values, values_used_in_step[last_dependent_step_idx]
                    ) or uses_any_value(
                        used_results, results_used_in_step[last_dependent_step_idx]
                    ):
                        break
                    last_dependent_step_idx -= 1

                if isinstance(instr, Call):
                    while (
                        last_dependent_step_idx < len(steps) - 1
                        and isinstance(steps[last_dependent_step_idx + 1][0], Call)
                        and instr.callee != steps[last_dependent_step_idx + 1][0].callee
                    ):
                        last_dependent_step_idx += 1

                if last_dependent_step_idx == len(steps) - 1:
                    # The current instruction depends on the last step, so add it to a new step at the end.
                    steps.append([instr])
                    values_used_in_step.append(set(used_values))
                    results_used_in_step.append(set(used_results))
                else:
                    # The last dependent step is before the end, so add the current instruction to the
                    # step after it.
                    steps[last_dependent_step_idx + 1].append(instr)
                    values_used_in_step[last_dependent_step_idx + 1].update(used_values)
                    results_used_in_step[last_dependent_step_idx + 1].update(
                        used_results
                    )

        # Insert the instructions back into the block in the correct order.
        self.builder.insert_at_end(block)
        for step in steps:
            for instr in sorted(step, key=instr_key):
                self.builder.instr(instr)
        # Add output recording instructions and terminator at the end of the block.
        for instr in output_recording:
            self.builder.instr(instr)
        if terminator:
            self.builder.instr(terminator)
