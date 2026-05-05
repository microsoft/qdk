# Custom Settings: define configurable parameters for the selector
class OrbitalEnergyWindowSettings(Settings):
    def __init__(self):
        super().__init__()
        # _set_default(key, type_str, default, description, limits)
        # type_str options: 'int', 'double', 'bool', 'string', 'list[int]', 'list[double]'
        self._set_default(
            "window_hartree", "double", 1.0,
            "Half-width of energy window around the HOMO-LUMO midpoint (Hartree)",
            (0.01, 100.0),
        )

s = OrbitalEnergyWindowSettings()
print(f"Default: window_hartree = {s.get('window_hartree')}")
s.set("window_hartree", 0.5)
print(f"After set(0.5): window_hartree = {s.get('window_hartree')}")
print()
print("The settings object is attached to the selector via self._settings.")
print("Users configure it with sel.settings().set('window_hartree', 0.5) after create().")