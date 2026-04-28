# Copilot instructions for the `qsharp` repo

## Overview

This repo contains the Microsoft Quantum Development Kit (QDK), which provides tooling for the Q# language. Q# is an open-source programming language designed for developing and running quantum algorithms. This repo is publicly available at https://github.com/microsoft/qdk .

## Architecture

All internal source code for the compiler and related tooling has been moved under the `source/` directory at the repository root. This does not include Q# libraries (`library/`), samples (`samples/`), katas (`katas/`), or any standard files and folders typically found at the root of a repository (such as configuration, documentation, or build scripts).

Most of the core components are implemented in Rust. These components are packaged in two ways:

1. Compiled as a native Python module and packaged into the `qsharp` Python package
2. Compiled into WebAssembly and packaged into the `qsharp-lang` npm package

## Repo Layout

Directories under `source/` (Rust, Python, JS/TS tooling):

- **compiler/**: Q# and OpenQASM compiler crates (`qsc_*`)
- **language_service/**: Editor features (completions, diagnostics, etc.)
- **resource_estimator/**, **qre/**: Quantum resource estimation
- **noisy_simulator/**, **simulators/**: Quantum simulation
- **wasm/**: WebAssembly bindings for the compiler
- **pip/**: `qsharp` Python package
- **jupyterlab/**, **widgets/**: Jupyter integration
- **npm/**: `qsharp-lang` npm package
- **vscode/**: VS Code extension
- **playground/**: Q# Playground website

Directories at the repo root (Q# content):

- **library/**: Q# standard and domain-specific libraries
- **katas/**: Quantum computing tutorials and exercises
- **samples/**: Example Q# programs

## Development Workflow

**Important**: The build script (`build.py`) and many development tasks require Python. Always use the `get_python_environment_details` and `configure_python_environment` tools, if available, to determine the correct Python environment before running any Python commands. Do not assume a system-level Python is available or correct.

- `./build.py` runs full CI checks, including lints and unit tests.
- `./build.py --wasm --npm --vscode` only builds the VS Code extension, including its dependencies the WASM module and the `qsharp-lang` npm package.
- `./build.py --pip` only builds the `qsharp` Python package, including its native dependencies.
- Pass `--no-check` to `./build.py`, in combination with any other command line options, to skip the lints and formatting checks.
- When working in Rust parts of the codebase, using `cargo` commands is usually more efficient than building via `./build.py`.
  - Many lints can be auto-fixed via `cargo clippy --fix`.
- When working in JavaScript/TypeScript parts of the codebase, using `npm` scripts is usually more efficient than building via `./build.py`.

## Coding Standards

- When adding new tests, follow the patterns established in existing tests in the same file or suite. Often, tests will use helper functions for brevity and readability. Design your tests to reuse these helpers where possible.
- Before opening a PR, ensure the following.
  - Code **must** be formatted by running `cargo fmt` and `npm run prettier:fix`.
  - `./build.py` without any command-line arguments **must** run without errors or warnings.
