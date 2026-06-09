from typing import Any

from qdk._interpreter import _get_default_context

from ._context import Context


class OperationTestHelper:
    """Helper for testing Q# operations."""

    def __init__(self, context: Context | None = None):
        self.context = context or _get_default_context()

    def get_action_on_state(self, op: Any, num_qubits: int) -> list[complex]:
        """Returns the state vector after applying an operation to the zero state.

        Uses big-endian convention for basis-state numbering.

        Args:
            op: Q# callable from ``Context.code`` or a string that evaluates to
                a Q# callable. The callable must have signature
                ``(Qubit[] => Unit)``.
            num_qubits: Number of qubits the operation acts on.

        Returns:
            The state vector as a list of ``2**num_qubits`` complex numbers.
        """

        if type(op) is str:
            op = self.context.eval(op)

        self.context.eval("""
        operation _GetActionOnZeroState(num_qubits: Int, op: (Qubit[] => Unit)) : Unit {
            use qubits = Qubit[num_qubits];
            op(qubits);
            Microsoft.Quantum.Diagnostics.DumpRegister(qubits);
            ResetAll(qubits);
        }
        """)
        result = self.context.run(
            self.context.code._GetActionOnZeroState,
            1,
            num_qubits,
            op,
            save_events=True,
        )[0]
        state = result["events"][-1].state_dump().get_dict()
        result = [0.0] * (2**num_qubits)
        for key, value in state.items():
            result[key] = value
        return result
