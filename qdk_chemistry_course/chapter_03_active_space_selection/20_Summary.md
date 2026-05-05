<h2 style="color:#D30982;">Summary</h2>

In this chapter you:
- Surveyed all five active space selectors and their settings
- Applied `qdk_valence` directly from the HF wavefunction with three manual configurations
- Applied `pyscf_avas` with AO labels (`"N 2s"`, `"N 2p"`) to select by atomic character from the HF wavefunction
- Built a MACIS ASCI post-HF wavefunction with 1- and 2-RDM, then ran entropy-based (`qdk_autocas`, `qdk_autocas_eos`) and occupation-based (`qdk_occupation`) selectors
- Compared all selections by active orbital count and energy accuracy for stretched N₂ with cc-pvdz

The autoCAS-EOS wavefunction (`wfn_eos`) is the recommended output to carry forward: it uses the most automated, knowledge-free selection criterion and is the input to Hamiltonian construction in Chapter 4.

**Key pattern:**
```python
# Build Hamiltonian → run CASCI with RDMs → autoCAS-EOS
hamiltonian_constructor = create("hamiltonian_constructor")
loc_hamiltonian = hamiltonian_constructor.run(wfn_mp2.get_orbitals())

macis = create("multi_configuration_calculator", "macis_asci",
               calculate_one_rdm=True, calculate_two_rdm=True)
macis.settings().set("core_selection_strategy", "fixed")
_, wfn_sci = macis.run(loc_hamiltonian, *wfn_mp2.get_active_num_electrons())

wfn_active = create("active_space_selector", "qdk_autocas_eos").run(wfn_sci)
```