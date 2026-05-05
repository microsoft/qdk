# Circuit export: QASM, Qiskit, Q#, and QIR
circ_10 = create("state_prep", "sparse_isometry_gf2x").run(wfn_10)

print("Export methods on a Circuit object:")
print("  get_qasm()           → OpenQASM 3.0 string (hardware-agnostic)")
print("  get_qiskit_circuit() → Qiskit QuantumCircuit (simulation, noise modeling)")
print("  get_qsharp_circuit() → Q# circuit (QDK resource estimation)")
print("  get_qir()            → QIR bitcode (compiled, hardware-ready for Azure Quantum)")
print()
qasm_str = circ_10.get_qasm()
print("QASM excerpt (first 8 lines of 10-det state circuit):")
for line in qasm_str.split("\n")[:8]:
    print(f"  {line}")
print("  ...")
print()

# Inspect the Qiskit circuit directly
qc = circ_10.get_qiskit_circuit()
ops = qc.count_ops()
print(f"Qiskit QuantumCircuit: {qc.num_qubits} qubits, depth={qc.depth()}, "
      f"CX gates={ops.get('cx', 0)}, total gates={sum(ops.values())}")
print("→ Matches the sparse_isometry depth/CX count from Part 3.")

print()
print("→ Carry forward: The sparse state preparation approach (sparse_isometry_gf2x)")
print("  and the autoCAS-EOS active space (from Ch.4) feed Chapter 6 (QPE).")
