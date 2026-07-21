# qdk

The Quantum Development Kit (QDK) provides a single, cohesive Python entry point for compiling, simulating, and estimating resources for quantum programs (Q# and OpenQASM), with optional extras for visualization, cloud workflows, and interoperability with Qiskit and Cirq.

## Install

To install the core functionality, which include Q\# \& OpenQASM simulation, compilation, and resource estimation support:

```bash
pip install qdk
```

To include the Jupyter extra, which adds visualizations using Jupyter Widgets in the `qdk.widgets` submodule and syntax highlighting for Jupyter notebooks in the browser:

```bash
pip install "qdk[jupyter]"
```

To add the Azure Quantum extra, which includes functionality for working with the Azure Quantum service in the `qdk.azure` submodule:

```bash
pip install "qdk[azure]"
```

For Qiskit integration, which exposes Qiskit interop utilities in the `qdk.qiskit` submodule:

```bash
pip install "qdk[qiskit]"
```

For Cirq integration, which exposes Cirq interop utilities in the `qdk.cirq` submodule:

```bash
pip install "qdk[cirq]"
```

To easily install all the above extras:

```bash
pip install "qdk[all]"
```

## Quick Start

```python
from qdk import qsharp

result = qsharp.run("{ use q = Qubit(); H(q); return MResetZ(q); }", shots=100)
print(result)
```

To use widgets (installed via `qdk[jupyter]` extra):

```python
from qdk.qsharp import eval, run
from qdk.widgets import Histogram

eval("""
operation BellPair() : Result[] {
    use qs = Qubit[2];
    H(qs[0]);CX(qs[0], qs[1]);
    MResetEachZ(qs)
}
""")
results = run("BellPair()", shots=1000, noise=(0.005, 0.0, 0.0))
Histogram(results)
```

## Public API Surface

Submodules:

- `qdk.qsharp` – Q# interpreter functions: `init`, `eval`, `run`, `compile`, `circuit`, `estimate`, `dump_machine`, `dump_circuit`, `dump_operation`, and related types.
- `qdk.openqasm` – OpenQASM compilation and execution.
- `qdk.estimator` – resource estimation utilities.
- `qdk.simulation` – noise-aware simulation utilities: `NeutralAtomDevice`, `NoiseConfig`, `run_qir`, `DensityMatrixSimulator`, `StateVectorSimulator`, and related types.
- `qdk.code` – dynamic namespace populated at runtime with user-defined Q# and OpenQASM callables.
- `qdk.qre` – quantum resource estimation v3: `estimate`, `Application`, `Architecture`, `ISA`, `ISATransform`, and related types.
- `qdk.applications` – domain-specific quantum applications (e.g. `qdk.applications.magnets`).
- `qdk.widgets` – Jupyter widgets for visualization (requires the `qdk[jupyter]` extra).
- `qdk.azure` – Azure Quantum service integration (requires the `qdk[azure]` extra).
- `qdk.qiskit` – Qiskit interop: `QSharpBackend`, `NeutralAtomBackend`, and related types (requires the `qdk[qiskit]` extra).
- `qdk.cirq` – Cirq interop: `NeutralAtomSampler` (requires the `qdk[cirq]` extra).

### Top level exports

For convenience, the following helpers and types are also importable directly from the `qdk` root (e.g. `from qdk import code, Result`). Algorithm execution APIs (like `run` / `estimate`) remain under `qdk.qsharp` or `qdk.openqasm`.

| Symbol               | Type     | Origin                          | Description                                                            |
| -------------------- | -------- | ------------------------------- | ---------------------------------------------------------------------- |
| `code`               | module   | `qdk.code`                      | Exposes operations defined in Q\# or OpenQASM                          |
| `init`               | function | `qdk.qsharp.init`               | Initialize/configure the QDK interpreter (target profile, options).    |
| `set_quantum_seed`   | function | `qdk.qsharp.set_quantum_seed`   | Deterministic seed for quantum randomness (simulators).                |
| `set_classical_seed` | function | `qdk.qsharp.set_classical_seed` | Deterministic seed for classical host RNG.                             |
| `dump_machine`       | function | `qdk.qsharp.dump_machine`       | Emit a structured dump of full quantum state (simulator dependent).    |
| `Result`             | class    | `qdk.qsharp.Result`             | Measurement result token.                                              |
| `TargetProfile`      | class    | `qdk.qsharp.TargetProfile`      | Target capability / profile descriptor.                                |
| `StateDump`          | class    | `qdk.qsharp.StateDump`          | Structured state dump object.                                          |
| `ShotResult`         | class    | `qdk.qsharp.ShotResult`         | Multi-shot execution results container.                                |
| `PauliNoise`         | class    | `qdk.qsharp.PauliNoise`         | Pauli channel noise model spec.                                        |
| `DepolarizingNoise`  | class    | `qdk.qsharp.DepolarizingNoise`  | Depolarizing noise model spec.                                         |
| `BitFlipNoise`       | class    | `qdk.qsharp.BitFlipNoise`       | Bit-flip noise model spec.                                             |
| `PhaseFlipNoise`     | class    | `qdk.qsharp.PhaseFlipNoise`     | Phase-flip noise model spec.                                           |
| `Context`            | class    | `qdk.Context`                   | Isolated Q# and OpenQASM interpreter context for independent sessions. |

### Configuration Map

You can provide configuration at initialization time as a Python dictionary.

In Python, pass `qsharp_config: dict[str, int | float | str | bool]` to `Context(...)`.
If `qsharp_config` is omitted, the configuration map is empty. The map is immutable
after initialization. To use different configuration values, create a new `Context`.

In Q#, read values with `Std.Core.ConfigValue(name, defaultValue)`. In Q# code, config
values are immutable: in the same program, repeated calls with the same
`(name, defaultValue)` produce the same result.

Supported types: `int`, `float`, `str`, and `bool` (corresponding to `Int`, `Double`,
`String` and `Bool` in Q#). The type of each value in `qsharp_config` must match the
type of its corresponding default value.

Example:

```python
import qdk
context = qdk.Context(qsharp_config={"experiment_name": "baseline", "shots": 1000})
assert context.eval('Std.Core.ConfigValue("experiment_name", "")') == "baseline"
assert context.eval('Std.Core.ConfigValue("shots", 100)') == 1000
assert context.eval('Std.Core.ConfigValue("noise_level", 0.01)') == 0.01
```

## Telemetry

This library sends telemetry. Minimal anonymous data is collected to help measure feature usage and performance.
All telemetry events can be seen in the source file [telemetry_events.py](https://github.com/microsoft/qdk/tree/main/source/qdk_package/qdk/telemetry_events.py).

To disable sending telemetry from this package, set the environment variable `QDK_PYTHON_TELEMETRY=none`

## Support

For more information about the Microsoft Quantum Development Kit, visit [https://aka.ms/qdk](https://aka.ms/qdk).

## Contributing

Q# welcomes your contributions! Visit the Q# GitHub repository at [https://github.com/microsoft/qdk] to find out more about the project.
