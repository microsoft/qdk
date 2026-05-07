# Q# Language Support for Python

> **Note:** The `qsharp` package is deprecated. Please use the [`qdk`](https://pypi.org/project/qdk/) package instead. This package is a thin compatibility shim that re-exports the `qdk` public API so that existing code continues to work.

Q# is an open-source, high-level programming language for developing and running quantum algorithms.
The `qsharp` package for Python provides interoperability with the Q# interpreter, making it easy
to simulate Q# programs within Python.

## Installation

```bash
pip install qdk
```

For backward compatibility, `pip install qsharp` also works and will install `qdk` as a dependency.

## Usage

```python
from qdk import qsharp
```

Then, use the `%%qsharp` cell magic to run Q# directly in Jupyter notebook cells:

```qsharp
%%qsharp

import Std.Diagnostics.*;

@EntryPoint()
operation BellState() : Unit {
    use qs = Qubit[2];
    H(qs[0]);
    CNOT(qs[0], qs[1]);
    DumpMachine();
    ResetAll(qs);
}

BellState()
```

## Telemetry

This library sends telemetry. Minimal anonymous data is collected to help measure feature usage and performance.
All telemetry events can be seen in the source file [telemetry_events.py](https://github.com/microsoft/qdk/tree/main/source/qdk_package/qdk/telemetry_events.py).

To disable sending telemetry from this package, set the environment variable `QDK_PYTHON_TELEMETRY=none`

## Support

For more information about the Microsoft Quantum Development Kit, visit [https://aka.ms/qdk](https://aka.ms/qdk).

## Contributing

Q# welcomes your contributions! Visit the Q# GitHub repository at [https://github.com/microsoft/qdk] to find out more about the project.
