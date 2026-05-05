# Inspect time evolution builder settings
print("Available time evolution builders:", available("time_evolution_builder"))
print()
for name in available("time_evolution_builder"):
    print(f"--- {name} ---")
    print_settings("time_evolution_builder", name)
    print()

# Three builder families:
#   trotter             — Suzuki-Trotter product formula. Order 1 = Lie-Trotter;
#                         order 2 = symmetric Strang splitting (lower error).
#                         Deterministic — error fully analyzable via commutator bounds.
#   qdrift              — Randomly samples Pauli terms weighted by |h_j|. Fewer step terms
#                         for large Hamiltonians; error ∝ λ²t²/N not t²/N.
#   partially_randomized — Deterministic Trotter for heavy terms, qDRIFT for light ones.
#                          Hybrid: reduces both step count and systematic Trotter error.
