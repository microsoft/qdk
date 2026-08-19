# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Any

from .._interpreter import _get_default_context


def import_openqasm(
    source: str,
    **kwargs: Any,
) -> Any:
    """
    Import OpenQASM source into the active QDK context.

    With the default ``ProgramType.Operation``, the source becomes a Q#
    operation in the global namespace. Declared classical inputs and qubits
    become parameters, and declared outputs form the return value.

    ``ProgramType.File`` creates a stand-alone operation in the
    ``qasm_import`` namespace. It takes only classical inputs, manages declared
    qubits internally, and returns declared outputs. ``ProgramType.Fragments``
    evaluates the source in the current interactive scope so its declarations
    remain available to later evaluations.

    :param source: An OpenQASM program or fragment.
    :type source: str
    :param **kwargs: Additional keyword arguments. Common options:

        - ``name`` (str): The name of the program. This is used as the entry point for the program.
        - ``search_path`` (str): The optional search path for resolving file references.
        - ``output_semantics`` (OutputSemantics): The output semantics for the compilation.
        - ``program_type`` (ProgramType): The type of program compilation to perform.
          Defaults to ``ProgramType.Operation``.
    :return: The interpreter value of the imported source. Declarations usually
        return ``None``; fragments can return the value of their final statement.
    :rtype: Any
    :raises QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
    :raises qdk.qsharp.QSharpError: If there is an error compiling or evaluating the program.
    """
    return _get_default_context().import_openqasm(source, **kwargs)
