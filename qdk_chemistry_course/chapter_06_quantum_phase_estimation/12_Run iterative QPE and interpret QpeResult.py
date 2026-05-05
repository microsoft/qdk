# Run iterative QPE and interpret QpeResult
teb = create("time_evolution_builder", "trotter")
teb.settings().set("order", 1)
mapper   = create("controlled_evolution_circuit_mapper", "pauli_sequence")
executor = create("circuit_executor", "???")  # fill in: "qdk_sparse_state_simulator" or "qdk_full_state_simulator"
executor.settings().set("seed", 42)

pe = create("phase_estimation", "iterative")
pe.settings().set("evolution_time", T_max)
pe.settings().set("num_bits", 8)
pe.settings().set("shots_per_bit", 10)

result = pe.run(circ_eos, qham,
                evolution_builder=teb, circuit_mapper=mapper, circuit_executor=executor)

print(result.get_summary())
print(f"\nE_exact:    {E_exact:.6f} Hartree")
print(f"raw_energy: {result.raw_energy:.6f} Hartree  (error: {(result.raw_energy - E_exact)*1000:.1f} mHa)")
print()
print("Alias branches (2π/T periodicity, 5 candidates):")
for b in result.branching:
    marker = " ← selected" if abs(b - result.raw_energy) < 1e-4 else ""
    print(f"  {b:>10.4f} Hartree{marker}")
print()
print("QPE measures the phase φ of eigenvalue e^{-iEt}; energy E = -φ/T.")
print(f"2π/T_max ≈ {2*np.pi/T_max:.2f} Hartree — alias spacing.")
print("For ambiguous cases, pass reference_energy to QpeResult.from_phase_fraction()")
print("to select the alias closest to a classical reference (e.g., CCSD energy).")
print()
# Iteration circuit anatomy: IQPE uses 1 ancilla + n_system qubits.
# Iteration k applies controlled-U^(2^(num_bits-k-1)); depths halve each step.
print("IQPE iteration circuit depths (9 qubits = 1 ancilla + 8 system):")
print(f"{'Iteration':<12} {'Depth':>10} {'CX gates':>10}")
print("-" * 36)
for i, circ in enumerate(pe._iteration_circuits[:4]):
    qc = circ.get_qiskit_circuit()
    ops = qc.count_ops()
    print(f"{i+1:<12} {qc.depth():>10} {ops.get('cx', 0):>10}")
print("  ... (each iteration halves the depth)")
print()
print("Iteration 1 applies U^128 (= U^(2^7)): deepest circuit, dominates hardware cost.")
print("Total circuit depth ∝ step_terms × 2^num_bits — direct link back to Ch.4 Schatten norm.")
