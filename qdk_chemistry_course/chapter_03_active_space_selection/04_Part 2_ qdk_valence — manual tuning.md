<h2 style="color:#D30982;">Part 2: qdk_valence — manual tuning</h2>

`qdk_valence` is the simplest selector: it takes explicit electron and orbital counts and carves out that subspace from the SCF wavefunction. `compute_valence_space_parameters()` infers sensible defaults, but those are just a starting point.

For stretched N₂ with cc-pvdz, the valence heuristic returns 10 electrons in 8 orbitals. You can shrink to (6e, 6o) to focus on the σ-system only, or expand to (10e, 10o) to include more virtuals. Each choice has a different qubit cost.

Let's run three `qdk_valence` configurations and compare their qubit footprints.