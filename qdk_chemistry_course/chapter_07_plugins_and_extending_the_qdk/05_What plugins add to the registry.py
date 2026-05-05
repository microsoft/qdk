# External plugins register their implementations at import time
# The pyscf plugin adds: pyscf (scf_solver), pyscf_multi (orbital_localizer), pyscf_avas (active_space_selector)
# The qiskit plugin adds: qiskit_regular_isometry (state_prep), qiskit_standard (phase_estimation),
#                          qiskit_aer_simulator (circuit_executor)

print(f"{'Algorithm type':<38} {'Plugin source':<18} {'Implementation added'}")
print("-" * 78)
for alg_type, plugin, impl in [
    ("active_space_selector",  "pyscf plugin",  "pyscf_avas"),
    ("scf_solver",             "pyscf plugin",  "pyscf"),
    ("orbital_localizer",      "pyscf plugin",  "pyscf_multi"),
    ("state_prep",             "qiskit plugin", "qiskit_regular_isometry"),
    ("phase_estimation",       "qiskit plugin", "qiskit_standard"),
    ("circuit_executor",       "qiskit plugin", "qiskit_aer_simulator"),
]:
    present = impl in available(alg_type)
    status = "✓ loaded" if present else "✗ not loaded"
    print(f"  {alg_type:<36} {plugin:<18} {impl:<30} {status}")
print()
print("Full active_space_selector list:", available("active_space_selector"))