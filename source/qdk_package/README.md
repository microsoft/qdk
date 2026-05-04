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

For Cirq integration, which exposes Cirq interop utilities in the `qdk.azure.cirq` submodule:

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

- `qdk.qsharp` – exports the same APIs as the `qsharp` Python package
- `qdk.openqasm` – exports the same APIs as the `openqasm` submodule of the `qsharp` Python package.
- `qdk.estimator` – exports the same APIs as the `estimator` submodule of the `qsharp` Python package.
- `qdk.simulation` – noise-aware simulation utilities: `NeutralAtomDevice`, `NoiseConfig`, `run_qir`, `DensityMatrixSimulator`, `StateVectorSimulator`, and related types.
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
| `Result`             | class    | `qsharp.Result`             | Measurement result token.                                           |
| `TargetProfile`      | class    | `qsharp.TargetProfile`      | Target capability / profile descriptor.                             |
| `StateDump`          | class    | `qsharp.StateDump`          | Structured state dump object.                                       |
| `ShotResult`         | class    | `qsharp.ShotResult`         | Multi-shot execution results container.                             |
| `PauliNoise`         | class    | `qsharp.PauliNoise`         | Pauli channel noise model spec.                                     |
| `DepolarizingNoise`  | class    | `qsharp.DepolarizingNoise`  | Depolarizing noise model spec.                                      |
| `BitFlipNoise`       | class    | `qsharp.BitFlipNoise`       | Bit-flip noise model spec.                                          |
| `PhaseFlipNoise`     | class    | `qsharp.PhaseFlipNoise`     | Phase-flip noise model spec.                                        |

## Telemetry

This library sends telemetry. Minimal anonymous data is collected to help measure feature usage and performance.
All telemetry events can be seen in the source file [telemetry_events.py](https://github.com/microsoft/qdk/tree/main/source/pip/qsharp/telemetry_events.py).

## Target Package Structure (Migration WIP)

The `qsharp` package (pip/) is being deprecated. All implementation is moving into `qdk` (qdk_package/). The `qsharp` package will become a thin deprecation shim that depends on `qdk`.

```
qdk_package/
├── Cargo.toml
├── pyproject.toml
├── MANIFEST.in
├── README.md
├── test_requirements.txt
│
├── src/                                # Rust source for _native
│   └── *.rs
│
├── qdk/
│   ├── __init__.py                     # Same public API as today
│   │
│   │── # ——— Moved from pip/qsharp/ (implementation modules) ———
│   ├── _native.pyd/.so                 # Built by maturin (module-name = "qdk._native")
│   ├── _types.py                       # Pure Python types (PauliNoise, StateDump, etc.)
│   ├── _interpreter.py                 # Interpreter lifecycle & operations
│   ├── _ipython.py                     # %%qsharp cell magic
│   ├── _http.py                        # fetch_github()
│   ├── _fs.py                          # File system callbacks
│   ├── _adaptive_pass.py
│   ├── _adaptive_bytecode.py
│   ├── telemetry.py
│   ├── telemetry_events.py
│   │
│   ├── code/
│   │   └── __init__.py                 # Dynamic Q# callables namespace
│   │
│   ├── estimator/                      # Direct module — no re-export shim needed
│   │   └── __init__.py
│   │
│   ├── openqasm/                       # Direct module — no re-export shim needed
│   │   └── __init__.py
│   │
│   │
│   ├── qiskit/                         # Lifted out of interop/
│   │   ├── __init__.py                 # QSharpBackend, NeutralAtomBackend, etc.
│   │   ├── backends/__init__.py
│   │   ├── passes/__init__.py
│   │   ├── jobs/__init__.py
│   │   └── execution/__init__.py
│   │
│   ├── cirq/                           # Lifted out of interop/
│   │   └── __init__.py                 # NeutralAtomSampler
│   │
│   ├── _device/
│   │   ├── __init__.py
│   │   └── _atom/
│   │       └── __init__.py             # NeutralAtomDevice
│   │
│   ├── qre/
│   │   ├── __init__.py
│   │   ├── application/__init__.py
│   │   ├── models/__init__.py
│   │   │   ├── qubits/__init__.py
│   │   │   ├── qec/__init__.py
│   │   │   └── factories/__init__.py
│   │   ├── interop/__init__.py
│   │   ├── property_keys.py            # Merged with custom_property helpers
│   │   └── instruction_ids.py
│   │
│   ├── applications/
│   │   ├── __init__.py
│   │   └── magnets/
│   │       ├── __init__.py
│   │       ├── utilities/__init__.py
│   │       ├── trotter/__init__.py
│   │       ├── models/__init__.py
│   │       └── geometry/__init__.py
│   │
│   │── # ——— Re-export / facade modules ———
│   ├── qsharp.py                       # Re-exports full qsharp-like API from _types + _interpreter
│   │
│   ├── simulation/                     # Simulation facade package
│   │   ├── __init__.py                 # Public API: NeutralAtomDevice, NoiseConfig, run_qir, etc.
│   │   ├── _simulation.py             # QIR simulation implementation (internal)
│   │   ├── _noisy_simulator.py         # Private wrapper for noisy simulator types
│   │   └── _noisy_simulator.pyi        # Type stubs
│   │
│   │── # ——— Unchanged ———
│   ├── widgets.py                      # from qsharp_widgets import * (external)
│   │
│   └── azure/                          # Unchanged — re-exports from azure.quantum
│       ├── __init__.py
│       ├── job.py
│       ├── qiskit.py
│       ├── cirq.py
│       ├── argument_types.py
│       └── target/
│           ├── __init__.py
│           └── rigetti.py
│
└── tests/
    ├── conftest.py
    ├── mocks.py
    ├── test_reexports.py
    ├── test_extras.py
    ├── test_integration/
    │   ├── test_*.py
    │   ├── utils.py
    │   └── resources/
    └── benchmarks/
        └── bench_qre.py
```
