# PR: Migrate `qsharp` pip package → `qdk`

> **Delete this file before merging.**

## Summary

This PR migrates all Python source code, Rust native extension code, tests, and build infrastructure from the `qsharp` pip package (`source/pip/`) into the `qdk` package (`source/qdk_package/`). After this change:

- **`qdk`** is the primary Python package containing all source code and the native Rust extension.
- **`qsharp`** becomes a thin, pure-Python deprecation shim that re-exports from the `qdk` package.

## Motivation

The repo is being rebranded from `qsharp` to `qdk`. Rather than maintaining two packages with real code, all functionality consolidates into `qdk`, and `qsharp` exists solely for backward compatibility during the transition period.

---

## What Changed

### 1. Rust native extension moved (`source/pip/src/` → `source/qdk_package/src/`)

The pyo3 native module (`_native`) and all its Rust source files (interpreter, QIR simulation, noisy simulator bindings, resource estimator, etc.) were moved from `source/pip/` into `source/qdk_package/`. The `Cargo.toml` at `source/qdk_package/` now defines the `qdk` crate (previously `qsharp`), and the root `Cargo.toml` workspace member was updated accordingly.

**Files moved (all "pure" renames):**
- `src/lib.rs`, `src/interpreter.rs`, `src/qir_simulation.rs`, `src/noisy_simulator.rs`, `src/qre.rs`, `src/fs.rs`, `src/interop.rs`, `src/generic_estimator/`, `src/displayable_output/`, `src/state_*_template.html`, and all sub-modules.

### 2. Python source reorganized (`source/pip/qsharp/` → `source/qdk_package/qdk/`)

All Python modules were moved from `qsharp.*` to `qdk.*` with import paths updated throughout.

**Key structural changes:**

| Before (`qsharp.*`) | After (`qdk.*`) | Notes |
|---|---|---|
| `qsharp/__init__.py` | `qdk/__init__.py` | New minimal root; exposes common utilities |
| `qsharp/_qsharp.py` | `qdk/_interpreter.py` + `qdk/_types.py` | Split: interpreter functions vs. type definitions |
| `qsharp/_simulation.py` | `qdk/simulation/_simulation.py` | Moved into `simulation/` subpackage |
| `qsharp/noisy_simulator/` | `qdk/simulation/_noisy_simulator.py` | Absorbed into `simulation/` subpackage |
| `qsharp/interop/qiskit/` | `qdk/qiskit/` | Promoted from nested `interop` to top-level subpackage |
| `qsharp/interop/cirq/` | `qdk/cirq/` | Promoted from nested `interop` to top-level subpackage |
| `qsharp/utils/_utils.py` | *(deleted)* | `dump_operation` moved into `_interpreter.py` |
| `qsharp/estimator/` | `qdk/estimator/` | Direct move, imports updated |
| `qsharp/openqasm/` | `qdk/openqasm/` | Direct move, imports updated |
| `qsharp/code/` | `qdk/code/` | Direct move |
| `qsharp/applications/` | `qdk/applications/` | Direct move |
| `qsharp/qre/` | `qdk/qre/` | Direct move, imports updated |
| `qsharp/_device/` | `qdk/_device/` | Direct move, circular import fixed (see below) |

**New `qdk` public API surface:**
- `qdk.qsharp` — Q# interpreter functions (`init`, `eval`, `run`, `compile`, `circuit`, `estimate`, etc.)
- `qdk.simulation` — Simulation APIs (`NeutralAtomDevice`, `NoiseConfig`, noisy simulator types)
- `qdk.qiskit` — Qiskit interop (`QSharpBackend`, `NeutralAtomBackend`, etc.)
- `qdk.cirq` — Cirq interop
- `qdk.estimator` — Resource estimator
- `qdk.openqasm` — OpenQASM compilation/execution
- `qdk.code` — Code analysis
- `qdk.applications` — Domain applications (magnets, etc.)
- `qdk.qre` — QRE v3

### 3. `qsharp` package converted to deprecation shim (`source/pip/`)

`source/pip/qsharp/__init__.py` now:
1. Emits a `DeprecationWarning` on import.
2. Re-exports the full public API from `qdk._types`, `qdk._interpreter`, and `qdk._native`.
3. Registers IPython magics from `qdk._ipython`.

All other Python files under `source/pip/qsharp/` are now thin re-export wrappers that import from their `qdk.*` counterparts. The package metadata in `source/pip/pyproject.toml` declares `dependencies = ["qdk==0.0.0"]` (version stamped at CI time).

### 4. Tests moved

- **Unit tests** remain at `source/qdk_package/tests/` — import paths updated from `qsharp.*` to `qdk.*`.
- **Integration tests** moved from `source/pip/tests-integration/` to `source/qdk_package/tests-integration/` — all imports updated to use `qdk.*`.

### 5. Build script changes (`build.py`)

- **`--qdk` flag**: Builds the maturin wheel (Rust + Python) and runs unit tests. Also runs integration tests when `--integration-tests` is passed.
- **`--pip` flag**: Builds only the pure-Python `qsharp` shim wheel via setuptools. No longer runs any tests (integration tests moved to `--qdk`).
- **Removed `install_qsharp_python_package()`**: No longer needed since integration tests don't depend on the `qsharp` shim.
- Install commands use `--no-deps --no-index` to install from local wheels without reaching PyPI.

### 6. CI/CD pipeline changes

**GitHub Actions (`ci.yml`)** — No changes needed. The `integration-tests` job already passed both `--qdk` and `--integration-tests`.

**Azure DevOps (`publish.yml`)** — Restructured for clean platform split:

| Job | Before | After |
|---|---|---|
| `Platform_Agnostic_Python` | `--jupyterlab --widgets --qdk` | `--jupyterlab --widgets --pip` |
| Per-platform matrix | `--pip --integration-tests` | `--qdk --integration-tests` |

This restructuring reflects the fact that `qdk` is now the platform-specific package (it contains the native Rust extension), while `qsharp` is now platform-agnostic (pure-Python shim). Previously it was the reverse: `qsharp` held the native code and `qdk` was a pure-Python meta-package. Each of the 6 OS/arch combinations now builds its own native `qdk` wheel, while the platform-agnostic wheels (`qsharp`, widgets, jupyterlab) are built once.

### 7. Circular import fix (`_device._atom` ↔ `simulation`)

`simulation/__init__.py` imports `NeutralAtomDevice` from `_device._atom`, while `_device._atom` needs `NoiseConfig` and `run_qir_*` from `simulation._simulation`. This was resolved by:
- Using `from __future__ import annotations` in `_device/_atom/__init__.py`
- Guarding `NoiseConfig` import behind `TYPE_CHECKING`
- Deferring runtime imports of `run_qir_*` into the `simulate()` method body

### 8. Miscellaneous

- **`.prettierignore`**: Added `source/qdk_package/src/**/*.html` and `source/qdk_package/tests-integration/**/*.inc` (these files moved from `source/pip/` which was fully ignored).
- **`.github/CODEOWNERS`**: Updated paths from `source/pip/` to `source/qdk_package/`.
- **`.github/copilot-instructions.md`**: Updated architecture documentation.
- **`Cargo.toml` (root)**: Updated workspace member from `source/pip` to `source/qdk_package`.
- **Sample notebooks**: Updated `import qsharp` to `import qdk` / `from qdk import qsharp`.

---

## How to Review

Due to the large number of file moves, GitHub's diff may be hard to follow. Suggested approach:

1. **Start with `build.py`** — understand the new build flow (`--qdk` builds native + runs tests, `--pip` just builds the shim).
2. **Read `source/qdk_package/qdk/__init__.py`** — see what the new `qdk` root exposes.
3. **Read `source/pip/qsharp/__init__.py`** — see the deprecation shim pattern.
4. **Skim `source/qdk_package/qdk/simulation/__init__.py`** and `source/qdk_package/qdk/_device/_atom/__init__.py` — these have the circular import fix.
5. **Check `.ado/publish.yml`** — verify the platform-agnostic vs. per-platform split.
6. **The rest is mostly mechanical** — file renames and `s/qsharp/qdk/g` import updates. GitHub should detect most as renames (95%+ similarity).

## Testing

- 1,248 unit tests pass (`source/qdk_package/tests/`)
- 338 integration tests pass per Qiskit version (`source/qdk_package/tests-integration/`), run against both Qiskit v1 (`>=1.3,<2`) and v2 (`>=2,<3`)
- Widgets build successfully
- `qsharp` shim wheel builds successfully

## `qdk` Package Structure

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
│   ├── __init__.py                     # Package root; exposes common utilities
│   │
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
│   ├── estimator/
│   │   └── __init__.py
│   │
│   ├── openqasm/
│   │   └── __init__.py
│   │
│   ├── qiskit/                         # Lifted out of interop/
│   │   ├── __init__.py
│   │   ├── backends/__init__.py
│   │   ├── passes/__init__.py
│   │   ├── jobs/__init__.py
│   │   └── execution/__init__.py
│   │
│   ├── cirq/                           # Lifted out of interop/
│   │   └── __init__.py
│   │
│   ├── _device/
│   │   ├── __init__.py
│   │   ├── _device.py
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
│   │   ├── property_keys.py
│   │   └── instruction_ids.py
│   │
│   ├── applications/
│   │   ├── __init__.py
│   │   └── magnets/
│   │       ├── __init__.py
│   │       ├── utilities/
│   │       ├── trotter/
│   │       ├── models/
│   │       └── geometry/
│   │
│   ├── qsharp.py                       # Re-exports full qsharp-like API from _types + _interpreter
│   │
│   ├── simulation/                     # Simulation facade package
│   │   ├── __init__.py                 # Public API: NeutralAtomDevice, NoiseConfig, run_qir, etc.
│   │   ├── _simulation.py              # QIR simulation implementation (internal)
│   │   ├── _noisy_simulator.py         # Private wrapper for noisy simulator types
│   │   └── _noisy_simulator.pyi        # Type stubs
│   │
│   ├── widgets.py                      # from qsharp_widgets import * (external)
│   │
│   └── azure/                          # Re-exports from azure.quantum
│       ├── __init__.py
│       ├── job.py
│       ├── qiskit.py
│       ├── cirq.py
│       ├── argument_types.py
│       └── target/
│           ├── __init__.py
│           └── rigetti.py
│
├── tests/                              # Unit tests (run with --qdk)
│   ├── conftest.py
│   ├── test_qsharp.py
│   ├── test_interpreter.py
│   ├── test_re.py
│   ├── test_qasm.py
│   ├── ... (30+ test modules)
│   ├── reexports/                      # Re-export verification tests
│   ├── qre/                            # QRE-specific tests
│   └── applications/                   # Application-specific tests
│
└── tests-integration/                  # Integration tests (run with --qdk --integration-tests)
    ├── conftest.py
    ├── utils.py
    ├── test_adaptive_ri_qir.py
    ├── test_adaptive_rif_qir.py
    ├── test_adaptive_rifla_qir.py
    ├── test_base_qir.py
    ├── devices/                        # Device integration tests
    ├── interop_qiskit/                 # Qiskit interop tests
    ├── interop_cirq/                   # Cirq interop tests
    └── resources/                      # Test resource files (QIR, etc.)
```

For a detailed breakdown of every public symbol exported by each `qdk` submodule, see [API_SURFACE.md](API_SURFACE.md).

## Follow-up Work

- **Move noise types to `qdk.simulation`**: The `PauliNoise`, `DepolarizingNoise`, `BitFlipNoise`, and `PhaseFlipNoise` classes currently live in `qdk._types` and are re-exported through `qdk.qsharp`. These are simulation concepts and should canonically live in `qdk.simulation`, with backward-compatible re-exports from `qdk.qsharp` and `qdk._types`. Deferred from this PR to avoid additional circular import complexity.
- **`NoiseConfig` in `qdk.qsharp`**: Similarly, `NoiseConfig` (from `_native`) is re-exported in `qdk.qsharp.__all__` but semantically belongs in `qdk.simulation` (where it's already exported). The `qdk.qsharp` re-export should be removed in a follow-up.
- **Audit and rewrite docstrings**: Module and function docstrings throughout the package still reference the old `qsharp` import paths and naming conventions. These need to be audited and updated to reflect the new `qdk.*` namespace for accurate generated documentation.
