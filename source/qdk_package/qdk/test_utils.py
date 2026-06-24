"""Helper functions for testing Q# code."""

from typing import Any

from qdk._interpreter import _get_context_or_default

from ._context import Context


def dump_operation_on_state(
    op: Any,
    num_qubits: int,
    initial_state: list[float] | None = None,
    context: Context | None = None,
) -> list[complex]:
    """Returns statevector after applying operation to the given state.

    Uses big-endian convention for basis-state numbering.

    Args:
        op: Q# callable from ``Context.code`` or a string that evaluates to
            a Q# callable. The callable must have signature
            ``(Qubit[] => Unit)``.
        num_qubits: Number of qubits the operation acts on.
        initial_state: Initial state given by list of `2**num_qubits` real amplitudes.
            If the list is shorter, it will be padded with zeros.
            If not provided, the initial state is zero state (|00..0>).
        context: `qdk.Context` from which the operation was created (optional). If
            not provided, will attempt to infer it from `op` and then fall back to
            default context.

    Returns:
        The state vector as a list of `2**num_qubits` complex numbers.
    """
    context = context or _get_context_or_default(op)
    if initial_state is None:
        initial_state = [1.0]  # |00..0> state.
    if type(op) is str:
        op = context.eval(op)

    if not hasattr(context.code, "_DumpOperationOnState"):
        context.eval("""
        operation _DumpOperationOnState(
            op : (Qubit[] => Unit),
            num_qubits : Int,
            initial_state : Double[]
        ) : Unit {
            use qubits = Qubit[num_qubits];
            if (Length(initial_state) > 1) {
                Std.StatePreparation.PreparePureStateD(initial_state, qubits);
            }
            op(qubits);
            Std.Diagnostics.DumpRegister(qubits);
            ResetAll(qubits);
        }
        """)

    result = context.run(
        context.code._DumpOperationOnState,
        1,  # shots
        op,
        num_qubits,
        initial_state,
        save_events=True,
    )[0]
    state: dict[int, complex] = result["events"][-1].state_dump().get_dict()
    statevector = [0.0j] * (2**num_qubits)
    for index, amplitude in state.items():
        statevector[index] = amplitude
    return statevector
