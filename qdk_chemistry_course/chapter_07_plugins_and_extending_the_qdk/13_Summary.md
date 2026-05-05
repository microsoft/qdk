<h2 style="color:#D30982;">Summary</h2>

In this chapter you:
- Explored the QDK algorithm registry: `available()`, `show_default()`, `create()`, and the factory pattern
- Mapped what the PySCF and Qiskit plugins add to the registry at import time
- Defined a custom `Settings` subclass with a typed, constrained `window_hartree` parameter
- Built `OrbitalEnergyWindowSelector`: a custom `ActiveSpaceSelector` that selects orbitals by Fock eigenvalue proximity to the HOMO-LUMO midpoint
- Registered it with `register(lambda: OrbitalEnergyWindowSelector())` and verified it appears in `available()`
- Compared the energy-window selection to the built-in `qdk_valence` selector across window sizes

**Key pattern (custom algorithm registration):**
```python
class MySelector(ActiveSpaceSelector):
    def __init__(self):
        super().__init__()
        self._settings = MySettings()    # custom Settings subclass

    def name(self):
        return "my_selector"             # registry key

    def _run_impl(self, wavefunction):
        ...                              # selection logic
        return new_wavefunction          # Wavefunction with active space set

register(lambda: MySelector())           # adds to the active_space_selector factory
sel = create("active_space_selector", "my_selector")
```

**End-to-end course complete:**
Structure (Ch.0–1) → HF/SCF (Ch.2) → active space (Ch.3) → qubit Hamiltonian (Ch.4) → state prep (Ch.5) → QPE (Ch.6) → **plugins & extension** (Ch.7) → custom quantum chemistry workflows on Magne