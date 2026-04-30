# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
Q# Session Management

This module provides the Session class for managing Q# interpreter contexts.
Each Session instance has its own interpreter and code namespace, allowing multiple
independent Q# environments to coexist.
"""

import sys
import types
from dataclasses import make_dataclass
from time import monotonic
from typing import (
    Any,
    Callable,
    Iterable,
    List,
    Optional,
    Set,
    Tuple,
    Union,
    cast,
)

from . import telemetry_events
from ._native import (  # type: ignore
    Circuit,
    CircuitConfig,
    CircuitGenerationMethod,
    Closure,
    GlobalCallable,
    Interpreter,
    NoiseConfig,
    Output,
    Pauli,
    PrimitiveKind,
    QSharpError,
    Result,
    StateDumpData,
    TargetProfile,
    TypeIR,
    TypeKind,
    UdtValue,
)
from ._noise import (
    BitFlipNoise,
    DepolarizingNoise,
    PauliNoise,
    PhaseFlipNoise,
)
from ._types import Config, QirInputData, ShotResult, StateDump
from .estimator._estimator import LogicalCounts

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


class Session:
    """
    An isolated Q# interpreter environment.

    A Session provides a self-contained Q# execution environment where code is
    evaluated, compiled, and executed in isolation from other Session instances.
    Each Session maintains its own code namespace.

    A session has attribute `code` which is a Python module containing all Q# operations
    and types defined in this session. This allows you to call Q# operations from
    Python.

    Example:

    .. code-block:: python

        s = qsharp.Session()
        s.eval("operation Main() : Result { use q = Qubit(); X(q); MResetZ(q) }")
        assert s.run("Main()", 2) == [qsharp.Result.One, qsharp.Result.One]
        assert s.code.Main() == qsharp.Result.One
    """

    _interpreter: Interpreter
    _config: Config
    code: types.ModuleType
    _code_prefix: str
    _disposed: bool

    def __init__(
        self,
        *,
        target_profile: TargetProfile = TargetProfile.Unrestricted,
        target_name: Optional[str] = None,
        project_root: Optional[str] = None,
        language_features: Optional[List[str]] = None,
        trace_circuit: Optional[bool] = None,
        _code_module: Optional[types.ModuleType] = None,
        _code_prefix: Optional[str] = None,
    ):
        """
        Initializes a new isolated Q# session.

        :keyword target_profile: Setting the target profile allows the Q#
            interpreter to generate programs that are compatible
            with a specific target. See :class:`TargetProfile`.

        :keyword target_name: An optional name of the target machine to use for 
            inferring the compatible target_profile setting.

        :keyword project_root: An optional path to a root directory with a Q# project to
            include. It must contain a qsharp.json project manifest.

        :keyword language_features: An optional list of language feature flags to 
            enable. These correspond to experimental or preview Q# language features.
            Valid values are:

            - ``"v2-preview-syntax"``: Enables Q# v2 preview syntax. This removes 
              support for the scoped qubit allocation block form 
              (``use q = Qubit() { ... }``), requiring the statement form instead 
              (``use q = Qubit();``). It also removes the requirement to use the ``set``
              keyword for mutable variable assignments.

        :keyword trace_circuit: Enables tracing of circuit during execution.
            Passing `True` is required for the `dump_machine()` function to return a 
            circuit trace. 
            The `circuit()` method is *not* affected by this parameter and will always
            generate a circuit diagram.
        """
        self._disposed = False

        if _code_module is not None:
            self.code = _code_module
            self._code_prefix = _code_prefix or "qsharp.code"
        else:
            self._code_prefix = f"qsharp._session_{id(self)}"
            self.code = types.ModuleType(self._code_prefix)

        from ._fs import exists, join, list_directory, read_file, resolve
        from ._http import fetch_github

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

        self._interpreter = Interpreter(
            target_profile,
            language_features,
            project_root,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            self._make_callable,
            self._make_class,
            trace_circuit,
        )

        self._config = Config(
            target_profile, language_features, manifest_contents, project_root
        )

    def _qsharp_value_to_python_value(self, obj):
        """Converts Q# value to Python value."""
        # Base case: Primitive types
        if isinstance(obj, (bool, int, float, complex, str, Pauli, Result)):
            return obj

        # Recursive case: Tuple
        if isinstance(obj, tuple):
            # Special case Value::UNIT maps to None.
            if not obj:
                return None
            return tuple(self._qsharp_value_to_python_value(elt) for elt in obj)

        # Recursive case: Array
        if isinstance(obj, list):
            return [self._qsharp_value_to_python_value(elt) for elt in obj]

        # Recursive case: Callable or Closure
        if isinstance(obj, (GlobalCallable, Closure)):
            return obj

        # Recursive case: Udt
        if isinstance(obj, UdtValue):
            class_name = obj.name
            fields = []
            args = []
            for name, value_ir in obj.fields:
                val = self._qsharp_value_to_python_value(value_ir)
                ty = type(val)
                args.append(val)
                fields.append((name, ty))
            return make_dataclass(class_name, fields)(*args)

    def _lower_python_obj(
        self, obj: object, visited: Optional[Set[object]] = None
    ) -> Any:
        """Converts Python value to Q# value."""

        # Base case: Primitive types
        if isinstance(obj, (bool, int, float, complex, str, Pauli, Result)):
            return obj

        obj_id = id(obj)
        if visited is None:
            visited = set()
        if obj_id in visited:
            raise QSharpError("Cannot send circular objects from Python to Q#.")
        visited.add(obj_id)

        try:
            # Recursive case: Tuple
            if isinstance(obj, tuple):
                return tuple(self._lower_python_obj(elt, visited) for elt in obj)

            # Recursive case: Dict
            if isinstance(obj, dict):
                return {
                    name: self._lower_python_obj(val, visited)
                    for name, val in obj.items()
                }

            # Base case: Callable or Closure
            if hasattr(obj, "__global_callable"):
                self._check_same_session_callable(obj)
                return obj.__getattribute__("__global_callable")
            if isinstance(obj, (GlobalCallable, Closure)):
                return obj

            # Recursive case: Class with slots
            if hasattr(obj, "__slots__"):
                self._check_same_session_struct(obj)
                fields = {}
                for name in getattr(obj, "__slots__"):
                    if name == "__dict__":
                        for name, val in obj.__dict__.items():
                            fields[name] = self._lower_python_obj(val, visited)
                    else:
                        val = getattr(obj, name)
                        fields[name] = self._lower_python_obj(val, visited)
                return fields

            # Recursive case: Class
            if hasattr(obj, "__dict__"):
                self._check_same_session_struct(obj)
                fields = {
                    name: self._lower_python_obj(val, visited)
                    for name, val in obj.__dict__.items()
                }
                return fields

            # Recursive case: Array
            # By using `Iterable` instead of `list`, we can handle other kind of 
            # iterables like numpy arrays and generators.
            if isinstance(obj, Iterable):
                return [self._lower_python_obj(elt, visited) for elt in obj]

            raise TypeError(f"unsupported type: {type(obj)}")
        finally:
            visited.remove(obj_id)

    def _python_args_to_interpreter_args(self, args: tuple[Any, ...]):
        """Turns `args` to the format expected by the Q# interpreter."""
        if len(args) == 0:
            return None
        elif len(args) == 1:
            return self._lower_python_obj(args[0])
        else:
            return self._lower_python_obj(args)

    def _display(self, output: Output) -> None:
        """Displays output in Jupyter (if alvailable), otherwise prints."""
        if _in_jupyter:
            try:
                display(output)
                return
            except Exception:
                # If IPython is not available, fall back to printing the output.
                pass
        print(output, flush=True)

    def _make_callable(
        self, callable: GlobalCallable, namespace: List[str], callable_name: str
    ):
        """Registers a Q# callable in this session's code module."""
        module = self.code
        accumulated_namespace = self._code_prefix + "."
        for name in namespace:
            accumulated_namespace += name
            if hasattr(module, name):
                module = module.__getattribute__(name)
                if sys.modules.get(accumulated_namespace) is None:
                    sys.modules[accumulated_namespace] = module
            else:
                new_module = types.ModuleType(accumulated_namespace)
                module.__setattr__(name, new_module)
                sys.modules[accumulated_namespace] = new_module
                module = new_module
            accumulated_namespace += "."

        def _callable_fn(*args):
            if self._disposed:
                raise QSharpError(
                    "This callable belongs to a disposed Q# session. "
                    "Re-evaluate the callable in a current session."
                )
            ipython_helper()

            args = self._python_args_to_interpreter_args(args)
            output = self._interpreter.invoke(callable, args, self._display)
            return self._qsharp_value_to_python_value(output)

        setattr(_callable_fn, "_qdk_session", self)
        setattr(_callable_fn, "__global_callable", callable)

        if module.__dict__.get(callable_name) is None:
            module.__setattr__(callable_name, _callable_fn)
        else:
            for key, val in module.__dict__.get(callable_name).__dict__.items():
                if key != "__global_callable":
                    _callable_fn.__dict__[key] = val
            module.__setattr__(callable_name, _callable_fn)

    def _make_class(self, qsharp_type: TypeIR, namespace: List[str], class_name: str):
        """Registers a Q# type as a Python dataclass in this session's code module."""
        module = self.code
        accumulated_namespace = self._code_prefix + "."
        for name in namespace:
            accumulated_namespace += name
            if hasattr(module, name):
                module = module.__getattribute__(name)
            else:
                new_module = types.ModuleType(accumulated_namespace)
                module.__setattr__(name, new_module)
                sys.modules[accumulated_namespace] = new_module
                module = new_module
            accumulated_namespace += "."

        QSharpClass = make_class_rec(qsharp_type)
        QSharpClass.__qsharp_class = True
        setattr(QSharpClass, "_qdk_session", self)
        module.__setattr__(class_name, QSharpClass)

    def _check_same_session_callable(self, callable_fn: Any) -> None:
        """Raise if a callable belongs to a different session."""
        # Callable must originate from Q#, so it always has a session.
        assert hasattr(callable_fn, "_qdk_session")
        callable_session = getattr(callable_fn, "_qdk_session")
        if callable_session is not self:
            raise QSharpError("This callable belongs to a different Session. ")

    def _check_same_session_struct(self, struct: Any) -> None:
        """Raise if a struct belongs to a different session."""
        # Struct values originating from Q# are not themselves tagged with _qdk_session,
        # but their classes are (in _make_class).
        struct_type = type(struct)
        if not hasattr(struct_type, "_qdk_session"):
            # Ignore objects not originating from Q#.
            return
        if getattr(struct_type, "_qdk_session") is not self:
            raise QSharpError("This struct belongs to a different Session. ")

    def eval(
        self,
        source: str,
        *,
        save_events: bool = False,
    ) -> Any:
        """
        Evaluates Q# source code.

        Output is printed to console.

        :param source: The Q# source code to evaluate.
        :keyword save_events: If true, all output will be saved and returned. If false,
            they will be printed.
        :return: The value returned by the last statement in the source code, or the
            saved output if ``save_events`` is true.
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

        telemetry_events.on_eval()
        start_time = monotonic()

        output = self._interpreter.interpret(
            source, on_save_events if save_events else self._display
        )
        results["result"] = self._qsharp_value_to_python_value(output)

        durationMs = (monotonic() - start_time) * 1000
        telemetry_events.on_eval_end(durationMs)

        if save_events:
            return results
        else:
            return results["result"]

    def run(
        self,
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

        :param entry_expr: The entry expression. Alternatively, a callable can be
            provided, which must be a Q# callable.
        :param shots: The number of shots to run.
        :param *args: The arguments to pass to the callable, if one is provided.
        :param on_result: A callback function that will be called with each result.
        :param save_events: If true, the output of each shot will be saved. If false, 
            they will be printed.
        :param noise: The noise to use in simulation.
        :param qubit_loss: The probability of qubit loss in simulation.
        :param seed: The seed to use for the random number generator in simulation, if
            any.

        :return: A list of results or runtime errors. If ``save_events`` is true, a list
            of ``ShotResult`` is returned.
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

        callable = None
        run_entry_expr = None
        if isinstance(entry_expr, Callable) and hasattr(
            entry_expr, "__global_callable"
        ):
            self._check_same_session_callable(entry_expr)
            args = self._python_args_to_interpreter_args(args)
            callable = getattr(entry_expr, "__global_callable")
        elif isinstance(entry_expr, (GlobalCallable, Closure)):
            args = self._python_args_to_interpreter_args(args)
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
            # We also don't want every shot to return the same results, so we update the
            # seed for the next shot with the shot number. This keeps the behavior 
            # deterministic if a seed was provided.
            if seed is not None:
                shot_seed = shot + seed

            results.append(
                {
                    "result": None,
                    "events": [],
                    "messages": [],
                    "matrices": [],
                    "dumps": [],
                }
            )
            run_results = self._interpreter.run(
                run_entry_expr,
                on_save_events if save_events else self._display,
                noise_config,
                noise,
                qubit_loss,
                callable,
                args,
                shot_seed,
            )
            run_results = self._qsharp_value_to_python_value(run_results)
            results[-1]["result"] = run_results
            if on_result:
                on_result(results[-1])
            # For every shot after the first, treat the entry expression as None to 
            # trigger a rerun of the last executed expression without paying the cost 
            # for any additional compilation.
            run_entry_expr = None

        durationMs = (monotonic() - start_time) * 1000
        telemetry_events.on_run_end(durationMs, shots)

        if save_events:
            return results
        else:
            return [shot["result"] for shot in results]

    def compile(
        self, entry_expr: Union[str, Callable, GlobalCallable, Closure], *args
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
        target_profile = self._config.get_target_profile()
        telemetry_events.on_compile(target_profile)
        if isinstance(entry_expr, Callable) and hasattr(
            entry_expr, "__global_callable"
        ):
            self._check_same_session_callable(entry_expr)
            args = self._python_args_to_interpreter_args(args)
            ll_str = self._interpreter.qir(
                callable=getattr(entry_expr, "__global_callable"), args=args
            )
        elif isinstance(entry_expr, (GlobalCallable, Closure)):
            args = self._python_args_to_interpreter_args(args)
            ll_str = self._interpreter.qir(callable=entry_expr, args=args)
        else:
            assert isinstance(entry_expr, str)
            ll_str = self._interpreter.qir(entry_expr=entry_expr)
        res = QirInputData("main", ll_str)
        durationMs = (monotonic() - start) * 1000
        telemetry_events.on_compile_end(durationMs, target_profile)
        return res

    def circuit(
        self,
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

        :param entry_expr: An entry expression. Alternatively, a callable can be 
            provided, which must be a Q# callable.
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

        :keyword max_operations: The maximum number of operations to include in the 
            circuit. Defaults to ``None`` which means no limit.
        :kwtype max_operations: int

        :keyword source_locations: If ``True``, annotates each gate with its source 
            location.
        :kwtype source_locations: bool

        :keyword group_by_scope: If ``True``, groups operations by their containing 
            scope, such as function declarations or loop blocks.
        :kwtype group_by_scope: bool

        :keyword prune_classical_qubits: If ``True``, removes qubits that are never used
            in a quantum gate (e.g. qubits only used as classical controls).
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

        if isinstance(entry_expr, Callable) and hasattr(
            entry_expr, "__global_callable"
        ):
            self._check_same_session_callable(entry_expr)
            args = self._python_args_to_interpreter_args(args)
            res = self._interpreter.circuit(
                config=config,
                callable=getattr(entry_expr, "__global_callable"),
                args=args,
            )
        elif isinstance(entry_expr, (GlobalCallable, Closure)):
            args = self._python_args_to_interpreter_args(args)
            res = self._interpreter.circuit(
                config=config, callable=entry_expr, args=args
            )
        else:
            assert entry_expr is None or isinstance(entry_expr, str)
            res = self._interpreter.circuit(config, entry_expr, operation=operation)

        durationMs = (monotonic() - start) * 1000
        telemetry_events.on_circuit_end(durationMs)

        return res

    def logical_counts(
        self,
        entry_expr: Union[str, Callable, GlobalCallable, Closure],
        *args,
    ) -> LogicalCounts:
        """
        Extracts logical resource counts from Q# source code.
        Either an entry expression or a callable with arguments must be provided.

        :param entry_expr: The entry expression. Alternatively, a callable can be
            provided, which must be a Q# callable.

        :return: Program resources in terms of logical gate counts.
        :rtype: LogicalCounts
        """
        ipython_helper()

        if isinstance(entry_expr, Callable) and hasattr(
            entry_expr, "__global_callable"
        ):
            self._check_same_session_callable(entry_expr)
            args = self._python_args_to_interpreter_args(args)
            res_dict = self._interpreter.logical_counts(
                callable=getattr(entry_expr, "__global_callable"), args=args
            )
        elif isinstance(entry_expr, (GlobalCallable, Closure)):
            args = self._python_args_to_interpreter_args(args)
            res_dict = self._interpreter.logical_counts(callable=entry_expr, args=args)
        else:
            assert isinstance(entry_expr, str)
            res_dict = self._interpreter.logical_counts(entry_expr=entry_expr)
        return LogicalCounts(res_dict)

    def set_quantum_seed(self, seed: Optional[int]) -> None:
        """
        Sets the seed for the random number generator used for quantum measurements.
        This applies to all Q# code executed, compiled, or estimated.

        :param seed: The seed to use for the quantum random number generator.
            If None, the seed will be generated from entropy.
        """
        self._interpreter.set_quantum_seed(seed)

    def set_classical_seed(self, seed: Optional[int]) -> None:
        """
        Sets the seed for the random number generator used for standard
        library classical random number operations.
        This applies to all Q# code executed, compiled, or estimated.

        :param seed: The seed to use for the classical random number generator.
            If None, the seed will be generated from entropy.
        """
        self._interpreter.set_classical_seed(seed)

    def dump_machine(self) -> StateDump:
        """
        Returns the sparse state vector of the simulator as a StateDump object.

        :return: The state of the simulator.
        :rtype: StateDump
        """
        ipython_helper()
        return StateDump(self._interpreter.dump_machine())

    def import_openqasm(
        self,
        source: str,
        **kwargs: Any,
    ) -> Any:
        """
        Imports OpenQASM source code into this session's interpreter. ABC.

        :param source: An OpenQASM program or fragment.
        :type source: str
        :param **kwargs: Additional keyword arguments. Common options:

            - ``name`` (str): The name of the program. This is used as the entry point
              for the program.
            - ``search_path`` (str): The optional search path for resolving file 
              references.
            - ``output_semantics`` (OutputSemantics): The output semantics for the
              compilation.
            - ``program_type`` (ProgramType): The type of program compilation to 
              perform:
                - ``ProgramType.Operation`` (default): the source becomes a Q# operation
                  in the global namespace with parameters for any declared classical
                  inputs and parameters for each of the declared qubits, while any 
                  explicit or implicit output declarations become the return type of the
                  operation.
                - ``ProgramType.File``: will treat the input source as a stand-alone
                  program and create an operation in the ``qasm_import`` namespace that
                  only takes classical parameters, allocates the required qubits 
                  internally and releases them at the end of the operation.
                - ``ProgramType.Fragments``: executes the provided source in the current
                  interactive interpreter, defining any declared variables or operations
                  in the current scope and returning the value of the last statement in
                  the source.
        :return: The value returned by the last statement in the source code.
        :rtype: Any
        :raises QasmError: If there is an error generating, parsing, or analyzing the
            OpenQASM source.
        :raises QSharpError: If there is an error compiling the program.
        """
        from ._fs import list_directory, read_file, resolve
        from ._http import fetch_github
        from .openqasm._ipython import display_or_print

        ipython_helper()

        telemetry_events.on_import_qasm()
        start_time = monotonic()

        kwargs = {k: v for k, v in kwargs.items() if k is not None and v is not None}
        if "search_path" not in kwargs:
            kwargs["search_path"] = "."

        res = self._interpreter.import_qasm(
            source,
            display_or_print,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            **kwargs,
        )

        durationMs = (monotonic() - start_time) * 1000
        telemetry_events.on_import_qasm_end(durationMs)

        return res
