# Load N₂ structure from file and from coordinates
structure = Structure.from_xyz_file(N2_XYZ)
print("Loaded from file:")
print(structure.to_xyz())

# Method 2: build directly from coordinates (in Bohr)
# 1.27 Å × ANGSTROM_TO_BOHR (≈ 1.889 Bohr/Å) = 2.4008 Bohr
N2_BOND_BOHR = 1.27 * ANGSTROM_TO_BOHR
structure_manual = Structure(
    coordinates=np.array([[0.0, 0.0, 0.0], [0.0, 0.0, N2_BOND_BOHR]]),
    symbols=["N", "N"]
)
print("\nBuilt from coordinates (Bohr input):")
print(structure_manual.to_xyz())