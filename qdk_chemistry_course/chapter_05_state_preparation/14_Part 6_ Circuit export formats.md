<h2 style="color:#D30982;">Part 6: Circuit export formats</h2>

A prepared `Circuit` object can be exported in four formats:
- `get_qasm()` — OpenQASM 3.0 string (hardware-agnostic, human-readable)
- `get_qiskit_circuit()` — Qiskit `QuantumCircuit` (for noise simulation or transpilation)
- `get_qsharp_circuit()` — Q# circuit (for QDK resource estimation)
- `get_qir()` — <a href="https://github.com/qir-alliance" target="_blank">QIR</a> bitcode (compiled, hardware-ready for Azure Quantum)

QIR (Quantum Intermediate Representation) is the native compiled format for Azure Quantum hardware — the format you submit when running on real quantum hardware.

Typical workflow: inspect and validate via QASM → simulate noise via Qiskit → project hardware costs via Q# resource estimation → submit compiled QIR to Azure Quantum. The state preparation circuit produced here feeds directly into the IQPE iteration circuits in Chapter 6.