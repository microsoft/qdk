<h2 style="color:#D30982;">Part 4: Building and Registering the Selector</h2>

To build a custom active space selector, subclass `ActiveSpaceSelector` and implement two methods:

- `name(self) → str` — the registry key; used in `create("active_space_selector", name)` and shown by `available()`
- `_run_impl(self, wavefunction) → Wavefunction` — selection logic; receives the input wavefunction and must return a new `Wavefunction` with active space indices set on its `Orbitals`

The `OrbitalEnergyWindowSelector` below selects all molecular orbitals whose SCF orbital energy (Fock eigenvalue) lies within `±window_hartree` of the HOMO-LUMO midpoint. Unlike `qdk_occupation` (which uses fractional RDM occupations from a post-HF run), this criterion is computable directly from the HF solution — a fast heuristic for initial active space estimation.

The construction pattern in `_run_impl`: compute the midpoint between HOMO (ε[n_α−1]) and LUMO (ε[n_α]); select all orbitals within the window; identify frozen core orbitals (occupied but outside the window); build new `Orbitals` with those indices; wrap in a `Wavefunction` with the HF reference determinant.

Fill in the `name()` return value and the `register()` call in the cell below.