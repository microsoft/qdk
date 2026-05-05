<h2 style="color:#D30982;">Part 2: Plugins enable runtime execution</h2>

Importing the plugin modules doesn't add new names to the registry — the registry already knows about all algorithm types and implementations. What plugins provide is the **runtime backend**: without `plugins.pyscf`, calling `scf_solver.run()` would fail even though `scf_solver` appears in `available()`. Without `plugins.qiskit`, state preparation and qubit mapping would be unavailable at runtime.

This is an intentional design: dependencies (PySCF, Qiskit) are optional. The registry API is stable regardless of which backends are installed.

Import both plugins below. Confirm the registry is unchanged, then verify what the plugins actually enable by checking which implementations require them to run.