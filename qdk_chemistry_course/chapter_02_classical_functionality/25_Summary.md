<h2 style="color:#D30982;">Summary</h2>

In this chapter you:
- Used `compute_valence_space_parameters()` and `qdk_valence` to reduce to the chemically relevant orbital subspace
- Applied three orbital localizers and compared their output via `get_summary()`
- Explored the PySCF plugin localizer (Foster-Boys, Edmiston-Ruedenberg) and ran the stability checker to test whether the HF wavefunction is a true energy minimum

The MP2-localized valence wavefunction (`wfn_mp2`) is the recommended input for active space selection in Chapter 3.

**Key pattern to remember:**
```python
num_val_e, num_val_o = compute_valence_space_parameters(wfn_hf, charge=0)
wfn_valence = create("active_space_selector", "qdk_valence",
                     num_active_electrons=num_val_e,
                     num_active_orbitals=num_val_o).run(wfn_hf)
indices = wfn_valence.get_orbitals().get_active_space_indices()
wfn_loc = create("orbital_localizer", "qdk_mp2_natural_orbitals").run(wfn_valence, *indices)
```