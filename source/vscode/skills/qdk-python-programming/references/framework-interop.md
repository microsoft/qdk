# Framework Interop and Azure Quantum Submission

## Qiskit Interop

Use Qiskit circuits with the QDK simulator and resource estimator. Requires `pip install "qdk[qiskit]"`.

### Running Qiskit Circuits Locally

```python
from qiskit import QuantumCircuit
from qsharp.interop.qiskit import QSharpBackend

circuit = QuantumCircuit(2, 2)
circuit.h(0)
circuit.cx(0, 1)
circuit.measure([0, 1], [0, 1])

backend = QSharpBackend()
job = backend.run(circuit, shots=100)
counts = job.result().get_counts()
```

### Resource Estimation of Qiskit Circuits

```python
from qsharp.interop.qiskit import estimate, ResourceEstimatorBackend
from qsharp.estimator import EstimatorParams

circuit = QuantumCircuit(...)
params = EstimatorParams()
result = estimate(circuit, params, skip_transpilation=True)
```

### Generating QIR from Qiskit

```python
backend = QSharpBackend()
qir = backend.qir(circuit, target_profile=TargetProfile.Base)
```

## Cirq Interop

Requires `pip install "qdk[cirq]"`.

```python
import cirq
from qdk.openqasm import compile
from qdk import TargetProfile

q0, q1 = cirq.LineQubit.range(2)
circuit = cirq.Circuit(cirq.H(q0), cirq.measure(q0, key="m0"))

qasm_str = circuit.to_qasm(version="3.0")
qir = compile(qasm_str, target_profile=TargetProfile.Base)
```

## PennyLane Interop

```python
import pennylane as qml
from qdk.openqasm import compile
from qdk import TargetProfile

dev = qml.device("default.qubit", wires=2)

@qml.qnode(dev)
def circuit(theta):
    qml.H(0)
    qml.RY(theta, wires=1)
    return qml.expval(qml.PauliZ(1))

qasm_str = qml.to_openqasm(circuit)(0.3)
qir = compile(qasm_str, target_profile=TargetProfile.Base)
```

## Azure Quantum Submission

Submit compiled programs to Azure Quantum hardware and simulators. Requires `pip install "qdk[azure]"`.

### Workspace Setup

```python
from azure.quantum import Workspace

workspace = Workspace(
    subscription_id="...",
    resource_group="...",
    name="...",
    location="westus",
)
```

### Submit Q# to Azure

```python
qsharp.init(target_profile=qsharp.TargetProfile.Base)
qir = qsharp.compile("Main()")

target = workspace.get_targets("quantinuum.sim.h1-1e")
job = target.submit(qir, "my-job", shots=100)
job.wait_until_completed()
results = job.get_results()
```

### Submit from Any Framework

All frameworks follow the same pattern: export to OpenQASM 3.0, compile to QIR, submit.

```python
from qdk.openqasm import compile
from qdk import TargetProfile

# Qiskit
from qiskit import qasm3
qasm_str = qasm3.dumps(qiskit_circuit)
qir = compile(qasm_str, target_profile=TargetProfile.Base)

# Cirq
qasm_str = cirq_circuit.to_qasm(version="3.0")
qir = compile(qasm_str, target_profile=TargetProfile.Base)

# PennyLane
qasm_str = qml.to_openqasm(pennylane_circuit)(params)
qir = compile(qasm_str, target_profile=TargetProfile.Base)

# Submit any of them
target = workspace.get_targets("rigetti.sim.qvm")
job = target.submit(qir, "my-job", shots=100)
```
