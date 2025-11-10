# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from abc import ABC, abstractmethod
import datetime
import logging
from time import monotonic
from typing import Dict, Any, List, Optional, Union
from warnings import warn

from qiskit import transpile
from qiskit.circuit import (
    QuantumCircuit,
)
from qiskit.version import get_version_info

from qiskit.qasm3.exporter import Exporter
from qiskit.providers import BackendV2, Options
from qiskit.result import Result
from qiskit.transpiler import PassManager
from qiskit.transpiler.passes import RemoveBarriers, RemoveResetInZeroState
from qiskit.transpiler.target import Target

from .compilation import Compilation
from .errors import Errors
from .qirtarget import QirTarget
from ..jobs import QsJob
from ..passes import RemoveDelays
from .... import TargetProfile

logger = logging.getLogger(__name__)

_QISKIT_NON_GATE_INSTRUCTIONS = [
    "control_flow",
    "if_else",
    "switch_case",
    "while_loop",
    "break",
    "continue",
    "store",
    "for_loop",
    "measure",
    "reset",
]

_QISKIT_STDGATES = [
    "p",
    "x",
    "y",
    "z",
    "h",
    "s",
    "sdg",
    "t",
    "tdg",
    "sx",
    "rx",
    "ry",
    "rz",
    "cx",
    "cy",
    "cz",
    "cp",
    "crx",
    "cry",
    "crz",
    "ch",
    "swap",
    "ccx",
    "cswap",
    "cu",
    "CX",
    "phase",
    "cphase",
    "id",
    "u1",
    "u2",
    "u3",
    "U",
]


def filter_kwargs(func, **kwargs) -> Dict[str, Any]:
    import inspect

    sig = inspect.signature(func)
    supported_args = set(sig.parameters.keys())
    extracted_kwargs = {
        k: kwargs.get(k) for k in list(kwargs.keys()) if k in supported_args
    }
    return extracted_kwargs


def get_transpile_options(**kwargs) -> Dict[str, Any]:
    args = filter_kwargs(transpile, **kwargs)
    return args


def get_exporter_options(**kwargs) -> Dict[str, Any]:
    return filter_kwargs(Exporter.__init__, **kwargs)


class BackendBase(BackendV2, ABC):
    """
    A virtual backend for transpiling to a Q# ecosystem compatible format.
    """

    def __init__(
        self,
        target: Optional[Target] = None,
        qiskit_pass_options: Optional[Dict[str, Any]] = None,
        transpile_options: Optional[Dict[str, Any]] = None,
        qasm_export_options: Optional[Dict[str, Any]] = None,
        skip_transpilation: bool = False,
        **fields,
    ):
        """
        Parameters:
            target (Target): The target to use for the backend.
            qiskit_pass_options (Dict): Options for the Qiskit passes.
            transpile_options (Dict): Options for the transpiler.
            qasm_export_options (Dict): Options for the QASM3 exporter.
            **options: Additional keyword arguments to pass to the
                execution used by subclasses.
        """
        super().__init__(
            name="QSharpBackend",
            description="A virtual BackendV2 for transpiling to a Q# compatible format.",
            backend_version="0.0.1",
        )

        if fields is not None:
            # we need to rename the seed_simulator to seed. This
            # is a convenience for aer users.
            # if the user passes in seed_simulator, we will rename it to seed
            # but only if the seed field is defined in the backend options.
            if "seed_simulator" in fields and "seed" in self._options.data:
                warn("seed_simulator passed, but field is called seed.")
                fields["seed"] = fields.pop("seed_simulator")

            # updates the options with the fields passed in, if the backend
            # doesn't have the field, it will raise an error.
            self.set_options(**fields)

        self._qiskit_pass_options = Options(
            supports_barrier=False,
            supports_delay=False,
            remove_reset_in_zero_state=True,
        )
        self._skip_transpilation = skip_transpilation

        # we need to set the target after the options are set
        # so that the target_profile can be used to determine
        # which gates/instructions are available
        if target is not None:
            # update the properties so that we are internally consistent
            self._qiskit_pass_options.update_options(
                **{
                    "supports_barrier": target.instruction_supported("barrier"),
                    "supports_delay": target.instruction_supported("delay"),
                    "remove_reset_in_zero_state": True,
                }
            )

            self._target = target
        else:
            self._target = self._build_target()

        self._transpile_options = {}

        basis_gates = None
        if qasm_export_options is not None and "basis_gates" in qasm_export_options:
            basis_gates = qasm_export_options.pop("basis_gates")
        else:
            # here we get the gates that are in the target but not in qasm's
            # stdgates so that we can build the basis gates list for the exporter.
            # A user can override this list by passing in a basis_gates list
            # We also remove any non-gate instructions from the list.
            target_gates = set(self.target.operation_names)
            target_gates -= set(_QISKIT_NON_GATE_INSTRUCTIONS)
            target_gates -= set(_QISKIT_STDGATES)
            basis_gates = list(target_gates)

        # set the default options for the exporter
        self._qasm_export_options = {
            "includes": ("stdgates.inc",),
            "alias_classical_registers": False,
            "allow_aliasing": False,
            "disable_constants": True,
            "basis_gates": basis_gates,
        }

        if qiskit_pass_options is not None:
            self._qiskit_pass_options.update_options(**qiskit_pass_options)
        if transpile_options is not None:
            self._transpile_options.update(**transpile_options)
        if qasm_export_options is not None:
            self._qasm_export_options.update(**qasm_export_options)

    def _build_target(self) -> Target:
        supports_barrier = self._qiskit_pass_options["supports_barrier"]
        supports_delay = self._qiskit_pass_options["supports_delay"]

        # explicitly set ``num_qubits`` to ``None`` to indicate a :class:`Target` representing a
        # simulator or other abstract machine that imposes no limits on the number of qubits.
        return QirTarget.build_target(
            num_qubits=None,
            target_profile=self._options["target_profile"],
            supports_barrier=supports_barrier,
            supports_delay=supports_delay,
        )

    @property
    def target(self) -> Target:
        """Returns the target of the Backend object."""
        return self._target

    @property
    def max_circuits(self):
        """
        Returns the maximum number of circuits that can be executed simultaneously.
        """
        return None

    @abstractmethod
    def _execute(self, programs: List[Compilation], **input_params) -> Dict[str, Any]:
        """Execute circuits on the backend.

        Parameters:
            programs (List of QuantumCompilation): simulator input.
            input_params (Dict): configuration for simulation/compilation.

        Returns:
            dict: return a dictionary of results.
        """

    @abstractmethod
    def run(
        self,
        run_input: Union[QuantumCircuit, List[QuantumCircuit]],
        **options,
    ) -> QsJob:
        pass

    def _run(
        self,
        run_input: List[QuantumCircuit],
        **options,
    ) -> QsJob:
        if "name" not in options and len(run_input) == 1:
            options["name"] = run_input[0].name

        # Get out default options
        # Look at all of the kwargs and see if they match any of the options
        # If they do, set the option to the value of the kwarg as an override
        # We only to remove the options that are in the backend options for
        # the run so that other options can be passed to other calls.
        input_params: Dict[str, Any] = vars(self.options).copy()
        input_params.update(options)

        return self._submit_job(run_input, **input_params)

    def run_job(
        self, run_input: List[QuantumCircuit], job_id: str, **options
    ) -> Result:
        start = monotonic()

        compilations = self._compile(run_input, **options)

        output = self._execute(compilations, **options)

        if not isinstance(output, dict):
            logger.error("%s: run failed.", self.name)
            if output:
                logger.error("Output: %s", output)
            from .... import QSharpError

            raise QSharpError(str(Errors.RUN_TERMINATED_WITHOUT_OUTPUT))

        output["job_id"] = job_id
        output["date"] = str(datetime.datetime.now().isoformat())
        output["status"] = "COMPLETED"
        output["backend_name"] = self.name
        output["backend_version"] = self.backend_version

        duration = monotonic() - start
        output["time_taken"] = str(duration)
        output["config"] = {
            "qasm_export_options": str(self._build_qasm_export_options(**options)),
            "qiskit_pass_options": str(self._build_qiskit_pass_options(**options)),
            "transpile_options": str(self._build_transpile_options(**options)),
        }
        output["header"] = {}
        return self._create_results(output)

    @abstractmethod
    def _submit_job(self, run_input: List[QuantumCircuit], **input_params) -> QsJob:
        pass

    def _compile(self, run_input: List[QuantumCircuit], **options) -> List[Compilation]:
        # for each run input, convert to qasm
        compilations = []
        for circuit in run_input:
            args = options.copy()
            assert isinstance(
                circuit, QuantumCircuit
            ), "Input must be a QuantumCircuit."
            start = monotonic()
            qasm = self._qasm(circuit, **args)
            end = monotonic()

            time_taken = end - start
            compilation = Compilation(circuit, qasm, time_taken)
            compilations.append(compilation)
        return compilations

    @abstractmethod
    def _create_results(self, output: Dict[str, Any]) -> Any:
        pass

    def _transpile(self, circuit: QuantumCircuit, **options) -> QuantumCircuit:
        if options.get("skip_transpilation", self._skip_transpilation):
            return circuit

        circuit = self.run_qiskit_passes(circuit, options)

        transpile_options = self._build_transpile_options(**options)
        backend = transpile_options.pop("backend", self)
        target = transpile_options.pop("target", self.target)
        if get_version_info().startswith("1.2"):
            # The older Qiskit version does not support the `qubits_initially_zero` option
            transpiled_circuit = transpile(
                circuit,
                backend=backend,
                target=target,
                **transpile_options,
            )
        else:
            transpiled_circuit = transpile(
                circuit,
                backend=backend,
                target=target,
                qubits_initially_zero=True,
                **transpile_options,
            )
        return transpiled_circuit

    def run_qiskit_passes(self, circuit, options):
        pass_options = self._build_qiskit_pass_options(**options)

        pass_manager = PassManager()
        if not pass_options["supports_barrier"]:
            pass_manager.append(RemoveBarriers())
        if not pass_options["supports_delay"]:
            pass_manager.append(RemoveDelays())
        if pass_options["remove_reset_in_zero_state"]:
            # when doing state initialization, qiskit will reset all qubits to 0
            # As our semantics are different, we can remove these resets
            # as it will double the number of qubits if we have to reset them
            # before using them when using the base profile.
            pass_manager.append(RemoveResetInZeroState())

        circuit = pass_manager.run(circuit)
        return circuit

    def _build_qiskit_pass_options(self, **kwargs) -> Dict[str, Any]:
        params: Dict[str, Any] = vars(self._qiskit_pass_options).copy()
        for opt in params.copy():
            if opt in kwargs:
                params[opt] = kwargs.pop(opt)
        if "supports_barrier" not in params:
            params["supports_barrier"] = False
        if "supports_delay" not in params:
            params["supports_delay"] = False
        if "remove_reset_in_zero_state" not in params:
            params["remove_reset_in_zero_state"] = True

        return params

    def _build_transpile_options(self, **kwargs) -> Dict[str, Any]:
        # create the default options from the backend
        args = self._transpile_options.copy()
        # gather any remaining options that are not in the default list
        transpile_args = get_transpile_options(**kwargs)
        args.update(transpile_args)
        return args

    def _build_qasm_export_options(self, **kwargs) -> Dict[str, Any]:
        # Disable aliasing until we decide want to support it
        # The exporter defaults to only having the U gate.
        # When it sees the stdgates.inc in the default includes list, it adds
        # bodyless symbols for that fixed gate set.
        # We set the basis gates for any gates that we want that wouldn't
        # be defined when stdgates.inc is included.

        # any gates that are not in the stdgates.inc file need to be defined
        # in the basis gates list passed to the exporter. The exporter doesn't
        # know about the gates defined in the backend's target.
        # Anything in the basis_gates gets added to the qasm builder's global
        # namespace as an opaque gate. All parameter information comes from the
        # gate object itself in the circuit.

        # create the default options from the backend
        args = self._qasm_export_options.copy()
        # gather any remaining options that are not in the default list
        exporter_args = get_exporter_options(**kwargs)
        args.update(exporter_args)
        return args

    def transpile(self, circuit: QuantumCircuit, **options) -> QuantumCircuit:
        transpiled_circuit = self._transpile(circuit, **options)
        return transpiled_circuit

    def _qasm(self, circuit: QuantumCircuit, **options) -> str:
        """Converts a Qiskit QuantumCircuit to QASM 3 for the current backend.

        Args:
            circuit (QuantumCircuit): The QuantumCircuit to be executed.
            **options: Additional options for the execution.
              - Any options for the transpiler, exporter, or Qiskit passes
                  configuration. Defaults to backend config values. Common
                  values include: 'optimization_level', 'basis_gates',
                  'includes', 'search_path'.

        Returns:
            str: The converted QASM code as a string. Any supplied includes
            are emitted as include statements at the top of the program.

        :raises QasmError: If there is an error generating or parsing QASM.
        """
        transpiled_circuit = self.transpile(circuit, **options)
        try:
            export_options = self._build_qasm_export_options(**options)
            exporter = Exporter(**export_options)
            qasm3_source = exporter.dumps(transpiled_circuit)
            # Qiskit QASM exporter doesn't handle experimental features correctly and always emits
            # OPENQASM 3.0; even though switch case is not supported in QASM 3.0, so we bump
            # the version to 3.1 for now.
            qasm3_source = qasm3_source.replace("OPENQASM 3.0", "OPENQASM 3.1")
            return qasm3_source
        except Exception as ex:
            from .. import QasmError

            raise QasmError(str(Errors.FAILED_TO_EXPORT_QASM)) from ex

    def _qsharp(self, circuit: QuantumCircuit, **kwargs) -> str:
        """
        Converts a Qiskit QuantumCircuit to Q# for the current backend.

        The generated Q# code will not be idiomatic Q# code, but will be
        a direct translation of the Qiskit circuit.

        Args:
            circuit (QuantumCircuit): The QuantumCircuit to be executed.
            **options: Additional options for the execution. Defaults to backend config values.
              - Any options for the transpiler, exporter, or Qiskit passes
                  configuration. Defaults to backend config values. Common
                  values include: 'optimization_level', 'basis_gates',
                  'includes', 'search_path'.
              - output_semantics (OutputSemantics, optional): The output semantics for the compilation.
        Returns:
            str: The converted QASM code as a string. Any supplied includes
            are emitted as include statements at the top of the program.

        :raises QSharpError: If there is an error evaluating the source code.
        :raises QasmError: If there is an error generating, parsing, or compiling QASM.
        """

        qasm_source = self._qasm(circuit, **kwargs)

        args = {
            "name": kwargs.get("name", circuit.name),
        }

        if search_path := kwargs.pop("search_path", "."):
            args["search_path"] = search_path

        if output_semantics := kwargs.pop(
            "output_semantics", self.options.get("output_semantics", default=None)
        ):
            args["output_semantics"] = output_semantics

        qsharp_source = self._qasm_to_qsharp(qasm_source, **args)
        return qsharp_source

    def qir(
        self,
        circuit: QuantumCircuit,
        **kwargs,
    ) -> str:
        """
        Converts a Qiskit QuantumCircuit to QIR (Quantum Intermediate Representation).

        Args:
            circuit ('QuantumCircuit'): The input Qiskit QuantumCircuit object.
            **kwargs: Additional options for the execution.
              - params (str, optional): The entry expression for the QIR conversion. Defaults to None.
              - target_profile (TargetProfile, optional): The target profile for the backend. Defaults to backend config value.
              - output_semantics (OutputSemantics, optional): The output semantics for the compilation. Defaults to backend config value.
              - search_path (str, optional): The search path for the backend. Defaults to '.'.
        Returns:
            str: The converted QIR code as a string.

        :raises QSharpError: If there is an error evaluating the source code.
        :raises QasmError: If there is an error generating, parsing, or compiling QASM.
        :raises ValueError: If the backend configuration does not support QIR generation.
        """
        name = kwargs.pop("name", circuit.name)
        target_profile = kwargs.pop("target_profile", self.options.target_profile)
        if target_profile == TargetProfile.Unrestricted:
            raise ValueError(str(Errors.UNRESTRICTED_INVALID_QIR_TARGET))

        qasm_source = self._qasm(circuit, **kwargs)

        args = {
            "name": name,
            "target_profile": target_profile,
        }

        if search_path := kwargs.pop("search_path", "."):
            args["search_path"] = search_path

        if params := kwargs.pop("params", None):
            args["params"] = params

        if output_semantics := kwargs.pop(
            "output_semantics", self.options.get("output_semantics", default=None)
        ):
            args["output_semantics"] = output_semantics

        return self._qasm_to_qir(qasm_source, **args)

    def _qasm_to_qir(
        self,
        source: str,
        **kwargs,
    ) -> str:
        from ...._native import compile_qasm_program_to_qir
        from ...._fs import read_file, list_directory, resolve
        from ...._http import fetch_github

        return compile_qasm_program_to_qir(
            source,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            **kwargs,
        )

    def _qasm_to_qsharp(
        self,
        source: str,
        **kwargs,
    ) -> str:
        from ...._native import compile_qasm_to_qsharp
        from ...._fs import read_file, list_directory, resolve
        from ...._http import fetch_github

        return compile_qasm_to_qsharp(
            source,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            **kwargs,
        )
