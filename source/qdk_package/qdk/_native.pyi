# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from enum import Enum
from typing import (
    Any,
    Callable,
    ClassVar,
    Optional,
    Dict,
    Iterator,
    List,
    Sequence,
    Tuple,
    TypedDict,
    Literal,
    Union,
    overload,
)

from .qre import Trace

# pylint: disable=unused-argument
# E302 is fighting with the formatter for number of blank lines
# flake8: noqa: E302

class OutputSemantics(Enum):
    """
    Controls which classical values a compiled OpenQASM program returns.

    The selected mode also controls the semantic checks applied to output
    declarations.
    """

    Qiskit: OutputSemantics
    """
    Returns all classical registers in reverse declaration order, with the
    bits in each register also reversed to match Qiskit conventions.
    """

    OpenQasm: OutputSemantics
    """
    Follows `OpenQASM 3 output semantics
    <https://openqasm.com/language/directives.html#input-output>`_: returns
    variables declared as ``output``, or all declared classical variables when
    the program has no explicit outputs.
    """

    ResourceEstimation: OutputSemantics
    """
    No output semantics are applied. The entry point returns `Unit`.
    """

class ProgramType(Enum):
    """
    Controls how OpenQASM source is introduced into a Q# context.
    """

    File: ProgramType
    """
    Treats the source as a stand-alone program. The generated operation takes
    declared classical inputs as parameters, allocates declared qubits
    internally, and returns declared outputs. It is marked as an entry point
    when it has no input parameters.
    """

    Operation: ProgramType
    """
    Creates a callable operation whose parameters include declared classical
    inputs and qubits, and whose return value contains declared outputs.
    """

    Fragments: ProgramType
    """
    Evaluates the source as interactive fragments, adding its declarations to
    the current scope and returning the value of its final statement.
    """

class TargetProfile(Enum):
    """
    A Q# target profile.

    A target profile describes the capabilities of the hardware or simulator
    which will be used to run the Q# program.
    """

    @classmethod
    def from_str(cls, value: str) -> TargetProfile: ...
    """
    Creates a target profile from a string.
    :param value: The string to parse.
    :raises ValueError: If the string does not match any target profile.
    """

    Base: TargetProfile
    """
    Target supports the minimal set of capabilities required to run a quantum
    program.

    This option maps to the Base Profile as defined by the QIR specification.
    """

    Adaptive_RI: TargetProfile
    """
    Target supports the Adaptive profile with the integer computation extension.

    This profile includes all of the required Adaptive Profile
    capabilities, as well as the optional integer computation
    extension defined by the QIR specification.
    """

    Adaptive_RIF: TargetProfile
    """
    Target supports the Adaptive profile with integer & floating-point
    computation extensions.

    This profile includes all required Adaptive Profile and `Adaptive_RI`
    capabilities, as well as the optional floating-point computation
    extension defined by the QIR specification.
    """

    Adaptive: TargetProfile
    """
    Target supports the Adaptive profile with all supported extensions.

    This profile includes all required Adaptive Profile features and
    all the optional extensions defined by the QIR specification.
    """

    Unrestricted: TargetProfile
    """
    Describes the unrestricted set of capabilities required to run any Q# program.
    """

class GlobalCallable:
    """
    An opaque Q# global handle accepted by its originating interpreter's APIs.
    Retains checked type arguments and an ownership token, not the interpreter itself.
    Python arguments do not instantiate an unbound generic declaration.
    """

    ...

class Closure:
    """
    An opaque Q# closure handle retaining captured values and lexical type arguments.
    It belongs to its originating interpreter. For QIR or static-circuit codegen,
    pass it as an argument to a global wrapper rather than as the entry target.
    """

    ...

class Interpreter:
    """A Q# interpreter.

    Callable handles must originate here. Their instantiated signatures are
    checked before invocation; Python arguments do not specialize an unbound
    generic declaration.
    """

    def __init__(
        self,
        target_profile: TargetProfile,
        language_features: Optional[List[str]],
        project_root: Optional[str],
        read_file: Callable[[str], Tuple[str, str]],
        list_directory: Callable[[str], List[Dict[str, str]]],
        resolve_path: Callable[[str, str], str],
        fetch_github: Callable[[str, str, str, str], str],
        make_callable: Optional[Callable[[GlobalCallable, List[str], str, bool], None]],
        make_class: Optional[Callable[[TypeIR, List[str], str], None]],
        trace_circuit: Optional[bool],
        qdk_config: Optional[Dict[str, int | float | str | bool]] = None,
    ) -> None:
        """
        Initializes the Q# interpreter.

        :param target_profile: The target profile to use for the interpreter.
        :param project_root: A directory that contains a `qsharp.json` manifest.
        :param read_file: A function that reads a file from the file system.
        :param list_directory: A function that lists the contents of a directory.
        :param resolve_path: A function that joins path segments and normalizes the resulting path.
        :param make_callable: A function that registers a Q# callable in the in the environment module.
        :param trace_circuit: Enables tracing of circuit during execution.
            Passing `True` is required for the `dump_circuit` function to return a circuit.
            The `circuit` function is *NOT* affected by this parameter will always generate a circuit.
        :param qdk_config: A dictionary of configuration parameters that will be accessible
            in Q# code using ``Std.Core.ConfigValue``.
        """
        ...

    def interpret(self, input: str, output_fn: Callable[[Output], None]) -> Any:
        """
        Interprets Q# source code.

        :param input: The Q# source code to interpret.
        :param output_fn: A callback function that will be called with each output.

        :returns value: The value returned by the last statement in the input.

        :raises QSharpError: If there is an error interpreting the input.
        """
        ...

    def run(
        self,
        entry_expr: Optional[str],
        output_fn: Optional[Callable[[Output], None]],
        noise_config: Optional[NoiseConfig],
        noise: Optional[Tuple[float, float, float]],
        qubit_loss: Optional[float],
        callable: Optional[GlobalCallable | Closure],
        args: Optional[Any],
        seed: Optional[int],
        sim_type: Optional[Literal["sparse", "clifford"]],
        num_qubits: Optional[int],
    ) -> Any:
        """
        Runs the given Q# expression with an independent instance of the simulator.

        :param entry_expr: The entry expression.
        :param output_fn: A callback function that will be called with each output.
        :param noise_config: The noise configuration to use in simulation.
        :param noise: A tuple with probabilities of Pauli-X, Pauli-Y, and Pauli-Z errors
            to use in simulation as a parametric Pauli noise.
        :param qubit_loss: The probability of qubit loss in simulation.
        :param callable: The callable to run, if no entry expression is provided.
        :param args: The arguments to pass to the callable, if any.
        :param seed: The seed to use for the random number generator in simulation, if any.
        :param sim_type: The type of simulator to use. If not specified, the default sparse state vector simulation will be used.
        :param num_qubits: The number of qubits to use for the simulation type "clifford".
            If not specified, the Clifford simulator assumes a default of 1000 qubits.

        :returns values: A result or runtime errors.

        :raises QSharpError: If there is an error interpreting the input.
        """
        ...

    def invoke(
        self,
        callable: GlobalCallable | Closure,
        args: Any,
        output_fn: Callable[[Output], None],
    ) -> Any:
        """
        Invokes the callable with the given arguments, converted into the appropriate Q# values.
        Callable ownership and instantiated signatures are checked before execution.
        :param callable: The callable to invoke.
        :param args: The arguments to pass to the callable.
        :param output_fn: A callback function that will be called with each output.
        :returns values: A result or runtime errors.
        :raises QSharpError: If there is an error interpreting the input.
        """
        ...

    def qir(
        self,
        entry_expr: Optional[str] = None,
        callable: Optional[GlobalCallable | Closure] = None,
        args: Optional[Any] = None,
    ) -> str:
        """
        Generates QIR from Q# source code. Either an entry expression or a callable with arguments must be provided.
        A callable entry must be a global handle without applied functors. Closures
        can be supplied as arguments. All handles must belong to this interpreter,
        and generic type arguments must already have been established by Q#.

        :param entry_expr: The entry expression.
        :param callable: The callable to generate QIR for, if no entry expression is provided.
        :param args: The arguments to pass to the callable, if any.

        :returns qir: The QIR string.
        """
        ...

    def circuit(
        self,
        config: CircuitConfig,
        entry_expr: Optional[str] = None,
        *,
        operation: Optional[str] = None,
        callable: Optional[GlobalCallable | Closure] = None,
        args: Optional[Any] = None,
    ) -> Circuit:
        """
        Synthesizes a circuit for a Q# program. Either an entry
        expression or an operation must be provided.
        Static generation supports Base, Adaptive_RI, and Adaptive_RIF, and requires
        a global callable entry without applied functors. Closures can be arguments.

        :param config: Circuit generation options.

        :param entry_expr: An entry expression.

        :keyword operation: The operation to synthesize. This can be a name of
            an operation of a lambda expression. The operation must take only
            qubits or arrays of qubits as parameters.

        :keyword callable: The callable to synthesize the circuit for, if no entry expression is provided.

        :keyword args: The arguments to pass to the callable, if any.

        :raises QSharpError: If there is an error synthesizing the circuit.
        """
        ...

    def estimate(
        self,
        params: str,
        entry_expr: Optional[str] = None,
        callable: Optional[GlobalCallable | Closure] = None,
        args: Optional[Any] = None,
    ) -> str:
        """
        Estimates resources for Q# source code.

        :param params: The parameters to configure estimation.
        :param entry_expr: The entry expression to estimate.
        :param callable: The callable to estimate resources for, if no entry expression is provided.
        :param args: The arguments to pass to the callable, if any.

        :returns resources: The estimated resources.
        """
        ...

    def logical_counts(
        self,
        entry_expr: Optional[str] = None,
        callable: Optional[GlobalCallable | Closure] = None,
        args: Optional[Any] = None,
    ) -> Dict[str, int]:
        """
        Estimates logical operation counts for Q# source code.

        :param entry_expr: The entry expression to estimate.
        :param callable: The callable to estimate resources for, if no entry expression is provided.
        :param args: The arguments to pass to the callable, if any.

        :returns resources: The logical resources.
        """
        ...

    def trace(
        self,
        entry_expr: Optional[str] = None,
        callable: Optional[GlobalCallable | Closure] = None,
        args: Optional[Any] = None,
    ) -> Trace:
        """
        Builds a QRE trace for Q# source code.

        :param entry_expr: The entry expression to run.
        :param callable: The callable to run, if no entry expression is provided.
        :param args: The arguments to pass to the callable, if any.

        :returns trace: The resource-estimation trace.
        """
        ...

    def set_quantum_seed(self, seed: Optional[int]) -> None:
        """
        Sets the seed for the quantum random number generator.

        :param seed: The seed to use for the quantum random number generator. If None,
            the seed will be generated from entropy.
        """
        ...

    def set_classical_seed(self, seed: Optional[int]) -> None:
        """
        Sets the seed for the classical random number generator.

        :param seed: The seed to use for the classical random number generator. If None,
            the seed will be generated from entropy.
        """
        ...

    def dump_machine(self) -> StateDumpData:
        """
        Returns the sparse state vector of the simulator as a StateDump object.

        :return: The state of the simulator.
        """
        ...

    def dump_circuit(self) -> Circuit:
        """
        Dumps a circuit showing the current state of the simulator.

        This circuit will contain the gates that have been applied
        in the simulator up to the current point.

        Requires the interpreter to be initialized with `trace_circuit=True`.

        :raises QSharpError: If the interpreter was not initialized with ``trace_circuit=True``.
        """
        ...

    def import_qasm(
        self,
        source: str,
        output_fn: Callable[[Output], None],
        read_file: Callable[[str], Tuple[str, str]],
        list_directory: Callable[[str], List[Dict[str, str]]],
        resolve_path: Callable[[str, str], str],
        fetch_github: Callable[[str, str, str, str], str],
        **kwargs: Any,
    ) -> Any:
        """
        Imports OpenQASM source code into the active Q# interpreter.

        :param source: An OpenQASM program or fragment.
        :param output_fn: The function to handle the output of the execution.
        :param read_file: A callable that reads a file and returns its content and path.
        :param list_directory: A callable that lists the contents of a directory.
        :param resolve_path: A callable that resolves a file path given a base path and a relative path.
        :param fetch_github: A callable that fetches a file from GitHub.
        :param **kwargs: Common options:

          - ``name`` (str): The name of the program.
          - ``search_path`` (str): The optional search path for resolving file references.
          - ``output_semantics`` (OutputSemantics): The output semantics for the compilation.
          - ``program_type`` (ProgramType): The type of program compilation to perform.
        :return: The value returned by the last statement in the source code.
        :raises QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
        :raises QSharpError: If there is an error compiling or evaluating the program.
        """
        ...

class Result(Enum):
    """
    A Q# measurement result.
    """

    Zero: int
    One: int
    Loss: int

class Pauli(Enum):
    """
    A Q# Pauli operator.
    """

    I: int
    X: int
    Y: int
    Z: int

class Output:
    """
    An output returned from the Q# interpreter.
    Outputs can be a state dumps or messages. These are normally printed to the console.
    """

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def _repr_markdown_(self) -> Optional[str]: ...
    def state_dump(self) -> Optional[StateDumpData]: ...
    def is_state_dump(self) -> bool: ...
    def is_matrix(self) -> bool: ...
    def is_message(self) -> bool: ...

class StateDumpData:
    """
    A state dump returned from the Q# interpreter.
    """

    """
    The number of allocated qubits at the time of the dump.
    """
    qubit_count: int

    """
    Get the amplitudes of the state vector as a dictionary from state integer to
    complex amplitudes.
    """
    def get_dict(self) -> dict: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def _repr_markdown_(self) -> str: ...
    def _repr_latex_(self) -> Optional[str]: ...

class CircuitConfig:
    """
    Configuration options for circuit generation.
    """

    def __init__(
        self,
        *,
        max_operations: Optional[int] = None,
        max_loop_iterations: Optional[int] = None,
        generation_method: Optional["CircuitGenerationMethod"] = None,
        source_locations: bool = False,
        group_by_scope: bool = False,
        prune_classical_qubits: bool = False,
    ) -> None: ...

    max_operations: Optional[int]
    """
    The maximum number of operations to include in the generated circuit.
    """

    max_loop_iterations: Optional[int]
    """
    The maximum number of loop iterations to render in full, or ``None`` to disable
    truncation.
    """

    generation_method: Optional[CircuitGenerationMethod]
    """
    The method to use for circuit generation.
    """

    source_locations: Optional[bool]
    """
    Whether to include source locations in the generated circuit.
    """

class CircuitGenerationMethod(Enum):
    """
    The method to use for circuit generation.
    """

    ClassicalEval: CircuitGenerationMethod
    """
    Use classical evaluation to generate the circuit.
    """

    Simulate: CircuitGenerationMethod
    """
    Use simulation to generate the circuit.
    """

    Static: CircuitGenerationMethod
    """
    Compile the program and transform to a circuit using partial evaluation.
    Only works for AdaptiveRIF-compliant programs.
    Requires a non-Unrestricted target profile (e.g. TargetProfile.Adaptive_RIF).
    """

class Circuit:
    """
    A quantum circuit diagram generated from a Q# or OpenQASM program.

    Returned by :func:`qsharp.circuit` and :func:`qsharp.dump_circuit`.
    """

    def json(self) -> str: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class QSharpError(BaseException):
    """
    An error returned from the Q# interpreter.
    """

    ...

class QasmError(BaseException):
    """
    An error raised while parsing, analyzing, compiling, or running OpenQASM.
    """

    ...

class StimError(BaseException):
    """
    EXPERIMENTAL:

    An error returned from the Stim compiler.
    """

    ...

def physical_estimates(logical_resources: str, params: str) -> str:
    """
    Estimates physical resources from pre-calculated logical resources.

    :param logical_resources: The logical resources to estimate from.
    :param params: The parameters to configure physical estimation.

    :return: The estimated resources.
    :rtype: str
    """
    ...

def compile_visual_circuit_to_qsharp(
    file_name: str,
    contents: str,
    index: int,
    program_type: ProgramType,
) -> Tuple[str, str]:
    """
    Converts a visual circuit file to Q# source.

    .. note::
        This call is not intended to be used directly by the user.
        It is intended to be used by the Python wrapper which will handle
        file loading and callable registration.

    :param file_name: The base name to use for the generated operation.
    :param contents: The visual circuit JSON contents.
    :param index: The circuit index to import in file mode.
    :param program_type: The type of Q# source to generate.
    :return: The sanitized operation name and generated Q# source.
    :rtype: Tuple[str, str]
    """
    ...

def circuit_qasm_program(
    source: str,
    config: CircuitConfig,
    read_file: Callable[[str], Tuple[str, str]],
    list_directory: Callable[[str], List[Dict[str, str]]],
    resolve_path: Callable[[str, str], str],
    fetch_github: Callable[[str, str, str, str], str],
    **kwargs: Any,
) -> Circuit:
    """
    Synthesizes a circuit for an OpenQASM program.

    .. note::
        This call while exported is not intended to be used directly by the user.
        It is intended to be used by the Python wrapper which will handle the
        callbacks and other Python specific details.

    :param source: An OpenQASM program.
    :param config: Circuit generation options.
    :param read_file: A callable that reads a file and returns its content and path.
    :param list_directory: A callable that lists the contents of a directory.
    :param resolve_path: A callable that resolves a file path given a base path and a relative path.
    :param fetch_github: A callable that fetches a file from GitHub.
    :param **kwargs: Common options:

      - ``name`` (str): The name of the program.
      - ``search_path`` (str): The optional search path for resolving file references.
    :return: The synthesized circuit.
    :rtype: Circuit
    :raises QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
    :raises QSharpError: If there is an error evaluating or synthesizing the circuit.
    """
    ...

def compile_qasm_program_to_qir(
    source: str,
    read_file: Callable[[str], Tuple[str, str]],
    list_directory: Callable[[str], List[Dict[str, str]]],
    resolve_path: Callable[[str, str], str],
    fetch_github: Callable[[str, str, str, str], str],
    **kwargs: Any,
) -> str:
    """
    Compiles the OpenQASM source code into a program that can be submitted to a
    target as QIR (Quantum Intermediate Representation).

    .. note::
        This call while exported is not intended to be used directly by the user.
        It is intended to be used by the Python wrapper which will handle the
        callbacks and other Python specific details.

    :param source: The OpenQASM source code to compile to QIR.
    :param read_file: A callable that reads a file and returns its content and path.
    :param list_directory: A callable that lists the contents of a directory.
    :param resolve_path: A callable that resolves a file path given a base path and a relative path.
    :param fetch_github: A callable that fetches a file from GitHub.
    :param **kwargs: Common options:

      - ``name`` (str): The name of the circuit.
      - ``target_profile`` (TargetProfile): The target profile to use for code generation.
      - ``search_path`` (str): The optional search path for resolving file references.
      - ``output_semantics`` (OutputSemantics): The output semantics for the compilation.
    :return: The converted QIR code as a string.
    :rtype: str
    :raises QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
    :raises QSharpError: If there is an error compiling the program.
    """
    ...

def compile_qasm_to_qsharp(
    source: str,
    read_file: Callable[[str], Tuple[str, str]],
    list_directory: Callable[[str], List[Dict[str, str]]],
    resolve_path: Callable[[str, str], str],
    fetch_github: Callable[[str, str, str, str], str],
    **kwargs: Any,
) -> str:
    """
    Converts a OpenQASM program to Q#.

    .. note::
        This call while exported is not intended to be used directly by the user.
        It is intended to be used by the Python wrapper which will handle the
        callbacks and other Python specific details.

    :param source: The OpenQASM source code to convert.
    :param read_file: A callable that reads a file and returns its content and path.
    :param list_directory: A callable that lists the contents of a directory.
    :param resolve_path: A callable that resolves a file path given a base path and a relative path.
    :param fetch_github: A callable that fetches a file from GitHub.
    :param **kwargs: Common options:

      - ``name`` (str): The name of the circuit.
      - ``search_path`` (str): The optional search path for resolving file references.
    :return: The converted Q# code as a string.
    :rtype: str
    """
    ...

def compile_stim_to_qir(
    source: str, noise: Optional[NoiseConfig]
) -> Tuple[str, NoiseConfig]:
    """
    EXPERIMENTAL:

    Converts a Stim program to QIR.

    :param source: The Stim source code to convert.
    :param noise: The noise configuration to use.
    :return: The converted QIR code as a string and the noise configuration.
    :rtype: Tuple[str, NoiseConfig]
    :raises StimError: If there is an error compiling the Stim program.
    """
    ...

def resource_estimate_qasm_program(
    source: str,
    job_params: str,
    read_file: Callable[[str], Tuple[str, str]],
    list_directory: Callable[[str], List[Dict[str, str]]],
    resolve_path: Callable[[str, str], str],
    fetch_github: Callable[[str, str, str, str], str],
    **kwargs: Any,
) -> str:
    """
    Estimates the resource requirements for executing OpenQASM source code.

    .. note::
        This call while exported is not intended to be used directly by the user.
        It is intended to be used by the Python wrapper which will handle the
        callbacks and other Python specific details.

    :param source: The OpenQASM source code to estimate resource requirements for.
    :param job_params: The parameters for the job as a JSON string.
    :param read_file: A callable that reads a file and returns its content and path.
    :param list_directory: A callable that lists the contents of a directory.
    :param resolve_path: A callable that resolves a file path given a base path and a relative path.
    :param fetch_github: A callable that fetches a file from GitHub.
    :param **kwargs: Common options:

      - ``name`` (str): The name of the circuit. Defaults to ``'program'``.
      - ``search_path`` (str): The optional search path for resolving imports.
    :return: The estimated resource requirements as a JSON string.
    :rtype: str
    """
    ...

def run_qasm_program(
    source: str,
    output_fn: Callable[[Output], None],
    noise_config: Optional[NoiseConfig],
    noise: Optional[Tuple[float, float, float]],
    qubit_loss: Optional[float],
    read_file: Callable[[str], Tuple[str, str]],
    list_directory: Callable[[str], List[Dict[str, str]]],
    resolve_path: Callable[[str, str], str],
    fetch_github: Callable[[str, str, str, str], str],
    **kwargs: Any,
) -> Any:
    """
    Runs the given OpenQASM program for the given number of shots.
    Each shot uses an independent instance of the simulator.

    .. note::
        This call while exported is not intended to be used directly by the user.
        It is intended to be used by the Python wrapper which will handle the
        callbacks and other Python specific details.

    :param source: The OpenQASM source code to execute.
    :param output_fn: The function to handle the output of the execution.
    :param noise_config: Optional noise configuration for noisy simulation.
    :param noise: Optional Pauli noise as a tuple of ``(x, y, z)`` probabilities.
    :param qubit_loss: The probability of qubit loss in simulation.
    :param read_file: A callable that reads a file and returns its contents.
    :param list_directory: A callable that lists the contents of a directory.
    :param resolve_path: A callable that resolves a path given a base path and a relative path.
    :param fetch_github: A callable that fetches a file from GitHub.
    :param **kwargs: Common options:

      - ``target_profile`` (TargetProfile): The target profile to use for execution.
      - ``name`` (str): The name of the circuit. Defaults to ``'program'``.
      - ``search_path`` (str): The optional search path for resolving imports.
      - ``output_semantics`` (OutputSemantics): The output semantics for the compilation.
      - ``shots`` (int): The number of shots to run. Defaults to ``1``.
      - ``seed`` (int): The seed to use for the random number generator.
    :return: The result of the execution.
    :rtype: Any
    :raises QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
    :raises QSharpError: If there is an error interpreting the input.
    """
    ...

def estimate_custom(
    algorithm: Any,
    qubit: dict,
    qec: Any,
    factories: List = [],
    *,
    error_budget: float = 0.01,
    max_factories: Optional[int] = None,
    logical_depth_factor: Optional[float] = None,
    max_physical_qubits: Optional[int] = None,
    max_duration: Optional[int] = None,
    error_budget_pruning: bool = False,
) -> Dict:
    """
    Estimates quantum resources for a given algorithm, qubit, and code.

    :param algorithm: Python object representing the algorithm.
    :param qubit: The qubit properties as a dictionary.
    :param qec: Python object representing the quantum error correction code.
    :param factories: List of python objects representing factories. Defaults to ``[]``.
    :type factories: List
    :keyword error_budget: The total error budget, which is uniformly distributed. Defaults to ``0.01``.
    :kwtype error_budget: float
    :keyword max_factories: Constrains the number of factories. Defaults to ``None``.
    :kwtype max_factories: int
    :keyword logical_depth_factor: Extends algorithmic logical depth by a factor >= 1. Defaults to ``None``.
    :kwtype logical_depth_factor: float
    :keyword max_physical_qubits: Forces estimator to not exceed provided number of physical qubits, may fail.
        Defaults to ``None``.
    :kwtype max_physical_qubits: int
    :keyword max_duration: Allows estimator to run for given runtime in nanoseconds, may fail.
        Defaults to ``None``.
    :kwtype max_duration: int
    :keyword error_budget_pruning: Will try to prune the error budget to increase magic state error budget.
        Defaults to ``False``.
    :kwtype error_budget_pruning: bool
    :return: A dictionary with resource estimation results.
    :rtype: Dict
    """
    ...

class UdtValue:
    """
    A Q# UDT value. Objects of this class represent UDT values generated
    in Q# and sent to Python. It is then converted into a Python object
    in the `qsharp_value_to_python_value` function in `_qsharp.py`.
    """

    name: str
    fields: List[Tuple[str, Any]]

class TypeIR:
    """
    A Q# type. Objects of this class represent a Q# type. This is used
    to send the definitions of the Q# UDTs defined by the user to Python
    and creating equivalent Python dataclasses in `qsharp.code.*`.
    """

    def kind(self) -> TypeKind: ...
    def unwrap_primitive(self) -> PrimitiveKind: ...
    def unwrap_tuple(self) -> List[TypeIR]: ...
    def unwrap_array(self) -> List[TypeIR]: ...
    def unwrap_udt(self) -> UdtIR: ...

class TypeKind(Enum):
    """
    A Q# type kind.
    """

    Primitive: int
    Tuple: int
    Array: int
    Udt: int

class PrimitiveKind(Enum):
    """
    A Q# primitive.
    """

    Bool: int
    Int: int
    Double: int
    Complex: int
    String: int
    Pauli: int
    Result: int

class UdtIR:
    """
    A Q# Udt.
    """

    name: str
    fields: List[Tuple[str, TypeIR]]

class QirInstructionId(Enum):
    I: QirInstructionId
    H: QirInstructionId
    X: QirInstructionId
    Y: QirInstructionId
    Z: QirInstructionId
    S: QirInstructionId
    SAdj: QirInstructionId
    SX: QirInstructionId
    SXAdj: QirInstructionId
    T: QirInstructionId
    TAdj: QirInstructionId
    CNOT: QirInstructionId
    CX: QirInstructionId
    CY: QirInstructionId
    CZ: QirInstructionId
    CCX: QirInstructionId
    SWAP: QirInstructionId
    RX: QirInstructionId
    RY: QirInstructionId
    RZ: QirInstructionId
    RXX: QirInstructionId
    RYY: QirInstructionId
    RZZ: QirInstructionId
    RESET: QirInstructionId
    M: QirInstructionId
    MResetZ: QirInstructionId
    MZ: QirInstructionId
    Move: QirInstructionId
    ReadResult: QirInstructionId
    ResultRecordOutput: QirInstructionId
    BoolRecordOutput: QirInstructionId
    IntRecordOutput: QirInstructionId
    DoubleRecordOutput: QirInstructionId
    TupleRecordOutput: QirInstructionId
    ArrayRecordOutput: QirInstructionId
    CorrelatedNoise: QirInstructionId
    ReadoutNoise: QirInstructionId

class QirInstruction: ...

class IdleNoiseParams:
    s_probability: float

class LossPolicy(Enum):
    """
    Specifies the behavior of a multi-qubit gate when at least one of its
    qubit operands is lost.
    """

    # If any operand of a gate is lost, skip the gate entirely.
    # This policy can apply to all multi-qubit gates.
    SKIP = 0
    # If any operand of a gate is lost, propagate the loss to the other operands.
    # This policy can apply to all multi-qubit gates.
    PROPAGATE = 1
    # For multi-qubit rotations, degrade the unitary to its single-qubit version
    # on the surviving operand (e.g. rxx -> rx). Falls back to SKIP for gates with
    # no single-qubit reduction (cx, cy, cz, swap, and single-qubit gates).
    # This policy only applies to the rxx, ryy, and rzz gates, in which case
    # they degrade to rx, ry, and rz on the remaining qubit respectively.
    DEGRADE = 2
    # Skip the gate and instead apply an S adjoint to each surviving operand.
    # This policy can apply to all multi-qubit gates.
    RESIDUAL_S_DAGGER = 3
    # This policy only applies to the swap gate, in which case the qubit states
    # are exchanged, including their loss flags.
    APPLY_ANYWAY = 4

class NoiseTable:
    # Deprecated. Setting `loss` distributes the per-qubit loss probability
    # across the correlated loss fault strings ('L' for a single-qubit
    # operation; 'IL', 'LI', and 'LL' for a two-qubit operation), so that it
    # is equivalent to applying loss independently to each qubit. Reading
    # `loss` reconstructs that per-qubit probability. Prefer setting the loss
    # fault strings directly via `set_pauli_noise`.
    loss: float
    on_loss: LossPolicy

    def __init__(self, num_qubits: int):
        """
        Initializes a new noise table for an operation that targets `num_qubits` qubits.
        """

    def __getattr__(self, name: str) -> float:
        """
        Defining __getattr__ allows getting noise like this

        noise_table.ziz

        for arbitrary pauli fields.
        """

    def __setattr__(self, name: str, value: float) -> None:
        """
        Defining __setattr__ allows setting noise like this

        noise_table = NoiseTable(3)
        noise_table.ziz = 0.005

        for arbitrary pauli fields. Setting an element that was
        previously set overrides that entry with the new value.

        In addition to the Pauli characters 'I', 'X', 'Y', 'Z', a string
        may contain 'L' to indicate that the corresponding qubit is lost
        when this entry is sampled. Loss is correlated with the rest of the
        string: the Pauli is applied to the non-lost qubits and the qubits
        marked 'L' are lost (measured and reset). For example, `noise_table.xl`
        applies an X to the first qubit and loses the second.
        """

    @overload
    def set_pauli_noise(self, lst: list[tuple[str, float]]) -> None:
        """
        The correlated pauli noise to use in simulation. Setting an element
        that was previously set overrides that entry with the new value.

        In addition to the Pauli characters 'I', 'X', 'Y', 'Z', a string
        may contain 'L' to indicate that the corresponding qubit is lost
        when this entry is sampled. Loss is correlated with the rest of the
        string: the Pauli is applied to the non-lost qubits and the qubits
        marked 'L' are lost (measured and reset). For example, `noise_table.xl`
        applies an X to the first qubit and loses the second.

        Example::

            noise_table = NoiseTable(2)
            noise_table.set_pauli_noise([("XI", 1e-10), ("XL", 1e-8)])
        """

    @overload
    def set_pauli_noise(self, pauli_strings: list[str], values: list[float]) -> None:
        """
        The correlated pauli noise to use in simulation. Setting an element
        that was previously set overrides that entry with the new value.

        Example::

            noise_table = NoiseTable(2)
            noise_table.set_pauli_noise(["XI", "XZ"], [1e-10, 3.7e-8])
        """

    @overload
    def set_pauli_noise(self, pauli_string: str, value: float) -> None:
        """
        The correlated pauli noise to use in simulation. Setting an element
        that was previously set overrides that entry with the new value.

        Example::

            noise_table = NoiseTable(2)
            noise_table.set_pauli_noise("XZ", 1e-10)
        """

    def set_depolarizing(self, value: float) -> None:
        """
        The depolarizing noise to use in simulation.
        """

    def set_bitflip(self, value: float) -> None:
        """
        The bit flip noise to use in simulation.
        """

    def set_phaseflip(self, value: float) -> None:
        """
        The phase flip noise to use in simulation.
        """

    def is_noiseless(self) -> bool:
        """
        Returns `true` if there is no noise set.
        """

class NoiseIntrinsicsTable:
    def __contains__(self, name: str) -> bool:
        """
        This enables support for `in` membership checks.
        """

    def __getitem__(self, name: str) -> NoiseTable:
        """
        Defining __getitem__ allows getting intrinsic noise tables like this:
            noise_config = NoiseConfig()
            my_intrinsic_noise_table = noise_config.intrinsics["my_intrinsic"]
        """

    def __setitem__(self, name: str, value: float) -> None:
        """
        Defining __setitem__ allows setting intrinsic noise tables like this:
            noise_config = NoiseConfig()
            my_intrinsic_noise_table = NoiseTable(3)
            my_intrinsic_noise_table.ziz = 0.01
            noise_config.intrinsics["my_intrinsic"] = my_intrinsic_noise_table
        """

    def get_intrinsic_id(self, name: str) -> int:
        """
        Each intrinsic inserted in the table is assigned an integer id.
        This method returns that id given an intrinsic's name.
        """

class NoiseConfig:
    x: NoiseTable
    y: NoiseTable
    z: NoiseTable
    h: NoiseTable
    s: NoiseTable
    s_adj: NoiseTable
    t: NoiseTable
    t_adj: NoiseTable
    sx: NoiseTable
    sx_adj: NoiseTable
    rx: NoiseTable
    ry: NoiseTable
    rz: NoiseTable
    cx: NoiseTable
    cy: NoiseTable
    cz: NoiseTable
    rxx: NoiseTable
    ryy: NoiseTable
    rzz: NoiseTable
    # The simulator assumes a `swap` is either a logical swap (relabel) or a
    # physical exchange of the two qubits, so it exchanges their loss state. A
    # `swap` is never treated as an information exchange via three CX gates; that
    # form is decomposed into other instructions before reaching the simulator.
    swap: NoiseTable
    mov: NoiseTable
    mz: NoiseTable
    mresetz: NoiseTable
    # idle: IdleNoiseParams
    intrinsics: NoiseIntrinsicsTable

    def intrinsic(self, name: str, num_qubits: int) -> NoiseTable:
        """
        The noise table for a custom intrinsic.
        """

    def load_csv_dir(self, dir_path: str) -> None:
        """
        Loads noise tables from the specified directory path. For each .csv file found in the directory,
        the noise table is loaded and associated with a unique identifier. The name of the file (without the .csv extension)
        is used as the label for the noise table, which should match the QIR instruction that will apply noise using this table.

        Each line of the table should be of the format: "IXYZ,1.345e-4" where IXYZ is a string of Pauli operators
        representing the error on each qubit (Z applying to the first qubit argument, Y to the second, etc.), and the second value
        is the corresponding error probability for that specific Pauli string.

        Blank lines, lines starting with #, or lines that start with the string "pauli" (i.e., a column header) are ignored.
        """
        ...

def run_clifford(
    input: List[QirInstruction],
    num_qubits: int,
    num_results: int,
    shots: int,
    noise: Optional[NoiseConfig],
    seed: Optional[int],
) -> List[str]:
    """
    Run the given list of QIR instructions in a Clifford simulator,
    using the given `NoiseConfig`, if any.

    Returns a list of result strings. Each result string is composed
    of '0's, '1's, and 'L's, representing if each measurement result
    was a Zero, One, or Loss respectively.
    """
    ...

def run_cpu_full_state(
    input: List[QirInstruction],
    num_qubits: int,
    num_results: int,
    shots: int,
    noise: Optional[NoiseConfig],
    seed: Optional[int],
) -> List[str]:
    """
    Run the given list of QIR instructions in a CPU full-state simulator,
    using the given `NoiseConfig`, if any.

    Returns a list of result strings. Each result string is composed
    of '0's, '1's, and 'L's, representing if each measurement result
    was a Zero, One, or Loss respectively.
    """
    ...

def run_cpu_adaptive(
    input: dict,
    shots: int,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List[str]:
    """
    Run an adaptive profile QIR program on a CPU full-state simulator.

    The input is an `AdaptiveProgram` converted to a dict using the
    .as_dict() method. Uses 64-bit bytecode for full LLVM i64 semantics.

    Returns a list of result strings. Each result string is composed
    of '0's, '1's, and 'L's, representing if each measurement result
    was a Zero, One, or Loss respectively.
    """
    ...

def run_clifford_adaptive(
    input: dict,
    shots: int,
    noise: Optional[NoiseConfig] = None,
    seed: Optional[int] = None,
) -> List[str]:
    """
    Run an adaptive profile QIR program on a Clifford stabilizer simulator.

    The input is an `AdaptiveProgram` converted to a dict using the
    .as_dict() method. Uses 64-bit bytecode for full LLVM i64 semantics.

    Returns a list of result strings. Each result string is composed
    of '0's, '1's, and 'L's, representing if each measurement result
    was a Zero, One, or Loss respectively.
    """
    ...

def try_create_gpu_adapter() -> str:
    """
    Checks if a compatible GPU adapter is available on the system.

    This function attempts to request a GPU adapter to determine if GPU-accelerated
    quantum simulation is supported. It's useful for capability detection before
    attempting to run GPU-based simulations.

    # Errors

    Raises `OSError` if:
    - No compatible GPU is found
    - GPU drivers are missing or not functioning properly
    """
    pass

def run_parallel_shots(
    input: List[QirInstruction],
    qubit_count: int,
    result_count: int,
    shots: int,
    noise: Optional[NoiseConfig],
    seed: Optional[int],
) -> List[str]:
    """ """
    ...

def run_adaptive_parallel_shots(
    input: dict,
    shots: int,
    noise: Optional[NoiseConfig],
    seed: Optional[int],
) -> List[str]:
    """
    Run the given list of QIR instructions in a CPU full-state simulator,
    using the given `NoiseConfig`, if any.

    The input is an `AdaptiveProgram` converted to a dict using the
    .as_dict() method.

    Returns a list of result strings. Each result string is composed
    of '0's, '1's, and 'L's, representing if each measurement result
    was a Zero, One, or Loss respectively.
    """
    ...

# This is a little clunky, but until we move to Python 3.11 as a minimum, the NotRequired annotation
# for Dict fields that may be missing is not availalble. See https://peps.python.org/pep-0655/#motivation
class _GpuShotResultsBase(TypedDict):
    shot_results: List[object]
    """Processed output for each shot, or `None` when a shot fails."""

    shot_result_codes: List[int]
    """Result codes for each shot. 0 = Success, else Failure  (Specific codes are an internal detail)."""

class GpuShotResults(_GpuShotResultsBase, total=False):
    """
    Results from running shots on the GPU simulator.
    """

    shot_output_records: List[List[object]]
    """Per-shot ordered output record values (``Result`` enum values for
    measurement results, plus native ``bool``/``int``/``float`` for classical
    records), in the order they were recorded during the shot."""

    diagnostics: str
    """Diagnostic information if available. (Useful primarly for debugging by the development team)"""

class GpuContext:
    def load_noise_tables(self, dir_path: str) -> List[Tuple[int, str, int]]:
        """
        Loads noise tables from the specified directory path. For each .csv file found in the directory,
        the noise table is loaded and associated with a unique identifier. The name of the file (without the .csv extension)
        is used as the label for the noise table, which should match the QIR instruction that will apply noise using this table.

        Each line of the table should be for the format: "IXYZ,1.345e-4" where IXYZ is a string of Pauli operators
        representing the error on each qubit (Z applying to the first qubit argument, Y to the second, etc.), and the second value
        is the corresponding error probability for that specific Pauli string.

        Blank lines, lines starting with #, or lines that start with the string "pauli" (i.e., a column header) are ignored.
        """
        ...

    def get_noise_table_ids(self) -> List[Tuple[int, str, int]]:
        """
        Retrieves the currently loaded noise table as a string.
        """
        ...

    def set_program(
        self,
        input: List[QirInstruction],
        qubit_count: int,
        result_count: int,
    ) -> None:
        """
        Sets the QIR program to be executed on the GPU.
        """
        ...

    def set_adaptive_program(self, program: dict) -> None:
        """
        Sets an Adaptive Profile QIR program for GPU execution.

        The program dict contains bytecode instructions, block/function tables,
        quantum op pool, and side tables produced by AdaptiveProfilePass.
        """
        ...

    def set_noise(self, noise: NoiseConfig) -> None:
        """
        Sets the noise configuration for the GPU simulation.
        """
        ...

    def run_shots(self, shot_count: int, seed: int) -> GpuShotResults:
        """
        Runs the specified number of shots of the loaded program on the GPU.
        """
        ...

    def run_adaptive_shots(self, shot_count: int, seed: int) -> GpuShotResults:
        """
        Runs the specified number of shots of the loaded adaptive program on the GPU.
        """
        ...

class QuantumRegister:
    """
    A named, non-overlapping view over a contiguous range of qubit indices.
    """

    def __init__(self, name: str, size: int) -> None: ...
    @property
    def name(self) -> str: ...
    @property
    def start(self) -> int: ...
    @property
    def size(self) -> int: ...
    def __len__(self) -> int: ...
    def __getitem__(self, index: int) -> int:
        """
        Returns the absolute qubit index for a position in this register.
        """
        ...

class ClassicalRegister:
    """
    A named, non-overlapping view over a contiguous range of classical bits.
    """

    def __init__(self, name: str, size: int) -> None: ...
    @property
    def name(self) -> str: ...
    @property
    def start(self) -> int: ...
    @property
    def size(self) -> int: ...
    def __len__(self) -> int: ...
    def __getitem__(self, index: int) -> int: ...

class Parameter:
    """
    A compile-time symbolic value, bound before compilation.

    Parameters belong to the circuit that created them.
    """

    @property
    def name(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def __add__(self, other: "Angle") -> "AngleExpression": ...
    def __radd__(self, other: "Angle") -> "AngleExpression": ...
    def __sub__(self, other: "Angle") -> "AngleExpression": ...
    def __rsub__(self, other: "Angle") -> "AngleExpression": ...
    def __mul__(self, other: "Angle") -> "AngleExpression": ...
    def __rmul__(self, other: "Angle") -> "AngleExpression": ...
    def __truediv__(self, other: "Angle") -> "AngleExpression": ...
    def __rtruediv__(self, other: "Angle") -> "AngleExpression": ...
    def __neg__(self) -> "AngleExpression": ...

class AngleExpression:
    """
    Arithmetic over angles, built from parameters and numbers.

    Resolved before compilation, so an expression never reaches the target as a
    computation and never raises the capability a circuit demands.
    """

    def __add__(self, other: "Angle") -> "AngleExpression": ...
    def __radd__(self, other: "Angle") -> "AngleExpression": ...
    def __sub__(self, other: "Angle") -> "AngleExpression": ...
    def __rsub__(self, other: "Angle") -> "AngleExpression": ...
    def __mul__(self, other: "Angle") -> "AngleExpression": ...
    def __rmul__(self, other: "Angle") -> "AngleExpression": ...
    def __truediv__(self, other: "Angle") -> "AngleExpression": ...
    def __rtruediv__(self, other: "Angle") -> "AngleExpression": ...
    def __neg__(self) -> "AngleExpression": ...

Angle = Union[float, Parameter, AngleExpression]

class LoopVariable:
    """
    A loop index read while the program runs.

    Can be tested in a condition, unlike a Parameter, and cannot stand in for
    an angle.
    """

    @property
    def name(self) -> str: ...

class ConditionView:
    """
    A condition as a readable tree.

    ``kind`` is one of ``literal``, ``clbit``, ``variable``, ``not``, or
    ``binary``. The fields that do not apply to a kind are ``None``.
    """

    @property
    def kind(self) -> str: ...
    @property
    def op(self) -> Optional[str]: ...
    @property
    def clbit(self) -> Optional[int]: ...
    @property
    def name(self) -> Optional[str]: ...
    @property
    def value(self) -> Optional[Union[bool, int, float]]: ...
    @property
    def operands(self) -> List["ConditionView"]: ...

class InstructionView:
    """
    A read-only view of one stored instruction.

    Mutating this object does not change the circuit it came from.
    """

    @property
    def name(self) -> str: ...
    @property
    def qubits(self) -> List[int]: ...
    @property
    def clbits(self) -> List[int]: ...
    @property
    def blocks(self) -> List[int]:
        """
        Blocks a control-flow construct transfers into, empty for anything else.

        Reported as identifiers rather than as their contents, so reading an
        instruction costs the same whatever it contains. Pass one to
        ``QuantumCircuit.block`` to read it.
        """
        ...

    @property
    def condition(self) -> Optional[ConditionView]: ...
    @property
    def values(self) -> List[Union[bool, int, float]]:
        """Values a for loop runs over."""
        ...

    @property
    def loop_index(self) -> Optional[str]:
        """Name of the index a for loop binds."""
        ...

    @property
    def label(self) -> Optional[str]:
        """Label carried by a box."""
        ...

    @property
    def cases(self) -> List[List[Union[bool, int, float]]]:
        """
        Values each case of a switch matches.

        Aligned with the leading entries of ``blocks``, so the first case's
        body is ``blocks[0]``.
        """
        ...

    @property
    def default_block(self) -> Optional[int]:
        """Block a switch takes when no case matches."""
        ...

    @property
    def params(self) -> List[Union[float, Parameter, AngleExpression]]:
        """
        Stored angles. A symbolic angle is reported as its parameter or as the
        arithmetic that produces it.
        """
        ...

class BlockBuilder:
    """
    A control-flow body opened by a builder.

    Use as a context manager. Leaving the body emits the construct, unless the
    body raised, in which case nothing is emitted.
    """

    def __enter__(self) -> "QuantumCircuit": ...
    def __exit__(
        self, exc_type: object, exc_value: object, traceback: object
    ) -> bool: ...
    def otherwise(self) -> "BlockBuilder":
        """
        Opens the alternative branch of a conditional.
        """
        ...

class ForLoopBuilder:
    """
    A for-loop body, which hands its index to the code inside it.

    Separate from BlockBuilder because a `with` statement receives the loop
    index here rather than the circuit.
    """

    @property
    def index(self) -> Union[Parameter, LoopVariable]: ...
    def __enter__(self) -> Union[Parameter, LoopVariable]: ...
    def __exit__(
        self, exc_type: object, exc_value: object, traceback: object
    ) -> bool: ...

class SwitchBuilder:
    """
    A switch, and the cases collected into it.

    Use as a context manager. The construct is emitted once, when the switch
    closes, rather than after each case.
    """

    def __enter__(self) -> "SwitchBuilder": ...
    def __exit__(
        self, exc_type: object, exc_value: object, traceback: object
    ) -> bool: ...
    def case(self, *values: int) -> "SwitchCaseBuilder":
        """
        Opens the body taken when the selector equals any of these values.
        """
        ...

    def default(self) -> "SwitchCaseBuilder":
        """
        Opens the body taken when no case matches.
        """
        ...

class SwitchCaseBuilder:
    """
    One case body of a switch.
    """

    def __enter__(self) -> "QuantumCircuit": ...
    def __exit__(
        self, exc_type: object, exc_value: object, traceback: object
    ) -> bool: ...

class QuantumCircuit:
    """
    A quantum circuit.
    """

    def __init__(self, qubits: int, clbits: int = 0) -> None: ...
    @classmethod
    def from_registers(
        cls, *registers: Union[QuantumRegister, ClassicalRegister], clbits: int = 0
    ) -> "QuantumCircuit":
        """
        Builds a circuit from named registers laid out end to end.

        Registers of each kind are laid out in their own index space, in the
        order given. ``clbits`` adds unnamed classical bits after the named ones.
        """
        ...

    @property
    def num_qubits(self) -> int: ...
    @property
    def num_clbits(self) -> int: ...
    @property
    def global_phase(self) -> float:
        """
        The phase applied to the circuit as a whole, in radians.

        Unobservable when the circuit is run on its own, and observable when it
        is controlled or used as a subroutine. Reduced to one turn when set.
        """
        ...

    @global_phase.setter
    def global_phase(self, phase: float) -> None: ...
    @property
    def quantum_registers(self) -> List[QuantumRegister]: ...
    @property
    def classical_registers(self) -> List[ClassicalRegister]: ...
    def __len__(self) -> int: ...
    def __getitem__(self, index: int) -> InstructionView: ...
    def reserve(self, additional: int) -> "QuantumCircuit": ...
    def i(self, qubit: int) -> "QuantumCircuit": ...
    def x(self, qubit: int) -> "QuantumCircuit": ...
    def y(self, qubit: int) -> "QuantumCircuit": ...
    def z(self, qubit: int) -> "QuantumCircuit": ...
    def h(self, qubit: int) -> "QuantumCircuit": ...
    def s(self, qubit: int) -> "QuantumCircuit": ...
    def sdg(self, qubit: int) -> "QuantumCircuit": ...
    def sx(self, qubit: int) -> "QuantumCircuit": ...
    def sxdg(self, qubit: int) -> "QuantumCircuit": ...
    def t(self, qubit: int) -> "QuantumCircuit": ...
    def tdg(self, qubit: int) -> "QuantumCircuit": ...
    def cx(self, control: int, target: int) -> "QuantumCircuit": ...
    def cy(self, control: int, target: int) -> "QuantumCircuit": ...
    def cz(self, control: int, target: int) -> "QuantumCircuit": ...
    def swap(self, first: int, second: int) -> "QuantumCircuit": ...
    def cswap(self, control: int, first: int, second: int) -> "QuantumCircuit": ...
    def prepare_x(self, qubit: int) -> "QuantumCircuit": ...
    def prepare_y(self, qubit: int) -> "QuantumCircuit": ...
    def prepare_z(self, qubit: int) -> "QuantumCircuit": ...
    def measure_x(self, qubit: int, clbit: int) -> "QuantumCircuit": ...
    def measure_y(self, qubit: int, clbit: int) -> "QuantumCircuit": ...
    def ccx(self, control_a: int, control_b: int, target: int) -> "QuantumCircuit": ...
    def measure(self, qubit: int, clbit: int) -> "QuantumCircuit": ...
    def reset(self, qubit: int) -> "QuantumCircuit": ...
    def barrier(self, qubits: Optional[List[int]] = None) -> "QuantumCircuit": ...
    def delay(self, duration: float, qubit: int, unit: str = "dt") -> "QuantumCircuit":
        """
        Holds a qubit idle for a duration.

        The unit is one of ``dt``, ``ns``, ``us``, ``ms``, or ``s``. ``dt`` is
        the target's own time step, which is what a bare number means.
        """
        ...

    def rx(self, angle: "Angle", qubit: int) -> "QuantumCircuit": ...
    def ry(self, angle: "Angle", qubit: int) -> "QuantumCircuit": ...
    def rz(self, angle: "Angle", qubit: int) -> "QuantumCircuit": ...
    def crx(self, angle: "Angle", control: int, target: int) -> "QuantumCircuit": ...
    def cry(self, angle: "Angle", control: int, target: int) -> "QuantumCircuit": ...
    def crz(self, angle: "Angle", control: int, target: int) -> "QuantumCircuit": ...
    def rxx(self, angle: "Angle", first: int, second: int) -> "QuantumCircuit": ...
    def ryy(self, angle: "Angle", first: int, second: int) -> "QuantumCircuit": ...
    def rzz(self, angle: "Angle", first: int, second: int) -> "QuantumCircuit": ...
    def parameter(self, name: str) -> Parameter:
        """
        Declares a symbolic parameter owned by this circuit.
        """
        ...

    @property
    def parameters(self) -> List[Parameter]:
        """
        Parameters referenced by at least one instruction, in declaration order.

        A declared but unused parameter is omitted.
        """
        ...

    def assign_parameters(
        self, values: List[Tuple[Parameter, float]]
    ) -> "QuantumCircuit":
        """
        Returns a new circuit with every referenced parameter replaced by a value.
        """
        ...

    def if_test(self, subject: Union[int, LoopVariable], value: int) -> BlockBuilder:
        """
        Opens a conditional on a measurement result or a loop index.
        """
        ...

    def while_loop(self, subject: Union[int, LoopVariable], value: int) -> BlockBuilder:
        """
        Opens a loop that repeats while a measurement result holds.
        """
        ...

    def for_loop(
        self,
        values: Sequence[Union[int, float]],
        name: Optional[str] = None,
        runtime: bool = False,
    ) -> ForLoopBuilder:
        """
        Opens a loop over a compile-time known sequence of values.

        The index is handed to the body, so ``with circuit.for_loop(...) as i``
        binds it. By default it is a Parameter, usable as an angle, and the loop
        is unrolled during compilation. Pass ``runtime=True`` for a
        LoopVariable, usable in a condition, which the target can loop over
        itself.
        """
        ...

    def moment(self) -> BlockBuilder:
        """
        Opens a group of instructions that run at the same time.

        Unlike a box, which only groups, a moment asserts simultaneity, so two
        instructions inside it may not touch the same qubit.
        """
        ...

    def box_(self, label: Optional[str] = None) -> BlockBuilder:
        """
        Opens a grouped region of instructions.
        """
        ...

    def switch(self, subject: Union[int, LoopVariable]) -> SwitchBuilder:
        """
        Opens a multi-way branch on a classical bit or a loop variable.

        Use as a context manager. Cases are opened from the builder it yields::

            with circuit.switch(0) as selected:
                with selected.case(0):
                    circuit.x(0)
                with selected.default():
                    circuit.z(0)
        """
        ...

    def break_loop(self) -> "QuantumCircuit":
        """
        Leaves the loop this instruction sits in.

        Refused outside a loop body, because nothing can lower it there.
        """
        ...

    def continue_loop(self) -> "QuantumCircuit":
        """
        Starts the next iteration of the loop this instruction sits in.

        Refused outside a loop body, because nothing can lower it there.
        """
        ...

    def block(self, block: int) -> List[InstructionView]:
        """
        Instructions in a block, addressed by an identifier from a view.

        Reading a construct's branches one at a time keeps them
        distinguishable, which concatenating them would not.
        """
        ...

    def body_of(self, index: int) -> List[InstructionView]:
        """
        Instructions inside a control-flow body, for inspection.
        """
        ...

    def to_diagram(self) -> Circuit:
        """
        Converts the circuit into the diagram type used for display.

        The diagram is a display format rather than a program representation, so
        the conversion is lossy. Classical bit indices appear as operation
        arguments because the diagram addresses measurement results per qubit,
        and control-flow bodies are drawn as a placeholder spanning every qubit
        rather than being expanded.
        """
        ...

    def __str__(self) -> str: ...
    def to_qir(self, target_profile: TargetProfile) -> str:
        """
        Compiles the circuit to QIR for a target profile.

        The profile decides what the circuit is allowed to contain:
        `TargetProfile.Base` takes a straight-line program, and the adaptive
        profiles take measurement feedback.

        Raises ValueError if a control-flow block is still open, or if the
        circuit asks for a capability the profile lacks, naming the construct
        and the capability.
        """
        ...

    def to_qasm(self) -> str:
        """
        Exports the circuit as OpenQASM 3 source.

        Qubits and classical bits are written as one register each, so register
        names given to this circuit do not appear in the output. A program
        containing a switch declares OpenQASM 3.1; everything else declares 3.0.

        Raises ValueError if a control-flow block is still open, or if the
        circuit holds something OpenQASM cannot express.
        """
        ...

    @classmethod
    def from_qasm(cls, source: str) -> "QuantumCircuit":
        """
        Builds a circuit from OpenQASM 3 source.

        Registers are flattened into the circuit's qubit and classical bit
        indices in declaration order, and their names are not carried over.

        Raises ValueError if the source does not parse, or if it uses a
        construct that has no place in a circuit, naming the construct.
        """
        ...

    def to_graph(self) -> "QuantumGraph":
        """
        Lowers the circuit into the graph representation.

        The graph is straight-line. It has no form for control flow, for a
        delay, or for a parameter that is still symbolic.

        Raises ValueError if a control-flow block is still open, or if the
        circuit holds a construct the graph has no form for, naming it.
        """
        ...

    @classmethod
    def from_graph(cls, graph: "QuantumGraph") -> "QuantumCircuit":
        """
        Raises a graph back to a circuit.

        This is the only best-effort crossing in the API. The graph expresses
        programs the circuit has no form for. Registers are not recovered,
        because the graph never held them.

        Raises ValueError if the graph holds an operation the circuit has no
        form for, naming that operation, or if the graph is not shaped like
        anything a lowered circuit produces.
        """
        ...

    @classmethod
    def from_graph_with_origins(cls, graph: "QuantumGraph") -> "RaisedCircuit":
        """
        Raises a graph back to a circuit, reporting what each recovered
        instruction came from.

        Attribution is read from the origin each node has carried since it was
        created, so a pass that removes operations leaves the survivors naming
        the instructions they came from rather than their new positions. A node
        that carries no circuit origin reports nothing rather than a guess.

        Raises the same errors as `from_graph`.
        """
        ...

    def to_bytes(self) -> bytes:
        """
        Saves the circuit as a QDK-native Cap'n Proto envelope.

        Save and restore is exact: every circuit that can be built can be
        written and read back with the same instructions, operands, parameters,
        classical variables, control flow, and registers.

        Raises ValueError if a control-flow block is still open.
        """
        ...

    @classmethod
    def from_bytes(cls, data: bytes) -> "QuantumCircuit":
        """
        Restores a circuit from a binary or S-expression snapshot envelope.

        Raises ValueError if the text is not a saved circuit, if it was written
        by a different schema version, or if it describes a circuit that could
        not have been built.
        """
        ...

    def to_text(self) -> str:
        """Saves the same snapshot model as a readable S-expression envelope."""
        ...

    @classmethod
    def from_text(cls, text: str) -> "QuantumCircuit":
        """Restores a circuit saved by `to_text`, applying the same validation."""
        ...

class OriginKind(Enum):
    """Which kind of circuit construct a graph node came from."""

    Instruction: int
    Qubit: int
    Clbit: int
    GlobalPhase: int

class CircuitOrigin:
    """The circuit construct a raised operation came from."""

    def __init__(self, kind: OriginKind, index: Optional[int] = None) -> None: ...
    @property
    def kind(self) -> OriginKind: ...
    @property
    def index(self) -> Optional[int]:
        """
        The instruction position, qubit index, or classical bit index. The
        global phase has none.
        """
        ...

    def __repr__(self) -> str: ...

class RaisedCircuit:
    """A circuit recovered from a graph, with what each instruction came from."""

    @property
    def circuit(self) -> "QuantumCircuit": ...
    @property
    def origins(self) -> List[Optional[CircuitOrigin]]:
        """One entry per recovered instruction, in program order."""
        ...

    def __repr__(self) -> str: ...

class QuantumGraph:
    """
    A program in the graph representation.

    The handle is read-only. Reading answers with plain records rather than
    live views, so nothing here borrows the graph, and changing a graph means
    building a new one with `GraphBuilder`.
    """

    def to_bytes(self) -> bytes:
        """Saves a QDK-native Cap'n Proto graph envelope."""
        ...

    def to_text(self) -> str:
        """Saves the same graph snapshot as an S-expression envelope."""
        ...

    @classmethod
    def from_bytes(cls, data: bytes) -> "QuantumGraph":
        """Restores either graph envelope format, checking limits, digest and validity."""
        ...

    @classmethod
    def from_text(cls, text: str) -> "QuantumGraph":
        """Restores an S-expression graph envelope with the same checks."""
        ...

    @property
    def regions(self) -> List["RegionView"]: ...
    @property
    def nodes(self) -> List["NodeView"]: ...
    @property
    def ports(self) -> List["PortView"]: ...
    @property
    def edges(self) -> List["EdgeView"]: ...
    def diagnostics(self) -> List[str]:
        """
        Reports what the graph's own verification says about this graph.

        An empty list means the graph satisfies every rule. A graph built by
        hand can fail verification, because the builder checks each edit in
        isolation and the rules are about the whole program.
        """
        ...

    def to_qir(self, target_profile: TargetProfile) -> str:
        """
        Compiles the graph to QIR for a target profile.

        This is a path of its own rather than a raise followed by the circuit's
        compilation, so a program a pass produced reaches QIR whether or not a
        circuit could still express it. The profile names the same thing it
        names on a circuit.

        Raises ValueError naming the operation for a graph outside the
        vocabulary the circuit lowering emits, or naming the construct and the
        capability the profile lacks.
        """
        ...

    def to_qasm(self) -> str:
        """
        Exports the graph as OpenQASM 3 source.

        The graph raises to a circuit first and the circuit's own exporter
        writes it, so this offers nothing the raise does not.

        Raises ValueError naming the operation for a graph outside the
        circuit-expressible subset.
        """
        ...

    def bind_parameters(self, values: Dict[str, float]) -> "QuantumGraph":
        """
        Substitutes values for the parameters this graph's angles name.

        Returns a new graph, so the symbolic program survives and can be bound
        again with the next set of values, which is the shape a variational
        loop needs. Node, port, and edge identity carry across, so a rebind is
        a value update rather than a second lowering.

        A parameter this call does not name survives unbound, and a name no
        angle reads changes nothing.
        """
        ...

    def rewrite(
        self,
        operations: List[str],
        pass_: Callable[["PassMatch"], Optional["PassPatch"]],
        budget: Optional[int] = None,
    ) -> "PassResult":
        """
        Runs a patch-returning pass over the operations it names.

        The pass is called once for each matched node and returns a PassPatch
        or None. Matching and application stay native, so the cost at the
        boundary follows how much the pass changes rather than how large the
        graph is.

        Raises ValueError when a decision cannot be applied, naming the
        operation and the reason.
        """
        ...

    def expand(
        self,
        operations: List[str],
        pass_: Callable[["PassMatch"], Optional[List[str]]],
    ) -> "PassResult":
        """
        Runs a fragment-returning pass over the operations it names.

        The pass returns the sequence of operations that stands in the matched
        node's place, or None. This is separate from `rewrite` because a patch
        cannot create a node, so a decomposition that grows the program has no
        patch form.

        Raises ValueError when a node cannot be stood in for, naming the
        operation and the reason.
        """
        ...

    def __repr__(self) -> str: ...

class RegionKind(Enum):
    """What a region stands for."""

    ModuleOmega: int
    Lambda: int
    Gamma: int
    Theta: int
    Recursion: int
    Handler: int
    Stage: int
    ResidualCfg: int

class PortDirection(Enum):
    """Which way a port faces."""

    Input: int
    Output: int

class PortKind(Enum):
    """What a port carries."""

    Value: int
    Capability: int
    HardEffect: int
    ControlExit: int

class PortStage(Enum):
    """When a port's value is known."""

    Generation: int
    Residual: int

class PortUsage(Enum):
    """How many times a port's value may be used."""

    Copyable: int
    Affine: int
    Linear: int

class EdgeKind(Enum):
    """What an edge carries between two ports."""

    CopyableValue: int
    AffineCapability: int
    HardEffect: int
    ControlExit: int

class EffectDomain(Enum):
    """What an effect acts on."""

    Memory: int
    Quantum: int
    Result: int
    Allocation: int
    Output: int
    Abort: int
    Host: int
    Noise: int
    Unknown: int

class EffectMode(Enum):
    """What an effect does to what it acts on."""

    Read: int
    Write: int
    Irreversible: int
    Allocate: int
    Release: int
    Emit: int
    Abort: int
    Unknown: int

class InterfaceRole(Enum):
    """What a bound port is to the region boundary it crosses."""

    Argument: int
    Result: int
    Exit: int
    Capture: int
    Carried: int

class ValueType:
    """
    The type a port carries.

    Built rather than named, because three of the forms hold another type.
    """

    @staticmethod
    def unit() -> "ValueType": ...
    @staticmethod
    def bool() -> "ValueType": ...
    @staticmethod
    def int() -> "ValueType": ...
    @staticmethod
    def double() -> "ValueType": ...
    @staticmethod
    def string() -> "ValueType": ...
    @staticmethod
    def result() -> "ValueType": ...
    @staticmethod
    def qubit() -> "ValueType": ...
    @staticmethod
    def unknown() -> "ValueType": ...
    @staticmethod
    def array(element: "ValueType") -> "ValueType": ...
    @staticmethod
    def tuple(elements: List["ValueType"]) -> "ValueType": ...
    @staticmethod
    def location(target: "ValueType") -> "ValueType": ...
    def __repr__(self) -> str: ...

class SymbolId:
    """The source-level item a node came from."""

    def __init__(self, package: str, item: str) -> None: ...
    @property
    def package(self) -> str: ...
    @property
    def item(self) -> str: ...
    def __repr__(self) -> str: ...

class EffectEvent:
    """One observable effect a node performs."""

    def __init__(
        self,
        domain: EffectDomain,
        mode: EffectMode,
        resource: Optional[str] = None,
        observable: bool = True,
    ) -> None: ...
    @property
    def domain(self) -> EffectDomain: ...
    @property
    def mode(self) -> EffectMode: ...
    @property
    def resource(self) -> Optional[str]:
        """The named resource acted on, or None when it is not known."""
        ...

    @property
    def observable(self) -> bool: ...
    def __repr__(self) -> str: ...

class InterfaceBinding:
    """A child region port bound to the parent port it crosses to."""

    def __init__(self, outer: int, inner: int, role: InterfaceRole) -> None: ...
    @property
    def outer(self) -> int: ...
    @property
    def inner(self) -> int: ...
    @property
    def role(self) -> InterfaceRole: ...
    def __repr__(self) -> str: ...

class RegionView:
    """A region as the graph holds it."""

    @property
    def id(self) -> int: ...
    @property
    def parent(self) -> Optional[int]:
        """The enclosing region, or None for the outermost one."""
        ...

    @property
    def kind(self) -> RegionKind: ...
    @property
    def owner(self) -> Optional[int]:
        """The node this region is a body of, when one declared it."""
        ...

    def __repr__(self) -> str: ...

class AngleOp(Enum):
    """Arithmetic available in a symbolic angle."""

    Add = ...
    Sub = ...
    Mul = ...
    Div = ...

class NodeAngle:
    """
    An angle a node carries, which may still name a parameter.

    A parameter is named rather than indexed, so an angle read from one graph
    can be written to another without the circuit that produced either. Named
    apart from `Angle`, which is what a gate accepts as an argument rather than
    what a node holds.
    """

    @staticmethod
    def literal(value: float) -> NodeAngle: ...
    @staticmethod
    def parameter(name: str) -> NodeAngle: ...
    @staticmethod
    def negate(operand: NodeAngle) -> NodeAngle: ...
    @staticmethod
    def binary(op: AngleOp, left: NodeAngle, right: NodeAngle) -> NodeAngle: ...
    @property
    def value(self) -> Optional[float]:
        """The fixed value, or None while this angle still names a parameter."""
        ...

    @property
    def parameters(self) -> List[str]:
        """Every parameter this angle reads, in first-seen order."""
        ...

    def __repr__(self) -> str: ...

class NodeView:
    """A node as the graph holds it."""

    @property
    def id(self) -> int: ...
    @property
    def region(self) -> int: ...
    @property
    def operation(self) -> str: ...
    @property
    def inputs(self) -> List[int]: ...
    @property
    def outputs(self) -> List[int]: ...
    @property
    def effects(self) -> List[EffectEvent]: ...
    @property
    def symbol(self) -> Optional[SymbolId]:
        """The source-level item this node came from, when one was declared."""
        ...

    @property
    def number(self) -> Optional[float]:
        """The real constant this node carries, when it carries one."""
        ...

    @property
    def classical_bit(self) -> Optional[int]:
        """The classical bit this node targets, when it targets one."""
        ...

    @property
    def angle(self) -> Optional[NodeAngle]:
        """The angle this node carries, when it still names a parameter."""
        ...

    @property
    def origin(self) -> Optional[CircuitOrigin]:
        """The construct this node came from, when a producer recorded one."""
        ...

    def __repr__(self) -> str: ...

class PortView:
    """A port as the graph holds it."""

    @property
    def id(self) -> int: ...
    @property
    def node(self) -> int: ...
    @property
    def region(self) -> int: ...
    @property
    def direction(self) -> PortDirection: ...
    @property
    def kind(self) -> PortKind: ...
    @property
    def value_type(self) -> ValueType: ...
    @property
    def stage(self) -> PortStage: ...
    @property
    def usage(self) -> PortUsage: ...
    def __repr__(self) -> str: ...

class EdgeView:
    """An edge as the graph holds it."""

    @property
    def source(self) -> int: ...
    @property
    def target(self) -> int: ...
    @property
    def kind(self) -> EdgeKind: ...
    def __repr__(self) -> str: ...

class GraphBuilder:
    """
    Builds a graph region by region, node by node, port by port, and edge by
    edge.

    Each call is checked as it is made, so an edge between ports of different
    types is refused where it is written rather than at the end. Rules about
    the whole program are not checked here, which is what
    `QuantumGraph.diagnostics` reports.
    """

    def __init__(self) -> None: ...
    def add_region(self, kind: RegionKind, parent: Optional[int] = None) -> int:
        """Adds a region, optionally inside another one."""
        ...

    def add_node(
        self,
        region: int,
        operation: str,
        effects: Optional[List[EffectEvent]] = None,
    ) -> int:
        """
        Adds a node to a region, named by the operation it performs.

        Raises ValueError if the region does not exist.
        """
        ...

    def add_port(
        self,
        node: int,
        direction: PortDirection,
        kind: PortKind,
        value_type: ValueType,
        stage: PortStage,
        usage: PortUsage,
    ) -> int:
        """
        Adds a port to a node.

        Raises ValueError if the node does not exist.
        """
        ...

    def connect(self, source: int, target: int, kind: EdgeKind) -> None:
        """
        Connects an output port to an input port.

        Raises ValueError if either port is unknown or the edge violates the
        kind, type, region, or stage rules.
        """
        ...

    def declare_symbol(self, node: int, symbol: SymbolId) -> None:
        """
        Declares which source-level item a node came from.

        Raises ValueError if the node does not exist.
        """
        ...

    def declare_payload(
        self,
        node: int,
        number: Optional[float] = None,
        classical_bit: Optional[int] = None,
        angle: Optional[NodeAngle] = None,
        origin: Optional[CircuitOrigin] = None,
    ) -> None:
        """
        Declares the value a node carries and the construct it came from.

        A pass that rebuilds a graph calls this to carry forward what each node
        held, since a node's value and origin are not recoverable from its name.

        Raises ValueError if the node does not exist, if more than one value is
        given, or if an origin kind that needs an index has none.
        """
        ...

    def bind_interface(self, region: int, binding: InterfaceBinding) -> None:
        """
        Binds a port inside a region to the parent port it crosses to.

        Raises ValueError if either port is unknown or the binding does not
        cross the declared parent boundary with a matching type.
        """
        ...

    def declare_region_owner(self, region: int, node: int) -> None:
        """
        Declares the node a region is a body of.

        Branch regions are siblings under one parent, so nesting alone cannot
        say which node they belong to or which predicate selects between them.

        Raises ValueError if the region or the node is unknown.
        """
        ...

    def demand(self, node: int) -> None:
        """Marks a node as demanded, so that analysis treats it as needed."""
        ...

    def finish(self) -> QuantumGraph:
        """
        Produces the graph, leaving the builder spent.

        Raises ValueError if this builder has already produced a graph.
        """
        ...

class PassStage(Enum):
    """What the program is at an anchor."""

    Circuit: int
    Graph: int

class PassAnchor(Enum):
    """
    Where a pass runs in the progression from a circuit to generated code.

    See PASS-EXTENSION-POINTS.md for what each anchor sees and why it exists
    separately from the others.
    """

    CircuitBuilt: int
    GraphLowered: int
    GraphRewritten: int
    CircuitRaised: int

    @staticmethod
    def all() -> List["PassAnchor"]:
        """Every anchor, in the order the progression reaches them."""
        ...

    @property
    def name(self) -> str:
        """The stable name of the anchor."""
        ...

    @property
    def stage(self) -> PassStage:
        """What the program is when a pass at this anchor runs."""
        ...

    @property
    def outbound(self) -> bool:
        """Whether the anchor is reached going out of the graph."""
        ...

class PassMatch:
    """
    A node a pass matched.

    Plain data rather than a view, because a pass runs while the graph is not
    borrowed and a live view could not survive that.
    """

    @property
    def node(self) -> int: ...
    @property
    def operation(self) -> str: ...
    @property
    def operands(self) -> int: ...
    def __repr__(self) -> str: ...

class PassPatch:
    """
    What a pass asks to happen at a matched node.

    Edges are not exposed, because an edge in this graph names an operand
    rather than a node.
    """

    @staticmethod
    def replace(operation: str) -> "PassPatch":
        """Keeps the node and its operands, changes what it is."""
        ...

    @staticmethod
    def remove() -> "PassPatch":
        """Retires the node and splices each operand thread past it."""
        ...

    def __repr__(self) -> str: ...

class PassResult:
    """What a pass run produced."""

    @property
    def graph(self) -> QuantumGraph: ...
    @property
    def outcome(self) -> str:
        """unchanged, committed, budget_exhausted, or oscillation."""
        ...

    @property
    def applications(self) -> int: ...
    @property
    def crossings(self) -> int:
        """How many times the pass was consulted, which is one per match."""
        ...

    def __repr__(self) -> str: ...

class PassListing:
    """One registered pass, as the pipeline lists it."""

    @property
    def anchor(self) -> PassAnchor: ...
    @property
    def name(self) -> str: ...
    @property
    def built_in(self) -> bool:
        """Whether the pass came with the compiler rather than from a caller."""
        ...

    def __repr__(self) -> str: ...

class PipelineResult:
    """What a pipeline run produced."""

    @property
    def circuit(self) -> QuantumCircuit:
        """The circuit that came back out, after the outbound anchor."""
        ...

    @property
    def graph(self) -> QuantumGraph:
        """The graph as it stood when the last graph anchor finished with it."""
        ...

    @property
    def executed(self) -> List[PassListing]:
        """The passes that ran, in the order they ran."""
        ...

    @property
    def changed(self) -> List[str]:
        """The passes that reported changing the program, in run order."""
        ...

    @property
    def diagnostics(self) -> List[Tuple[str, str]]:
        """What the passes reported that was not a failure, as (pass, message)."""
        ...

    @property
    def analyses_computed(self) -> int:
        """
        How many analyses the run computed rather than served from the cache one
        manager keeps for the whole run.
        """
        ...

    @property
    def crossings(self) -> int:
        """How many times a pass was consulted, totalled over the run."""
        ...

    def __repr__(self) -> str: ...

class PassPipeline:
    """
    Passes to run at the named anchors, in one declared order.

    Adding a pass to an anchor is the only edit. Nothing reorders, replaces, or
    removes what is already registered.
    """

    def __init__(
        self, *, built_ins: bool = True, optimization_level: str = "default"
    ) -> None:
        """
        Builds a pipeline, carrying the compiler's own passes unless declined.

        `optimization_level` is one of "none", "default", or "aggressive", and
        every pass in the pipeline reads the same one.
        """
        ...

    def add_circuit_pass(
        self,
        anchor: PassAnchor,
        name: str,
        pass_: Callable[[QuantumCircuit], Optional[QuantumCircuit]],
    ) -> None:
        """
        Registers a pass at an anchor where the program is a circuit.

        Raises ValueError if the anchor is one where the program is a graph.
        """
        ...

    def add_rewrite_pass(
        self,
        anchor: PassAnchor,
        name: str,
        operations: List[str],
        pass_: Callable[[PassMatch], Optional[PassPatch]],
        budget: Optional[int] = None,
    ) -> None:
        """
        Registers a patch-returning pass at an anchor where the program is a graph.

        Raises ValueError if the anchor is one where the program is a circuit.
        """
        ...

    def add_expand_pass(
        self,
        anchor: PassAnchor,
        name: str,
        operations: List[str],
        pass_: Callable[[PassMatch], Optional[List[str]]],
    ) -> None:
        """
        Registers a fragment-returning pass at an anchor where the program is a graph.

        Raises ValueError if the anchor is one where the program is a circuit.
        """
        ...

    def listing(self) -> List[PassListing]:
        """Every registered pass, in the order a run reaches it."""
        ...

    def run(self, circuit: QuantumCircuit) -> PipelineResult:
        """
        Walks a circuit through the whole progression, running each anchor.

        The circuit handed in is not changed. Raises whatever a pass raised, and
        ValueError if the circuit cannot be lowered or the graph cannot be raised.
        """
        ...

    def __repr__(self) -> str: ...
