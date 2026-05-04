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
