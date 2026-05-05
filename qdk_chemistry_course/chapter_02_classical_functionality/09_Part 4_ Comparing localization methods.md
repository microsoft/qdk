<h2 style="color:#D30982;">Part 4: Comparing localization methods</h2>

The QDK provides two additional localizers with different objectives:

- **`qdk_pipek_mezey`**: Maximizes orbital localization by minimizing the spread of each orbital over atomic centers (<a href="https://doi.org/10.1063/1.456588" target="_blank">Pipek & Mezey, 1989</a>). Produces orbitals resembling lone pairs and bonds. Takes the active space indices.
- **`qdk_vvhv`**: Valence-virtual hybrid localizer. Unlike the others, it requires **all virtual orbital indices** (from `n_alpha_electrons` to `num_molecular_orbitals-1`) — not just the active space. It localizes the entire virtual manifold, which is useful when downstream steps need well-localized virtual orbitals beyond the active space.

We will apply both localizers to `wfn_valence`, and compare all three orbital summaries side by side.