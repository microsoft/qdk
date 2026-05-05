# List available state preparers and their settings
print("Available state preparers:", available("state_prep"))
print()
for name in available("state_prep"):
    print(f"--- {name} ---")
    print_settings("state_prep", name)
    print()

# Two preparers:
#   sparse_isometry_gf2x     — GF2X-based sparse method; exploits few-determinant structure;
#                              depth scales with the number of non-zero amplitudes.
#   qiskit_regular_isometry  — Dense unitary decomposition; general-purpose but exponentially
#                              expensive; only practical for very small qubit counts.
