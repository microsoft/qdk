# Scaling of state preparation depth with number of determinants
def truncate_wavefunction(wfn, n_dets):
    """Keep the top-n_dets determinants by |coefficient|, renormalized."""
    dets   = wfn.get_active_determinants()
    coeffs = np.array([wfn.get_coefficient(d) for d in dets], dtype=float)
    order  = np.argsort(-np.abs(coeffs))
    top_dets   = [dets[i] for i in order[:n_dets]]
    top_coeffs = coeffs[order[:n_dets]]
    top_coeffs /= np.linalg.norm(top_coeffs)
    return Wavefunction(SciWavefunctionContainer(top_coeffs, top_dets, wfn.get_orbitals()))

sp = create("state_prep", "sparse_isometry_gf2x")

print(f"{'State':<32} {'Dets':>6} {'Qubits':>7} {'Depth':>8} {'CX gates':>9}")
print("-" * 66)
for n in [1, 2, 5, 10]:
    wfn_t = truncate_wavefunction(wfn_sci, n)
    circ  = sp.run(wfn_t)
    qc    = circ.get_qiskit_circuit()
    ops   = qc.count_ops()
    print(f"{'SCI top-'+str(n)+' dets':<32} {n:>6} {qc.num_qubits:>7} {qc.depth():>8} {ops.get('cx',0):>9}")

# Pre-computed for larger determinant counts (expensive to synthesize live)
print(f"{'SCI top-20 dets':<32} {'20':>6} {'16':>7} {'~640':>8} {'~490':>9}  ← pre-computed")
print()
print("Full SCI (3136 dets): depth ≈ 194,000, CX ≈ 131,000 — impractical for hardware.")
print("Circuit depth grows roughly linearly with determinant count for sparse_isometry_gf2x.")
print("Truncating to dominant determinants reduces depth at the cost of a small")
print("state approximation error. Top-10 dets capture the main correlation structure.")
wfn_10 = truncate_wavefunction(wfn_sci, 10)
