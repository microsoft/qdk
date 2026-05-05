# OrbitalEnergyWindowSelector: full implementation and registration
class OrbitalEnergyWindowSelector(ActiveSpaceSelector):
    """Selects active orbitals within an energy window around the HOMO-LUMO midpoint.

    Unlike occupation-based selectors (qdk_occupation), this criterion uses SCF orbital
    energies from the Fock matrix — selecting orbitals whose energy lies within
    ±window_hartree of the midpoint between HOMO and LUMO.
    """
    def __init__(self):
        super().__init__()
        self._settings = OrbitalEnergyWindowSettings()

    def name(self):
        return "???"  # fill in: the registry key used in create("active_space_selector", "???")

    def _run_impl(self, wavefunction):
        orbs          = wavefunction.get_orbitals()
        n_a, _        = wavefunction.get_total_num_electrons()
        n_mo          = orbs.get_num_molecular_orbitals()
        energies_a, _ = orbs.get_energies()
        window        = self.settings().get("window_hartree")

        midpoint     = (energies_a[n_a - 1] + energies_a[n_a]) / 2.0
        active_idx   = [i for i in range(n_mo) if abs(energies_a[i] - midpoint) <= window]
        inactive_idx = [i for i in range(n_a) if i not in active_idx]
        n_active_a   = sum(1 for i in active_idx if i < n_a)
        det_str      = "2" * n_active_a + "0" * (len(active_idx) - n_active_a)

        a_coeff, _  = orbs.get_coefficients()
        bs          = orbs.get_basis_set()
        new_orbs    = Orbitals(a_coeff, None, None, bs, (active_idx, inactive_idx))
        hf_config   = Configuration(det_str)
        return Wavefunction(SciWavefunctionContainer(np.array([1.0]), [hf_config], new_orbs))


# Register with the QDK registry: supply a lambda factory
# Pattern: register(lambda: MyClass())
generator = None  # TODO: replace None with lambda: OrbitalEnergyWindowSelector()
if generator is not None:
    register(generator)

print("active_space_selector implementations after registration attempt:")
print(available("active_space_selector"))
print()
sel = create("active_space_selector", "energy_window")
print(f"Created: name={sel.name()!r}  type={sel.type_name()!r}")
print_settings("active_space_selector", "energy_window")