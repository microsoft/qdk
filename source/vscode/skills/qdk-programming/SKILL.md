---
name: qdk-programming
description: 'QDK (Quantum Development Kit) programming guide for Q#, OpenQASM, and Python. Use when: user asks to "write Q# code", "implement a quantum algorithm", "create a Q# operation", "write quantum gates", "write OpenQASM", "write QASM", "run a quantum program from Python", "estimate resources", "generate a circuit", "simulate with noise", "submit to Azure Quantum", "use Qiskit with QDK", "use Cirq with QDK", "write Q# tests", debug Q# or OpenQASM syntax, needs Q# standard library guidance, asks about Q# types/operators/control flow, OpenQASM 2.0 vs 3.0, stdgates.inc, qelib1.inc, or wants patterns for Grover, QFT, teleportation, error correction, VQE, QAOA. Covers Q# language syntax/types/operations/standard library, OpenQASM 2.0/3.0 syntax/gates, and qsharp/qdk Python API for execution/compilation/estimation/circuits/noise/Azure.'
---

# QDK Programming

Quantum programming with the Microsoft Quantum Development Kit (QDK).

## Two Modes

Most QDK features work in two modes:

1. **Tool mode** — use provided tools to work with standalone `.qs` / `.qasm` files directly. No Python required.
2. **Python mode** — the user writes Python scripts or Jupyter notebooks, using the `qsharp`/`qdk` Python packages as a driver for Q#/OpenQASM programs.

**Default to tool mode** unless the user is already working in Python or the feature is Python-only.

## Reference Files

| File                         | Content                                                                                         |
| ---------------------------- | ----------------------------------------------------------------------------------------------- |
| [qsharp.md](./qsharp.md)     | Q# language syntax, types, quantum operations, standard library, project structure              |
| [openqasm.md](./openqasm.md) | OpenQASM 2.0/3.0 syntax, standard gates, version differences                                    |
| [python.md](./python.md)     | `qsharp`/`qdk` Python API for execution, estimation, circuits, noise, Azure, Qiskit/Cirq, setup |

## Features

| Feature                                  | Tool mode                         | Python mode                                    |
| ---------------------------------------- | --------------------------------- | ---------------------------------------------- |
| **Writing Q# code**                      | Read [qsharp.md](./qsharp.md)     | Read [qsharp.md](./qsharp.md)                  |
| **Writing OpenQASM code**                | Read [openqasm.md](./openqasm.md) | Read [openqasm.md](./openqasm.md)              |
| **Running a program**                    | `qdkRunProgram`                   | [python.md](./python.md) — Running Q# Code     |
| **Resource estimation**                  | `qdkRunResourceEstimator`         | [python.md](./python.md) — Resource Estimation |
| **Circuit diagrams**                     | `qdkGenerateCircuit`              | [python.md](./python.md) — Circuit Generation  |
| **Azure Quantum**                        | Use the `azureQuantum*` tools     | [python.md](./python.md) — Azure Quantum       |
| **Noisy simulation**                     | — (Python only)                   | [python.md](./python.md) — Noisy Simulation    |
| **Q#/OpenQASM in Python and/or Jupyter** | — (inherently Python)             | [python.md](./python.md)                       |
| **Qiskit / Cirq / PennyLane interop**    | — (inherently Python)             | [python.md](./python.md) — Framework Interop   |

## Deprecations

- The QDK was rewritten in 2024. It no longer uses the IQ# Jupyter kernel or `dotnet` CLI tools.
