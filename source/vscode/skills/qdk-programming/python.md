# QDK Python Programming

Write, run, test, and deploy quantum programs using the QDK Python libraries (`qdk` / `qsharp`).

## Package Layout

```
qdk                          # top-level metapackage
├── qdk.qsharp               # re-exports qsharp.* (eval, run, compile, estimate, circuit, ...)
├── qdk.code                  # re-exports qsharp.code (dynamic Q# callable namespace)
├── qdk.openqasm              # re-exports qsharp.openqasm (run, compile, estimate, import_openqasm)
├── qdk.estimator             # re-exports qsharp.estimator (EstimatorParams, QubitParams, ...)
├── qdk.simulation            # NeutralAtomDevice, NoiseConfig  (always available)
├── qdk.widgets               # requires qdk[jupyter]: Circuit, Histogram, EstimateDetails, ...
├── qdk.azure                 # requires qdk[azure]: Workspace, Target, Job
├── qdk.azure.qiskit          # requires qdk[azure,qiskit]
├── qdk.azure.cirq            # requires qdk[azure,cirq]
└── qdk.qiskit                # requires qdk[qiskit]: QSharpBackend, ResourceEstimatorBackend
```

Users can import via the `qdk` namespace or directly from `qsharp`:

```python
# Either works — qdk.qsharp re-exports everything from qsharp
import qsharp
from qdk import qsharp as qs
```

## Initialization

Always call `init()` before using the Q# interpreter. It resets compiler and simulator state.

```python
import qsharp

# Default: Unrestricted target profile, no project
qsharp.init()

# With a specific target profile (required for Azure submission or resource estimation)
qsharp.init(target_profile=qsharp.TargetProfile.Base)

# With a Q# project (looks for qsharp.json in the given directory)
qsharp.init(project_root="./my_project")
```

### Target Profiles

| Profile                      | Use Case                                                    |
| ---------------------------- | ----------------------------------------------------------- |
| `TargetProfile.Unrestricted` | Local simulation with full Q# features (default)            |
| `TargetProfile.Base`         | No mid-circuit measurement; broadest hardware compatibility |
| `TargetProfile.Adaptive_RI`  | Mid-circuit measurement and classical feedback              |
| `TargetProfile.Adaptive_RIF` | Advanced adaptive with float/int classical compute          |

## Running Q# Code

### Inline Evaluation

```python
result = qsharp.eval("Message(\"Hello quantum!\")")
```

### Multi-shot Execution

```python
results = qsharp.run("CNOT_Measure()", 100)
# Returns a list of 100 results
```

### Loading Q# Files

```python
from pathlib import Path

# Load and evaluate a .qs file
code = Path("sample.qs").read_text()
qsharp.eval(code)

# Then call operations defined in that file
print(qsharp.eval("Main()"))
```

### Using Q# Projects

```python
qsharp.init(project_root="./my_project")  # directory with qsharp.json

# Import Q# callables as Python objects
from qsharp.code import Main
result = Main()

# Namespaced imports
from qsharp.code.MyNamespace import MyOperation
result = MyOperation(42)
```

### Passing Arguments to Q# Callables

```python
from qsharp.code import GenerateRandomBits
result = GenerateRandomBits(5)  # pass Q# function arguments directly
```

### Working with Q# Types in Python

| Q# Type  | Python Type                                |
| -------- | ------------------------------------------ |
| `Int`    | `int`                                      |
| `Double` | `float`                                    |
| `Bool`   | `bool`                                     |
| `String` | `str`                                      |
| `Result` | `qsharp.Result.Zero` / `qsharp.Result.One` |
| `Pauli`  | `qsharp.Pauli.I` / `.X` / `.Y` / `.Z`      |
| `Array`  | `list`                                     |
| `Tuple`  | `tuple`                                    |

### Inspecting Quantum State

```python
# After allocating qubits (without releasing them), dump the state vector
qsharp.eval("use qs = Qubit[2]; H(qs[0]); CNOT(qs[0], qs[1]);")
state = qsharp.dump_machine()

# As a dense vector (complex amplitudes)
amplitudes = state.as_dense_state()

# Compare states (ignoring global phase)
expected = [0.707107, 0, 0, 0.707107]
assert state.check_eq(expected)
```

### Error Handling

```python
from qsharp import QSharpError

try:
    qsharp.eval("fail \"something went wrong\"")
except QSharpError as e:
    print(f"Q# error: {e}")
```

## Compilation to QIR

Compile Q# to Quantum Intermediate Representation for hardware submission:

```python
qsharp.init(target_profile=qsharp.TargetProfile.Base)
qir = qsharp.compile("Main()")
# qir is a QirInputData object suitable for Azure Quantum submission
```

With arguments:

```python
from qsharp.code import RunExperiment
qir = qsharp.compile(RunExperiment, 100, qsharp.Pauli.Z)
```

## Circuit Generation

```python
# From a Q# expression
circuit = qsharp.circuit("GHZSample(3)")
print(circuit)  # text representation

# From an operation that takes a qubit array
circuit = qsharp.circuit(operation="PrepareCatState")

# Visualize in Jupyter (requires qdk[jupyter])
from qdk.widgets import Circuit
Circuit(circuit)
```

### Generation Methods

By default, circuit generation traces a single execution path through the program.
Programs with measurement-based conditionals require an explicit generation method.

```python
from qsharp import CircuitGenerationMethod

# Simulate: runs in the simulator and records the gates.
# Shows only one branch of any conditional.
circuit = qsharp.circuit("MyOp()", generation_method=CircuitGenerationMethod.Simulate)

# Static: compiles the program via partial evaluation.
# Shows ALL conditional branches as classically controlled groups.
# Requires a non-Unrestricted target profile (e.g. Adaptive_RIF).
qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
circuit = qsharp.circuit("MyOp()", generation_method=CircuitGenerationMethod.Static)
```

Static generation also works with Q# callables:

```python
qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
qsharp.eval("operation Foo() : Unit { use q = Qubit(); H(q); if M(q) == One { X(q); } Reset(q); }")
circuit = qsharp.circuit(qsharp.code.Foo, generation_method=CircuitGenerationMethod.Static)
```

### Visualizing Circuits via MCP

When the user wants to visualize a circuit generated from a Python script:

1. Write a Python script that generates the circuit and prints the JSON:
   ```python
   import qsharp
   qsharp.init()
   circuit = qsharp.circuit("GHZSample(3)")
   print(circuit.json())  # prints JSON-serialized CircuitGroup
   ```
2. Run the script and capture the JSON output.
3. Call the `renderCircuit` MCP tool with the JSON string as `circuitJson`.

`circuit.json()` returns a `CircuitGroup` JSON string that `renderCircuit` accepts directly.

## Resource Estimation

Estimate the physical resources needed to run a quantum algorithm on fault-tolerant hardware.

### Basic Estimation

```python
result = qsharp.estimate("Main()")

# Access results
logical_qubits = result["physicalCounts"]["breakdown"]["algorithmicLogicalQubits"]
runtime_ns = result["physicalCounts"]["runtime"]
```

### With Parameters

```python
from qsharp.estimator import EstimatorParams, QubitParams, QECScheme

params = EstimatorParams()
params.error_budget = 0.01
params.qubit_params.name = QubitParams.GATE_NS_E3
params.qec_scheme.name = QECScheme.SURFACE_CODE

result = qsharp.estimate("Main()", params)
```

### Batch Estimation (Multiple Configurations)

```python
result = qsharp.estimate("Main()", [
    {"qubitParams": {"name": "qubit_gate_ns_e3"}, "estimateType": "frontier"},
    {"qubitParams": {"name": "qubit_gate_ns_e4"}, "estimateType": "frontier"},
    {"qubitParams": {"name": "qubit_maj_ns_e6"},
     "qecScheme": {"name": "floquet_code"}, "estimateType": "frontier"},
])
```

### Custom Qubit Parameters

```python
result = qsharp.estimate("Main()", {
    "qubitParams": {
        "instructionSet": "GateBased",
        "oneQubitMeasurementTime": "100 ns", "oneQubitGateTime": "50 ns",
        "twoQubitGateTime": "200 ns", "tGateTime": "100 ns",
        "oneQubitMeasurementErrorRate": 1e-3, "oneQubitGateErrorRate": 1e-3,
        "twoQubitGateErrorRate": 2e-3, "tGateErrorRate": 5e-3,
    }
})
```

### Predefined Qubit Models

| Name               | Description                              |
| ------------------ | ---------------------------------------- |
| `qubit_gate_ns_e3` | Gate-based, nanosecond, 10⁻³ error rate  |
| `qubit_gate_ns_e4` | Gate-based, nanosecond, 10⁻⁴ error rate  |
| `qubit_gate_us_e3` | Gate-based, microsecond, 10⁻³ error rate |
| `qubit_gate_us_e4` | Gate-based, microsecond, 10⁻⁴ error rate |
| `qubit_maj_ns_e4`  | Majorana, nanosecond, 10⁻⁴ error rate    |
| `qubit_maj_ns_e6`  | Majorana, nanosecond, 10⁻⁶ error rate    |

### Visualization (Jupyter)

```python
from qdk.widgets import EstimateDetails, SpaceChart, EstimatesOverview, EstimatesPanel

EstimateDetails(result)       # interactive result table
SpaceChart(result)            # physical qubit distribution
EstimatesOverview(result)     # compare multiple estimates
EstimatesPanel(result)        # full interactive panel
```

## Noisy Simulation

Run quantum programs with realistic noise models.

### Built-in Noise Models

```python
# Depolarizing noise (uniform X, Y, Z errors)
results = qsharp.run("BellPair()", 1000, noise=qsharp.DepolarizingNoise(0.01))

# Bit-flip noise (X errors only)
results = qsharp.run("Cat5()", 1000, noise=qsharp.BitFlipNoise(0.01))

# Phase-flip noise (Z errors only)
results = qsharp.run("GHZ()", 1000, noise=qsharp.PhaseFlipNoise(0.05))

# Custom Pauli noise (px, py, pz)
results = qsharp.run("Main()", 1000, noise=qsharp.PauliNoise(0.01, 0.0, 0.02))
```

### Qubit Loss

```python
# Simulate qubit loss (measurement returns Result.Loss)
results = qsharp.run("BellPair()", 100, qubit_loss=0.1)
for r in results:
    if r == qsharp.Result.Loss:
        print("Qubit lost!")
```

### Sweeping Noise Parameters

```python
for p in [0.001, 0.01, 0.05, 0.1]:
    results = qsharp.run("BellPair()", 1000, noise=qsharp.DepolarizingNoise(p))
    correct = sum(1 for r in results if r == (qsharp.Result.Zero, qsharp.Result.Zero))
    print(f"p={p}: fidelity={correct/1000:.3f}")
```

## Neutral Atom Device Simulation

```python
from qdk.simulation import NeutralAtomDevice, NoiseConfig

device = NeutralAtomDevice()

# Compile Q# to QIR first
qsharp.init(target_profile=qsharp.TargetProfile.Base)
qir = qsharp.compile("Main()")

# Noiseless Clifford simulation
results = device.simulate(qir, shots=1000, type="clifford")

# View device-level gate decomposition and scheduling
device.show_trace(qir)
```

### With Noise Configuration

```python
noise = NoiseConfig()

# Single-qubit gate noise
noise.sx.loss = 0.001
noise.sx.set_bitflip(0.01)
noise.sx.set_depolarizing(0.002)

# Two-qubit gate noise
noise.cz.set_depolarizing(0.01)
noise.cz.loss = 0.003

# Movement noise
noise.mov.z = 1e-3
noise.mov.loss = 0.0005

results = device.simulate(qir, shots=1000, noise=noise, type="clifford")
```

## OpenQASM Interop

Run, compile, and estimate OpenQASM 3.0 programs. For OpenQASM syntax details, see [openqasm.md](./openqasm.md).

```python
from qsharp.openqasm import run, compile, estimate, import_openqasm, ProgramType

# Run OpenQASM directly
results = run(source, shots=100, as_bitstring=True)

# With noise
results = run(source, shots=1000, noise=DepolarizingNoise(0.01))

# Compile to QIR
qir = compile(source, target_profile=TargetProfile.Base)

# Import as a standalone file (manages its own qubits)
import_openqasm(source, name="Bell", program_type=ProgramType.File)
from qsharp.code.qasm_import import Bell
result = Bell()

# Import as an operation (qubits become parameters)
import_openqasm(source, name="MyGate", program_type=ProgramType.Operation)
qsharp.eval("{ use q = Qubit(); MyGate(q) }")

# Resource estimation
result = estimate(source, {"qubitParams": {"name": "qubit_gate_ns_e3"}})
```

## Qiskit, Cirq, PennyLane, and Azure Quantum

### Qiskit (requires `qdk[qiskit]`)

```python
from qsharp.interop.qiskit import QSharpBackend, estimate

backend = QSharpBackend()
job = backend.run(qiskit_circuit, shots=100)
counts = job.result().get_counts()

# Resource estimation
result = estimate(qiskit_circuit, params, skip_transpilation=True)
```

### Azure Quantum Submission (requires `qdk[azure]`)

```python
from azure.quantum import Workspace

workspace = Workspace(subscription_id="...", resource_group="...", name="...", location="westus")
target = workspace.get_targets("quantinuum.sim.h1-1e")

qsharp.init(target_profile=qsharp.TargetProfile.Base)
qir = qsharp.compile("Main()")
job = target.submit(qir, "my-job", shots=100)
job.wait_until_completed()
results = job.get_results()
```

Cirq and PennyLane follow the same pattern: export to OpenQASM 3.0 → compile to QIR → submit.

## Testing Q# from Python (pytest)

```python
import pytest
import qsharp
from qsharp.utils import dump_operation

@pytest.fixture(autouse=True)
def setup():
    qsharp.init(project_root=".")
    yield

def test_bell_state():
    qsharp.eval("use qs = Qubit[2]; H(qs[0]); CNOT(qs[0], qs[1]);")
    assert qsharp.dump_machine().check_eq([0.707107, 0, 0, 0.707107])

def test_cnot_matrix():
    assert dump_operation("qs => CNOT(qs[0], qs[1])", 2) == [
        [1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 0, 1], [0, 0, 1, 0],
    ]
```

## Visualization (Jupyter Widgets)

Requires `pip install "qdk[jupyter]"`.

```python
from qdk.widgets import Circuit, Histogram, EstimateDetails, SpaceChart
Circuit(qsharp.circuit("GHZSample(3)"))              # circuit diagram
Histogram(qsharp.run("Main()", 1000), labels="kets")  # histogram with ket labels
EstimateDetails(qsharp.estimate("Main()"))            # resource estimation table
SpaceChart(qsharp.estimate("Main()"))                 # physical qubit chart
```

## Seeding for Reproducibility

```python
qsharp.set_quantum_seed(42)    # deterministic quantum measurements
qsharp.set_classical_seed(42)  # deterministic classical RNG
```

## Common Patterns

### Sweeping Parameters

```python
for n in range(2, 20):
    result = qsharp.estimate(f"RandomCircuit({n}, 100)")
    print(f"n={n}: {result['physicalCounts']['breakdown']['algorithmicLogicalQubits']} logical qubits")
```

### Collecting Statistics from Multi-shot Runs

```python
from collections import Counter
results = qsharp.run("Main()", 1000)
counts = Counter(str(r) for r in results)
for outcome, count in counts.most_common():
    print(f"{outcome}: {count}")
```

## Quick Reference

| Task                   | Function                                               |
| ---------------------- | ------------------------------------------------------ |
| Initialize runtime     | `qsharp.init()`                                        |
| Evaluate Q# expression | `qsharp.eval("...")`                                   |
| Run N shots            | `qsharp.run("...", shots)`                             |
| Run with noise         | `qsharp.run("...", shots, noise=DepolarizingNoise(p))` |
| Compile to QIR         | `qsharp.compile("...")`                                |
| Generate circuit       | `qsharp.circuit("...")`                                |
| Estimate resources     | `qsharp.estimate("...", params)`                       |
| Dump quantum state     | `qsharp.dump_machine()`                                |
| Run OpenQASM           | `from qsharp.openqasm import run`                      |
| Qiskit backend         | `from qsharp.interop.qiskit import QSharpBackend`      |
| Device simulation      | `from qdk.simulation import NeutralAtomDevice`         |
| Submit to Azure        | `target.submit(qir, "name", shots=N)`                  |
