from typing import Sequence
import cirq
import numpy as np

def print_quantum_state(state, msg, threshold=1e-8):
    state_vector = np.array(state.state_vector())
    n_qubits = int(np.log2(len(state_vector)))

    print(msg)
    for i, amplitude in enumerate(state_vector):
        if abs(amplitude) > threshold:
            # Convert index to binary representation for basis state
            binary_str = format(i, f'0{n_qubits}b')
            basis_state = "|" + binary_str + "⟩"
            print(f"{basis_state}: {amplitude}")

def PrepareBellPair(qubit1: cirq.Qid, qubit2: cirq.Qid) -> Sequence[cirq.Operation]:
    return [
        cirq.H.on(qubit1),
        cirq.CNOT.on(qubit1, qubit2)
    ]

def PrepareMessage(alice: cirq.Qid, message: cirq.Qid) -> Sequence[cirq.Operation]:
    return [
        cirq.CNOT.on(message, alice),
        cirq.H.on(message)
    ]

def teleportation_with_state():
    simulator = cirq.Simulator()
    circuit = cirq.Circuit()

    alice = cirq.NamedQubit("alice_ent")  # Alice's entangled qubit
    bob = cirq.NamedQubit("bob_ent")      # Bob's entangled qubit

    circuit.append(PrepareBellPair(alice, bob))

    result = simulator.simulate(circuit) # NOTE simulate call here to get state vector
    print_quantum_state(result, "Quantum state after entangling Alice and Bob:")

    msg = cirq.NamedQubit("msg")  # Alice's message qubit
    circuit.append( cirq.rx(0.7)(msg) )  # Example state preparation

    result = simulator.simulate(circuit)
    print_quantum_state(result, "Quantum state after preparing state:")

    circuit.append(PrepareMessage(alice, msg))

    circuit.append(cirq.measure(msg, key='b1'))
    circuit.append(cirq.measure(alice, key='b2'))

    # At this point classical bits b1 and b2 are "sent" to the Bob's site.
    
    # Decode the message by applying corrections based on classical data b1 and b2.
    circuit.append(cirq.Z.on(bob).with_classical_controls('b1'))
    circuit.append(cirq.X.on(bob).with_classical_controls('b2'))

    result = simulator.simulate(circuit)
    print_quantum_state(result, "Quantum state after decoding:")

    circuit.append( cirq.rx(-0.7)(bob) )

    result = simulator.simulate(circuit)
    print_quantum_state(result, "Quantum state at the end:")

    circuit.append( cirq.measure(bob, key='result') )

if __name__ == "__main__":
    teleportation_with_state()