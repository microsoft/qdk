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


# Returns all values used by the instruction.
def get_used_values(instr: Instruction):
    vals = []
    meas_results = []
    if isinstance(instr, Call):
        vals = instr.args
        if (
            instr.callee.name == "__quantum__qis__mresetz__body"
            or instr.callee.name == "__quantum__qis__m__body"
            or instr.callee.name == "__quantum__qis__mz__body"
        ):
            # Measurement uses a result as the second argument
            meas_results += vals[1:]
            vals = vals[:1]
        elif (
            instr.callee.name == "__quantum__qis__read_result__body"
            or instr.callee.name == "__quantum__rt__read_result"
            or instr.callee.name == "__quantum__rt__read_atom_result"
        ):
            # Read result uses a result as the first argument
            meas_results += vals
            vals = []
    else:
        vals = instr.operands
    vals.append(instr)
    return (vals, meas_results)


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
