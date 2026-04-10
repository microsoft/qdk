# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from collections import Counter
import logging
from typing import Any, Dict, List, Optional, Tuple, Union
from uuid import uuid4

from qiskit import QuantumCircuit
from qiskit.providers import Options
from qiskit.transpiler.target import Target
from .... import TargetProfile
from .. import OutputSemantics
from ..execution import DetaultExecutor
from ..jobs import QsSimJob
from .backend_base import BackendBase
from .compilation import Compilation
from .errors import Errors

logger = logging.getLogger(__name__)


class QSharpBackend(BackendBase):
    """
    A virtual backend for running Qiskit circuits using the Q# simulator.
    """

    # This init is included for the docstring
    # pylint: disable=useless-parent-delegation
    def __init__(
        self,
        target: Optional[Target] = None,
        qiskit_pass_options: Optional[Dict[str, Any]] = None,
        transpile_options: Optional[Dict[str, Any]] = None,
        qasm_export_options: Optional[Dict[str, Any]] = None,
        skip_transpilation: bool = False,
        **options,
    ):
        """
        :param target: The target to use for the backend.
        :type target: Target, optional
        :param qiskit_pass_options: Options for the Qiskit passes.
        :type qiskit_pass_options: Dict, optional
        :param transpile_options: Options for the transpiler.
        :type transpile_options: Dict, optional
        :param qasm_export_options: Options for the QASM3 exporter.
        :type qasm_export_options: Dict, optional
        :param skip_transpilation: Skip Qiskit transpilation.
        :type skip_transpilation: bool
        :param **options: Default option overrides. These can also be overridden per-call via
            :meth:`run`. Common options:

            - ``name`` (str): The name of the circuit used as the entry point. Defaults to the circuit name.
            - ``target_profile`` (TargetProfile): The target profile to use for the compilation.
            - ``output_semantics`` (OutputSemantics): The output semantics for the compilation.
              Defaults to ``OutputSemantics.Qiskit``.
            - ``shots`` (int): The number of shots to run the program for. Defaults to ``1024``.
            - ``seed`` (int): The seed to use for the random number generator. Defaults to ``None``.
            - ``search_path`` (str): The path to search for imports. Defaults to ``'.'``.
            - ``output_fn`` (Callable): A callback function to receive the output of the circuit.
              Defaults to ``None``.
            - ``executor``: The executor to be used to submit the job. Defaults to ``SynchronousExecutor``.
        """

        super().__init__(
            target,
            qiskit_pass_options,
            transpile_options,
            qasm_export_options,
            skip_transpilation,
            **options,
        )

    @classmethod
    def _default_options(cls):
        return Options(
            name="program",
            params=None,
            search_path=".",
            shots=1024,
            seed=None,
            output_fn=None,
            target_profile=TargetProfile.Unrestricted,
            output_semantics=OutputSemantics.Qiskit,
            executor=DetaultExecutor(),
        )

    def run(
        self,
        run_input: Union[QuantumCircuit, List[QuantumCircuit]],
        **options,
    ) -> QsSimJob:
        """
        Runs the given QuantumCircuit using the Q# simulator.

        :param run_input: The QuantumCircuit to be executed.
        :type run_input: QuantumCircuit
        :param **options: Per-call option overrides. Common options:

            - ``name`` (str): The name of the circuit used as the entry point. Defaults to the circuit name.
            - ``target_profile`` (TargetProfile): The target profile to use for the compilation.
            - ``output_semantics`` (OutputSemantics): The output semantics for the compilation.
              Defaults to ``OutputSemantics.Qiskit``.
            - ``shots`` (int): The number of shots to run the program for. Defaults to ``1024``.
            - ``seed`` (int): The seed to use for the random number generator. Defaults to ``None``.
            - ``search_path`` (str): The path to search for imports. Defaults to ``'.'``.
            - ``output_fn`` (Callable): A callback function to receive the output of the circuit.
              Defaults to ``None``.
            - ``executor``: The executor to be used to submit the job. Defaults to ``SynchronousExecutor``.
        :return: The simulation job.
        :rtype: QsSimJob
        :raises QSharpError: If there is an error evaluating the source code.
        :raises QasmError: If there is an error generating, parsing, or compiling QASM.
        :raises ValueError: If run_input is not a QuantumCircuit or List[QuantumCircuit].
        """

        run_input = self._validate_quantum_circuits(run_input)
        return self._run(run_input, **options)

    def _execute(self, programs: List[Compilation], **input_params) -> Dict[str, Any]:
        exec_results: List[Tuple[Compilation, Dict[str, Any]]] = [
            (
                program,
                _run_qasm(program.qasm, vars(self.options).copy(), **input_params),
            )
            for program in programs
        ]
        job_results = []

        shots = input_params.get("shots")
        if shots is None:
            raise ValueError(str(Errors.MISSING_NUMBER_OF_SHOTS))

        for program, exec_result in exec_results:
            results = [self._shot_to_bitstring(result) for result in exec_result]

            counts = Counter(results)
            counts_dict = dict(counts)
            probabilities = {
                bitstring: (count / shots) for bitstring, count in counts_dict.items()
            }

            job_result = {
                "data": {"counts": counts_dict, "probabilities": probabilities},
                "success": True,
                "header": {
                    "metadata": {"qasm": program.qasm},
                    "name": program.circuit.name,
                    "compilation_time_taken": program.time_taken,
                },
                "shots": shots,
            }
            job_results.append(job_result)

        # All of these fields are required by the Result object
        result_dict = {
            "results": job_results,
            "qobj_id": str(uuid4()),
            "success": True,
        }

        return result_dict


def _run_qasm(
    qasm: str,
    default_options: Options,
    **options,
) -> Any:
    """
    Runs the supplied OpenQASM 3 program.
    Gates defined by stdgates.inc will be overridden with definitions
    from the Q# compiler.

    Any gates, such as matrix unitaries, that are not able to be
    transpiled will result in an error.

    :param source: The input OpenQASM 3 string to be processed.
    :param default_options: Default backend option values.
    :param **options: Common options:

        - ``target_profile`` (TargetProfile): The target profile to use for the compilation.
        - ``output_semantics`` (OutputSemantics): The output semantics for the compilation.
        - ``name`` (str): The name of the circuit. Defaults to ``'program'``.
        - ``search_path`` (str): The optional search path for resolving qasm imports.
        - ``shots`` (int): The number of shots to run the program for.
        - ``seed`` (int): The seed to use for the random number generator.
        - ``output_fn`` (Callable): A callback for each output. Defaults to ``None``.
    :return: A list of results or runtime errors.
    :raises QSharpError: If there is an error evaluating the source code.
    :raises QasmError: If there is an error generating, parsing, or compiling QASM.
    """

    from ...._native import run_qasm_program, Output  # type: ignore
    from ...._fs import read_file, list_directory, resolve
    from ...._http import fetch_github

    def callback(output: Output) -> None:
        print(output)

    output_fn = options.pop("output_fn", callback)

    def value_or_default(key: str) -> Any:
        return options.pop(key, default_options[key])

    # when passing the args into the rust layer, any kwargs with None values
    # will cause an error, so we need to filter them out.
    args = {}
    if name := value_or_default("name"):
        args["name"] = name

    if target_profile := value_or_default("target_profile"):
        args["target_profile"] = target_profile
    if output_semantics := value_or_default("output_semantics"):
        args["output_semantics"] = output_semantics

    if search_path := value_or_default("search_path"):
        args["search_path"] = search_path
    if shots := value_or_default("shots"):
        args["shots"] = shots
    if seed := value_or_default("seed"):
        args["seed"] = seed

    return run_qasm_program(
        qasm,
        output_fn,
        None,
        None,
        None,
        read_file,
        list_directory,
        resolve,
        fetch_github,
        **args,
    )
