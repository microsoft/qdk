# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
from time import monotonic
from typing import Any, Callable, Dict, List, Optional, Union, cast
from .._fs import read_file, list_directory, resolve
from .._http import fetch_github
from .._native import (  # type: ignore
    resource_estimate_qasm_program,
)
from ..estimator import EstimatorParams, EstimatorResult

from .._qsharp import (
    get_interpreter,
    ipython_helper,
    python_args_to_interpreter_args,
)
from .. import telemetry_events


def estimate(
    source: Union[str, Callable],
    params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None,
    *args: Any,
    **kwargs: Any,
) -> EstimatorResult:
    """
    Estimates the resource requirements for executing OpenQASM source code.
    Either a full program or a callable with arguments must be provided.

    Args:
        source (str): An OpenQASM program. Alternatively, a callable can be provided,
            which must be an already imported global callable.
        params: The parameters to configure estimation.
        callable: The callable to estimate resources for, if no entry expression is provided.
        *args: The arguments to pass to the callable, if one is provided.
        **kwargs: Additional keyword arguments to pass to the execution.
          - name (str): The name of the circuit. This is used as the entry point for the program. Defaults to 'program'.
          - search_path (str): The optional search path for resolving imports.

    Returns:
        EstimatorResult: The estimated resources.

    Raises:
        QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
        QSharpError: If there is an error compiling the program.
    """

    ipython_helper()

    def _coerce_estimator_params(
        params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None,
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
        return cast(List[Dict[str, Any]], params)

    params = _coerce_estimator_params(params)
    param_str = json.dumps(params)
    telemetry_events.on_estimate_qasm()
    start = monotonic()
    if isinstance(source, Callable) and hasattr(source, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        res_str = get_interpreter().estimate(
            param_str, callable=source.__global_callable, args=args
        )
    else:
        # remove any entries from kwargs with a None key or None value
        kwargs = {k: v for k, v in kwargs.items() if k is not None and v is not None}

        if "search_path" not in kwargs:
            kwargs["search_path"] = "."

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

    try:
        qubits = res[0]["logicalCounts"]["numQubits"]
    except (KeyError, IndexError):
        qubits = "unknown"

    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_estimate_qasm_end(durationMs, qubits)
    return EstimatorResult(res)
