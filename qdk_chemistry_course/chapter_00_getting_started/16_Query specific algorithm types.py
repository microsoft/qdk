# Query specific algorithm types
scf_implementations = available("scf_solver")
selector_implementations = available("active_space_selector")

print(f"scf_solver implementations:       {scf_implementations}")
print(f"active_space_selector implementations: {selector_implementations}")