<h2 style="color:#D30982;">Part 3: Builder Comparison</h2>

`step_terms` maps directly to 2-qubit gate blocks in each IQPE iteration circuit — fewer terms means shallower circuits and less gate noise on hardware.

On the autoCAS-EOS Hamiltonian (161 Pauli terms): Trotter order 1 = 161 steps; Trotter order 2 = 321 (symmetric forward + backward); qDRIFT = 70 (random subset); partially randomized = 124 (deterministic core + random light terms). qDRIFT wins on raw depth but adds shot-to-shot variance. For a fixed `shots_per_bit` budget, partially randomized often gives the best error-vs-depth trade-off.