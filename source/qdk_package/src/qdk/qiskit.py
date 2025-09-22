# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Re-export shim for the optional qiskit package as qdk.qiskit.

If qiskit is not installed (with the qdk[qiskit] extra), importing this
module raises an ImportError describing how to enable it.
"""

try:
    from qiskit import *
    from qsharp.interop.qiskit import *
except Exception as ex:
    raise ImportError(
        "qdk.qiskit requires the qiskit extra. Install with 'pip install qdk[qiskit]'."
    ) from ex
