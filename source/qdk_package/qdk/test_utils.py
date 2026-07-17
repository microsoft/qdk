"""Utilities for testing Q# code."""

import re
import types
from collections.abc import Callable
from typing import Optional

from ._context import Context
from ._interpreter import _get_context_or_default, _get_default_context


def dump_operation_on_state(
    op: Callable | str,
    num_qubits: int,
    initial_state: list[float] | None = None,
    context: Context | None = None,
) -> list[complex]:
    """Return the state vector after applying an operation to a given state.

    Uses big-endian convention for basis-state numbering.

    Args:
        op: Q# callable from ``Context.code`` or a string that evaluates to
            a Q# callable. The callable must have signature
            ``(Qubit[] => Unit)``.
        num_qubits: Number of qubits the operation acts on.
        initial_state: Initial state as a list of ``2**num_qubits`` real amplitudes.
            If the list is shorter, it will be padded with zeros.
            If not provided, the initial state is the zero state (|00..0>).
        context: Optional ``qdk.Context`` in which to evaluate the operation.
            If not provided, this function attempts to infer a context from ``op``
            and falls back to the default context.

    Returns:
        The state vector as a list of ``2**num_qubits`` complex numbers.
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


def _get_test_callables(context: Context) -> list[Callable]:
    """Return all Q# callables marked with the ``@Test`` attribute."""
    test_callables: list[Callable] = []

    # Iterate through all the attributes of self.code and check if they are
    # callables with the __is_test__ attribute set to True.
    # Recursively check for nested modules as well.
    def find_test_callables(module: types.ModuleType | types.SimpleNamespace) -> None:
        for attr_name in dir(module):
            attr = getattr(module, attr_name)
            if callable(attr) and getattr(attr, "__is_test__", False):
                test_callables.append(attr)
            elif isinstance(attr, types.ModuleType) or isinstance(
                attr, types.SimpleNamespace
            ):
                find_test_callables(attr)

    find_test_callables(context.code)
    return test_callables


def run_tests(
    *,
    context: Optional[Context] = None,
    seed: Optional[int] = None,
    regex: Optional[str] = None,
    verbose: int = 1,
) -> None:
    """
    Discover and run ``@Test`` Q# callables in the selected context, with
    optional name filtering and configurable verbosity.

    :param context: Optional `qdk.Context` to discover and run tests in. If not
        provided, the default context is used.
    :param seed: The seed to use for the random number generator in simulation, if any.
    :param regex: Optional regular expression used to filter tests by fully
        qualified test name (for example, ``MyNamespace.MyTest``). Only
        matching tests are run.
    :param verbose: Verbosity level.
        0 - Print nothing.
        1 - Print ``.`` for each successful test and suppress Q# output.
        2 - Print test names and suppress Q# output.
        3+ - Print test names and Q# output.
        For ``verbose >= 1``, failures and a summary are printed at the end.

    :raises RuntimeError: If one or more tests fail.
    """
    context = context or _get_default_context()
    tests = _get_test_callables(context)
    if regex is not None:
        tests = [test for test in tests if re.search(regex, test.__name__) is not None]

    if verbose >= 1:
        print(f"Running {len(tests)} tests...")
    failed_tests = []
    failures = []
    for test in tests:
        if verbose >= 2:
            print(f"Running {test.__name__}...")
        try:
            context.run(test, 1, seed=seed, save_events=(verbose <= 2))
            if verbose == 1:
                print(".", end="")
            elif verbose >= 2:
                print(f"PASSED: {test.__name__}")
        except Exception as e:
            if verbose >= 1:
                print()
                print(f"FAILED: {test.__name__}")
                print(e)
            failed_tests.append(test.__name__)
            failures.append(str(e))

    # Print summary.
    if verbose >= 1:
        num_passed = len(tests) - len(failed_tests)
        print()
        print(f"Finished tests: {num_passed} passed, {len(failed_tests)} failed.")
        for test_name in failed_tests:
            print(f" FAILED: {test.__name__}")

    # Construct descriptive error if there are any failures.
    if len(failed_tests) > 0:
        err = f"{len(failed_tests)} test(s) failed\n"
        for test_name, failure in zip(failed_tests, failures):
            err += f"FAILED: {test.__name__}\n{failure}\n"
        raise RuntimeError(err)
