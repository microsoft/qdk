# Localized orbitals: after MP2 localization
cube_localized = generate_cubefiles_from_orbitals(wfn_mp2.get_orbitals(), indices=show)
MoleculeViewer(molecule_data=structure.to_xyz(), cube_data=cube_localized, isoval=0.02)