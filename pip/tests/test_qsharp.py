# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import qsharp
from contextlib import redirect_stdout
import io


# Tests for the Python library for Q#


def test_stdout() -> None:
    f = io.StringIO()
    with redirect_stdout(f):
        result = qsharp.interpret('Message("Hello, world!")')

    assert result is None
    assert f.getvalue() == "Hello, world!\n"


def test_stdout_multiple_lines() -> None:
    f = io.StringIO()
    with redirect_stdout(f):
        qsharp.interpret(
            """
        use q = Qubit();
        Microsoft.Quantum.Diagnostics.DumpMachine();
        Message("Hello!");
        """
        )

    assert f.getvalue() == "STATE:\n|0⟩: 1.0000+0.0000i\nHello!\n"
