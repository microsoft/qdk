# Sparse vs. regular isometry on the same 10-determinant state
sp_sparse  = create("state_prep", "sparse_isometry_gf2x")
sp_regular = create("state_prep", "???")  # fill in: the dense isometry preparer name
# Note: sp_regular.run() decomposes a full 2^16-dimensional unitary (~5 minutes).
# Its result is pre-computed and shown below; only the sparse preparer runs here.

print(f"{'Preparer':<30} {'Depth':>8} {'CX gates':>10} {'Total gates':>12}")
print("-" * 64)

# Run sparse isometry (fast — depth scales with determinant count, not Hilbert space)
circ = sp_sparse.run(wfn_10)
qc   = circ.get_qiskit_circuit()
ops  = qc.count_ops()
print(f"{'sparse_isometry_gf2x':<30} {qc.depth():>8} {ops.get('cx',0):>10} {sum(ops.values()):>12}")

# Regular isometry result (pre-computed — run sp_regular.run(wfn_10) to verify, takes ~5 minutes)
print(f"{'qiskit_regular_isometry':<30} {'337756':>8} {'262120':>10} {'337801':>12}  ← pre-computed")

print()
print("sparse_isometry_gf2x exploits the few-determinant structure: GF2X elimination")
print("finds a short CNOT+X sequence that encodes the superposition. For this 10-det")
print("state: depth ~314 vs 337,756 — a ~1000x reduction in circuit depth.")
print("For near-term hardware, always use the sparse preparer.")
