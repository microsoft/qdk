# QPE accuracy vs number of phase bits
print(f"{'num_bits':<10} {'bitstring':<14} {'raw_energy':>15} {'error (mHa)':>12}")
print("-" * 54)
for nb in [4, 6, 8, 10]:
    teb_i = create("time_evolution_builder", "trotter")
    teb_i.settings().set("order", 1)
    exec_i = create("circuit_executor", "qdk_sparse_state_simulator")
    exec_i.settings().set("seed", 42)
    pe_i = create("phase_estimation", "iterative")
    pe_i.settings().set("evolution_time", T_max)
    pe_i.settings().set("num_bits", nb)
    pe_i.settings().set("shots_per_bit", 10)
    res = pe_i.run(circ_eos, qham,
                   evolution_builder=teb_i,
                   circuit_mapper=create("controlled_evolution_circuit_mapper", "pauli_sequence"),
                   circuit_executor=exec_i)
    err = (res.raw_energy - E_exact) * 1000
    print(f"{nb:<10} {res.bitstring_msb_first:<14} {res.raw_energy:>15.6f} {err:>12.1f}")

print()
print("Each added bit halves the energy grid: Δε = 2π / (T · 2ⁿ).")
print("10 bits → ~1.7 mHa error, approaching chemical accuracy (1.6 mHa = 1 kcal/mol).")
print("Error sources on real hardware:")
print("  Trotter error — systematic; reducible with higher order or more steps")
print("  Shot noise    — statistical; reducible with more shots_per_bit")
print("  Gate noise    — hardware decoherence; reducible with error correction")
print()
print("→ End-to-end pipeline complete: Structure → HF → active space →")
print("  qubit Hamiltonian (Ch.4) → state prep (Ch.5) → QPE → ground state energy.")
