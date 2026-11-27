---
name: qdk-programming
description: "QDK programming guide for Q#, OpenQASM, and the qsharp/qdk Python API. Use when: writing or editing Q# or OpenQASM code, running quantum programs, resource estimation, circuit generation, noisy simulation, Azure Quantum submission, Qiskit/Cirq/PennyLane interop, or Q# testing and standard library usage."
---

# QDK Programming

Quantum programming with the Microsoft Quantum Development Kit (QDK).

## Two Modes

Most QDK features work in two modes:

1. **Tool mode** — use provided tools to work with standalone `.qs` / `.qasm` files directly. No Python required.
2. **Python mode** — the user writes Python scripts or Jupyter notebooks, using the `qsharp`/`qdk` Python packages as a driver for Q#/OpenQASM programs.

**Default to tool mode** unless the user is already working in Python or the feature is Python-only.

## Reference Files

| File                                         | Content                                                                                                                                                                                   |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [qsharp.md](./qsharp.md)                     | Writing Q# that compiles on the current QDK: syntax and names that changed from the pre-2024 dialect, imports, numeric types, adjoint and controlled rules, gates, measurement, bit order |
| [qsharp-reference.md](./qsharp-reference.md) | Q# beyond that: entry points, structs, generics, lambdas, standard library by namespace, testing, project structure, dependencies, target profiles                                        |
| [openqasm.md](./openqasm.md)                 | OpenQASM 2.0/3.0 syntax, standard gates, version differences                                                                                                                              |
| [python.md](./python.md)                     | `qsharp`/`qdk` Python API for execution, estimation, circuits, noise, Azure, Qiskit/Cirq, OpenQASM parsing and semantic analysis, setup                                                   |

## Features

| Feature                                  | Tool mode                                         | Python mode                                                    |
| ---------------------------------------- | ------------------------------------------------- | -------------------------------------------------------------- |
| **Writing Q# code**                      | Read [qsharp.md](./qsharp.md)                     | Read [qsharp.md](./qsharp.md)                                  |
| **Q# tests, projects, target profiles**  | Read [qsharp-reference.md](./qsharp-reference.md) | Read [qsharp-reference.md](./qsharp-reference.md)              |
| **Writing OpenQASM code**                | Read [openqasm.md](./openqasm.md)                 | Read [openqasm.md](./openqasm.md)                              |
| **Running a program**                    | `qdkRunProgram`                                   | [python.md](./python.md) — Running Q# Code                     |
| **Resource estimation**                  | `qdkRunResourceEstimator`                         | [python.md](./python.md) — Resource Estimation                 |
| **Circuit diagrams**                     | `qdkGenerateCircuit`                              | [python.md](./python.md) — Circuit Generation                  |
| **Azure Quantum**                        | Use the `azureQuantum*` tools                     | [python.md](./python.md) — Azure Quantum                       |
| **Noisy simulation**                     | — (Python only)                                   | [python.md](./python.md) — Noisy Simulation                    |
| **Q#/OpenQASM in Python and/or Jupyter** | — (inherently Python)                             | [python.md](./python.md)                                       |
| **Qiskit / Cirq / PennyLane interop**    | — (inherently Python)                             | [python.md](./python.md) — Framework Interop                   |
| **OpenQASM parsing and analysis**        | — (Python only)                                   | [python.md](./python.md) — Parse, analyze, and navigate source |
| **Python environment setup**             | `qdkCreateNotebookVenv`                           | `qdkCreateNotebookVenv`                                        |

**Standard library**

Call the `qsharpGetLibraryDescriptions` tool for the authoritative list of Q# library items with their signatures. Do not guess at function names or signatures.

**Quantum Katas**

Quantum Katas are a collection of self-paced tutorials built into the QDK that teach quantum computing from the ground up. If the user wants to learn through the Quantum Katas, encourage them to use the `/qdk-learning` prompt to chat with the QDK Learning agent, which provides a guided, chat-embedded experience.

## Deprecations

- The QDK was rewritten in 2024. It no longer uses the IQ# Jupyter kernel or `dotnet` CLI tools.
- Q# changed with it: `Microsoft.Quantum.*` became `Std.*`, and much of the old syntax is gone. See [qsharp.md](./qsharp.md).
