<h2 style="color:#D30982;">Interoperability</h2>

The Chemistry QDK is designed to work alongside other quantum software stacks. The typed objects produced at each pipeline stage can be exported to — or built from — external libraries.

| External stack | Integration point | How |
|---|---|---|
| **Qiskit** | `QubitHamiltonian`, `Circuit`, circuit executor | Via optional plugin: `qubit_mapper` can produce a Qiskit `SparsePauliOp`; `qiskit_aer_simulator` runs circuits |
| **Cirq** | `Circuit` | Via QASM3 export: `circuit.get_qasm()` → `cirq.from_qasm()` |
| **PennyLane** | `Circuit` | Via QASM3 export: `circuit.get_qasm()` → PennyLane `from_qasm()` |
| **OpenFermion** | `Hamiltonian` | Export one- and two-body integrals via `get_one_body_integrals()` / `get_two_body_integrals()` |
| **RDKit** | `Structure` input | Build `Structure` from SMILES via `Structure(coordinates=coords, symbols=symbols)` |

Qiskit is not a dependency of the Chemistry QDK, but rather an optional plugin. The section below presents a few examples that illustrate what interoperability looks like. These cells require `qdk-chemistry[qiskit-extras]` to be installed. They show what interoperability looks like and are not a turnkey workflow.