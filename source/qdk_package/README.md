# qdk

Preview package for the Quantum Development Kit (QDK) that bundles the `qsharp` Python package together with optional extras under a single top level package.

## Install

To install the core functionality, which include Q\# \& OpenQASM simulation, compilation, and resource estimation support:

```bash
pip install qdk
```

To include the Jupyter extra, which adds visualizations using Jupyter Widgets in the `qdk.widgets` submodule and syntax highlighting for Jupyter notebooks in the browser:

```bash
pip install qdk[jupyter]
```

To add the Azure Quantum extra, which includes functionality for working with the Azure Quantum service in the `qdk.azure` submodule:

```bash
pip install qdk[azure]
```

For Qiskit integration, which exposes Qiskit interop utilities in the `qdk.qiskit` submodule:

```bash
pip install qdk[qiskit]
```

To easily install all the above extras:

```bash
pip install qdk[all]
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

- `qdk.qsharp` – exports the same APIs as the `qsharp` Python package
- `qdk.openqasm` – exports the same APIs as the `openqasm` submodule of the `qsharp` Python package.
- `qdk.estimator` – exports the same APIs as the `estimator` submodule of the `qsharp` Python package.
- `qdk.widgets` – exports the Jupyter widgets available from the `qsharp-widgets` Python package (requires the `qdk[jupyter]` extra to be installed).
- `qdk.azure` – exports the Python APIs available from the `azure-quantum` Python package (requires the `qdk[azure]` extra to be installed).
- `qdk.qiskit` – exports the same APIs as the `interop.qiskit` submodule of the `qsharp` Python package (requires the `qdk[qiskit]` extra to be installed).

### Top level exports

For convenience, the following helpers and types are also importable directly from the `qdk` root (e.g. `from qdk import code, Result`). Algorithm execution APIs (like `run` / `estimate`) remain under `qdk.qsharp` or `qdk.openqasm`.

| Symbol               | Type     | Origin                      | Description                                                         |
| -------------------- | -------- | --------------------------- | ------------------------------------------------------------------- |
| `code`               | module   | `qsharp.code`               | Exposes operations defined in Q\# or OpenQASM                       |
| `init`               | function | `qsharp.init`               | Initialize/configure the QDK interpreter (target profile, options). |
| `set_quantum_seed`   | function | `qsharp.set_quantum_seed`   | Deterministic seed for quantum randomness (simulators).             |
| `set_classical_seed` | function | `qsharp.set_classical_seed` | Deterministic seed for classical host RNG.                          |
| `dump_machine`       | function | `qsharp.dump_machine`       | Emit a structured dump of full quantum state (simulator dependent). |
| `dump_circuit`       | function | `qsharp.dump_circuit`       | Produce a circuit representation / diagram (when supported).        |
| `Result`             | class    | `qsharp.Result`             | Measurement result token.                                           |
| `TargetProfile`      | class    | `qsharp.TargetProfile`      | Target capability / profile descriptor.                             |
| `StateDump`          | class    | `qsharp.StateDump`          | Structured state dump object.                                       |
| `ShotResult`         | class    | `qsharp.ShotResult`         | Multi-shot execution results container.                             |
| `PauliNoise`         | class    | `qsharp.PauliNoise`         | Pauli channel noise model spec.                                     |
| `DepolarizingNoise`  | class    | `qsharp.DepolarizingNoise`  | Depolarizing noise model spec.                                      |
| `BitFlipNoise`       | class    | `qsharp.BitFlipNoise`       | Bit-flip noise model spec.                                          |
| `PhaseFlipNoise`     | class    | `qsharp.PhaseFlipNoise`     | Phase-flip noise model spec.                                        |
