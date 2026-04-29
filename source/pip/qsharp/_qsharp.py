# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
Q# Interpreter Module-Level Interface

This module provides the public API for interacting with the Q# interpreter.
It includes module-level functions for code evaluation, execution, compilation,
circuit generation, and resource estimation. These functions operate on a global
default session instance.
"""

import json
import sys
import types
import warnings
from time import monotonic
from typing import (
    Any,
    Callable,
    Dict,
    List,
    Optional,
    Tuple,
    Union,
    cast,
)

from . import code, telemetry_events
from ._native import (  # type: ignore
    Circuit,
    CircuitGenerationMethod,
    Closure,
    GlobalCallable,
    NoiseConfig,
    TargetProfile,
)
from ._noise import (
    BitFlipNoise,
    DepolarizingNoise,
    PauliNoise,
    PhaseFlipNoise,
)
from ._session import Session, ipython_helper
from ._types import Config, QirInputData, ShotResult, StateDump
from .estimator._estimator import (
    EstimatorParams,
    EstimatorResult,
    LogicalCounts,
)

# Global default session instance used by methods in this module.
_default_session: Optional[Session] = None


def _get_session(obj: Any = None) -> "Session":
    """
    Returns the global default context, lazily initializing if needed.
    """
    if hasattr(obj, "_qdk_session"):
        return getattr(obj, "_qdk_session")
    global _default_session
    if _default_session is None:
        init()
        assert _default_session is not None, "Failed to initialize the Q# interpreter."
    return _default_session


def _clear_code_module(code_module: types.ModuleType, module_prefix: str):
    """
    Removes dynamically added Q# callables, structs, and namespace modules from
    a code module and sys.modules.
    """
    keys_to_remove = []
    for key, val in code_module.__dict__.items():
        if (
            hasattr(val, "__global_callable")
            or hasattr(val, "__qsharp_class")
            or isinstance(val, types.ModuleType)
        ):
            keys_to_remove.append(key)
    for key in keys_to_remove:
        code_module.__delattr__(key)

    keys_to_remove = []
    for key in sys.modules:
        if key.startswith(module_prefix + "."):
            keys_to_remove.append(key)
    for key in keys_to_remove:
        sys.modules.__delitem__(key)


def init(
    *,
    target_profile: TargetProfile = TargetProfile.Unrestricted,
    target_name: Optional[str] = None,
    project_root: Optional[str] = None,
    language_features: Optional[List[str]] = None,
    trace_circuit: Optional[bool] = None,
) -> Config:
    """
    Initializes the Q# interpreter.

    :keyword target_profile: Setting the target profile allows the Q#
        interpreter to generate programs that are compatible
        with a specific target. See :class:`TargetProfile`.

    :keyword target_name: An optional name of the target machine to use for inferring the compatible
        target_profile setting.

    :keyword project_root: An optional path to a root directory with a Q# project to include.
        It must contain a qsharp.json project manifest.

    :keyword language_features: An optional list of language feature flags to enable.
        These correspond to experimental or preview Q# language features.
        Valid values are:

        - ``"v2-preview-syntax"``: Enables Q# v2 preview syntax. This removes support for
          the scoped qubit allocation block form (``use q = Qubit() { ... }``), requiring
          the statement form instead (``use q = Qubit();``). It also removes the requirement
          to use the ``set`` keyword for mutable variable assignments.

    :keyword trace_circuit: Enables tracing of circuit during execution.
        Passing `True` is required for the `dump_circuit` function to return a circuit.
        The `circuit` function is *NOT* affected by this parameter will always generate a circuit.
    :return: The Q# interpreter configuration.
    :rtype: Config
    """
    global _default_session

    # Dispose the old session so its callables fail gracefully.
    if _default_session is not None:
        _default_session._disposed = True

    # Clean up the global code namespace before creating a new session.
    _clear_code_module(code, "qsharp.code")

    _default_session = Session(
        target_profile=target_profile,
        target_name=target_name,
        project_root=project_root,
        language_features=language_features,
        trace_circuit=trace_circuit,
        _code_module=code,
        _code_prefix="qsharp.code",
    )
    return _default_session._config


def eval(
    source: str,
    *,
    save_events: bool = False,
) -> Any:
    """
    Evaluates Q# source code.

    Output is printed to console.

    :param source: The Q# source code to evaluate.
    :keyword save_events: If true, all output will be saved and returned. If false, they will be printed.
    :return: The value returned by the last statement in the source code, or the saved output if ``save_events`` is true.
    :rtype: Any
    :raises QSharpError: If there is an error evaluating the source code.
    """
    return _get_session().eval(source, save_events=save_events)


def run(
    entry_expr: Union[str, Callable, GlobalCallable, Closure],
    shots: int,
    *args,
    on_result: Optional[Callable[[ShotResult], None]] = None,
    save_events: bool = False,
    noise: Optional[
        Union[
            Tuple[float, float, float],
            PauliNoise,
            BitFlipNoise,
            PhaseFlipNoise,
            DepolarizingNoise,
            NoiseConfig,
        ]
    ] = None,
    qubit_loss: Optional[float] = None,
    seed: Optional[int] = None,
) -> List[Any]:
    """
    Runs the given Q# expression for the given number of shots.
    Each shot uses an independent instance of the simulator.

    :param entry_expr: The entry expression. Alternatively, a callable can be provided,
        which must be a Q# callable.
    :param shots: The number of shots to run.
    :param *args: The arguments to pass to the callable, if one is provided.
    :param on_result: A callback function that will be called with each result.
    :param save_events: If true, the output of each shot will be saved. If false, they will be printed.
    :param noise: The noise to use in simulation.
    :param qubit_loss: The probability of qubit loss in simulation.
    :param seed: The seed to use for the random number generator in simulation, if any.

    :return: A list of results or runtime errors. If ``save_events`` is true, a list of ``ShotResult`` is returned.
    :rtype: List[Any]
    :raises QSharpError: If there is an error interpreting the input.
    :raises ValueError: If the number of shots is less than 1.
    """
    return _get_session(entry_expr).run(
        entry_expr,
        shots,
        *args,
        on_result=on_result,
        save_events=save_events,
        noise=noise,
        qubit_loss=qubit_loss,
        seed=seed,
    )


def compile(
    entry_expr: Union[str, Callable, GlobalCallable, Closure], *args
) -> QirInputData:
    """
    Compiles the Q# source code into a program that can be submitted to a target.
    Either an entry expression or a callable with arguments must be provided.

    :param entry_expr: The Q# expression that will be used as the entrypoint
        for the program. Alternatively, a callable can be provided, which must
        be a Q# callable.
    :param *args: The arguments to pass to the callable, if one is provided.

    :return: The compiled program. Use ``str()`` to get the QIR string.
    :rtype: QirInputData

    Example:

    .. code-block:: python
        program = qsharp.compile("...")
        with open('myfile.ll', 'w') as file:
            file.write(str(program))
    """
    return _get_session(entry_expr).compile(entry_expr, *args)


def circuit(
    entry_expr: Optional[Union[str, Callable, GlobalCallable, Closure]] = None,
    *args,
    operation: Optional[str] = None,
    generation_method: Optional[CircuitGenerationMethod] = None,
    max_operations: Optional[int] = None,
    source_locations: bool = False,
    group_by_scope: bool = True,
    prune_classical_qubits: bool = False,
) -> Circuit:
    """
    Synthesizes a circuit for a Q# program. Either an entry
    expression or an operation must be provided.

    :param entry_expr: An entry expression. Alternatively, a callable can be provided,
        which must be a Q# callable.
    :type entry_expr: str or Callable

    :param *args: The arguments to pass to the callable, if one is provided.

    :keyword operation: The operation to synthesize. This can be a name of
        an operation or a lambda expression. The operation must take only
        qubits or arrays of qubits as parameters.
    :kwtype operation: str

    :keyword generation_method: The method to use for circuit generation.
        :attr:`~qsharp.CircuitGenerationMethod.ClassicalEval` evaluates classical
        control flow at circuit generation time.
        :attr:`~qsharp.CircuitGenerationMethod.Simulate` runs a full simulation to
        trace the circuit.
        :attr:`~qsharp.CircuitGenerationMethod.Static` uses partial evaluation and
        requires a non-``Unrestricted`` target profile. Defaults to ``None`` which
        auto-selects the generation method.
    :kwtype generation_method: :class:`~qsharp.CircuitGenerationMethod`

    :keyword max_operations: The maximum number of operations to include in the circuit.
        Defaults to ``None`` which means no limit.
    :kwtype max_operations: int

    :keyword source_locations: If ``True``, annotates each gate with its source location.
    :kwtype source_locations: bool

    :keyword group_by_scope: If ``True``, groups operations by their containing scope, such as function declarations or loop blocks.
    :kwtype group_by_scope: bool

    :keyword prune_classical_qubits: If ``True``, removes qubits that are never used in a quantum
        gate (e.g. qubits only used as classical controls).
    :kwtype prune_classical_qubits: bool

    :return: The synthesized circuit.
    :rtype: :class:`~qsharp._native.Circuit`
    :raises QSharpError: If there is an error synthesizing the circuit.
    """
    return _get_session(entry_expr).circuit(
        entry_expr,
        *args,
        operation=operation,
        generation_method=generation_method,
        max_operations=max_operations,
        source_locations=source_locations,
        group_by_scope=group_by_scope,
        prune_classical_qubits=prune_classical_qubits,
    )


def estimate(
    entry_expr: Union[str, Callable, GlobalCallable, Closure],
    params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None,
    *args,
) -> EstimatorResult:
    """
    Estimates resources for Q# source code.
    Either an entry expression or a callable with arguments must be provided.

    :param entry_expr: The entry expression. Alternatively, a callable can be provided,
        which must be a Q# callable.
    :param params: The parameters to configure physical estimation.

    :return: The estimated resources.
    :rtype: EstimatorResult
    """

    warnings.warn(
        "This version of QRE is deprecated and will be removed in a future release. Please use the new version of QRE in qdk.qre. Refer to aka.ms/qdk.QREv3 for more information.",
        DeprecationWarning,
        stacklevel=2,
    )

    ipython_helper()

    def _coerce_estimator_params(
        params: Optional[
            Union[Dict[str, Any], List[Dict[str, Any]], EstimatorParams]
        ] = None,
    ) -> List[Dict[str, Any]]:
        if params is None:
            return [{}]
        elif isinstance(params, EstimatorParams):
            if params.has_items:
                return cast(List[Dict[str, Any]], params.as_dict()["items"])
            else:
                return [params.as_dict()]
        elif isinstance(params, dict):
            return [params]
        return params

    params = _coerce_estimator_params(params)
    param_str = json.dumps(params)
    telemetry_events.on_estimate()
    start = monotonic()
    session = _get_session(entry_expr)
    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = session._python_args_to_interpreter_args(args)
        res_str = session._interpreter.estimate(
            param_str, callable=entry_expr.__global_callable, args=args
        )
    elif isinstance(entry_expr, (GlobalCallable, Closure)):
        args = session._python_args_to_interpreter_args(args)
        res_str = session._interpreter.estimate(
            param_str, callable=entry_expr, args=args
        )
    else:
        assert isinstance(entry_expr, str)
        res_str = session._interpreter.estimate(param_str, entry_expr=entry_expr)
    res = json.loads(res_str)

    try:
        qubits = res[0]["logicalCounts"]["numQubits"]
    except (KeyError, IndexError):
        qubits = "unknown"

    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_estimate_end(durationMs, qubits)
    return EstimatorResult(res)


def logical_counts(
    entry_expr: Union[str, Callable, GlobalCallable, Closure],
    *args,
) -> LogicalCounts:
    """
    Extracts logical resource counts from Q# source code.
    Either an entry expression or a callable with arguments must be provided.

    :param entry_expr: The entry expression. Alternatively, a callable can be provided,
        which must be a Q# callable.

    :return: Program resources in terms of logical gate counts.
    :rtype: LogicalCounts
    """
    return _get_session(entry_expr).logical_counts(entry_expr, *args)


def set_quantum_seed(seed: Optional[int]) -> None:
    """
    Sets the seed for the random number generator used for quantum measurements.
    This applies to all Q# code executed, compiled, or estimated.

    :param seed: The seed to use for the quantum random number generator.
        If None, the seed will be generated from entropy.
    """
    _get_session().set_quantum_seed(seed)


def set_classical_seed(seed: Optional[int]) -> None:
    """
    Sets the seed for the random number generator used for standard
    library classical random number operations.
    This applies to all Q# code executed, compiled, or estimated.

    :param seed: The seed to use for the classical random number generator.
        If None, the seed will be generated from entropy.
    """
    _get_session().set_classical_seed(seed)


def dump_machine() -> StateDump:
    """
    Returns the sparse state vector of the simulator as a StateDump object.

    :return: The state of the simulator.
    :rtype: StateDump
    """
    return _get_session().dump_machine()


def dump_circuit() -> Circuit:
    """
    Dumps a circuit showing the current state of the simulator.

    This circuit will contain the gates that have been applied
    in the simulator up to the current point.

    Requires the interpreter to be initialized with `trace_circuit=True`.

    :return: The current circuit trace.
    :rtype: Circuit
    :raises QSharpError: If the interpreter was not initialized with ``trace_circuit=True``.
    """
    return _get_session()._interpreter.dump_circuit()
