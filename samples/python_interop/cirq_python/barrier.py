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

MPrep = MessagePrepGate()

class MessageCorrection(cirq.Gate):
    """Custom two-qubit gate that performs message correction."""

    def __init__(self, b1: str, b2: str):
        super().__init__()
        self.b1 = b1
        self.b2 = b2

    def _num_qubits_(self) -> int:
        return 1

    def _decompose_(self, qubits):
        bob, = qubits
        yield cirq.Z.on(bob).with_classical_controls(self.b1)
        yield cirq.X.on(bob).with_classical_controls(self.b2)

    def _circuit_diagram_info_(self, args):
        return f"Z^{self.b1} X^{self.b2}"

# NOTE: No helper for MessageCorrection since it needs parameters

class BarrierGate(cirq.Gate):
    def __init__(self, n_qubits: int) -> None:
        super().__init__()
        self._n = n_qubits

    def _num_qubits_(self) -> int:
        return self._n

    def _decompose_(self, qubits: Sequence[cirq.Qid]) -> cirq.OP_TREE:
        return []

    def _circuit_diagram_info_(
        self, args: cirq.CircuitDiagramInfoArgs
    ) -> cirq.CircuitDiagramInfo:
        return cirq.CircuitDiagramInfo(wire_symbols=("│",) * self._n)


def simple_teleportation() -> Sequence[cirq.Operation]:
    alice = cirq.NamedQubit("alice")
    bob = cirq.NamedQubit("bob")

    program: Sequence[cirq.Operation] = []

    program.append(BP.on(alice, bob))

    msg = cirq.NamedQubit("msg")
    program.append( cirq.rx(0.7)(msg) ) # Example state preparation

    program.append(MPrep.on(msg, alice))

    program.append(cirq.measure(msg, key='b1'))
    program.append(cirq.measure(alice, key='b2'))

    # At this point classical bits b1 and b2 are "sent" to the Bob's site.
  
    # Decode the message by applying corrections based on classical data b1 and b2.

    #################################################
    # Here we need to insert a synchronization point.
    # Measurements must happen before use of the results.
    # Uncomment one of the following two lines to try the two approaches.
    #################################################
    program.append(BarrierGate(3).on(msg, alice, bob))  # Example barrier
    # program.append(cirq.Moment())  # Example moment
    #################################################

    program.append(MessageCorrection('b1', 'b2').on(bob))

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

    decomposed_custom = cirq.Circuit(cirq.decompose(circuit, keep=keep))
    print("\nDecomposed Custom Gates:")
    print(decomposed_custom)

    simulator = cirq.Simulator()
    result = simulator.run(circuit, repetitions=1000)
    hist = result.histogram(key='result')
    print(hist)