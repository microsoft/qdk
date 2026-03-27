# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .._qsharp import run
from typing import List, Optional, TYPE_CHECKING
import math

if TYPE_CHECKING:
    from .._qsharp import QdkContext


def dump_operation(
    operation: str, num_qubits: int, *, ctx: Optional["QdkContext"] = None
) -> List[List[complex]]:
    """
    Returns a square matrix of complex numbers representing the operation performed.

    :param operation: The operation to be performed, which must operate on a list of qubits.
    :param num_qubits: The number of qubits to be used.

    :returns: The matrix representing the operation.
    """
    code = f"""{{
        let op = {operation};
        use (targets, extra) = (Qubit[{num_qubits}], Qubit[{num_qubits}]);
            for i in 0..{num_qubits}-1 {{
                H(targets[i]);
                CNOT(targets[i], extra[i]);
            }}
            operation ApplyOp (op : (Qubit[] => Unit), targets : Qubit[]) : Unit {{ op(targets); }}
            ApplyOp(op, targets);
            Microsoft.Quantum.Diagnostics.DumpMachine();
            ResetAll(targets + extra);
    }}"""
    result = (ctx.run if ctx else run)(code, shots=1, save_events=True)[0]
    state = result["events"][-1].state_dump().get_dict()
    num_entries = pow(2, num_qubits)
    factor = math.sqrt(num_entries)
    ndigits = 6
    matrix = []
    for i in range(num_entries):
        matrix += [[]]
        for j in range(num_entries):
            entry = state.get(i * num_entries + j)
            if entry is None:
                matrix[i] += [complex(0, 0)]
            else:
                matrix[i] += [
                    complex(
                        round(factor * entry.real, ndigits),
                        round(factor * entry.imag, ndigits),
                    )
                ]
    return matrix
