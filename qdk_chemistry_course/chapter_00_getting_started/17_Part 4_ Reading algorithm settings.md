<h2 style="color:#D30982;">Part 4: Reading algorithm settings</h2>

Every algorithm exposes its configurable parameters through a settings interface. `inspect_settings(type, name)` returns a structured list of `(name, type, default, description, limits)` tuples. `print_settings(type, name)` prints a formatted table.

Inspect the settings for both SCF solver implementations and see if you can identify:

- Which parameter controls the SCF method (HF vs DFT)?
- What is the default convergence threshold?
- How do the settings differ between the <code>qdk</code> and <code>pyscf</code> implementations?
