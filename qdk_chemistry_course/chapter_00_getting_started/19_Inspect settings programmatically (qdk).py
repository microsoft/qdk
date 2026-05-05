# Inspect settings programmatically (qdk)
settings_info = inspect_settings("scf_solver", "qdk")

print(f"{'Name':<30} {'Type':<10} {'Default':<15} {'Limits'}")
print("-" * 75)
for name, python_type, default, description, limits in settings_info:
    print(f"{name:<30} {str(python_type):<10} {str(default):<15} {str(limits) if limits else ''}")