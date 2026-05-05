<h1 style="color:#D30982;text-align:center;">Chapter 7: Plugins &amp; Extending the QDK</h1>

<h2 style="color:#D30982;">What you'll learn</h2>

- How the QDK algorithm registry works: factories, `available()`, `create()`, `register()`
- What external plugins (PySCF, Qiskit) add to the registry at import time
- How to define custom settings with type constraints and defaults
- How to subclass an existing algorithm base class and register a new implementation
- How to build a custom active space selector using SCF orbital energies as the selection criterion

<h2 style="color:#D30982;">The plugin system</h2>

Every algorithm in the QDK is accessed through a central registry using three functions you have used throughout this course: `available()`, `create()`, and `print_settings()`. This chapter reveals what is happening behind the scenes — and shows you how to add your own algorithms to the same registry.

The registry uses a **factory pattern**: each algorithm type (e.g., `"active_space_selector"`, `"scf_solver"`) has one factory object that tracks all available implementations. When you call `create("active_space_selector", "qdk_valence")`, the factory looks up the `"qdk_valence"` implementation and returns a fresh instance. When you call `register(lambda: MySelector())`, you are adding a new implementation to the factory for your algorithm's type — making it available to `create()` like any built-in.

External packages like PySCF and Qiskit connect to this system through plugins: importing `qdk_chemistry.plugins.pyscf` triggers a registration sequence that adds PySCF-backed implementations to several factories. After the import, those implementations appear in `available()` alongside the native QDK ones.

The chapter ends with a complete, working example: an `OrbitalEnergyWindowSelector` that selects active orbitals based on how close their Fock eigenvalue is to the HOMO-LUMO midpoint — a different physical criterion from the built-in occupation-based selectors.