# Explore all registered algorithm types and implementations
print("All algorithm types and their implementations:")
for alg_type, impls in available().items():
    print(f"  {alg_type:<45} {impls}")
print()
print("Default implementations:")
for alg_type, default in registry.show_default().items():
    print(f"  {alg_type:<45} \u2192 {default!r}")
# The factory pattern: each algorithm type has one factory.
# register(generator)        → adds a new implementation to the matching factory
# register_factory(factory)  → adds an entirely new algorithm type
# create()                   → asks the factory to instantiate by name
# available()                → lists what the factory currently knows about