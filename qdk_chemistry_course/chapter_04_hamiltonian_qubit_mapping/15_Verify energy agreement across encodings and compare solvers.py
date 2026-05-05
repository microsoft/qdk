# Verify energy agreement across encodings and compare solvers
# Hint: use qdk_sparse_matrix_solver for systems beyond ~12 qubits; dense for smaller ones
solver = create("qubit_hamiltonian_solver", "???")  # choose: "qdk_dense_matrix_solver" or "qdk_sparse_matrix_solver"

print(f"{'Mapper / Encoding':<30} {'Energy (Hartree)':>20}")
print("-" * 52)
for key, qham in qubit_hams.items():
    energy, _ = solver.run(qham)
    print(f"{key:<30} {energy:>20.6f}")

print()
print("All encodings are unitarily equivalent representations of the same operator.")
print("Exact diagonalization is encoding-agnostic: eigenvalues must agree exactly.")
print()
dense_solver = create("qubit_hamiltonian_solver", "qdk_dense_matrix_solver")
e_dense, _ = dense_solver.run(qubit_hams["qdk/jordan-wigner"])
e_sparse, _ = create("qubit_hamiltonian_solver", "qdk_sparse_matrix_solver").run(qubit_hams["qdk/jordan-wigner"])
print(f"Dense  solver: {e_dense:.6f} Hartree")
print(f"Sparse solver: {e_sparse:.6f} Hartree  (← same eigenvalue, more memory-efficient algorithm)")