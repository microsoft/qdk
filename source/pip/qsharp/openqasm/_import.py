# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from time import monotonic
from typing import Any

from ._ipython import display_or_print
from .._fs import read_file, list_directory, resolve
from .._http import fetch_github
from .._qsharp import (
    get_interpreter,
    ipython_helper,
)
from .. import telemetry_events


def import_openqasm(
    source: str,
    **kwargs: Any,
) -> Any:
    """
    Imports OpenQASM source code into the active Q# interpreter.

    Args:
        source (str): An OpenQASM program or fragment.
        **kwargs: Additional keyword arguments to pass to the execution.
          - name (str): The name of the program. This is used as the entry point for the program.
          - search_path (Optional[str]): The optional search path for resolving file references.
          - output_semantics (OutputSemantics, optional): The output semantics for the compilation.
          - program_type (ProgramType, optional): The type of program compilation to perform. Defaults to `ProgramType.Operation`.

    Returns:
        value: The value returned by the last statement in the source code.

    Raises:
        QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
        QSharpError: If there is an error compiling the program.
    """

    ipython_helper()

    telemetry_events.on_import_qasm()
    start_time = monotonic()

    # remove any entries from kwargs with a None key or None value
    kwargs = {k: v for k, v in kwargs.items() if k is not None and v is not None}

    if "search_path" not in kwargs:
        kwargs["search_path"] = "."

    res = get_interpreter().import_qasm(
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
