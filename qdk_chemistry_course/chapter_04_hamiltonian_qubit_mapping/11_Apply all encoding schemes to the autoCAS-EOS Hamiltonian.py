# Apply all encoding schemes to the autoCAS-EOS Hamiltonian
encoding_cases = [
    ("qdk",    "jordan-wigner"),
    ("qdk",    "bravyi-kitaev"),
    ("qiskit", "jordan-wigner"),
    ("qiskit", "bravyi-kitaev"),
    ("qiskit", "???"),              # fill in: the third qiskit encoding (not JW or BK)
]

qubit_hams = {}
print(f"{'Mapper / Encoding':<30} {'Qubits':>7} {'Pauli terms':>12} {'Schatten norm':>14}")
print("-" * 67)
for impl, enc in encoding_cases:
    key = f"{impl}/{enc}"
    qham = create("qubit_mapper", impl, encoding=enc).run(active_ham)
    qubit_hams[key] = qham
    print(f"{key:<30} {qham.num_qubits:>7} {len(qham.pauli_strings):>12} {qham.schatten_norm:>14.4f}")

print()
print("For this small, highly symmetric system all encodings produce the same")
print("Pauli term count. For larger, lower-symmetry molecules the counts diverge.")
print("The Schatten norm is encoding-independent — it depends only on the Hamiltonian")
print("coefficients, not their Pauli representation. It sets the QPE evolution time.")