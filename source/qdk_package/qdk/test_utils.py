"""Utilities for testing Q# code."""

from collections.abc import Callable
import re
import types
from typing import Any, Optional

from qdk._interpreter import _get_context_or_default, _get_default_context

from ._context import Context


def dump_operation_on_state(
    op: Any,
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
    state = result["events"][-1].state_dump().get_dict()
    statevector = [0.0] * (2**num_qubits)
    for index, amplitude in state.items():
        statevector[index] = amplitude
    return statevector


def _get_test_callables(context: Context) -> list[Callable]:
    """Return all Q# callables marked with the ``@Test`` attribute."""
    test_callables: list[Callable] = []

    # Iterate through all the attributes of self.code and check if they are
    # callables with the __is_test__ attribute set to True.
    # Recursively check for nested modules as well.
    def find_test_callables(module):
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
