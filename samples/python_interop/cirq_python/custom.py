from typing import Sequence
import cirq


class BellPairGate(cirq.Gate):
    """Custom two-qubit gate that prepares a Bell pair."""

    def __init__(self):
        super().__init__()

    def _num_qubits_(self) -> int:
        return 2

    def _decompose_(self, qubits):
        control, target = qubits
        yield cirq.H(control)
        yield cirq.CNOT(control, target)

    def _circuit_diagram_info_(self, args):
        return "BP", "BP"

BP = BellPairGate()


class MessagePrepGate(cirq.Gate):
    """Custom two-qubit gate that prepares Alice's message qubit."""

    def __init__(self):
        super().__init__()

    def _num_qubits_(self) -> int:
        return 2

    def _decompose_(self, qubits):
        message, alice = qubits
        yield cirq.CNOT(message, alice)
        yield cirq.H(message)

    def _circuit_diagram_info_(self, args):
        return "PM_M", "PM_A"

MP = MessagePrepGate()


def simple_teleportation() -> Sequence[cirq.Operation]:
    alice = cirq.NamedQubit("alice")
    bob = cirq.NamedQubit("bob")

    program: Sequence[cirq.Operation] = []

    program.append(BP.on(alice, bob))

    msg = cirq.NamedQubit("msg")
    program.append( cirq.rx(0.7)(msg) ) # Example state preparation

    program.append(MP.on(msg, alice))

    program.append(cirq.measure(msg, key='b1'))
    program.append(cirq.measure(alice, key='b2'))

    # At this point classical bits b1 and b2 are "sent" to the Bob's site.
    
    # Decode the message by applying corrections based on classical data b1 and b2.
    program.append(cirq.Z.on(bob).with_classical_controls('b1')) # NOTE classical control line name
    program.append(cirq.X.on(bob).with_classical_controls('b2'))

    program.append( cirq.rx(-0.7)(bob) )
    program.append( cirq.measure(bob, key='result') )
    return program


def keep(op: cirq.Operation) -> bool:
    gate = getattr(op, "gate", None)
    return not isinstance(gate, (BellPairGate, MessagePrepGate))


if __name__ == "__main__":
    program = simple_teleportation()
    circuit = cirq.Circuit(program)
    print("Simple Teleportation Circuit:")
    print(circuit)

    decomposed_circuit = cirq.Circuit(cirq.decompose(circuit))
    print("\nDecomposed:")
    print(decomposed_circuit)

    decomposed_custom = cirq.Circuit(cirq.decompose(circuit, keep=keep))
    print("\nDecomposed Custom Gates:")
    print(decomposed_custom)
