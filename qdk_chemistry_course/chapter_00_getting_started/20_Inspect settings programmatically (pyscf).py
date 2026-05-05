# Inspect settings programmatically (pyscf)
settings_info = inspect_settings("scf_solver", "pyscf")

print(f"{'Name':<25} {'Type':<10} {'Default':<15} {'Limits'}")
print("-" * 70)
for name, python_type, default, description, limits in settings_info:
    print(f"{name:<25} {str(python_type):<10} {str(default):<15} {str(limits) if limits else ''}")