# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from concurrent.futures import Executor
import json
import logging
from typing import Any, Dict, List, Optional, Union
from uuid import uuid4

from qiskit import QuantumCircuit
from qiskit.providers import Options
from qiskit.transpiler.target import Target

from .compilation import Compilation
from .errors import Errors
from .backend_base import BackendBase
from .. import OutputSemantics
from ..jobs import ReJob
from ..execution import DetaultExecutor
from ...._fs import read_file, list_directory, resolve
from ...._http import fetch_github
from ...._native import resource_estimate_qasm_program
from .... import TargetProfile
from ....estimator import (
    EstimatorResult,
    EstimatorParams,
)

logger = logging.getLogger(__name__)


class ResourceEstimatorBackend(BackendBase):
    """
    A virtual backend for resource estimating Qiskit circuits levaraging
    Q# resource estimation capabilities.
    """

    # This init is included for the docstring
    # pylint: disable=useless-parent-delegation
    def __init__(
        self,
        num_qubits: Union[int, None] = 0,
        qiskit_pass_options: Optional[Dict[str, Any]] = None,
        qasm_export_options: Optional[Dict[str, Any]] = None,
        **fields,
    ):
        """
        Parameters:
            qiskit_pass_options (Dict): Options for the Qiskit passes.
            qasm_export_options (Dict): Options for the QASM3 exporter.
            **options: Additional options for the execution.
                - params (EstimatorParams): Configuration values for resource estimation.
                - name (str): The name of the circuit. This is used as the entry point for the program.
                        The circuit name will be used if not specified.
                - search_path (str): Path to search in for qasm imports. Defaults to '.'.
                - executor(ThreadPoolExecutor or other Executor):
                        The executor to be used to submit the job. Defaults to SynchronousExecutor.
        """

        super().__init__(
            num_qubits,
            qiskit_pass_options,
            qasm_export_options,
            **fields,
        )

    @property
    def max_circuits(self):
        """
        Returns the maximum number of circuits that can be executed simultaneously.
        """
        return 1

    @classmethod
    def _default_options(cls):
        return Options(
            params=None,
            name="program",
            search_path=".",
            target_profile=TargetProfile.Unrestricted,
            output_semantics=OutputSemantics.ResourceEstimation,
            executor=DetaultExecutor(),
        )

    def run(
        self,
        run_input: Union[QuantumCircuit, List[QuantumCircuit]],
        params: Optional[EstimatorParams] = None,
        **options,
    ) -> ReJob:
        """
        Performs resource estimation on the supplied QuantumCircuit via conversion
        to OpenQASM 3.

        Parameters:
            run_input ('QuantumCircuit'): The input Qiskit QuantumCircuit object.
            params (Optional EstimatorParams): Configuration values for resource estimation.
            **options: Additional options for the execution.
                - name (str): The name of the circuit. This is used as the entry point for the program.
                        The circuit name will be used if not specified.
                - search_path (str): Path to search in for qasm imports. Defaults to '.'.
                - target_profile (TargetProfile): The target profile to use for the backend.
                - executor(ThreadPoolExecutor or other Executor):
                        The executor to be used to submit the job.
        Returns:
            ReJob: The resource estimation job

        :raises QSharpError: If there is an error evaluating the source code.
        :raises QasmError: If there is an error generating, parsing, or compiling QASM.
        :raises ValueError: If the run_input is not a QuantumCircuit.
        """
        if isinstance(run_input, QuantumCircuit):
            run_input = [run_input]
        if len(run_input) != 1:
            raise ValueError(str(Errors.ONLY_ONE_CIRCUIT_ALLOWED))

        if params is not None:
            options["params"] = params
        return self._run(run_input, **options)

    def _estimate_qasm(
        self,
        source: str,
        **input_params,
    ) -> Dict[str, Any]:
        """
        Estimates the resource usage of a QASM source code.
        """
        params = input_params.pop("params", None)
        if params is None:
            params = [{}]
        elif isinstance(params, EstimatorParams):
            if params.has_items:
                params = params.as_dict()["items"]
            else:
                params = [params.as_dict()]
        elif isinstance(params, dict):
            params = [params]
        param_str = json.dumps(params)
        kwargs = {
            "name": input_params.pop("name"),
            "search_path": input_params.pop("search_path", "."),
        }
        kwargs.update(input_params)
        res_str = resource_estimate_qasm_program(
            source,
            param_str,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            **kwargs,
        )
        res = json.loads(res_str)
        return res

    def _execute(self, programs: List[Compilation], **input_params) -> Dict:
        exec_results = [
            (program, self._estimate_qasm(program.qasm, **input_params))
            for program in programs
        ]
        success = (
            all(
                "status" in res and res["status"] == "success"
                for (_, res) in exec_results
            )
            and len(exec_results) > 0
        )
        result_dict = {
            "results": [res for (_, res) in exec_results],
            "qobj_id": str(uuid4()),
            "success": success,
        }

        return result_dict

    def _create_results(self, output: Dict[str, Any]) -> EstimatorResult:
        return EstimatorResult(output["results"][0])

    def _submit_job(self, run_input: List[QuantumCircuit], **options) -> ReJob:
        job_id = str(uuid4())
        executor: Executor = options.pop("executor", DetaultExecutor())
        job = ReJob(self, job_id, self.run_job, run_input, options, executor)
        job.submit()
        return job
