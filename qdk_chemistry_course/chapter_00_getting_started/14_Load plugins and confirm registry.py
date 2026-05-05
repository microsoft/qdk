# Load plugins and confirm registry
import qdk_chemistry.plugins.pyscf   # enables: scf_solver, orbital_localizer, stability_checker
import qdk_chemistry.plugins.qiskit  # enables: qubit_mapper (qiskit encoding), state_prep (qiskit_regular_isometry)

registry_after_plugins = available()

# Confirm the registry names are unchanged: plugins affect execution, not registration
print("Registry is identical before and after plugin import:", registry_before_plugins == registry_after_plugins)
print()
print("Algorithm types and implementations (with plugins loaded):")
for alg_type, implementations in registry_after_plugins.items():
    print(f"  {alg_type}: {implementations}")