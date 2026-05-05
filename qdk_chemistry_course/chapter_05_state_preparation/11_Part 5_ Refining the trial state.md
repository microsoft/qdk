<h2 style="color:#D30982;">Part 5: Refining the trial state</h2>

`truncate_wavefunction` (Part 3) simply renormalizes the top-N SCI coefficients — the coefficients are frozen at their SCI values, just rescaled. The **projected multi-configuration calculator** (PMC) does better: given the same N determinants, it re-solves the Hamiltonian in that subspace and re-optimizes the coefficients. The result is a lower variational energy and, critically, higher **fidelity** with the true ground state.

Fidelity = $|\langle \psi_{pmc} | \psi_{pmc} \rangle |^2$ is the probability that QPE (Chapter 6) collapses to the ground state on the first shot (<a href="https://arxiv.org/abs/quant-ph/0005055" target="_blank">Brassard et al., 2002</a>). A trial state with fidelity F succeeds with probability F — making fidelity the direct link between state preparation quality and QPE reliability.

In the cell below, fill in the "???" and then answer the subsequent question.