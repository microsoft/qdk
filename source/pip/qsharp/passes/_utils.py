# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import (
    Instruction,
    Call,
    Constant,
    qubit_id,
    is_qubit_type,
    result_id,
    is_result_type,
)
from typing import Dict

TOLERANCE: float = 1.1920929e-7  # Machine epsilon for 32-bit IEEE FP numbers.


# If this is a call to a __qis__ gate, return a dict describing the gate and its arguments.
def as_qis_gate(instr: Instruction) -> Dict:
    if isinstance(instr, Call) and instr.callee.name.startswith("__quantum__qis__"):
        parts = instr.callee.name.split("__")
        return {
            "gate": parts[3] + ("_adj" if parts[4] == "adj" else ""),
            "qubit_args": [
                qubit_id(arg) for arg in instr.args if qubit_id(arg) is not None
            ],
            "result_args": [
                result_id(arg) for arg in instr.args if result_id(arg) is not None
            ],
            "other_args": [
                arg
                for arg in instr.args
                if qubit_id(arg) is None and result_id(arg) is None
            ],
        }
    return {}


# Return true if the first instruction depends on any values from the second instruction.
# This treats any qubit or result constants that share an id as a dependency to preserve ordering of
# quantum operations.
def depends_on(instr1: Instruction, instr2: Instruction):
    vals1 = []
    vals2 = []
    if isinstance(instr1, Call):
        vals1 = instr1.args
    else:
        vals1 = instr1.operands
    vals1.append(instr1)
    if isinstance(instr2, Call):
        vals2 = instr2.args
    else:
        vals2 = instr2.operands
    vals2.append(instr2)
    return any(
        [
            val in vals2
            for val in vals1
            if not isinstance(val, Constant)
            or (is_qubit_type(val.type) or is_result_type(val.type))
        ]
    )


# Returns all values used by the instruction.
def get_used_values(instr: Instruction):
    vals = []
    if isinstance(instr, Call):
        vals = instr.args
    else:
        vals = instr.operands
    return vals


# Returns true if any of the used values are in the existing values.
# Useful for determining if an instruction depends on any instructions in a set.
def uses_any_value(used_values, existing_values):
    return any(
        [
            val in existing_values
            for val in used_values
            if not isinstance(val, Constant)
            or (is_qubit_type(val.type) or is_result_type(val.type))
        ]
    )
