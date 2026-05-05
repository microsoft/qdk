**Key rules:**
- <code>orbital_localizer</code> and <code>active_space_selector</code> both take and return <code>Wavefunction</code> — not <code>Orbitals</code>
- <code>hamiltonian_constructor</code> takes <code>Orbitals</code> — call <code>.get_orbitals()</code> first
- <code>state_prep</code> takes <code>Wavefunction</code> (the CASCI/ASCI result), not <code>Orbitals</code>
- Core energy (<code>hamiltonian.get_core_energy()</code>) must be added to QPE raw energy to get the total
