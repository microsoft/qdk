# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from time import monotonic
from typing import Any, Callable, Dict, Optional, Union
from .._fs import read_file, list_directory, resolve
from .._http import fetch_github
from .._native import circuit_qasm_program  # type: ignore
from .._qsharp import (
    get_interpreter,
    ipython_helper,
    Circuit,
    CircuitConfig,
    python_args_to_interpreter_args,
)
from .. import telemetry_events


def circuit(
    source: Optional[Union[str, Callable]] = None,
    *args,
    **kwargs: Any,
) -> Circuit:
    """
    Synthesizes a circuit for an OpenQASM program. Either a program string or
    an operation must be provided.

    Args:
        source (str): An OpenQASM program. Alternatively, a callable can be provided,
            which must be an already imported global callable.
        *args: The arguments to pass to the callable, if one is provided.
        **kwargs: Additional keyword arguments to pass to the execution.
          - name (str): The name of the program. This is used as the entry point for the program.
          - search_path (Optional[str]): The optional search path for resolving file references.
    Returns:
        Circuit: The synthesized circuit.

    Raises:
        QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
        QSharpError: If there is an error evaluating the program.
        QSharpError: If there is an error synthesizing the circuit.
    """

    ipython_helper()
    start = monotonic()
    telemetry_events.on_circuit_qasm()

    max_operations = kwargs.pop("max_operations", None)
    generation_method = kwargs.pop("generation_method", None)
    source_locations = kwargs.pop("source_locations", None)
    config = CircuitConfig(
        max_operations=max_operations,
        generation_method=generation_method,
        source_locations=source_locations,
    )

    if isinstance(source, Callable) and hasattr(source, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        res = get_interpreter().circuit(
            config, callable=source.__global_callable, args=args
        )
    else:
        # remove any entries from kwargs with a None key or None value
        kwargs = {k: v for k, v in kwargs.items() if k is not None and v is not None}

        if "search_path" not in kwargs:
            kwargs["search_path"] = "."

        res = circuit_qasm_program(
            source,
            config,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            **kwargs,
        )

    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_circuit_qasm_end(durationMs)

    return res
