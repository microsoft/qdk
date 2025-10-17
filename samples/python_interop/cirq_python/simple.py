from typing import Sequence
import cirq

def qrng() -> Sequence[cirq.Operation]:
    a = cirq.NamedQubit("a")                 # NOTE: Also LinearQubit, GridQubit
    return [
        cirq.H.on(a),                        # NOTE Gate vs Operation
        cirq.measure(a, key='result')
    ]

if __name__ == "__main__":
    program = qrng()
    print(program)

    circuit = cirq.Circuit(program)
    print(circuit)

    simulator = cirq.Simulator()
    result = simulator.run(circuit, repetitions=1000)
    hist = result.histogram(key='result')
    print(hist)
