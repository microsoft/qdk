# qdk

Experimental meta-package for the Quantum Development Kit (QDK) that bundles the existing
`qsharp` Python package together with optional extras under a single, stable import root: `import qdk`.

The design is intentionally minimal: submodules plus import-time detection of optional components.

## Install

Base (always includes `qsharp`):

```bash
pip install qdk
```

Jupyter extra (bundles widgets + JupyterLab extension package — provides only the `qdk.widgets` Python surface):

```bash
pip install qdk[jupyter]
```

Azure Quantum extra (adds `azure-quantum`):

```bash
pip install qdk[azure]
```

Qiskit extra (adds `qiskit`):

```bash
pip install qdk[qiskit]
```

All extras:

```bash
pip install qdk[all]
```

## Quick Start

```python
from qdk import qsharp

result = qsharp.run("{ use q = Qubit(); H(q); return MResetZ(q); }", shots=100)
print(result)
```

Widgets (installed via jupyter extra):

```python
try:
    from qdk import widgets  # requires: pip install qdk[jupyter]
    # Use widgets per qsharp-widgets documentation
except ImportError:
    widgets = None  # Optional: feature not installed
```

Qiskit (if installed):

```python
try:
    from qdk import qiskit  # requires: pip install qdk[qiskit]
    qk = qiskit
    # Example: qk.transpile(...)
except ImportError:
    qk = None
```

Azure Quantum (if installed):

```python
try:
    from qdk import azure  # requires: pip install qdk[azure]
    # Example: azure.Workspace(...)
except ImportError:
    azure = None
```

## Public API Surface

Submodules:

- `qdk.qsharp` – direct passthrough to `qsharp` APIs (import explicitly: `import qdk.qsharp`).
- `qdk.widgets` – only if `qsharp-widgets` installed (through `qdk[jupyter]`).
- `qdk.azure` – only if `azure-quantum` installed. Now a package with subpackages:
  - `qdk.azure.target` → `azure.quantum.target`
  - `qdk.azure.argument_types` → `azure.quantum.argument_types`
  - `qdk.azure.job` → `azure.quantum.job`
    Import style example: `from qdk.azure.job import Job`
- `qdk.qiskit` – only if `qiskit` extra installed; exposes only QDK interop symbols (no blanket upstream re-export). Import upstream APIs directly from `qiskit`.
- `qdk.estimator` – shim re-export of `qsharp.estimator` (always present if underlying `qsharp` provides it).
- `qdk.openqasm` – shim re-export of `qsharp.openqasm` for OpenQASM integration.

### Lifted utilities from `qsharp`

For convenience, the following helpers and types are also importable directly from the `qdk` root (e.g. `from qdk import code, Result`). Algorithm execution APIs (like `run` / `estimate`) remain under `qdk.qsharp`.

| Symbol               | Type     | Origin                      | Description                                                         |
| -------------------- | -------- | --------------------------- | ------------------------------------------------------------------- |
| `code`               | module   | `qsharp.code`               | Define inline Q# snippets / code objects.                           |
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

If you need additional items, import them from `qdk.qsharp` directly rather than expanding the root surface.

## Design Notes

- Root re-exports selected utility symbols from `qsharp` (e.g. `code`, `init`, `set_quantum_seed`, types) for convenience; algorithm APIs still live under `qdk.qsharp`.
- Additional shims (`qdk.estimator`, `qdk.openqasm`) are thin pass-throughs to the corresponding `qsharp` submodules for discoverability.
- Optional extras are thin pass-through modules/packages; failure messages instruct how to install.
- `qdk.qiskit` deliberately does not re-export the full upstream Qiskit API—import those from `qiskit` directly.
- `qdk.azure` is structured as a package so dotted imports like `from qdk.azure.job import Job` work.
- Tests may stub dependencies in isolation environments.

## Testing

The test suite validates packaging & import contract without requiring the real
optional dependencies to be installed.

Current approach (kept intentionally lean):

1. Core behavior: ensure the root package exposes only the minimal public API.
2. A lightweight stub for the upstream `qsharp` package is injected (see `tests/conftest.py`)
   if the true package is not present, enabling fast iteration when working only on this meta-package.
3. Optional extras (widgets, azure, qiskit) are tested using synthetic modules created in `tests/mocks.py`:
   - `mock_widgets()` creates a lightweight `qsharp_widgets` module (with a version attribute). Tests assert the `qdk.widgets` shim imports.
   - `mock_azure()` creates the nested namespace `azure.quantum` (with a version attribute). Tests assert the `qdk.azure` shim imports.
   - `mock_qiskit()` creates a `qiskit` module exposing a callable `transpile()` so tests can assert a functional symbol survives re-export.
4. No network or cloud interactions are performed; all tests operate purely on import mechanics and mocks.

### Running the tests

Install test tooling:

```bash
python -m pip install pytest
python -m pytest -q qdk_package/tests
```

Because mocks are used, failures generally indicate packaging / import logic regressions
rather than upstream functional issues with the real dependencies.
