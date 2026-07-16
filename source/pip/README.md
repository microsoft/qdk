# `qsharp` (compatibility shim)

> **Deprecated.** The `qsharp` package is a thin compatibility shim that
> re-exports the [`qdk`](https://pypi.org/project/qdk/) public API.
> New projects should use `qdk` directly.

## Installation

```bash
pip install qsharp
```

This installs the `qdk` package as a dependency. For new projects, consider
installing `qdk` directly instead:

```bash
pip install qdk
```

## Migration

Replace:

```python
import qsharp
qsharp.init()
qsharp.eval("...")
```

With:

```python
import qdk
qdk.init()
```

Optional extras previously installed via `qsharp[…]` are now available as
`qdk[…]`:

| Old extra              | New extra        |
| ---------------------- | ---------------- |
| `qsharp[jupyterlab]`   | `qdk[jupyter]`   |
| `qsharp[widgets]`      | `qdk[jupyter]`   |
| `qsharp[qiskit]`       | `qdk[qiskit]`    |
| `qsharp[cirq]`         | `qdk[cirq]`      |

## What this package provides

When imported, the `qsharp` shim:

1. Emits a `DeprecationWarning` directing users to migrate to `qdk`.
2. Re-exports the core Q# interpreter API (`init`, `eval`, `run`, `compile`,
   `circuit`, `estimate`, `dump_machine`, `dump_circuit`, `dump_operation`,
   `set_quantum_seed`, `set_classical_seed`, etc.) from `qdk.qsharp`.
3. Re-exports key types: `Result`, `Pauli`, `QSharpError`, `TargetProfile`,
   `StateDump`, `ShotResult`, `PauliNoise`, `DepolarizingNoise`,
   `BitFlipNoise`, `PhaseFlipNoise`, `CircuitGenerationMethod`.

Submodules such as `qsharp.estimator`, `qsharp.openqasm`, and
`qsharp.code` similarly re-export from their `qdk` counterparts.

## Telemetry

This library sends telemetry via the `qdk` package. To disable it, set
the environment variable `QDK_PYTHON_TELEMETRY=none`.

## Support

For more information about the Microsoft Quantum Development Kit, visit
[https://aka.ms/qdk](https://aka.ms/qdk).

## Contributing

Visit the Quantum Development Kit GitHub repository at [https://github.com/microsoft/qdk](https://github.com/microsoft/qdk)
to find out more about the project.
