# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from time import monotonic
from typing import Any, Callable, Dict, Optional, Union
from .._fs import read_file, list_directory, resolve
from .._http import fetch_github

from .._native import (  # type: ignore
    compile_qasm_program_to_qir,
)
from .._qsharp import (
    QirInputData,
    get_interpreter,
    ipython_helper,
    TargetProfile,
    python_args_to_interpreter_args,
)
from .. import telemetry_events


def compile(
    source: Union[str, Callable],
    *args: Any,
    **kwargs: Any,
) -> QirInputData:
    """
    Compiles the OpenQASM source code into a program that can be submitted to a
    target as QIR (Quantum Intermediate Representation).
    Either a full program or a callable with arguments must be provided.

    Args:
        source (str): An OpenQASM program. Alternatively, a callable can be provided,
            which must be an already imported global callable.
        *args: The arguments to pass to the callable, if one is provided.
        **kwargs: Additional keyword arguments to pass to the compilation when source program is provided.
          - name (str): The name of the circuit. This is used as the entry point for the program.
          - target_profile (TargetProfile): The target profile to use for code generation.
          - search_path (Optional[str]): The optional search path for resolving file references.
          - output_semantics (OutputSemantics, optional): The output semantics for the compilation.

    Returns:
        QirInputData: The compiled program.

    Raises:
        QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
        QSharpError: If there is an error compiling the program.

    To get the QIR string from the compiled program, use `str()`.

    Example:

    .. code-block:: python
        from qsharp.openqasm import compile
        source = ...
        program = compile(source)
        with open('myfile.ll', 'w') as file:
            file.write(str(program))
    """

    ipython_helper()
    start = monotonic()

    # This doesn't work the same way as the Q# compile function as it doesn't
    # have access to the global configuration which has the target profile.
    # Instead, we get the target profile from the kwargs and pass it to the telemetry event.
    target_profile = kwargs.get("target_profile", "unspecified")

    telemetry_events.on_compile_qasm(target_profile)

    if isinstance(source, Callable) and hasattr(source, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        ll_str = get_interpreter().qir(
            entry_expr=None, callable=source.__global_callable, args=args
        )
    elif isinstance(source, str):
        # remove any entries from kwargs with a None key or None value
        kwargs = {k: v for k, v in kwargs.items() if k is not None and v is not None}

        if "search_path" not in kwargs:
            kwargs["search_path"] = "."
        if "target_profile" not in kwargs:
            kwargs["target_profile"] = TargetProfile.Base

        ll_str = compile_qasm_program_to_qir(
            source,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            **kwargs,
        )
    else:
        raise ValueError("source must be a string or a callable with __global_callable attribute")
    res = QirInputData("main", ll_str)

    durationMs = (monotonic() - start) * 1000
    telemetry_events.on_compile_qasm_end(durationMs, target_profile)

    return res
