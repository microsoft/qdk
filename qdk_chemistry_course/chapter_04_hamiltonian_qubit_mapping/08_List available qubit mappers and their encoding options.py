# List available qubit mappers and their encoding options
print("Available qubit mappers:", available("qubit_mapper"))
print()
for name in available("qubit_mapper"):
    print(f"--- {name} ---")
    print_settings("qubit_mapper", name)
    print()

# Two mapper implementations: 'qdk' (native) and 'qiskit' (requires plugin).
# 'qdk' supports Jordan-Wigner and Bravyi-Kitaev.
# 'qiskit' additionally supports Parity encoding.
# All encodings preserve the spectrum — they differ in how locality is distributed across qubits.
