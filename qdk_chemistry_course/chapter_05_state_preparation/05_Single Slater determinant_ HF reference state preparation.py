# Single Slater determinant: HF reference state preparation
sp_sparse = create("state_prep", "???")  # fill in: the sparse isometry preparer name
circ_hf   = sp_sparse.run(wfn_hf)

qc_hf = circ_hf.get_qiskit_circuit()
ops   = qc_hf.count_ops()
print(f"HF reference ({qc_hf.num_qubits} qubits):")
print(f"  Circuit depth : {qc_hf.depth()}")
print(f"  Total gates   : {sum(ops.values())}")
print(f"  CX gates      : {ops.get('cx', 0)}")
print()
print("QASM (first 8 lines):")
for line in circ_hf.get_qasm().split("\n")[:8]:
    print(f"  {line}")

# A single Slater determinant needs only X gates — one per occupied spin-orbital.
# Each X flips that spin-orbital from |0⟩ to |1⟩ to match the HF occupation pattern.
# No entangling gates: HF is a product state in the spin-orbital (Jordan-Wigner) basis.
