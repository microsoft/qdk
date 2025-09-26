# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from . import telemetry_events, code
from ._native import (  # type: ignore
    Interpreter,
    TargetProfile,
    StateDumpData,
    QSharpError,
    Output,
    Circuit,
    GlobalCallable,
    Pauli,
    Result,
    UdtValue,
    TypeIR,
    TypeKind,
    PrimitiveKind,
)
from typing import (
    Any,
    Callable,
    Dict,
    Optional,
    Tuple,
    TypedDict,
    Union,
    List,
    Set,
    Iterable,
)
from .estimator._estimator import (
    EstimatorResult,
    EstimatorParams,
    LogicalCounts,
)
import json
import os
import sys
import types
from time import monotonic
from dataclasses import make_dataclass


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

    # Recusive case: Dict
    if isinstance(obj, dict):
        return {name: lower_python_obj(val, visited) for name, val in obj.items()}

    # Recursive case: Class with slots
    if hasattr(obj, "__slots__"):
        fields = {}
        for name in obj.__slots__:
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


class Config:
    _config: Dict[str, str]
    """
    Configuration hints for the language service.
    """

    def __init__(
        self,
        target_profile: TargetProfile,
        language_features: Optional[List[str]],
        manifest: Optional[str],
        project_root: Optional[str],
    ):
        if target_profile == TargetProfile.Adaptive_RI:
            self._config = {"targetProfile": "adaptive_ri"}
        if target_profile == TargetProfile.Adaptive_RIF:
            self._config = {"targetProfile": "adaptive_rif"}
        elif target_profile == TargetProfile.Base:
            self._config = {"targetProfile": "base"}
        elif target_profile == TargetProfile.Unrestricted:
            self._config = {"targetProfile": "unrestricted"}

        self._config["languageFeatures"] = language_features
        self._config["manifest"] = manifest
        if project_root:
            # For now, we only support local project roots, so use a file schema in the URI.
            # In the future, we may support other schemes, such as github, if/when
            # we have VS Code Web + Jupyter support.
            normalized_root = os.path.normpath(os.path.join(os.getcwd(), project_root))
            self._config["projectRoot"] = "file://" + normalized_root

    def __repr__(self) -> str:
        return "Q# initialized with configuration: " + str(self._config)

    # See https://ipython.readthedocs.io/en/stable/config/integrating.html#rich-display
    # See https://ipython.org/ipython-doc/3/notebook/nbformat.html#display-data
    # This returns a custom MIME-type representation of the Q# configuration.
    # This data will be available in the cell output, but will not be displayed
    # to the user, as frontends would not know how to render the custom MIME type.
    # Editor services that interact with the notebook frontend
    # (i.e. the language service) can read and interpret the data.
    def _repr_mimebundle_(
        self, include: Union[Any, None] = None, exclude: Union[Any, None] = None
    ) -> Dict[str, Dict[str, str]]:
        return {"application/x.qsharp-config": self._config}


class PauliNoise(Tuple[float, float, float]):
    """
    The Pauli noise to use in simulation represented
    as probabilities of Pauli-X, Pauli-Y, and Pauli-Z errors
    """

    def __new__(cls, x: float, y: float, z: float):
        if x < 0 or y < 0 or z < 0:
            raise ValueError("Pauli noise probabilities must be non-negative.")
        if x + y + z > 1:
            raise ValueError("The sum of Pauli noise probabilities must be at most 1.")
        return super().__new__(cls, (x, y, z))


class DepolarizingNoise(PauliNoise):
    """
    The depolarizing noise to use in simulation.
    """

    def __new__(cls, p: float):
        return super().__new__(cls, p / 3, p / 3, p / 3)


class BitFlipNoise(PauliNoise):
    """
    The bit flip noise to use in simulation.
    """

    def __new__(cls, p: float):
        return super().__new__(cls, p, 0, 0)


class PhaseFlipNoise(PauliNoise):
    """
    The phase flip noise to use in simulation.
    """

    def __new__(cls, p: float):
        return super().__new__(cls, 0, 0, p)


def init(
    *,
    target_profile: TargetProfile = TargetProfile.Unrestricted,
    target_name: Optional[str] = None,
    project_root: Optional[str] = None,
    language_features: Optional[List[str]] = None,
) -> Config:
    """
    Initializes the Q# interpreter.

    :param target_profile: Setting the target profile allows the Q#
        interpreter to generate programs that are compatible
        with a specific target. See :py:class: `qsharp.TargetProfile`.

    :param target_name: An optional name of the target machine to use for inferring the compatible
        target_profile setting.

    :param project_root: An optional path to a root directory with a Q# project to include.
        It must contain a qsharp.json project manifest.
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
            (_, manifest_contents) = read_file(qsharp_json)
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
        if key.startswith("qsharp.code."):
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
    )

    _config = Config(target_profile, language_features, manifest_contents, project_root)
    # Return the configuration information to provide a hint to the
    # language service through the cell output.
    return _config


def get_interpreter() -> Interpreter:
    """
    Returns the Q# interpreter.

    :returns: The Q# interpreter.
    """
    global _interpreter
    if _interpreter is None:
        init()
        assert _interpreter is not None, "Failed to initialize the Q# interpreter."
    return _interpreter


class StateDump:
    """
    A state dump returned from the Q# interpreter.
    """

    """
    The number of allocated qubits at the time of the dump.
    """
    qubit_count: int

    __inner: dict
    __data: StateDumpData

    def __init__(self, data: StateDumpData):
        self.__data = data
        self.__inner = data.get_dict()
        self.qubit_count = data.qubit_count

    def __getitem__(self, index: int) -> complex:
        return self.__inner.__getitem__(index)

    def __iter__(self):
        return self.__inner.__iter__()

    def __len__(self) -> int:
        return len(self.__inner)

    def __repr__(self) -> str:
        return self.__data.__repr__()

    def __str__(self) -> str:
        return self.__data.__str__()

    def _repr_markdown_(self) -> str:
        return self.__data._repr_markdown_()

    def check_eq(
        self, state: Union[Dict[int, complex], List[complex]], tolerance: float = 1e-10
    ) -> bool:
        """
        Checks if the state dump is equal to the given state. This is not mathematical equality,
        as the check ignores global phase.

        :param state: The state to check against, provided either as a dictionary of state indices to complex amplitudes,
            or as a list of real amplitudes.
        :param tolerance: The tolerance for the check. Defaults to 1e-10.
        """
        phase = None
        # Convert a dense list of real amplitudes to a dictionary of state indices to complex amplitudes
        if isinstance(state, list):
            state = {i: val for i, val in enumerate(state)}
        # Filter out zero states from the state dump and the given state based on tolerance
        state = {k: v for k, v in state.items() if abs(v) > tolerance}
        inner_state = {k: v for k, v in self.__inner.items() if abs(v) > tolerance}
        if len(state) != len(inner_state):
            return False
        for key in state:
            if key not in inner_state:
                return False
            if phase is None:
                # Calculate the phase based on the first state pair encountered.
                # Every pair of states after this must have the same phase for the states to be equivalent.
                phase = inner_state[key] / state[key]
            elif abs(phase - inner_state[key] / state[key]) > tolerance:
                # This pair of states does not have the same phase,
                # within tolerance, so the equivalence check fails.
                return False
        return True

    def as_dense_state(self) -> List[complex]:
        """
        Returns the state dump as a dense list of complex amplitudes. This will include zero amplitudes.
        """
        return [self.__inner.get(i, complex(0)) for i in range(2**self.qubit_count)]


class ShotResult(TypedDict):
    """
    A single result of a shot.
    """

    events: List[Output]
    result: Any
    messages: List[str]
    matrices: List[Output]
    dumps: List[StateDump]


def eval(
    source: str,
    *,
    save_events: bool = False,
) -> Any:
    """
    Evaluates Q# source code.

    Output is printed to console.

    :param source: The Q# source code to evaluate.
    :param save_events: If true, all output will be saved and returned. If false, they will be printed.
    :returns value: The value returned by the last statement in the source code or the saved output if `save_events` is true.
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
            state_dump = StateDump(output.state_dump())
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


# Helper function that knows how to create a function that invokes a callable. This will be
# used by the underlying native code to create functions for callables on the fly that know
# how to get the currently intitialized global interpreter instance.
def _make_callable(callable: GlobalCallable, namespace: List[str], callable_name: str):
    module = code
    # Create a name that will be used to collect the hierachy of namespace identifiers if they exist and use that
    # to register created modules with the system.
    accumulated_namespace = "qsharp.code"
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
    module.__setattr__(callable_name, _callable)


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
                raise QSharpError(f"unknown primtive {prim_kind}")
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


def _make_class(qsharp_type: TypeIR, namespace: List[str], class_name: str):
    """
    Helper function to create a python class given a description of it. This will be
    used by the underlying native code to create classes on the fly corresponding to
    the currently initialized interpreter instance.
    """

    module = code
    # Create a name that will be used to collect the hierachy of namespace identifiers if they exist and use that
    # to register created modules with the system.
    accumulated_namespace = "qsharp.code"
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


def run(
    entry_expr: Union[str, Callable],
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
        ]
    ] = None,
    qubit_loss: Optional[float] = None,
) -> List[Any]:
    """
    Runs the given Q# expression for the given number of shots.
    Each shot uses an independent instance of the simulator.

    :param entry_expr: The entry expression. Alternatively, a callable can be provided,
        which must be a Q# global callable.
    :param shots: The number of shots to run.
    :param *args: The arguments to pass to the callable, if one is provided.
    :param on_result: A callback function that will be called with each result.
    :param save_events: If true, the output of each shot will be saved. If false, they will be printed.
    :param noise: The noise to use in simulation.
    :param qubit_loss: The probability of qubit loss in simulation.

    :returns values: A list of results or runtime errors. If `save_events` is true,
    a List of ShotResults is returned.

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
            results[-1]["dumps"].append(StateDump(output.state_dump()))
        elif output.is_message():
            results[-1]["messages"].append(str(output))

    callable = None
    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        callable = entry_expr.__global_callable
        entry_expr = None

    for shot in range(shots):
        results.append(
            {"result": None, "events": [], "messages": [], "matrices": [], "dumps": []}
        )
        run_results = get_interpreter().run(
            entry_expr,
            on_save_events if save_events else print_output,
            noise,
            qubit_loss,
            callable,
            args,
        )
        run_results = qsharp_value_to_python_value(run_results)
        results[-1]["result"] = run_results
        if on_result:
            on_result(results[-1])
        # For every shot after the first, treat the entry expression as None to trigger
        # a rerun of the last executed expression without paying the cost for any additional
        # compilation.
        entry_expr = None

    durationMs = (monotonic() - start_time) * 1000
    telemetry_events.on_run_end(durationMs, shots)

    if save_events:
        return results
    else:
        return [shot["result"] for shot in results]


# Class that wraps generated QIR, which can be used by
# azure-quantum as input data.
#
# This class must implement the QirRepresentable protocol
# that is defined by the azure-quantum package.
# See: https://github.com/microsoft/qdk-python/blob/fcd63c04aa871e49206703bbaa792329ffed13c4/azure-quantum/azure/quantum/target/target.py#L21
class QirInputData:
    # The name of this variable is defined
    # by the protocol and must remain unchanged.
    _name: str

    def __init__(self, name: str, ll_str: str):
        self._name = name
        self._ll_str = ll_str

    # The name of this method is defined
    # by the protocol and must remain unchanged.
    def _repr_qir_(self, **kwargs) -> bytes:
        return self._ll_str.encode("utf-8")

    def __str__(self) -> str:
        return self._ll_str


def compile(entry_expr: Union[str, Callable], *args) -> QirInputData:
    """
    Compiles the Q# source code into a program that can be submitted to a target.
    Either an entry expression or a callable with arguments must be provided.

    :param entry_expr: The Q# expression that will be used as the entrypoint
        for the program. Alternatively, a callable can be provided, which must
        be a Q# global callable.

    :returns QirInputData: The compiled program.

    To get the QIR string from the compiled program, use `str()`.

    Example:

    .. code-block:: python
        program = qsharp.compile("...")
        with open('myfile.ll', 'w') as file:
            file.write(str(program))
    """
    ipython_helper()
    start = monotonic()
    global _config
    target_profile = _config._config.get("targetProfile", "unspecified")
    telemetry_events.on_compile(target_profile)
    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        ll_str = get_interpreter().qir(
            entry_expr=None, callable=entry_expr.__global_callable, args=args
        )
    else:
        ll_str = get_interpreter().qir(entry_expr=entry_expr)
    res = QirInputData("main", ll_str)
    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_compile_end(durationMs, target_profile)
    return res


def circuit(
    entry_expr: Optional[Union[str, Callable]] = None,
    *args,
    operation: Optional[str] = None,
) -> Circuit:
    """
    Synthesizes a circuit for a Q# program. Either an entry
    expression or an operation must be provided.

    :param entry_expr: An entry expression. Alternatively, a callable can be provided,
        which must be a Q# global callable.

    :param *args: The arguments to pass to the callable, if one is provided.

    :param operation: The operation to synthesize. This can be a name of
    an operation of a lambda expression. The operation must take only
    qubits or arrays of qubits as parameters.

    :raises QSharpError: If there is an error synthesizing the circuit.
    """
    ipython_helper()
    start = monotonic()
    telemetry_events.on_circuit()
    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        res = get_interpreter().circuit(
            callable=entry_expr.__global_callable, args=args
        )
    else:
        res = get_interpreter().circuit(entry_expr, operation)

    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_circuit_end(durationMs)

    return res


def estimate(
    entry_expr: Union[str, Callable],
    params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None,
    *args,
) -> EstimatorResult:
    """
    Estimates resources for Q# source code.
    Either an entry expression or a callable with arguments must be provided.

    :param entry_expr: The entry expression. Alternatively, a callable can be provided,
        which must be a Q# global callable.
    :param params: The parameters to configure physical estimation.

    :returns `EstimatorResult`: The estimated resources.
    """

    ipython_helper()

    def _coerce_estimator_params(
        params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None,
    ) -> List[Dict[str, Any]]:
        if params is None:
            params = [{}]
        elif isinstance(params, EstimatorParams):
            if params.has_items:
                params = params.as_dict()["items"]
            else:
                params = [params.as_dict()]
        elif isinstance(params, dict):
            params = [params]
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
    else:
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
    entry_expr: Union[str, Callable],
    *args,
) -> LogicalCounts:
    """
    Extracts logical resource counts from Q# source code.
    Either an entry expression or a callable with arguments must be provided.

    :param entry_expr: The entry expression. Alternatively, a callable can be provided,
        which must be a Q# global callable.

    :returns `LogicalCounts`: Program resources in terms of logical gate counts.
    """

    ipython_helper()

    if isinstance(entry_expr, Callable) and hasattr(entry_expr, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        res_dict = get_interpreter().logical_counts(
            callable=entry_expr.__global_callable, args=args
        )
    else:
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

    :returns: The state of the simulator.
    """
    ipython_helper()
    return StateDump(get_interpreter().dump_machine())


def dump_circuit() -> Circuit:
    """
    Dumps the current circuit state of the interpreter.

    This circuit will contain the gates that have been applied
    in the simulator up to the current point.
    """
    ipython_helper()
    return get_interpreter().dump_circuit()
