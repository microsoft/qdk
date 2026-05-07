# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Q# interpreter lifecycle and core operations.

This module manages the singleton Q# interpreter instance and exposes the
public functions that drive it: :func:`init`, :func:`eval`, :func:`run`,
:func:`compile`, :func:`circuit`, :func:`estimate`, :func:`logical_counts`,
:func:`set_quantum_seed`, :func:`set_classical_seed`, :func:`dump_machine`,
and :func:`dump_circuit`.

Internal helpers such as :func:`get_interpreter`, :func:`ipython_helper`,
:func:`python_args_to_interpreter_args`, and
:func:`qsharp_value_to_python_value` are also defined here for use by other
submodules.
"""

import warnings
from . import telemetry_events, code
from ._native import (  # type: ignore
    Interpreter,
    TargetProfile,
    StateDumpData,
    QSharpError,
    Output,
    Circuit,
    GlobalCallable,
    Closure,
    Pauli,
    Result,
    UdtValue,
    TypeIR,
    TypeKind,
    PrimitiveKind,
    CircuitConfig,
    CircuitGenerationMethod,
    NoiseConfig,
)
from typing import (
    Any,
    Callable,
    Dict,
    List,
    Optional,
    Set,
    Tuple,
    Union,
    Iterable,
    Literal,
    cast,
)
from .estimator._estimator import (
    EstimatorResult,
    EstimatorParams,
    LogicalCounts,
)
from ._types import (
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
    StateDump,
    ShotResult,
    Config,
    QirInputData,
)
import json
import sys
import types
from time import monotonic
from dataclasses import make_dataclass

# ---------------------------------------------------------------------------
# Value conversion helpers
# ---------------------------------------------------------------------------


def lower_python_obj(obj: object, visited: Optional[Set[object]] = None) -> Any:
    if visited is None:
        visited = set()

    if id(obj) in visited:
        raise QSharpError("Cannot send circular objects from Python to Q#.")

    visited = visited.copy().add(id(obj))

    # Base case: Primitive types
    if isinstance(obj, (bool, int, float, complex, str, Pauli, Result)):
        return obj

    # Recursive case: Tuple
    if isinstance(obj, tuple):
        return tuple(lower_python_obj(elt, visited) for elt in obj)

    # Recursive case: Dict
    if isinstance(obj, dict):
        return {name: lower_python_obj(val, visited) for name, val in obj.items()}

    # Base case: Callable or Closure
    if hasattr(obj, "__global_callable"):
        return obj.__getattribute__("__global_callable")
    if isinstance(obj, (GlobalCallable, Closure)):
        return obj

    # Recursive case: Class with slots
    if hasattr(obj, "__slots__"):
        fields = {}
        for name in getattr(obj, "__slots__"):
            if name == "__dict__":
                for name, val in obj.__dict__.items():
                    fields[name] = lower_python_obj(val, visited)
            else:
                val = getattr(obj, name)
                fields[name] = lower_python_obj(val, visited)
        return fields

    # Recursive case: Class
    if hasattr(obj, "__dict__"):
        fields = {
            name: lower_python_obj(val, visited) for name, val in obj.__dict__.items()
        }
        return fields

    # Recursive case: Array
    # By using `Iterable` instead of `list`, we can handle other kind of iterables
    # like numpy arrays and generators.
    if isinstance(obj, Iterable):
        return [lower_python_obj(elt, visited) for elt in obj]

    raise TypeError(f"unsupported type: {type(obj)}")


def python_args_to_interpreter_args(args):
    """
    Helper function to turn the `*args` argument of this module
    to the format expected by the Q# interpreter.
    """
    if len(args) == 0:
        return None
    elif len(args) == 1:
        return lower_python_obj(args[0])
    else:
        return lower_python_obj(args)


def qsharp_value_to_python_value(obj):
    # Base case: Primitive types
    if isinstance(obj, (bool, int, float, complex, str, Pauli, Result)):
        return obj

    # Recursive case: Tuple
    if isinstance(obj, tuple):
        # Special case Value::UNIT maps to None.
        if not obj:
            return None
        return tuple(qsharp_value_to_python_value(elt) for elt in obj)

    # Recursive case: Array
    if isinstance(obj, list):
        return [qsharp_value_to_python_value(elt) for elt in obj]

    # Recursive case: Callable or Closure
    if isinstance(obj, (GlobalCallable, Closure)):
        return obj

    # Recursive case: Udt
    if isinstance(obj, UdtValue):
        class_name = obj.name
        fields = []
        args = []
        for name, value_ir in obj.fields:
            val = qsharp_value_to_python_value(value_ir)
            ty = type(val)
            args.append(val)
            fields.append((name, ty))
        return make_dataclass(class_name, fields)(*args)


def make_class_rec(qsharp_type: TypeIR) -> type:
    class_name = qsharp_type.unwrap_udt().name
    fields = {}
    for field in qsharp_type.unwrap_udt().fields:
        ty = None
        kind = field[1].kind()

        if kind == TypeKind.Primitive:
            prim_kind = field[1].unwrap_primitive()
            if prim_kind == PrimitiveKind.Bool:
                ty = bool
            elif prim_kind == PrimitiveKind.Int:
                ty = int
            elif prim_kind == PrimitiveKind.Double:
                ty = float
            elif prim_kind == PrimitiveKind.Complex:
                ty = complex
            elif prim_kind == PrimitiveKind.String:
                ty = str
            elif prim_kind == PrimitiveKind.Pauli:
                ty = Pauli
            elif prim_kind == PrimitiveKind.Result:
                ty = Result
            else:
                raise QSharpError(f"unknown primitive {prim_kind}")
        elif kind == TypeKind.Tuple:
            # Special case Value::UNIT maps to None.
            if not field[1].unwrap_tuple():
                ty = type(None)
            else:
                ty = tuple
        elif kind == TypeKind.Array:
            ty = list
        elif kind == TypeKind.Udt:
            ty = make_class_rec(field[1])
        else:
            raise QSharpError(f"unknown type {kind}")
        fields[field[0]] = ty

    return make_dataclass(
        class_name,
        fields,
    )


# ---------------------------------------------------------------------------
# Interpreter singleton
# ---------------------------------------------------------------------------


_interpreter: Union["Interpreter", None] = None
_config: Union["Config", None] = None

# Check if we are running in a Jupyter notebook to use the IPython display function
_in_jupyter = False
try:
    from IPython.display import display

    if get_ipython().__class__.__name__ == "ZMQInteractiveShell":  # type: ignore
        _in_jupyter = True  # Jupyter notebook or qtconsole
except:
    pass


# Reporting execution time during IPython cells requires that IPython
# gets pinged to ensure it understands the cell is active. This is done by
# simply importing the display function, which it turns out is enough to begin timing
# while avoiding any UI changes that would be visible to the user.
def ipython_helper():
    try:
        if __IPYTHON__:  # type: ignore
            from IPython.display import display
    except NameError:
        pass


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
    from ._fs import read_file, list_directory, exists, join, resolve
    from ._http import fetch_github

    global _interpreter
    global _config

    if isinstance(target_name, str):
        target = target_name.split(".")[0].lower()
        if target == "ionq" or target == "rigetti":
            target_profile = TargetProfile.Base
        elif target == "quantinuum":
            target_profile = TargetProfile.Adaptive_RI
        else:
            raise QSharpError(
                f'target_name "{target_name}" not recognized. Please set target_profile directly.'
            )

    manifest_contents = None
    if project_root is not None:
        # Normalize the project path (i.e. fix file separators and remove unnecessary '.' and '..')
        project_root = resolve(".", project_root)
        qsharp_json = join(project_root, "qsharp.json")
        if not exists(qsharp_json):
            raise QSharpError(
                f"{qsharp_json} not found. qsharp.json should exist at the project root and be a valid JSON file."
            )

        try:
            _, manifest_contents = read_file(qsharp_json)
        except Exception as e:
            raise QSharpError(
                f"Error reading {qsharp_json}. qsharp.json should exist at the project root and be a valid JSON file."
            ) from e

    # Loop through the environment module and remove any dynamically added attributes that represent
    # Q# callables or structs. This is necessary to avoid conflicts with the new interpreter instance.
    keys_to_remove = []
    for key, val in code.__dict__.items():
        if (
            hasattr(val, "__global_callable")
            or hasattr(val, "__qsharp_class")
            or isinstance(val, types.ModuleType)
        ):
            keys_to_remove.append(key)
    for key in keys_to_remove:
        code.__delattr__(key)

    # Also remove any namespace modules dynamically added to the system.
    keys_to_remove = []
    for key in sys.modules:
        if key.startswith("qdk.code."):
            keys_to_remove.append(key)
    for key in keys_to_remove:
        sys.modules.__delitem__(key)

    _interpreter = Interpreter(
        target_profile,
        language_features,
        project_root,
        read_file,
        list_directory,
        resolve,
        fetch_github,
        _make_callable,
        _make_class,
        trace_circuit,
    )

    _config = Config(target_profile, language_features, manifest_contents, project_root)
    # Return the configuration information to provide a hint to the
    # language service through the cell output.
    return _config


def get_interpreter() -> Interpreter:
    """
    Returns the Q# interpreter.

    :return: The Q# interpreter.
    :rtype: Interpreter
    """
    global _interpreter
    if _interpreter is None:
        init()
        assert _interpreter is not None, "Failed to initialize the Q# interpreter."
    return _interpreter


def get_config() -> Config:
    """
    Returns the Q# interpreter configuration.

    :return: The Q# interpreter configuration.
    :rtype: Config
    """
    global _config
    if _config is None:
        init()
        assert _config is not None, "Failed to initialize the Q# interpreter."
    return _config


# ---------------------------------------------------------------------------
# Callable / class factory helpers (used by native code)
# ---------------------------------------------------------------------------


# Helper function that knows how to create a function that invokes a callable. This will be
# used by the underlying native code to create functions for callables on the fly that know
# how to get the currently initialized global interpreter instance.
def _make_callable(callable: GlobalCallable, namespace: List[str], callable_name: str):
    module = code
    # Create a name that will be used to collect the hierarchy of namespace identifiers if they exist and use that
    # to register created modules with the system.
    accumulated_namespace = "qdk.code"
    accumulated_namespace += "."
    for name in namespace:
        accumulated_namespace += name
        # Use the existing entry, which should already be a module.
        if hasattr(module, name):
            module = module.__getattribute__(name)
            if sys.modules.get(accumulated_namespace) is None:
                # This is an existing entry that is not yet registered in sys.modules, so add it.
                # This can happen if a callable with the same name as this namespace is already
                # defined.
                sys.modules[accumulated_namespace] = module
        else:
            # This namespace entry doesn't exist as a module yet, so create it, add it to the environment, and
            # add it to sys.modules so it supports import properly.
            new_module = types.ModuleType(accumulated_namespace)
            module.__setattr__(name, new_module)
            sys.modules[accumulated_namespace] = new_module
            module = new_module
        accumulated_namespace += "."

    def _callable(*args):
        ipython_helper()

        def callback(output: Output) -> None:
            if _in_jupyter:
                try:
                    display(output)
                    return
                except:
                    # If IPython is not available, fall back to printing the output
                    pass
            print(output, flush=True)

        args = python_args_to_interpreter_args(args)

        output = get_interpreter().invoke(callable, args, callback)
        return qsharp_value_to_python_value(output)

    # Each callable is annotated so that we know it is auto-generated and can be removed on a re-init of the interpreter.
    _callable.__global_callable = callable

    # Add the callable to the module.
    if module.__dict__.get(callable_name) is None:
        module.__setattr__(callable_name, _callable)
    else:
        # Preserve any existing attributes on the attribute with the matching name,
        # since this could be a collision with an existing namespace/module.
        for key, val in module.__dict__.get(callable_name).__dict__.items():
            if key != "__global_callable":
                _callable.__dict__[key] = val
        module.__setattr__(callable_name, _callable)


def _make_class(qsharp_type: TypeIR, namespace: List[str], class_name: str):
    """
    Helper function to create a python class given a description of it. This will be
    used by the underlying native code to create classes on the fly corresponding to
    the currently initialized interpreter instance.
    """

    module = code
    # Create a name that will be used to collect the hierarchy of namespace identifiers if they exist and use that
    # to register created modules with the system.
    accumulated_namespace = "qdk.code"
    accumulated_namespace += "."
    for name in namespace:
        accumulated_namespace += name
        # Use the existing entry, which should already be a module.
        if hasattr(module, name):
            module = module.__getattribute__(name)
        else:
            # This namespace entry doesn't exist as a module yet, so create it, add it to the environment, and
            # add it to sys.modules so it supports import properly.
            new_module = types.ModuleType(accumulated_namespace)
            module.__setattr__(name, new_module)
            sys.modules[accumulated_namespace] = new_module
            module = new_module
        accumulated_namespace += "."

    QSharpClass = make_class_rec(qsharp_type)

    # Each class is annotated so that we know it is auto-generated and can be removed on a re-init of the interpreter.
    QSharpClass.__qsharp_class = True

    # Add the class to the module.
    module.__setattr__(class_name, QSharpClass)


# ---------------------------------------------------------------------------
# Public API functions
# ---------------------------------------------------------------------------


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
    ipython_helper()

    results: ShotResult = {
        "events": [],
        "result": None,
        "messages": [],
        "matrices": [],
        "dumps": [],
    }

    def on_save_events(output: Output) -> None:
        # Append the output to the last shot's output list
        if output.is_matrix():
            results["events"].append(output)
            results["matrices"].append(output)
        elif output.is_state_dump():
            dump_data = cast(StateDumpData, output.state_dump())
            state_dump = StateDump(dump_data)
            results["events"].append(state_dump)
            results["dumps"].append(state_dump)
        elif output.is_message():
            stringified = str(output)
            results["events"].append(stringified)
            results["messages"].append(stringified)

    def callback(output: Output) -> None:
        if _in_jupyter:
            try:
                display(output)
                return
            except:
                # If IPython is not available, fall back to printing the output
                pass
        print(output, flush=True)

    telemetry_events.on_eval()
    start_time = monotonic()

    output = get_interpreter().interpret(
        source, on_save_events if save_events else callback
    )
    results["result"] = qsharp_value_to_python_value(output)

    durationMs = (monotonic() - start_time) * 1000
    telemetry_events.on_eval_end(durationMs)

    if save_events:
        return results
    else:
        return results["result"]


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
    type: Optional[Literal["sparse", "clifford"]] = None,
    num_qubits: Optional[int] = None,
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
    :param type: The type of simulator to use. If not specified, the default sparse state vector simulation will be used.
    :param num_qubits: The number of qubits to use for the simulation type "clifford".
        If not specified, the Clifford simulator assumes a default of 1000 qubits.

    :return: A list of results or runtime errors. If ``save_events`` is true, a list of ``ShotResult`` is returned.
    :rtype: List[Any]
    :raises QSharpError: If there is an error interpreting the input.
    :raises ValueError: If the number of shots is less than 1.
    """
    ipython_helper()

    if shots < 1:
        raise ValueError("The number of shots must be greater than 0.")

    telemetry_events.on_run(
        shots,
        noise=(noise is not None and noise != (0.0, 0.0, 0.0)),
        qubit_loss=(qubit_loss is not None and qubit_loss > 0.0),
    )
    start_time = monotonic()

    results: List[ShotResult] = []

    def print_output(output: Output) -> None:
        if _in_jupyter:
            try:
                display(output)
                return
            except:
                # If IPython is not available, fall back to printing the output
                pass
        print(output, flush=True)

    def on_save_events(output: Output) -> None:
        # Append the output to the last shot's output list
        results[-1]["events"].append(output)
        if output.is_matrix():
            results[-1]["matrices"].append(output)
        elif output.is_state_dump():
            dump_data = cast(StateDumpData, output.state_dump())
            results[-1]["dumps"].append(StateDump(dump_data))
        elif output.is_message():
            results[-1]["messages"].append(str(output))

    if type == "clifford":
        if noise is not None and not isinstance(noise, NoiseConfig):
            raise ValueError(
                "only `NoiseConfig` is supported when using noise with the clifford simulator."
            )

    callable = None
    run_entry_expr = None
    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        callable = entry_expr.__global_callable
    elif isinstance(entry_expr, (GlobalCallable, Closure)):
        args = python_args_to_interpreter_args(args)
        callable = entry_expr
    else:
        assert isinstance(entry_expr, str)
        run_entry_expr = entry_expr

    noise_config = None
    if isinstance(noise, NoiseConfig):
        noise_config = noise
        noise = None

    shot_seed = seed
    for shot in range(shots):
        # We also don't want every shot to return the same results, so we update the seed for
        # the next shot with the shot number. This keeps the behavior deterministic if a seed
        # was provided.
        if seed is not None:
            shot_seed = shot + seed

        results.append(
            {"result": None, "events": [], "messages": [], "matrices": [], "dumps": []}
        )
        run_results = get_interpreter().run(
            run_entry_expr,
            on_save_events if save_events else print_output,
            noise_config,
            noise,
            qubit_loss,
            callable,
            args,
            shot_seed,
            type,
            num_qubits,
        )
        run_results = qsharp_value_to_python_value(run_results)
        results[-1]["result"] = run_results
        if on_result:
            on_result(results[-1])
        # For every shot after the first, treat the entry expression as None to trigger
        # a rerun of the last executed expression without paying the cost for any additional
        # compilation.
        run_entry_expr = None

    durationMs = (monotonic() - start_time) * 1000
    telemetry_events.on_run_end(durationMs, shots)

    if save_events:
        return results
    else:
        return [shot["result"] for shot in results]


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
    ipython_helper()
    start = monotonic()
    interpreter = get_interpreter()
    target_profile = get_config().get_target_profile()
    telemetry_events.on_compile(target_profile)
    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        ll_str = interpreter.qir(callable=entry_expr.__global_callable, args=args)
    elif isinstance(entry_expr, (GlobalCallable, Closure)):
        args = python_args_to_interpreter_args(args)
        ll_str = interpreter.qir(callable=entry_expr, args=args)
    else:
        assert isinstance(entry_expr, str)
        ll_str = interpreter.qir(entry_expr=entry_expr)
    res = QirInputData("main", ll_str)
    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_compile_end(durationMs, target_profile)
    return res


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
    ipython_helper()
    start = monotonic()
    telemetry_events.on_circuit()
    config = CircuitConfig(
        max_operations=max_operations,
        generation_method=generation_method,
        source_locations=source_locations,
        group_by_scope=group_by_scope,
        prune_classical_qubits=prune_classical_qubits,
    )

    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        res = get_interpreter().circuit(
            config=config, callable=entry_expr.__global_callable, args=args
        )
    elif isinstance(entry_expr, (GlobalCallable, Closure)):
        args = python_args_to_interpreter_args(args)
        res = get_interpreter().circuit(config=config, callable=entry_expr, args=args)
    else:
        assert entry_expr is None or isinstance(entry_expr, str)
        res = get_interpreter().circuit(config, entry_expr, operation=operation)

    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_circuit_end(durationMs)

    return res


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

    ipython_helper()

    warnings.warn(
        "This version of QRE is deprecated and will be removed in a future release. "
        "Please use the new version of QRE in qdk.qre. Refer to aka.ms/qdk.QREv3 for more information.",
        DeprecationWarning,
        stacklevel=2,
    )

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
    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        res_str = get_interpreter().estimate(
            param_str, callable=entry_expr.__global_callable, args=args
        )
    elif isinstance(entry_expr, (GlobalCallable, Closure)):
        args = python_args_to_interpreter_args(args)
        res_str = get_interpreter().estimate(param_str, callable=entry_expr, args=args)
    else:
        assert isinstance(entry_expr, str)
        res_str = get_interpreter().estimate(param_str, entry_expr=entry_expr)
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

    ipython_helper()

    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        res_dict = get_interpreter().logical_counts(
            callable=entry_expr.__global_callable, args=args
        )
    elif isinstance(entry_expr, (GlobalCallable, Closure)):
        args = python_args_to_interpreter_args(args)
        res_dict = get_interpreter().logical_counts(callable=entry_expr, args=args)
    else:
        assert isinstance(entry_expr, str)
        res_dict = get_interpreter().logical_counts(entry_expr=entry_expr)
    return LogicalCounts(res_dict)


def set_quantum_seed(seed: Optional[int]) -> None:
    """
    Sets the seed for the random number generator used for quantum measurements.
    This applies to all Q# code executed, compiled, or estimated.

    :param seed: The seed to use for the quantum random number generator.
        If None, the seed will be generated from entropy.
    """
    get_interpreter().set_quantum_seed(seed)


def set_classical_seed(seed: Optional[int]) -> None:
    """
    Sets the seed for the random number generator used for standard
    library classical random number operations.
    This applies to all Q# code executed, compiled, or estimated.

    :param seed: The seed to use for the classical random number generator.
        If None, the seed will be generated from entropy.
    """
    get_interpreter().set_classical_seed(seed)


def dump_machine() -> StateDump:
    """
    Returns the sparse state vector of the simulator as a StateDump object.

    :return: The state of the simulator.
    :rtype: StateDump
    """
    ipython_helper()
    return StateDump(get_interpreter().dump_machine())


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
    ipython_helper()
    return get_interpreter().dump_circuit()


def dump_operation(operation: str, num_qubits: int) -> List[List[complex]]:
    """
    Returns a square matrix of complex numbers representing the operation performed.

    :param operation: The operation to be performed, which must operate on a list of qubits.
    :param num_qubits: The number of qubits to be used.

    :return: The matrix representing the operation.
    :rtype: List[List[complex]]
    """
    import math

    code_str = f"""{{\n        let op = {operation};\n        use (targets, extra) = (Qubit[{num_qubits}], Qubit[{num_qubits}]);\n            for i in 0..{num_qubits}-1 {{\n                H(targets[i]);\n                CNOT(targets[i], extra[i]);\n            }}\n            operation ApplyOp (op : (Qubit[] => Unit), targets : Qubit[]) : Unit {{ op(targets); }}\n            ApplyOp(op, targets);\n            Microsoft.Quantum.Diagnostics.DumpMachine();\n            ResetAll(targets + extra);\n    }}"""
    result = run(code_str, shots=1, save_events=True)[0]
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


# ---------------------------------------------------------------------------
# __all__
# ---------------------------------------------------------------------------

__all__ = [
    # Types (re-exported from _types for convenience)
    "PauliNoise",
    "DepolarizingNoise",
    "BitFlipNoise",
    "PhaseFlipNoise",
    "StateDump",
    "ShotResult",
    "Config",
    "QirInputData",
    # Native types re-exported
    "Interpreter",
    "TargetProfile",
    "QSharpError",
    "Output",
    "Circuit",
    "GlobalCallable",
    "Closure",
    "Pauli",
    "Result",
    "CircuitConfig",
    "CircuitGenerationMethod",
    "NoiseConfig",
    "StateDumpData",
    # Estimator types
    "EstimatorResult",
    "EstimatorParams",
    "LogicalCounts",
    # Interpreter lifecycle
    "init",
    "get_interpreter",
    "get_config",
    # Core operations
    "eval",
    "run",
    "compile",
    "circuit",
    "estimate",
    "logical_counts",
    # Seed / state
    "set_quantum_seed",
    "set_classical_seed",
    "dump_machine",
    "dump_circuit",
    "dump_operation",
    # Helpers (used by other submodules)
    "ipython_helper",
    "python_args_to_interpreter_args",
    "qsharp_value_to_python_value",
    "lower_python_obj",
    "make_class_rec",
]
