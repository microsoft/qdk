<h2 style="color:#D30982;">Part 5: SCF settings and DFT</h2>

The SCF solver's `method` setting takes `"hf"` for Hartree-Fock or a DFT functional name for density functional theory — for example, `"b3lyp"`, `"pbe"`, or `"m06-2x"`. Settings are passed as keyword arguments to `create()`.

Let's run a DFT calculation on N₂ with `cc-pvdz`. Fill in the `???` below with a common hybrid functional, then compare the DFT and HF energies. Which is lower, and why?