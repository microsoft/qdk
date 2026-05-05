<h2 style="color:#D30982;">Part 3: Applying MP2 natural orbital localization</h2>

`qdk_mp2_natural_orbitals` uses second-order Møller-Plesset perturbation theory to compute natural orbitals, which are the eigenstates of the correlated one-particle density matrix. These have fractional occupations that directly indicate correlation strength: occupations far from 0 or 2 flag strongly correlated orbitals.

The localizer's `run()` method takes the wavefunction and the active space index range (occupied start, virtual end). We will apply MP2 natural orbital localization to the valence space, and compare the orbital summary before and after.