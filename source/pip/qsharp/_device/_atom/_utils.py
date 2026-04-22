# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pyqir import (
    Instruction,
    Call,
    Constant,
    PointerType,
    Value,
    ptr_id,
)
from typing import Dict

TOLERANCE: float = 1.1920929e-7  # Machine epsilon for 32-bit IEEE FP numbers.

# QIS gates that consume a measurement result; the value is the 0-based index
# of the result argument. All other pointer-typed arguments of a QIS call are
# qubit arguments.
_RESULT_ARG_INDEX: Dict[str, int] = {
    "__quantum__qis__m__body": 1,
    "__quantum__qis__mz__body": 1,
    "__quantum__qis__mresetz__body": 1,
    "__quantum__qis__read_result__body": 0,
}


# If this is a call to a __qis__ gate, return a dict describing the gate and its arguments.
def as_qis_gate(instr: Instruction) -> Dict:
    if isinstance(instr, Call) and instr.callee.name.startswith("__quantum__qis__"):
        parts = instr.callee.name.split("__")
        result_idx = _RESULT_ARG_INDEX.get(instr.callee.name)
        qubit_args = []
        result_args = []
        other_args = []
        for i, arg in enumerate(instr.args):
            if isinstance(arg.type, PointerType):
                pid = ptr_id(arg)
                if pid is None:
                    other_args.append(arg)
                elif result_idx is not None and i == result_idx:
                    result_args.append(pid)
                else:
                    qubit_args.append(pid)
            else:
                other_args.append(arg)
        return {
            "gate": parts[3] + ("_adj" if parts[4] == "adj" else ""),
            "qubit_args": qubit_args,
            "result_args": result_args,
            "other_args": other_args,
        }
    return {}


# Returns all values and, separately, all measurement results used by the instruction.
def get_used_values(instr: Instruction) -> tuple[list[Value], list[Value]]:
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
def uses_any_value(used_values, existing_values) -> bool:
    return any(
        [
            val in existing_values
            for val in used_values
            if not isinstance(val, Constant) or isinstance(val.type, PointerType)
        ]
    )
