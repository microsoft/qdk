# Import Qiskit SparsePauliOp into the QDK
import numpy as np                                                                                                                        
from qiskit.quantum_info import SparsePauliOp                                                                                             
from qiskit import QuantumCircuit, qasm3                                                                                                  
from qiskit.circuit.library import StatePreparation as QiskitStatePreparation                                                             
from qdk_chemistry.data import Circuit, QubitHamiltonian
from qdk_chemistry.algorithms import create

# H2 minimal basis 2-qubit Hamiltonian defined in Qiskit
h2_op = SparsePauliOp.from_list([
    ("II", -1.05342108),
    ("IZ",  0.39484436),
    ("XX",  0.18121046),
    ("ZI", -0.39484436),
    ("ZZ", -0.01124616),
])

# Bridge into QDK — fill in the blanks
qdk_h2 = QubitHamiltonian(
    pauli_strings="???",  
    coefficients="???",   
)

# Trial state: HF reference (|01⟩ in parity basis)
trial_vec = np.array([0.0, 1.0, 0.0, 0.0], dtype=complex)
qc = QuantumCircuit(2, name="trial")
qc.append(QiskitStatePreparation(trial_vec), [0, 1])
trial_circuit = Circuit(qasm3.dumps(qc))

# Run iterative QPE
iqpe = create("phase_estimation", "iterative",
              num_bits=6, evolution_time=np.pi/4, shots_per_bit=3)
simulator = create("circuit_executor", "qiskit_aer_simulator", seed=42)
evolution_builder = create("time_evolution_builder", "trotter")
circuit_mapper = create("controlled_evolution_circuit_mapper", "pauli_sequence")

result = iqpe.run(
    state_preparation=trial_circuit,
    qubit_hamiltonian=qdk_h2,
    circuit_executor=simulator,
    evolution_builder=evolution_builder,
    circuit_mapper=circuit_mapper,
)

energy = result.resolved_energy if result.resolved_energy is not None else result.raw_energy
NUCLEAR_REPULSION = 0.71510434 # hardcoded for now for H2 at 0.74 Å
print(f"Total ground state energy: {energy + NUCLEAR_REPULSION:.4f} Ha")