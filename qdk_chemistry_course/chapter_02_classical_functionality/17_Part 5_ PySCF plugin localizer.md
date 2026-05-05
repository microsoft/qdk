<h2 style="color:#D30982;">Part 5: PySCF plugin localizer</h2>

The three built-in QDK localizers (`qdk_mp2_natural_orbitals`, `qdk_pipek_mezey`, `qdk_vvhv`) cover the most common workflows. Importing the PySCF plugin registers an additional `"pyscf_multi"` localizer that exposes PySCF's full localization suite via a `method` setting:

| `method` | Algorithm |
|---|---|
| `"pipek-mezey"` | Pipek-Mezey (default) |
| `"foster-boys"` | <a href="https://doi.org/10.1103/RevModPhys.32.300" target="_blank">Foster-Boys</a> |
| `"edmiston-ruedenberg"` | Edmiston-Ruedenberg |
| `"cholesky"` | Cholesky-based |

In the code cell below, we will apply Foster-Boys localization via the PySCF plugin and compare the orbital summary with the built-in Pipek-Mezey result from Part 4. To run the code, fill in the ```???``` first, and then answer the question below.