"""Helper functions for testing Q# code."""

from typing import Callable

from ._context import Context
from ._interpreter import _get_context_or_default


def dump_operation_on_state(
    op: Callable | str,
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
    if isinstance(op, str):
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


class ArithmeticOpTester:
    """Runs an in-place Q# arithmetic operation on classical integer inputs.

    On each call, the registers are initialized to the given integers, the
    operation is applied, and the resulting integers are read back. Numbers use
    the unsigned little-endian convention. Register sizes are fixed, so an
    instance can be reused across many inputs.

    Args:
        op: The operation under test, as a Q# callable from ``Context.code`` or
            a string that evaluates to one. It takes one ``Qubit[]`` per
            register, in the same order as `arg_sizes`.
        arg_sizes: Number of qubits in each register; its length is the arity.
        context: `qdk.Context` to evaluate the operation in. If omitted, it is
            inferred from `op` and otherwise falls back to the default context.

    Example:
        >>> tester = ArithmeticOpTester("Std.Arithmetic.IncByLE", [8, 8])
        >>> tester.run([5, 7])  # in-place y += x
        [5, 12]
    """

    def __init__(
        self, op: Callable | str, arg_sizes: list[int], context: Context | None = None
    ):
        context = context or _get_context_or_default(op)
        self.arity = len(arg_sizes)
        args_expanded = ",".join(f"r[{i}]" for i in range(self.arity))

        if isinstance(op, str):
            context.eval(f"""
            operation _RunOpOnInputs(inputs: BigInt[]) : BigInt[] {{
                return Std.ArithmeticTestUtils.TestArithmeticOp(
                    r=>{op}({args_expanded}),{arg_sizes},inputs
                );           
            }}
            """)
            self.test_callable = context.code._RunOpOnInputs
        else:
            input_signature = ",".join(["Qubit[]"] * self.arity)
            context.eval(f"""
            operation _RunOpOnInputs(
                op: ({input_signature}) => Unit, 
                inputs: BigInt[]
            ) : BigInt[] {{
                return Std.ArithmeticTestUtils.TestArithmeticOp(
                    r=>op({args_expanded}),{arg_sizes},inputs
                );           
            }}
            """)
            self.test_callable = lambda x: context.code._RunOpOnInputs(op, x)

    def run(self, args: list[int]) -> list[int]:
        """Runs the operation on one integer per register and returns the results."""
        assert len(args) == self.arity, f"Must pass exactly {self.arity} inputs"
        return self.test_callable(args)

    @staticmethod
    def run_op(
        op: Callable | str,
        arg_sizes: list[int],
        args: list[int],
        context: Context | None = None,
    ) -> list[int]:
        """Constructs a tester and runs the operation once."""
        return ArithmeticOpTester(op, arg_sizes, context=context).run(args)

    @staticmethod
    def run_unary_op(
        op: Callable | str,
        arg_size: int,
        arg: int,
        context: Context | None = None,
    ) -> int:
        """Constructs a tester and runs a single-register operation once."""
        return ArithmeticOpTester(op, [arg_size], context=context).run([arg])[0]
