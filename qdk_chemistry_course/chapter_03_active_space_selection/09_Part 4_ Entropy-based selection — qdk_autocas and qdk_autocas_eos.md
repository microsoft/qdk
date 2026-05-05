<h2 style="color:#D30982;">Part 4: Entropy-based selection — qdk_autocas and qdk_autocas_eos</h2>

The autoCAS selectors (<a href="https://pubs.acs.org/doi/10.1021/acs.jctc.6b00156" target="_blank">Stein & Reiher, 2016</a>) are the most **automated** option: they derive the active space directly from the wavefunction's quantum information structure, requiring no prior knowledge of the molecule's chemistry. They compute **single-orbital entanglement entropies** from the 2-RDM of a multi-configuration wavefunction and orbitals with high entropy are strongly entangled with the rest of the system and belong in the active space; orbitals near zero can be frozen.

This requires a wavefunction with 1- and 2-RDM:
1. A classical Hamiltonian for the active space (via `hamiltonian_constructor`)
2. A multi-configuration wavefunction with 1-RDM and 2-RDM (via `multi_configuration_calculator`, here MACIS ASCI (<a href="https://aip.scitation.org/doi/10.1063/1.4955109" target="_blank">Tubman et al., 2016</a>))

`qdk_autocas_eos` adds an *entropy of substitution* step: it tests whether swapping each active orbital changes the entropy landscape, catching orbitals the standard criterion misses.

In the cells below, we build the Hamiltonian from `wfn_mp2`, run MACIS ASCI with RDMs, then apply both autoCAS methods. Compare what each selects.