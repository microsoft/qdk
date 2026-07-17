# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from enum import Enum
from typing import (
    Any,
    Callable,
    Optional,
    Dict,
    Iterator,
    List,
    Tuple,
    TypedDict,
    Literal,
    overload,
)

# pylint: disable=unused-argument
# E302 is fighting with the formatter for number of blank lines
# flake8: noqa: E302

class OutputSemantics(Enum):
    """
    Represents the output semantics for OpenQASM 3 compilation.
    Each has implications on the output of the compilation
    and the semantic checks that are performed.
    """

    Qiskit: OutputSemantics
    """
    The output is in Qiskit format meaning that the output
    is all of the classical registers, in reverse order
    in which they were added to the circuit with each
    bit within each register in reverse order.
    """

    OpenQasm: OutputSemantics
    """
    [OpenQASM 3 has two output modes](https://openqasm.com/language/directives.html#input-output)
    - If the programmer provides one or more `output` declarations, then
        variables described as outputs will be returned as output.
        The spec make no mention of endianness or order of the output.
    - Otherwise, assume all of the declared variables are returned as output.
    """

    ResourceEstimation: OutputSemantics
    """
    No output semantics are applied. The entry point returns `Unit`.
    """

class ProgramType(Enum):
    """
    Represents the type of compilation output to create
    """

    File: ProgramType
    """
    Creates an operation in a namespace as if the program is a standalone
    file. Inputs are lifted to the operation params. Output are lifted to
    the operation return type. The operation is marked as `@EntryPoint`
    as long as there are no input parameters.
    """

    Operation: ProgramType
    """
    Programs are compiled to a standalone function. Inputs are lifted to
    the operation params. Output are lifted to the operation return type.
    """

    Fragments: ProgramType
    """
    Creates a list of statements from the program. This is useful for
    interactive environments where the program is a list of statements
    imported into the current scope.
    This is also useful for testing individual statements compilation.
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
    A callable reference that can be invoked with arguments.
    """

    ...

class Closure:
    """
    A closure reference that can be passed back into Q#.
    """

    ...

class Interpreter:
    """A Q# interpreter."""

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
        qsharp_config: Optional[Dict[str, int | float | str | bool]] = None,
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
        :param qsharp_config: A dictionary of configuration parameters that will be accessible
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
        generation_method: Optional["CircuitGenerationMethod"] = None,
        source_locations: bool = False,
        group_by_scope: bool = False,
        prune_classical_qubits: bool = False,
    ) -> None: ...

    max_operations: Optional[int]
    """
    The maximum number of operations to include in the generated circuit.
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
    An error returned from the OpenQASM parser.
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
    shot_results: List[str]
    """Bit strings for each shot ('0', '1', or 'L' for lost qubits)."""

    shot_result_codes: List[int]
    """Result codes for each shot. 0 = Success, else Failure  (Specific codes are an internal detail)."""

class GpuShotResults(_GpuShotResultsBase, total=False):
    """
    Results from running shots on the GPU simulator.
    """

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

# ---------------------------------------------------------------------------
# qdk.openqasm AST bindings
# ---------------------------------------------------------------------------

class Span:
    """A hashable value representing a half-open UTF-8 byte range."""

    def __init__(self, lo: int, hi: int) -> None: ...
    @property
    def lo(self) -> int: ...
    @property
    def hi(self) -> int: ...
    def __hash__(self) -> int: ...

class PositionEncoding(Enum):
    """The column encoding used by a source position."""

    UTF8: PositionEncoding
    CODE_POINT: PositionEncoding
    UTF16: PositionEncoding
    @property
    def value(self) -> str: ...
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Position:
    """A frozen, hashable zero-based line and column in a source file.

    Raises ``OverflowError`` if ``line`` or ``column`` is negative or greater
    than ``2**32 - 1``.
    """

    def __init__(
        self,
        line: int,
        column: int,
        encoding: PositionEncoding = ...,
    ) -> None: ...
    @property
    def line(self) -> int: ...
    @property
    def column(self) -> int: ...
    @property
    def encoding(self) -> PositionEncoding: ...
    def __hash__(self) -> int: ...

class SourceRange:
    """A frozen, hashable range within one source file.

    Raises ``OverflowError`` if ``source_id`` is negative or greater than
    ``2**32 - 1``.
    """

    def __init__(self, source_id: int, start: Position, end: Position) -> None: ...
    @property
    def source_id(self) -> int: ...
    @property
    def start(self) -> Position: ...
    @property
    def end(self) -> Position: ...
    def __hash__(self) -> int: ...

class SourceFile:
    """One immutable, hashable source file in a parse snapshot."""

    @property
    def id(self) -> int: ...
    @property
    def path(self) -> str: ...
    @property
    def text(self) -> str: ...
    @property
    def span(self) -> Span: ...
    @property
    def is_entry(self) -> bool: ...
    @property
    def is_resolved(self) -> bool: ...
    @property
    def resolution_status(self) -> str: ...
    def __hash__(self) -> int: ...

class SourceMap:
    """An immutable collection of source files in parser pre-order.

    Lines and columns are zero based. Coordinate conversion is strict and
    raises ``ValueError`` rather than clamping invalid boundaries. Source maps
    compare by value and are intentionally unhashable.
    """

    @property
    def entry(self) -> SourceFile: ...
    @property
    def files(self) -> Tuple[SourceFile, ...]: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[SourceFile]: ...
    def get(self, source_id: int) -> SourceFile: ...
    def find(self, path: str) -> Optional[SourceFile]: ...
    def find_all(self, path: str) -> Tuple[SourceFile, ...]: ...
    def position_at(
        self,
        source_id: int,
        byte_offset: int,
        *,
        encoding: PositionEncoding = ...,
    ) -> Position: ...
    def byte_offset(self, source_id: int, position: Position) -> int: ...
    def range_from_span(
        self,
        span: Span,
        *,
        encoding: PositionEncoding = ...,
    ) -> SourceRange: ...
    def span_from_range(self, source_range: SourceRange) -> Span: ...
    __hash__: None

class SourceDocument:
    """The immutable, value-comparable, unhashable sources in one snapshot."""

    @property
    def entry(self) -> SourceFile: ...
    @property
    def source_map(self) -> SourceMap: ...
    __hash__: None

class Severity(Enum):
    """The severity of a :class:`Diagnostic`."""

    Error: Severity
    Warning: Severity
    Advice: Severity
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Label:
    """A frozen, hashable labeled region associated with a diagnostic."""

    @property
    def span(self) -> Span: ...
    @property
    def message(self) -> Optional[str]: ...
    def __hash__(self) -> int: ...

class Diagnostic:
    """A frozen, value-comparable, unhashable diagnostic projection."""

    @property
    def message(self) -> str: ...
    @property
    def severity(self) -> Severity: ...
    @property
    def code(self) -> Optional[str]: ...
    @property
    def labels(self) -> List[Label]: ...
    @property
    def related(self) -> List[Diagnostic]: ...
    def __str__(self) -> str:
        """The pretty, source-annotated rendering of the diagnostic."""
        ...

    def render(
        self,
        *,
        color: Optional[bool] = None,
        unicode: Optional[bool] = None,
        width: Optional[int] = None,
    ) -> str:
        """Render the diagnostic to its pretty, source-annotated form.

        Unlike ``str(diagnostic)`` (a fixed no-color rendering), this lets the
        caller tune the output for the current terminal. ``color`` defaults to
        on only when standard output is a terminal and ``NO_COLOR`` is unset;
        ``unicode`` defaults to ``True``; ``width`` defaults to 80 columns.
        """
        ...

    __hash__: None

class QASMNode:
    """The abstract root of every `OpenQASM` AST node."""

    @property
    def span(self) -> Span: ...

class Expression(QASMNode):
    """The abstract base of every expression node."""

class Statement(QASMNode):
    """The abstract base of every statement node."""

    @property
    def annotations(self) -> List["Annotation"]: ...

class Annotation(QASMNode):
    """An annotation attached to an OpenQASM statement."""

    @property
    def identifier(self) -> str: ...
    @property
    def value(self) -> Optional[str]: ...
    @property
    def value_span(self) -> Optional[Span]: ...
    def children(self) -> List[QASMNode]: ...

# --- syntactic nodes (qdk.openqasm.parser) ---

class Program(QASMNode):
    """The root of a parsed `OpenQASM` program."""

    @property
    def version(self) -> Optional[str]: ...
    @property
    def document(self) -> SourceDocument: ...
    @property
    def statements(self) -> List[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class QuantumGateModifier(QASMNode):
    """A quantum gate modifier (for example ``ctrl @`` or ``pow(2) @``)."""

    @property
    def modifier(self) -> str: ...
    @property
    def argument(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class RangeDefinition(QASMNode):
    @property
    def start(self) -> Optional[Expression]: ...
    @property
    def step(self) -> Optional[Expression]: ...
    @property
    def end(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class DiscreteSet(QASMNode):
    @property
    def values(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class IndexList(QASMNode):
    @property
    def values(self) -> List[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class SwitchCase(QASMNode):
    @property
    def labels(self) -> List[Expression]: ...
    @property
    def body(self) -> List[Statement]: ...
    def children(self) -> List[QASMNode]: ...

class SubroutineParameter(QASMNode):
    @property
    def identifier(self) -> Expression: ...
    @property
    def type_name(self) -> str: ...
    @property
    def type_expressions(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class Identifier(Expression):
    """An identifier expression (a reference to a name)."""

    @property
    def name(self) -> str: ...
    def children(self) -> List[QASMNode]: ...

class IndexedIdentifier(Expression):
    """An indexed identifier (for example ``a[i]``) in an l-value position."""

    @property
    def name(self) -> Identifier: ...
    @property
    def indices(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class HardwareQubit(Expression):
    """A hardware-qubit gate operand (for example ``$0``)."""

    @property
    def name(self) -> str: ...
    def children(self) -> List[QASMNode]: ...

class ErrorExpression(Expression):
    """An expression with invalid syntax that could not be parsed."""

    def children(self) -> List[QASMNode]: ...

class UnaryExpression(Expression):
    """A unary operator expression."""

    @property
    def op(self) -> str: ...
    @property
    def operand(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class BinaryExpression(Expression):
    """A binary operator expression."""

    @property
    def op(self) -> str: ...
    @property
    def lhs(self) -> Expression: ...
    @property
    def rhs(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class LiteralExpression(Expression):
    """A literal expression.

    The ``kind`` names the underlying literal (for example ``"Int"``,
    ``"Float"``, ``"Bool"``, or ``"Array"``). Scalar literals expose a Python
    ``value``; array literals expose their element expressions via ``elements``.
    """

    @property
    def value(self) -> Optional[Any]: ...
    @property
    def elements(self) -> List[Expression]: ...
    @property
    def kind(self) -> str: ...
    def children(self) -> List[QASMNode]: ...

class FunctionCall(Expression):
    """A function-call expression."""

    @property
    def name(self) -> Identifier: ...
    @property
    def args(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class Cast(Expression):
    """A type-cast expression."""

    @property
    def type_name(self) -> str: ...
    @property
    def operand(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class IndexExpression(Expression):
    """An index expression (for example ``a[i]``)."""

    @property
    def collection(self) -> Expression: ...
    @property
    def indices(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class ParenExpression(Expression):
    """A parenthesized expression."""

    @property
    def operand(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class DurationOf(Expression):
    """A ``durationof`` expression over a block of statements."""

    @property
    def body(self) -> List[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class Concatenation(Expression):
    """A concatenation r-value (for example ``a ++ b``)."""

    @property
    def operands(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class QuantumMeasurement(Expression):
    """A measurement r-value (for example ``measure q``)."""

    @property
    def qubits(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class QubitDeclaration(Statement):
    """A qubit declaration statement (for example ``qubit q;``)."""

    @property
    def qubit(self) -> Identifier: ...
    @property
    def size(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class AliasStatement(Statement):
    """An alias declaration statement (``let``)."""

    @property
    def target(self) -> Expression: ...
    @property
    def exprs(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class ClassicalAssignment(Statement):
    """A classical assignment statement (``a = b;``)."""

    @property
    def lhs(self) -> Expression: ...
    @property
    def rhs(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class CompoundAssignment(Statement):
    """A compound assignment statement (for example ``a += b;``)."""

    @property
    def op(self) -> str: ...
    @property
    def lhs(self) -> Expression: ...
    @property
    def rhs(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class QuantumBarrier(Statement):
    """A ``barrier`` statement."""

    @property
    def qubits(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class Box(Statement):
    """A ``box`` statement."""

    @property
    def duration(self) -> Optional[Expression]: ...
    @property
    def body(self) -> List[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class BreakStatement(Statement):
    """A ``break`` statement."""

    def children(self) -> List[QASMNode]: ...

class CompoundStatement(Statement):
    """A block of statements (``{ ... }``)."""

    @property
    def statements(self) -> List[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class CalibrationStatement(Statement):
    """A ``cal`` calibration block."""

    @property
    def body(self) -> str: ...
    def children(self) -> List[QASMNode]: ...

class CalibrationGrammarDeclaration(Statement):
    """A ``defcalgrammar`` declaration."""

    @property
    def name(self) -> str: ...
    def children(self) -> List[QASMNode]: ...

class ClassicalDeclaration(Statement):
    """A classical variable declaration."""

    @property
    def type_name(self) -> str: ...
    @property
    def identifier(self) -> Identifier: ...
    @property
    def init_expr(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class ConstantDeclaration(Statement):
    """A ``const`` declaration."""

    @property
    def type_name(self) -> str: ...
    @property
    def identifier(self) -> Identifier: ...
    @property
    def init_expr(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class ContinueStatement(Statement):
    """A ``continue`` statement."""

    def children(self) -> List[QASMNode]: ...

class SubroutineDefinition(Statement):
    """A ``def`` subroutine definition."""

    @property
    def name(self) -> Identifier: ...
    @property
    def params(self) -> List[Expression]: ...
    @property
    def return_type_name(self) -> Optional[str]: ...
    @property
    def body(self) -> List[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class CalibrationDefinition(Statement):
    """A ``defcal`` calibration definition."""

    @property
    def body(self) -> str: ...
    def children(self) -> List[QASMNode]: ...

class DelayInstruction(Statement):
    """A ``delay`` instruction."""

    @property
    def duration(self) -> Expression: ...
    @property
    def qubits(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class EndStatement(Statement):
    """An ``end`` statement."""

    def children(self) -> List[QASMNode]: ...

class ExpressionStatement(Statement):
    """A statement that evaluates an expression."""

    @property
    def expr(self) -> Expression: ...
    def children(self) -> List[QASMNode]: ...

class ExternDeclaration(Statement):
    """An ``extern`` declaration."""

    @property
    def name(self) -> Identifier: ...
    @property
    def param_type_names(self) -> List[str]: ...
    @property
    def return_type_name(self) -> Optional[str]: ...
    def children(self) -> List[QASMNode]: ...

class ForInLoop(Statement):
    """A ``for`` loop over an iterable set."""

    @property
    def type_name(self) -> str: ...
    @property
    def identifier(self) -> Identifier: ...
    @property
    def iterable(self) -> List[Expression]: ...
    @property
    def body(self) -> QASMNode: ...
    def children(self) -> List[QASMNode]: ...

class BranchingStatement(Statement):
    """An ``if`` / ``else`` branching statement."""

    @property
    def condition(self) -> Expression: ...
    @property
    def if_body(self) -> QASMNode: ...
    @property
    def else_body(self) -> Optional[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class QuantumGate(Statement):
    """A quantum gate call."""

    @property
    def name(self) -> Identifier: ...
    @property
    def modifiers(self) -> List[QuantumGateModifier]: ...
    @property
    def args(self) -> List[Expression]: ...
    @property
    def qubits(self) -> List[Expression]: ...
    @property
    def duration(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class QuantumPhase(Statement):
    """A ``gphase`` statement."""

    @property
    def modifiers(self) -> List[QuantumGateModifier]: ...
    @property
    def args(self) -> List[Expression]: ...
    @property
    def qubits(self) -> List[Expression]: ...
    @property
    def duration(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class Include(Statement):
    """An ``include`` directive."""

    @property
    def filename(self) -> str: ...
    def children(self) -> List[QASMNode]: ...

class IODeclaration(Statement):
    """An ``input`` / ``output`` declaration."""

    @property
    def io_keyword(self) -> str: ...
    @property
    def type_name(self) -> str: ...
    @property
    def identifier(self) -> Identifier: ...
    def children(self) -> List[QASMNode]: ...

class QuantumMeasurementStatement(Statement):
    """A measurement statement (for example ``c = measure q;``)."""

    @property
    def qubits(self) -> List[Expression]: ...
    @property
    def target(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class Pragma(Statement):
    """A ``pragma`` directive."""

    @property
    def name(self) -> Optional[str]: ...
    @property
    def value(self) -> Optional[str]: ...
    def children(self) -> List[QASMNode]: ...

class QuantumGateDefinition(Statement):
    """A ``gate`` definition."""

    @property
    def name(self) -> Identifier: ...
    @property
    def params(self) -> List[str]: ...
    @property
    def qubits(self) -> List[str]: ...
    @property
    def body(self) -> List[QASMNode]: ...
    def children(self) -> List[QASMNode]: ...

class QuantumReset(Statement):
    """A ``reset`` statement."""

    @property
    def qubits(self) -> List[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class ReturnStatement(Statement):
    """A ``return`` statement."""

    @property
    def value(self) -> Optional[Expression]: ...
    def children(self) -> List[QASMNode]: ...

class SwitchStatement(Statement):
    """A ``switch`` statement."""

    @property
    def target(self) -> Expression: ...
    @property
    def cases(self) -> List[SwitchCase]: ...
    @property
    def default(self) -> Optional[List[Statement]]: ...
    def children(self) -> List[QASMNode]: ...

class WhileLoop(Statement):
    """A ``while`` loop."""

    @property
    def condition(self) -> Expression: ...
    @property
    def body(self) -> QASMNode: ...
    def children(self) -> List[QASMNode]: ...

class ErrorStatement(Statement):
    """A statement with invalid syntax that could not be parsed."""

    def children(self) -> List[QASMNode]: ...

class _semantic:
    """Stub for the ``qdk._native._semantic`` native submodule.

    The semantic OpenQASM node classes keep their ``Sem``-prefixed Rust
    identifiers but are exposed to Python under clean, un-prefixed names inside
    this attribute-only submodule (for example ``SemGateCall`` -> ``QuantumGate``).
    Modeling the submodule as a nested class lets pyright resolve
    ``_semantic.<Name>`` from the single ``_native.pyi`` stub without a stub
    package or ``sys.modules`` registration. Sibling references between the
    nested classes are written as qualified strings (for example
    ``"_semantic.Type"``) because pyright does not resolve bare sibling names in
    method annotations.
    """

    class Type:
        """An opaque, read-only view of a resolved semantic type."""

        @property
        def name(self) -> str: ...
        @property
        def is_const(self) -> bool: ...
        @property
        def width(self) -> Optional[int]: ...
        def __str__(self) -> str: ...

    class Symbol:
        """A read-only view of a resolved symbol."""

        @property
        def id(self) -> int: ...
        @property
        def name(self) -> str: ...
        @property
        def span(self) -> Span: ...
        @property
        def ty(self) -> "_semantic.Type": ...
        @property
        def io_kind(self) -> str: ...
        @property
        def const_value(self) -> Optional[Any]: ...

    class SymbolTable:
        """An iterable, read-only projection of the resolved symbol table."""

        def __len__(self) -> int: ...
        def __iter__(self) -> Any: ...
        def get(self, id: int) -> "Optional[_semantic.Symbol]": ...
        def lookup(self, name: str) -> "Optional[_semantic.Symbol]": ...
        def symbols(self) -> "List[_semantic.Symbol]": ...

    class SemanticExpression(Expression):
        """The base of every semantic expression node."""

        @property
        def ty(self) -> "_semantic.Type": ...
        @property
        def const_value(self) -> Optional[Any]: ...
        @property
        def symbol(self) -> "Optional[_semantic.Symbol]": ...

    class SemanticStatement(Statement):
        """The base of every semantic statement node."""

    class Program(QASMNode):
        """The root of a semantic `OpenQASM` program."""

        @property
        def version(self) -> Optional[str]: ...
        @property
        def pragmas(self) -> List[QASMNode]: ...
        @property
        def statements(self) -> List[QASMNode]: ...
        def children(self) -> List[QASMNode]: ...

    class HardwareQubit(Expression):
        """A hardware-qubit gate operand (for example ``$0``)."""

        @property
        def name(self) -> str: ...
        def children(self) -> List[QASMNode]: ...

    class QuantumGateModifier(QASMNode):
        """A semantic quantum gate modifier."""

        @property
        def modifier(self) -> str: ...
        @property
        def argument(self) -> Optional[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class RangeDefinition(QASMNode):
        @property
        def start(self) -> Optional[Expression]: ...
        @property
        def step(self) -> Optional[Expression]: ...
        @property
        def end(self) -> Optional[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class DiscreteSet(QASMNode):
        @property
        def values(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class SwitchCase(QASMNode):
        @property
        def labels(self) -> List[Expression]: ...
        @property
        def body(self) -> List[Statement]: ...
        def children(self) -> List[QASMNode]: ...

    class SubroutineParameter(QASMNode):
        @property
        def name(self) -> Optional[str]: ...
        @property
        def symbol_id(self) -> int: ...
        @property
        def symbol(self) -> "_semantic.Symbol": ...
        @property
        def type_expressions(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    # --- semantic expression nodes ---

    class ErrorExpression(SemanticExpression):
        """An expression that could not be resolved."""

        def children(self) -> List[QASMNode]: ...

    class Identifier(SemanticExpression):
        """A reference to a resolved symbol."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def symbol_id(self) -> int: ...
        def children(self) -> List[QASMNode]: ...

    class CapturedIdentifier(SemanticExpression):
        """A reference to a symbol captured from an enclosing scope."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def symbol_id(self) -> int: ...
        def children(self) -> List[QASMNode]: ...

    class UnaryExpression(SemanticExpression):
        """A unary operator expression."""

        @property
        def op(self) -> str: ...
        @property
        def operand(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class BinaryExpression(SemanticExpression):
        """A binary operator expression."""

        @property
        def op(self) -> str: ...
        @property
        def lhs(self) -> Expression: ...
        @property
        def rhs(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class LiteralExpression(SemanticExpression):
        """A literal expression."""

        @property
        def value(self) -> Optional[Any]: ...
        @property
        def elements(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class FunctionCall(SemanticExpression):
        """A call to a resolved function."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def callee_id(self) -> int: ...
        @property
        def args(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class BuiltinFunctionCall(SemanticExpression):
        """A call to a built-in function."""

        @property
        def name(self) -> str: ...
        @property
        def args(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class Cast(SemanticExpression):
        """A type cast expression."""

        @property
        def operand(self) -> Expression: ...
        @property
        def ty_exprs(self) -> List[Expression]: ...
        @property
        def kind(self) -> str: ...
        def children(self) -> List[QASMNode]: ...

    class IndexExpression(SemanticExpression):
        """An indexing expression."""

        @property
        def collection(self) -> Expression: ...
        @property
        def indices(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class ParenExpression(SemanticExpression):
        """A parenthesized expression."""

        @property
        def operand(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class QuantumMeasurement(SemanticExpression):
        """A measurement expression."""

        @property
        def qubits(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class RuntimeSizeof(SemanticExpression):
        """A runtime ``sizeof`` expression."""

        @property
        def array(self) -> Expression: ...
        @property
        def dimension(self) -> Expression: ...
        @property
        def array_rank(self) -> int: ...
        def children(self) -> List[QASMNode]: ...

    class DurationOf(SemanticExpression):
        """An evaluated ``durationof`` expression."""

        @property
        def body(self) -> List[Statement]: ...
        def children(self) -> List[QASMNode]: ...

    class Concatenation(SemanticExpression):
        """A concatenation expression."""

        @property
        def operands(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    # --- semantic statement nodes ---

    class AliasStatement(SemanticStatement):
        """An alias declaration statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def exprs(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class ClassicalAssignment(SemanticStatement):
        """An assignment statement."""

        @property
        def lhs(self) -> Expression: ...
        @property
        def rhs(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class QuantumBarrier(SemanticStatement):
        """A barrier statement."""

        @property
        def qubits(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class Box(SemanticStatement):
        """A box statement."""

        @property
        def duration(self) -> Optional[Expression]: ...
        @property
        def body(self) -> List[Statement]: ...
        def children(self) -> List[QASMNode]: ...

    class CompoundStatement(SemanticStatement):
        """A block of statements."""

        @property
        def statements(self) -> List[Statement]: ...
        def children(self) -> List[QASMNode]: ...

    class BreakStatement(SemanticStatement):
        """A break statement."""

        def children(self) -> List[QASMNode]: ...

    class CalibrationStatement(SemanticStatement):
        """A calibration statement."""

        @property
        def content(self) -> str: ...
        def children(self) -> List[QASMNode]: ...

    class CalibrationGrammarDeclaration(SemanticStatement):
        """A calibration grammar statement."""

        @property
        def name(self) -> str: ...
        def children(self) -> List[QASMNode]: ...

    class ClassicalDeclaration(SemanticStatement):
        """A classical variable declaration statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def ty_exprs(self) -> List[Expression]: ...
        @property
        def init_expr(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class ContinueStatement(SemanticStatement):
        """A continue statement."""

        def children(self) -> List[QASMNode]: ...

    class SubroutineDefinition(SemanticStatement):
        """A subroutine definition statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def params(self) -> List["_semantic.SubroutineParameter"]: ...
        @property
        def return_types(self) -> List[Expression]: ...
        @property
        def body(self) -> List[Statement]: ...
        def children(self) -> List[QASMNode]: ...

    class CalibrationDefinition(SemanticStatement):
        """A ``defcal`` statement."""

        @property
        def content(self) -> str: ...
        def children(self) -> List[QASMNode]: ...

    class DelayInstruction(SemanticStatement):
        """A delay statement."""

        @property
        def duration(self) -> Expression: ...
        @property
        def qubits(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class EndStatement(SemanticStatement):
        """An end statement."""

        def children(self) -> List[QASMNode]: ...

    class ExpressionStatement(SemanticStatement):
        """An expression statement."""

        @property
        def expr(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class ExternDeclaration(SemanticStatement):
        """An extern declaration statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def ty_exprs(self) -> List[Expression]: ...
        @property
        def return_types(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class ForInLoop(SemanticStatement):
        """A ``for`` loop statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def ty_exprs(self) -> List[Expression]: ...
        @property
        def iterable(self) -> QASMNode: ...
        @property
        def body(self) -> Statement: ...
        def children(self) -> List[QASMNode]: ...

    class QuantumGate(SemanticStatement):
        """A gate call statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def modifiers(self) -> List["_semantic.QuantumGateModifier"]: ...
        @property
        def args(self) -> List[Expression]: ...
        @property
        def qubits(self) -> List[Expression]: ...
        @property
        def duration(self) -> Optional[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class BranchingStatement(SemanticStatement):
        """An ``if`` statement."""

        @property
        def condition(self) -> Expression: ...
        @property
        def then_block(self) -> Statement: ...
        @property
        def else_block(self) -> Optional[Statement]: ...
        def children(self) -> List[QASMNode]: ...

    class Include(SemanticStatement):
        """An include statement."""

        @property
        def filename(self) -> str: ...
        def children(self) -> List[QASMNode]: ...

    class IndexedClassicalAssignment(SemanticStatement):
        """An indexed assignment statement."""

        @property
        def lhs(self) -> Expression: ...
        @property
        def indices(self) -> List[Expression]: ...
        @property
        def rhs(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class InputDeclaration(SemanticStatement):
        """An input declaration statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def ty_exprs(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class OutputDeclaration(SemanticStatement):
        """An output declaration statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def ty_exprs(self) -> List[Expression]: ...
        @property
        def init_expr(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class QuantumMeasurementStatement(SemanticStatement):
        """A measurement-to-target statement."""

        @property
        def qubits(self) -> List[Expression]: ...
        @property
        def target(self) -> Optional[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class Pragma(SemanticStatement):
        """A pragma statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def value(self) -> Optional[str]: ...
        def children(self) -> List[QASMNode]: ...

    class QuantumGateDefinition(SemanticStatement):
        """A quantum gate definition statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def params(self) -> List[str]: ...
        @property
        def qubits(self) -> List[str]: ...
        @property
        def body(self) -> List[Statement]: ...
        def children(self) -> List[QASMNode]: ...

    class QubitDeclaration(SemanticStatement):
        """A qubit declaration statement."""

        @property
        def name(self) -> Optional[str]: ...
        def children(self) -> List[QASMNode]: ...

    class QubitArrayDeclaration(SemanticStatement):
        """A qubit array declaration statement."""

        @property
        def name(self) -> Optional[str]: ...
        @property
        def size(self) -> Expression: ...
        def children(self) -> List[QASMNode]: ...

    class QuantumReset(SemanticStatement):
        """A reset statement."""

        @property
        def qubits(self) -> List[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class ReturnStatement(SemanticStatement):
        """A return statement."""

        @property
        def value(self) -> Optional[Expression]: ...
        def children(self) -> List[QASMNode]: ...

    class SwitchStatement(SemanticStatement):
        """A switch statement."""

        @property
        def target(self) -> Expression: ...
        @property
        def cases(self) -> List["_semantic.SwitchCase"]: ...
        @property
        def default(self) -> Optional[List[Statement]]: ...
        def children(self) -> List[QASMNode]: ...

    class WhileLoop(SemanticStatement):
        """A ``while`` loop statement."""

        @property
        def condition(self) -> Expression: ...
        @property
        def body(self) -> Statement: ...
        def children(self) -> List[QASMNode]: ...

    class ErrorStatement(SemanticStatement):
        """A statement that could not be resolved."""

        def children(self) -> List[QASMNode]: ...

class AnalysisResult:
    """The result of a semantic :func:`analyze`."""

    @property
    def program(self) -> _semantic.Program: ...
    @property
    def symbols(self) -> _semantic.SymbolTable: ...
    @property
    def diagnostics(self) -> List[Diagnostic]: ...
    @property
    def errors(self) -> List[Diagnostic]: ...
    @property
    def has_errors(self) -> bool: ...

def analyze(
    source: str,
    path: str = ...,
    includes: Optional[Any] = ...,
) -> AnalysisResult:
    """Parses and semantically analyzes `OpenQASM` source text."""
    ...

class ParseResult:
    """The result of a syntactic :func:`parse`."""

    @property
    def program(self) -> Program: ...
    @property
    def document(self) -> SourceDocument: ...
    @property
    def diagnostics(self) -> List[Diagnostic]: ...
    @property
    def errors(self) -> List[Diagnostic]: ...
    @property
    def has_errors(self) -> bool: ...

class _QASMUnparseError(ValueError):
    """Internal checked serialization error carrier."""

    code: str
    span: Optional[Span]
    diagnostics: Tuple[Diagnostic, ...]

def parse(
    source: str,
    path: str = ...,
    includes: Optional[Any] = ...,
) -> ParseResult:
    """Parses `OpenQASM` source text into a syntax tree."""
    ...

def qasm_dumps(program: Program) -> str:
    """Canonically serializes a syntactic program from its entry source."""
    ...
