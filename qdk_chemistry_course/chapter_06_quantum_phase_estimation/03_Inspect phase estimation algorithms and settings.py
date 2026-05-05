# Inspect phase estimation algorithms and settings
print("Available phase estimators:", available("phase_estimation"))
print()
for name in available("phase_estimation"):
    print(f"--- {name} ---")
    print_settings("phase_estimation", name)
    print()

# Two QPE strategies:
#   iterative       — IQPE: 1 ancilla qubit; num_bits sequential single-qubit measurements.
#                     Each iteration k applies controlled-U^(2^(n-k-1)), then measures.
#                     shots_per_bit: majority vote per bit (≥10 for reliable phase readout).
#                     Low qubit overhead; the practical choice for near-term simulation.
#   qiskit_standard — Textbook QPE: n ancilla qubits + inverse QFT at the end.
#                     Produces a single exportable full circuit (QASM/QIR for hardware).
#                     qft_do_swaps: toggles the final swap layer in the inverse QFT.
#                     Requires n_ancilla + n_system qubits; deeper but parallelised.
