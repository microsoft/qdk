# Export QDK QubitHamiltonian to Qiskit
import qdk_chemistry.plugins.qiskit
from qdk_chemistry.utils import compute_valence_space_parameters
from qiskit.quantum_info import SparsePauliOp

# Build a minimal active space from the STO-3G wavefunction (already computed in Part 2)
num_val_e, num_val_o = compute_valence_space_parameters(wfn_sto3g, charge=0)
wfn_valence = create("active_space_selector", "qdk_valence",
                     num_active_electrons=num_val_e,
                     num_active_orbitals=num_val_o).run(wfn_sto3g)
hamiltonian = create("hamiltonian_constructor").run(wfn_valence.get_orbitals())

# Map to qubits using the Qiskit Jordan-Wigner mapper
qubit_mapper = create("qubit_mapper", "qiskit", encoding="jordan-wigner")
qubit_hamiltonian = qubit_mapper.run(hamiltonian)

print(f"QDK QubitHamiltonian: {len(qubit_hamiltonian.pauli_strings)} Pauli strings")

# Construct a native Qiskit SparsePauliOp from the QDK representation
qiskit_op = SparsePauliOp(qubit_hamiltonian.pauli_strings, qubit_hamiltonian.coefficients)
print(f"Qiskit object type:   {type(qiskit_op).__name__}")
print(f"Num qubits:           {qiskit_op.num_qubits}")
print(f"\nFirst 3 Pauli terms:")
for pauli, coeff in list(zip(qiskit_op.paulis, qiskit_op.coeffs))[:3]:
    print(f"  {pauli}  coeff={coeff:.6f}")