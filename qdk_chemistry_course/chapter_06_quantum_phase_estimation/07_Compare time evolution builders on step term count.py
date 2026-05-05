# Compare time evolution builders on step term count
print(f"{'Builder':<28} {'Step terms':>12}")
print("-" * 43)
for label, name, params in [
    ("trotter (order=1)",      "trotter",              {"order": 1}),
    ("trotter (order=2)",      "trotter",              {"order": 2}),
    ("qdrift",                 "qdrift",               {"num_samples": 100, "seed": 42}),
    ("partially_randomized",   "partially_randomized", {"seed": 42}),
]:
    teb = create("time_evolution_builder", name)
    for k, v in params.items():
        teb.settings().set(k, v)
    tev = teb.run(qham, T_max)
    c   = tev.get_container()
    print(f"{label:<28} {len(c.step_terms):>12}")

print()
print("Trotter order=1: one pass through all 161 Pauli terms = 161 step terms.")
print("Trotter order=2: symmetric product (forward + reversed) = 2×161 = 321.")
print("qDRIFT: random subset — only 70 sampled terms; unbiased but adds variance.")
print("Partially randomized: 124 — deterministic core reduces systematic Trotter error.")
# Each step term → a 2-qubit gate block in the IQPE iteration circuit.
# Fewer step terms → shallower circuits → less gate noise on hardware.
