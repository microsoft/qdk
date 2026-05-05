# Projected multi-configuration refinement and trial state fidelity
# truncate_wavefunction (above) renormalizes the top-N SCI coefficients as-is.
# The projected multi-configuration calculator (PMC) re-solves the Hamiltonian
# in the subspace of those N determinants, optimizing the coefficients — giving
# a lower variational energy and better overlap with the full SCI state.
#
# Fidelity = |⟨ψ_pmc | ψ_sci⟩|² = probability that QPE (Chapter 6) collapses
# to the ground state on the first shot. Higher fidelity → more reliable QPE.

pmc = create("projected_multi_configuration_calculator", "???")  # fill in: PMC algorithm name

print(f"{'N dets':<8} {'Fidelity':>10} {'Circ depth':>12}")
print("-" * 34)
for n in [1, 2, 5, 10]:
    top_dets = wfn_sci.get_top_determinants(max_determinants=n)
    det_list = list(top_dets.keys())
    _, wfn_pmc = pmc.run(loc_ham, det_list)

    c_sci = np.array([wfn_sci.get_coefficient(d) for d in det_list])
    c_pmc = np.array([wfn_pmc.get_coefficient(d) for d in det_list])
    fidelity = float(np.abs(np.vdot(c_pmc, c_sci)) ** 2)
    depth = sp.run(wfn_pmc).get_qiskit_circuit().depth()

    print(f"{n:<8} {fidelity:>10.4f} {depth:>12}")

print()
print("PMC re-optimizes coefficients within the subspace — unlike simple truncation.")
print("Fidelity → 1.0 as N grows: more determinants, better QPE success probability.")
# Carry the 10-det PMC-refined state forward to Chapter 6
top_10 = list(wfn_sci.get_top_determinants(max_determinants=10).keys())
_, wfn_pmc_10 = pmc.run(loc_ham, top_10)