# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
Shared helpers for working with ``pyqir`` modules.

Both ``_simulation.py`` and ``_qir_circuit.py`` import from here to avoid
duplicating pyqir boilerplate for entry-point discovery and value extraction.
"""

from __future__ import annotations

from typing import Any, Optional, Tuple

import pyqir


def find_entry_point(module: Any) -> Any:
    """Return the entry-point function of a ``pyqir.Module``.

    Raises ``ValueError`` if no entry point is found.
    """
    for func in module.functions:
        if pyqir.is_entry_point(func):
            return func
    raise ValueError("No entry point found in the QIR module")


def get_entry_point_info(module: Any) -> Tuple[Any, int, int]:
    """Return ``(entry_func, num_qubits, num_results)`` for a module.

    Raises ``ValueError`` if no entry point is found.
    """
    func = find_entry_point(module)
    num_qubits = pyqir.required_num_qubits(func)
    if num_qubits is None:
        num_qubits = 0
    num_results = pyqir.required_num_results(func)
    if num_results is None:
        num_results = 0
    return func, num_qubits, num_results


def qubit_id(value: Any) -> int:
    """Extract the qubit id from a pyqir value, raising on ``None``."""
    qid = pyqir.qubit_id(value)
    if qid is None:
        raise ValueError("expected a qubit operand")
    return qid


def result_id(value: Any) -> int:
    """Extract the result id from a pyqir value, raising on ``None``."""
    rid = pyqir.result_id(value)
    if rid is None:
        raise ValueError("expected a result operand")
    return rid


def float_value(value: Any) -> float:
    """Extract a float from a ``pyqir.FloatConstant``."""
    if not isinstance(value, pyqir.FloatConstant):
        raise TypeError(f"expected FloatConstant, got {type(value).__name__}")
    return value.value


def int_value(value: Any) -> int:
    """Extract an int from a ``pyqir.IntConstant``."""
    if not isinstance(value, pyqir.IntConstant):
        raise TypeError(f"expected IntConstant, got {type(value).__name__}")
    return value.value


def tag_string(value: Any) -> str:
    """Extract a tag string from a pyqir byte-string value."""
    bs = pyqir.extract_byte_string(value)
    if bs is None:
        return ""
    return bs.decode("utf-8")


def is_void_type(ty: Any) -> bool:
    """Return ``True`` if *ty* is the LLVM void type (plain ``pyqir.Type``,
    not a subclass like ``IntType`` or ``PointerType``)."""
    return type(ty) is pyqir.Type  # noqa: E721


def is_qubit_type(ty: Any) -> bool:
    """Thin re-export of ``pyqir.is_qubit_type``."""
    return pyqir.is_qubit_type(ty)


def is_result_type(ty: Any) -> bool:
    """Thin re-export of ``pyqir.is_result_type``."""
    return pyqir.is_result_type(ty)


def is_float_constant(value: Any) -> bool:
    """Check if *value* is a ``pyqir.FloatConstant``."""
    return isinstance(value, pyqir.FloatConstant)


def is_int_constant(value: Any) -> bool:
    """Check if *value* is a ``pyqir.IntConstant``."""
    return isinstance(value, pyqir.IntConstant)


def is_bool_int_type(value: Any) -> bool:
    """Check if *value* has ``i1`` (boolean) integer type."""
    return isinstance(value.type, pyqir.IntType) and value.type.width == 1
