# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
Walks a ``pyqir.Module`` and converts it into the instruction-dict format
expected by the native ``circuit_from_qir_program`` function, which in turn
invokes the Rust ``rir_to_circuit`` circuit-generation pipeline.
"""

from __future__ import annotations

from typing import Any, Dict, List, Optional, Tuple

from ._native import CircuitConfig, Circuit, circuit_from_qir_program
from ._qir_utils import (
    get_entry_point_info,
    is_void_type,
    is_float_constant,
    is_int_constant,
    is_bool_int_type,
    is_qubit_type,
    is_result_type,
    qubit_id as _qubit_id,
    result_id as _result_id,
)
from pyqir import Opcode


def circuit_from_qir(
    module: Any,
    *,
    config: Optional[CircuitConfig] = None,
) -> Circuit:
    """Generate a circuit diagram from a ``pyqir.Module``.

    Parameters
    ----------
    module:
        A ``pyqir.Module`` instance.
    config:
        Optional ``CircuitConfig`` controlling diagram generation.
    """
    if config is None:
        config = CircuitConfig()

    # Find the entry point function and extract requirements.
    entry_point, num_qubits, _ = get_entry_point_info(module)

    if not entry_point.basic_blocks:
        raise ValueError("Entry point has no basic blocks")

    # Assign integer IDs to basic blocks.
    block_id_map: Dict[Any, int] = {}
    for idx, block in enumerate(entry_point.basic_blocks):
        block_id_map[block] = idx

    # Variable tracking: map pyqir Value → variable id
    var_counter = 0
    var_map: Dict[int, int] = {}  # id(value) → var_id

    def _get_or_create_var(value: Any) -> int:
        nonlocal var_counter
        key = id(value)
        if key not in var_map:
            var_map[key] = var_counter
            var_counter += 1
        return var_map[key]

    def _value_to_opr(value: Any) -> Dict[str, Any]:
        """Convert a pyqir Value to an operand dict."""
        if is_qubit_type(value.type):
            qid = _qubit_id(value)
            return {"kind": "lit", "lit": {"kind": "Qubit", "value": qid}}
        if is_result_type(value.type):
            rid = _result_id(value)
            return {"kind": "lit", "lit": {"kind": "Result", "value": rid}}
        # Check for constant float (must be before int check since
        # FloatConstant is not an IntConstant).
        if is_float_constant(value):
            return {
                "kind": "lit",
                "lit": {"kind": "Double", "value": value.value},
            }
        # Check for constant int
        if is_int_constant(value):
            if is_bool_int_type(value):
                return {
                    "kind": "lit",
                    "lit": {"kind": "Bool", "value": value.value != 0},
                }
            return {
                "kind": "lit",
                "lit": {"kind": "Integer", "value": value.value},
            }
        # Fall back to variable
        var_id = _get_or_create_var(value)
        ty = _infer_var_ty(value)
        return {"kind": "var", "var": {"id": var_id, "ty": ty}}

    def _infer_var_ty(value: Any) -> str:
        """Infer the VarTy string from a pyqir value."""
        if is_qubit_type(value.type):
            return "Qubit"
        if is_result_type(value.type):
            return "Result"
        type_str = str(value.type)
        if type_str == "double":
            return "Double"
        if type_str == "i1":
            return "Boolean"
        if type_str.startswith("i"):
            return "Integer"
        return "Pointer"

    def _var_dict(value: Any) -> Dict[str, Any]:
        """Create a variable dict for an instruction output."""
        var_id = _get_or_create_var(value)
        ty = _infer_var_ty(value)
        return {"id": var_id, "ty": ty}

    def _convert_call(instr: Any) -> Dict[str, Any]:
        """Convert a pyqir.Call instruction to a Call dict."""
        name = instr.callee.name
        args = [_value_to_opr(arg) for arg in instr.args]
        output = None
        # If the call has a non-void return, create an output variable.
        if not is_void_type(instr.type):
            output = _var_dict(instr)
        return {
            "kind": "Call",
            "callable_name": name,
            "args": args,
            "output": output,
            "dbg_location": None,
        }

    def _convert_icmp(instr: Any) -> Optional[Dict[str, Any]]:
        """Convert an icmp instruction."""
        predicate_map = {
            "eq": "Eq",
            "ne": "Ne",
            "slt": "Slt",
            "sle": "Sle",
            "sgt": "Sgt",
            "sge": "Sge",
        }
        pred_str = str(instr.predicate).lower()  # pyqir exposes .predicate
        cc = predicate_map.get(pred_str)
        if cc is None:
            return None
        ops = list(instr.operands)
        if len(ops) < 2:
            return None
        return {
            "kind": "Icmp",
            "condition": cc,
            "operand0": _value_to_opr(ops[0]),
            "operand1": _value_to_opr(ops[1]),
            "variable": _var_dict(instr),
        }

    def _convert_phi(instr: Any) -> Dict[str, Any]:
        """Convert a PHI instruction."""
        preds: List[Tuple[Dict[str, Any], int]] = []
        for incoming_val, incoming_block in instr.incoming:
            opr = _value_to_opr(incoming_val)
            bid = block_id_map.get(incoming_block, 0)
            preds.append((opr, bid))
        return {
            "kind": "Phi",
            "predecessors": preds,
            "variable": _var_dict(instr),
        }

    _BINOP_MAP = {
        "add": "Add",
        "sub": "Sub",
        "mul": "Mul",
        "sdiv": "Sdiv",
        "srem": "Srem",
        "shl": "Shl",
        "ashr": "Ashr",
        "fadd": "Fadd",
        "fsub": "Fsub",
        "fmul": "Fmul",
        "fdiv": "Fdiv",
        "and": "BitwiseAnd",
        "or": "BitwiseOr",
        "xor": "BitwiseXor",
    }

    def _convert_binop(instr: Any) -> Optional[Dict[str, Any]]:
        """Convert a binary operation instruction."""
        opcode_str = str(instr.opcode).lower().split(".")[-1]
        op = _BINOP_MAP.get(opcode_str)
        if op is None:
            return None
        ops = list(instr.operands)
        if len(ops) < 2:
            return None
        return {
            "kind": "BinOp",
            "op": op,
            "operand0": _value_to_opr(ops[0]),
            "operand1": _value_to_opr(ops[1]),
            "variable": _var_dict(instr),
        }

    def _process_block(block: Any) -> Tuple[int, List[Dict[str, Any]]]:
        """Process a basic block into (block_id, [instruction_dicts])."""
        bid = block_id_map[block]
        instrs: List[Dict[str, Any]] = []

        for instr in block.instructions:
            opcode = instr.opcode
            if opcode == Opcode.CALL:
                instrs.append(_convert_call(instr))
            elif opcode == Opcode.ICMP:
                converted = _convert_icmp(instr)
                if converted is not None:
                    instrs.append(converted)
            elif opcode == Opcode.PHI:
                instrs.append(_convert_phi(instr))
            elif opcode in (
                Opcode.ADD,
                Opcode.SUB,
                Opcode.MUL,
                Opcode.SDIV,
                Opcode.SREM,
                Opcode.SHL,
                Opcode.ASHR,
                Opcode.FADD,
                Opcode.FSUB,
                Opcode.FMUL,
                Opcode.FDIV,
                Opcode.AND,
                Opcode.OR,
                Opcode.XOR,
            ):
                converted = _convert_binop(instr)
                if converted is not None:
                    instrs.append(converted)
            # Skip RET, BR, SWITCH — handled as terminators below

        # Handle terminator
        term = block.terminator
        if term is not None:
            if term.opcode == Opcode.RET:
                instrs.append({"kind": "Return"})
            elif term.opcode == Opcode.BR:
                successors = list(term.successors)
                if len(successors) == 1:
                    # Unconditional branch
                    target_id = block_id_map.get(successors[0], 0)
                    instrs.append({"kind": "Jump", "target": target_id})
                elif len(successors) == 2:
                    # Conditional branch
                    condition_operand = term.operands[0]
                    condition_var = _var_dict(condition_operand)
                    true_id = block_id_map.get(successors[0], 0)
                    false_id = block_id_map.get(successors[1], 0)
                    instrs.append(
                        {
                            "kind": "Branch",
                            "condition": condition_var,
                            "true_block": true_id,
                            "false_block": false_id,
                            "dbg_location": None,
                        }
                    )

        return (bid, instrs)

    # Process all blocks.
    blocks: List[Tuple[int, List[Dict[str, Any]]]] = []
    for block in entry_point.basic_blocks:
        blocks.append(_process_block(block))

    entry_block_id = block_id_map[entry_point.basic_blocks[0]]

    return circuit_from_qir_program(entry_block_id, num_qubits, blocks, config)
