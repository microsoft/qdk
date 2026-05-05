# Canonical orbitals: before localization
from qdk_chemistry.utils.cubegen import generate_cubefiles_from_orbitals
from qdk.widgets import MoleculeViewer

n_alpha, _ = wfn_valence.get_total_num_electrons()
show = [n_alpha - 1, n_alpha]  # HOMO and LUMO

cube_canonical = generate_cubefiles_from_orbitals(wfn_valence.get_orbitals(), indices=show)
MoleculeViewer(molecule_data=structure.to_xyz(), cube_data=cube_canonical, isoval=0.02)