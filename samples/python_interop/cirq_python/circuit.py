from typing import Sequence
import cirq

def PrepareBellPair(qubit1: cirq.Qid, qubit2: cirq.Qid) -> Sequence[cirq.Operation]:
    return [
        cirq.H.on(qubit1),     # NOTE Gate vs Operation
        cirq.CNOT.on(qubit1, qubit2)
    ]

def PrepareMessage(alice: cirq.Qid, message: cirq.Qid) -> Sequence[cirq.Operation]:
    return [
        cirq.CNOT.on(message, alice),
        cirq.H.on(message)
    ]

def simple_teleportation() -> Sequence[cirq.Operation]:
    alice = cirq.NamedQubit("alice")
    bob = cirq.NamedQubit("bob")

    program: Sequence[cirq.Operation] = []

    program.extend(PrepareBellPair(alice, bob))

    msg = cirq.NamedQubit("msg")
    program.append( cirq.rx(0.7)(msg) ) # Example state preparation

    program.extend(PrepareMessage(alice, msg))

    program.append(cirq.measure(msg, key='b1')) # NOTE classical control line name
    program.append(cirq.measure(alice, key='b2'))

    # At this point classical bits b1 and b2 are "sent" to the Bob's site.
    
    # Decode the message by applying corrections based on classical data b1 and b2.
    program.append(cirq.Z.on(bob).with_classical_controls('b1')) # NOTE classical control line name
    program.append(cirq.X.on(bob).with_classical_controls('b2'))

    program.append( cirq.rx(-0.7)(bob) )
    program.append( cirq.measure(bob, key='result') )
    return program


if __name__ == "__main__":
    program = simple_teleportation()
    circuit = cirq.Circuit(program)
    print("Simple Teleportation Circuit:")
    print(circuit)

    simulator = cirq.Simulator()
    result = simulator.run(circuit, repetitions=1000)
    hist = result.histogram(key='result')
    print(hist)
