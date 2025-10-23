# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
from typing import Any, Dict, List, Optional, Union

from ...estimator import EstimatorParams, EstimatorResult
from ..._native import OutputSemantics, ProgramType, QasmError
from .backends import QSharpBackend, ResourceEstimatorBackend, QirTarget
from .jobs import QsJob, QsSimJob, ReJob, QsJobSet
from .execution import DetaultExecutor
from qiskit import QuantumCircuit


def estimate(
    circuit: QuantumCircuit,
    params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None,
    **options,
) -> EstimatorResult:
    """
    Estimates resources for Qiskit QuantumCircuit.

    :param circuit: The input Qiskit QuantumCircuit object.
    :param params: The parameters to configure physical estimation.
    :**options: Additional options for the execution.
        - Any options for the transpiler, exporter, or Qiskit passes
            configuration. Defaults to backend config values. Common
            values include: 'basis_gates', 'includes', 'search_path'.
    :raises QasmError: If there is an error generating or parsing QASM.

    :returns `EstimatorResult`: The estimated resources.
    """
    from ..._qsharp import ipython_helper

    ipython_helper()
    backend = ResourceEstimatorBackend(circuit.num_qubits)
    job = backend.run(circuit, params=params, **options)
    return job.result()
