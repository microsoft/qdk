---
description: Build circuits in Python, C, Go, and Fortran and simulate their QIR on CPU or GPU.
---

# Circuit API notebooks

| Notebook | Additional examples |
| --- | --- |
| [Python](circuit_api_python.ipynb) | Named registers, symbolic parameter sweeps, exact snapshots, graph inspection, inverse-pair cancellation |
| [Python experiments and custom passes](circuit_api_python_advanced.ipynb) | Five-qubit GHZ, if/else feedback, subprocess parameter binding, identity removal, X decomposition, custom pipelines |
| [C](circuit_api_c.ipynb) | Explicit handles and output ownership, a C-built symbolic template bound in Python |
| [Go](circuit_api_go.ipynb) | Reusable Go binding, scoped feedback builders, independent clones, native-to-Python graph inspection |
| [Fortran](circuit_api_fortran.ipynb) | Reusable derived types, scoped feedback, independent clones, binary stream output and native QIR |

The introductory Python and native notebooks build both a Base-profile Bell
circuit and an Adaptive-profile measurement-feedback circuit. Both produce
correlated `00`/`11` readouts, through entanglement and classical feedback
respectively. The second Python notebook extends these experiments to a five-qubit
GHZ state and if/else feedback, then demonstrates cross-process parameter binding
and custom graph transformations with phase-sensitive simulation checks.

There is no direct circuit simulation method. Compile with `to_qir(profile)` and
call `qdk.simulation.run_qir`. Native programs can also export QIR. A pass
pipeline's `run` method transforms a program; it does not simulate one.

## Prerequisites and setup

The circuit API and native ABI are experimental. Use the Python package, native
library, header, and sample sources from the same branch checkout. A released
PyPI package might not contain these APIs.

1. Install Python 3.10+ and the repository's [build prerequisites](../../../prereqs.py),
   including Rust/Cargo and a C toolchain.
2. Activate your virtual environment from the repository root, then build and
   install this branch's Python package:

   ```sh
   source .venv/bin/activate
   python build.py --qdk --editable
   ```

3. Install `ipykernel`, IPython, and `nbconvert` in that environment with your
   Python environment manager. Use JupyterLab or VS Code's Python and Jupyter
   extensions. Register the kernel if needed:

   ```sh
   python -m ipykernel install --user --name qdk-circuit-samples --display-name "QDK circuit samples"
   ```

4. For the native notebooks, build the standalone C API:

   ```sh
   python source/circuit_capi/build.py
   ```

5. Install Go 1.20+ with cgo support for the Go notebook. For the Fortran notebook,
   install LLVM Flang or GNU Fortran. All native notebooks require a
   GCC/Clang-compatible C compiler. They use macOS/Linux-style linker flags;
   run them in WSL2 on Windows.
6. Open a notebook and select the environment containing this QDK build.
   All notebooks use a Python kernel; no separate language kernels are needed.

The native notebooks show the editable programs in [native/](native/), compile
them, and execute them as separate processes without embedding Python.
The [build helper](_native.py) discovers Cargo's target directory, including
`CARGO_TARGET_DIR`, and uses the debug SDK by default. It honors `CC`, `FC`,
`QDK_CIRCUIT_CAPI_LIBRARY` (library file), and `QDK_CIRCUIT_CAPI_INCLUDE`
(directory containing `qdk_circuit.h`). These overrides can select a matching
release SDK. Build commands and compiler failures remain visible.

The Go client imports the [experimental Go binding](../../../source/circuit_capi/bindings/go/README.md).
The helper creates a temporary Go module with a local replacement pointing to
the SDK's staged `bindings/go/` directory, alongside `include/` and `lib/`.
No binding download or globally installed Go package is needed. The binding
hides cgo, diagnostics, output ownership, and callback-scoped builder cleanup.

The Fortran client uses the [experimental Fortran binding](../../../source/circuit_capi/bindings/fortran/README.md).
The helper compiles the SDK's staged `qdk_circuit.f90` and C adapter with the
selected compiler in the notebook's temporary directory. Circuit types hide
opaque pointers and native buffers; errors are returned to the application.
Assignment aliases a circuit, while `clone` creates independent ownership.
The callback is a module procedure for compatibility with macOS Flang.

C uses the staged public C header and library directly, without a second wrapper
ABI. All bindings are experimental; the Go and Fortran surfaces cover only the
operations demonstrated by these samples.

## GPU execution

CPU execution is enabled by default. For GPU execution, install a GPU and drivers
supported by the QDK simulator, change `RUN_GPU = False` to `True` in the setup
cell, and rerun the notebook. The code requests `type="gpu"` explicitly and
does not fall back to CPU when initialization fails. No Azure account is needed.
Small circuits demonstrate the API, not a GPU performance advantage.

## Run and validate

Run all cells in order. Each notebook asserts snapshot integrity, shot counts,
and expected outcomes. Native notebooks also compare native QIR against QIR
compiled from the loaded snapshot. Statistical counts can vary; CPU/GPU shot
sequences need not agree even with the same seed.

For headless execution from the repository root:

```sh
python -m jupyter nbconvert --to notebook --execute \
  --ExecutePreprocessor.kernel_name=qdk-circuit-samples \
  --ExecutePreprocessor.timeout=120 \
  --output-dir=/tmp/qdk-circuit-notebooks \
  samples/notebooks/circuit_api/circuit_api_python.ipynb \
  samples/notebooks/circuit_api/circuit_api_python_advanced.ipynb \
  samples/notebooks/circuit_api/circuit_api_c.ipynb \
  samples/notebooks/circuit_api/circuit_api_go.ipynb \
  samples/notebooks/circuit_api/circuit_api_fortran.ipynb
```

Keep committed notebooks output-free. Both Python notebooks participate in the
regular notebook integration lane. The three native notebooks are excluded from
that lane because they require the separate native SDK and language compilers;
run them explicitly with the command above.

Every notebook saves both binary `.qdk` and readable UTF-8 `.sexpr` circuit
snapshots, checks that both restore the same circuit, and prints their full paths.
Native clients also emit `.ll` QIR files; the C client saves its symbolic template
in both snapshot formats. The advanced Python notebook additionally saves graph
snapshots and launches a local worker using the kernel's Python executable.

Generated files live in a unique temporary directory whose full path is printed
by the notebook. They remain available until the final cleanup cell removes that
directory, including native executables and modules. Copy any files you want to
retain elsewhere before running cleanup.

See the [Python circuit guide](../../../source/qdk_package/CIRCUIT-USAGE.md)
and [C API guide](../../../source/circuit_capi/README.md) for additional operations,
graph passes, and ownership rules.
