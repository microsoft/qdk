# Circuit executor and controlled circuit mapper settings
print("Available circuit executors:", available("circuit_executor"))
print()
for name in available("circuit_executor"):
    print(f"--- {name} ---")
    print_settings("circuit_executor", name)
    print()

print("Available controlled evolution circuit mappers:", available("controlled_evolution_circuit_mapper"))
print()
for name in available("controlled_evolution_circuit_mapper"):
    print(f"--- {name} ---")
    print_settings("controlled_evolution_circuit_mapper", name)
    print()

# Three circuit executors:
#   qdk_sparse_state_simulator — Sparse statevector; efficient for ≤~20 qubits.
#                                 Handles IQPE ancilla+system circuits without full 2^n RAM.
#   qdk_full_state_simulator   — Dense statevector; builds the full 2^n × 1 vector.
#                                 Practical only for very small qubit counts (< 14 qubits).
#   qiskit_aer_simulator       — Qiskit Aer backend; supports QuantumErrorProfile noise models
#                                 for realistic hardware noise simulation. Requires qiskit plugin.
# One mapper: pauli_sequence — expresses each exp(-i θ/2 P) as a CNOT ladder + Rz rotation,
#             wrapped with a control qubit. The only available mapper; power=1 for single-step.
