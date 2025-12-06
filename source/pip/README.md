# Q# Language Support for Python

Q# is an open-source, high-level programming language for developing and running quantum algorithms.
The `qsharp` package for Python provides interoperability with the Q# interpreter, making it easy
to simulate Q# programs within Python.

## Installation

To install the Q# language package, run:

```bash
pip install qsharp
```

## Usage

First, import the `qsharp` module:

```python
import qsharp
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
All telemetry events can be seen in the source file [telemetry_events.py](https://github.com/microsoft/qdk/tree/main/source/pip/qsharp/telemetry_events.py).

To disable sending telemetry from this package, set the environment variable `QSHARP_PYTHON_TELEMETRY=none`

## Support

For more information about the Azure Quantum Development Kit, visit [https://aka.ms/AQ/Documentation](https://aka.ms/AQ/Documentation).

## Contributing

Q# welcomes your contributions! Visit the Q# GitHub repository at [https://github.com/microsoft/qsharp] to find out more about the project.
