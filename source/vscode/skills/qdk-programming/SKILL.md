---
name: qdk-programming
description: 'QDK (Quantum Development Kit) programming guide for Q#, OpenQASM, and Python. Use when: user asks to "write Q# code", "implement a quantum algorithm", "create a Q# operation", "write quantum gates", "write OpenQASM", "write QASM", "run a quantum program from Python", "estimate resources", "generate a circuit", "simulate with noise", "submit to Azure Quantum", "use Qiskit with QDK", "use Cirq with QDK", "write Q# tests", debug Q# or OpenQASM syntax, needs Q# standard library guidance, asks about Q# types/operators/control flow, OpenQASM 2.0 vs 3.0, stdgates.inc, qelib1.inc, or wants patterns for Grover, QFT, teleportation, error correction, VQE, QAOA. Covers Q# language syntax/types/operations/standard library, OpenQASM 2.0/3.0 syntax/gates, and qsharp/qdk Python API for execution/compilation/estimation/circuits/noise/Azure.'
---

# QDK Programming

Comprehensive guide for quantum programming with the Microsoft Quantum Development Kit (QDK). Covers three domains:

| Domain         | Reference                    | Use For                                                                                                                        |
| -------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| **Q#**         | [qsharp.md](./qsharp.md)     | Q# language syntax, types, quantum operations, standard library, algorithms, testing, project structure                        |
| **OpenQASM**   | [openqasm.md](./openqasm.md) | OpenQASM 2.0/3.0 syntax, standard gates, version differences, custom gates                                                     |
| **Python API** | [python.md](./python.md)     | Running Q#/OpenQASM from Python, resource estimation, circuit generation, noisy simulation, Azure Quantum, Qiskit/Cirq interop |

## Routing

Read the appropriate reference file(s) based on the user's question:

- **Learning quantum computing / practicing exercises** (katas, tutorials, structured learning path, "teach me Q#") → defer to the **quantum-tutor** skill instead of this one
- **Writing Q# code** (syntax, algorithms, gates, tests, projects) → read [qsharp.md](./qsharp.md)
- **Writing OpenQASM code** (QASM syntax, stdgates.inc, qelib1.inc) → read [openqasm.md](./openqasm.md)
- **Running quantum programs from Python** (qsharp.eval, run, compile, estimate, circuit) → read [python.md](./python.md)
- **Resource estimation** → read [python.md](./python.md) (Resource Estimation section)
- **Circuit generation / visualization** → read [python.md](./python.md) (Circuit Generation section)
- **Noisy simulation** → read [python.md](./python.md) (Noisy Simulation section)
- **Azure Quantum submission** → read [python.md](./python.md) (Azure Quantum section)
- **OpenQASM from Python** → read both [openqasm.md](./openqasm.md) for syntax and [python.md](./python.md) (OpenQASM Interop section)
- **Qiskit / Cirq / PennyLane interop** → read [python.md](./python.md) (framework interop section)
- **Q# + Python together** → read both [qsharp.md](./qsharp.md) and [python.md](./python.md)

## Critical: Tool Usage

- **Always call `qsharpGetLibraryDescriptions`** before generating Q# code that uses standard library functions. This returns the authoritative, up-to-date list of all Q# library items with signatures. Do not guess at function names or signatures.
- To execute or compile programs, use the provided tools when available (e.g., `qsharpRunCode`, `qsharpRunQasmCode`, `qsharpCompileCode`, `qsharpEstimateCode`).
- The QDK was rewritten in 2024 and no longer uses the IQ# Jupyter kernel or `dotnet` CLI tools. For job management, use tool calls through GitHub Copilot or Python code with the `qsharp` and `azure-quantum` packages.
