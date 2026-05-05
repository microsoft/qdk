<h1 style="color:#D30982;text-align:center;">Chapter 2: Classical Functionality: SCF, Localization, Stability</h1>

<h2 style="color:#D30982;">What you'll learn</h2>

- Why canonical SCF orbitals are often unsuitable for quantum workflows — and how localization fixes this
- The built-in QDK localizers (`qdk_mp2_natural_orbitals`, `qdk_pipek_mezey`, `qdk_vvhv`) and the PySCF plugin localizer (Foster-Boys, Edmiston-Ruedenberg)
- How to interpret `get_summary()` to compare orbital character before and after localization
- What the stability checker does, when to use it, and how stability instability differs from convergence failure

<h2 style="color:#D30982;">Why localize?</h2>

Canonical molecular orbitals from SCF are delocalized across the entire molecule: they're eigenstates of the Fock operator, optimized for energy, not for physical interpretability. For active space selection, delocalized orbitals obscure which electrons are strongly correlated. Localization rotates the orbital basis to produce spatially compact, chemically meaningful orbitals without changing the total energy or wavefunction.