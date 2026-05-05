# Inspect qubit_hamiltonian_solver settings
print("Available qubit Hamiltonian solvers:", available("qubit_hamiltonian_solver"))
print()
for name in available("qubit_hamiltonian_solver"):
    print(f"--- {name} ---")
    print_settings("qubit_hamiltonian_solver", name)
    print()

# Two solvers:
#   qdk_sparse_matrix_solver — iterative Lanczos/Davidson; stores only non-zero entries;
#                              efficient for systems up to ~20 qubits.
#   qdk_dense_matrix_solver  — builds the full 2^n × 2^n matrix in memory;
#                              only practical for very small qubit counts (< 14 qubits).
