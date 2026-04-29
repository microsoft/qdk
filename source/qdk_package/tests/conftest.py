# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# Tests rely on the qdk wheel being installed in the venv (with the compiled
# _native extension).  Do NOT add the source tree to sys.path here – that
# would shadow the installed package with the local source directory, which
# does not contain the compiled extension module.

# ---------------------------------------------------------------------------
# Many test files use ``import qdk as qsharp`` and then access symbols like
# ``qsharp.eval()``, ``qsharp.run()``, etc.  Those symbols were part of the
# old *qsharp* public API but are intentionally NOT exported from ``qdk``
# (whose public API must stay unchanged).
#
# Rather than rewriting hundreds of call-sites, we monkey-patch the extra
# symbols onto the ``qdk`` module here so the test alias keeps working.
# This is test infrastructure only – it does not affect the public package.
# ---------------------------------------------------------------------------
import qdk
from qdk._qsharp import (  # noqa: E402
    eval,
    run,
    compile,
    circuit,
    estimate,
    logical_counts,
    QSharpError,
    CircuitGenerationMethod,
)
from qdk._native import estimate_custom  # type: ignore  # noqa: E402

qdk.eval = eval
qdk.run = run
qdk.compile = compile
qdk.circuit = circuit
qdk.estimate = estimate
qdk.logical_counts = logical_counts
qdk.QSharpError = QSharpError
qdk.CircuitGenerationMethod = CircuitGenerationMethod
qdk.estimate_custom = estimate_custom
