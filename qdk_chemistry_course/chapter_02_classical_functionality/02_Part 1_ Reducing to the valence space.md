<h2 style="color:#D30982;">Part 1: Reducing to the valence space</h2>

Before localizing, we reduce to the **valence space**, the chemically relevant frontier orbitals. The N₂ cc-pvdz calculation produces 28 orbitals, most of which are high-energy virtuals that contribute negligibly to chemistry. Working in the valence space is a prerequisite for localization in this workflow.

`compute_valence_space_parameters()` is a utility that computes the number of valence electrons and orbitals for a given wavefunction. The `qdk_valence` active space selector then applies these to extract the valence subspace. Here, let's compute the valence space parameters for stretched N₂ and apply the valence selector. Take a look at the result.