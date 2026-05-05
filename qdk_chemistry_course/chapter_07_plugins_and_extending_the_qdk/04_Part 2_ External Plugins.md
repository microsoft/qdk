<h2 style="color:#D30982;">Part 2: External Plugins</h2>

Plugins are Python modules that call `register()` at import time. Importing `qdk_chemistry.plugins.pyscf` adds PySCF-backed implementations to several existing factories; importing `qdk_chemistry.plugins.qiskit` adds Qiskit-backed implementations. Once imported, their implementations are indistinguishable from native QDK ones — you create and configure them the same way.

The table below shows which implementations each plugin adds. All of these were imported in the setup cell, which is why they appear in `available()` throughout this course: `pyscf_avas`, `pyscf` (SCF), `pyscf_multi` (localizer) from the PySCF plugin; `qiskit_regular_isometry`, `qiskit_standard`, `qiskit_aer_simulator` from the Qiskit plugin.