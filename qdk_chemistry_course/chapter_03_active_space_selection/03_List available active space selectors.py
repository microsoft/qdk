# List available active space selectors
selectors = available("active_space_selector")
print(f"Available active space selectors: {selectors}")

print()
for name in selectors:
    print(f"\n--- {name} ---")
    print_settings("active_space_selector", name)
