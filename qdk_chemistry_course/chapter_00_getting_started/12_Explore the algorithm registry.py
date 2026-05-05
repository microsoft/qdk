# Explore the algorithm registry
from qdk_chemistry.algorithms import available

# Snapshot the registry before loading any plugins
registry_before_plugins = {k: list(v) for k, v in available().items()}

print("Algorithm types available (no plugins loaded):")
for alg_type, implementations in registry_before_plugins.items():
    print(f"  {alg_type}: {implementations}")