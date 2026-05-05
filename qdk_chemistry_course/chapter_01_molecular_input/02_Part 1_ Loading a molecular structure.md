<h2 style="color:#D30982;">Part 1: Loading a molecular structure</h2>

`Structure` is the entry point for all chemistry in the QDK. It wraps atomic coordinates and exposes methods for visualization and conversion.

The most common input route is an XYZ file via `Structure.from_xyz_file()`. You can also build a `Structure` directly from coordinates in memory using `Structure(coordinates=coords, symbols=symbols)` — useful when working programmatically or integrating with RDKit (see the interoperability section at the end of this chapter).

**Note**: the constructor takes coordinates in **Bohr**. XYZ files are in Angstrom and are converted automatically by `from_xyz_file()`. In the code snippet Load stretched N₂ from the XYZ file. Then build the same molecule directly from coordinates (in Bohr) and confirm they match.