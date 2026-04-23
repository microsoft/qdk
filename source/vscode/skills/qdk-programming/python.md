# QDK Python Package

Write, run, test, and deploy quantum programs using the `qdk` Python package.

## Installation

- **Python >= 3.10**
- Recommended: **Python VS Code extension** (`ms-python.python`) for environment management, IntelliSense, and Jupyter support

```bash
pip install qdk
```

This provides: Q# and OpenQASM compilation, local quantum simulation, and resource estimation.

### Optional Extras

Install extras using bracket syntax. Multiple extras can be combined with commas:

```bash
pip install "qdk[jupyter,qiskit,azure]"
```

| Extra     | Command                      | What It Adds                                                             |
| --------- | ---------------------------- | ------------------------------------------------------------------------ |
| `jupyter` | `pip install "qdk[jupyter]"` | Jupyter widgets                                                          |
| `azure`   | `pip install "qdk[azure]"`   | Azure Quantum workspace connectivity and job submission                  |
| `qiskit`  | `pip install "qdk[qiskit]"`  | Qiskit interop — run Qiskit circuits on QDK simulators and Azure Quantum |
| `cirq`    | `pip install "qdk[cirq]"`    | Cirq interop — run Cirq circuits on QDK simulators and Azure Quantum     |
| `all`     | `pip install "qdk[all]"`     | All of the above                                                         |

### Version Alignment

These QDK packages are versioned together and must be kept in sync:

- `qdk` (metapackage)
- `qsharp` (core compiler/simulator)
- `qsharp-widgets`
- `qsharp-jupyterlab`

They share the same version number (e.g., `1.26.1234`). **Never mix versions** across these packages.

Third-party dependencies (`azure-quantum`, `qiskit`, `cirq-core`, `pyqir`) have their own versioning. The `qdk` metapackage pins compatible ranges, so installing via `qdk` ensures compatibility.

When upgrading:

```bash
pip install --upgrade "qdk[jupyter,azure]"
```

Always upgrade via the `qdk` metapackage to keep versions aligned.

## Package Layout

```text
qdk                          # top-level package — pip install qdk
├── qdk.qsharp               # eval, run, compile, estimate, circuit, ...
├── qdk.code                  # dynamic namespace for Q# callables (see below)
├── qdk.openqasm              # run, compile, estimate, import_openqasm
├── qdk.estimator             # EstimatorParams, QubitParams, QECScheme, ...
├── qdk.simulation            # NeutralAtomDevice, NoiseConfig
├── qdk.widgets               # requires qdk[jupyter]: Circuit, Histogram, EstimateDetails, ...
├── qdk.azure                 # requires qdk[azure]: Workspace, Target, Job
├── qdk.azure.qiskit          # requires qdk[azure,qiskit]: AzureQuantumProvider
├── qdk.azure.cirq            # requires qdk[azure,cirq]: AzureQuantumService
└── qdk.qiskit                # requires qdk[qiskit]: QSharpBackend, ResourceEstimatorBackend
```

## Working with Q# and OpenQASM

### Initialization

The QDK initializes automatically with default parameters (Unrestricted target profile, no project).
Call `init()` to reset compiler and simulator state or to configure a specific target profile or project.

```python
from qdk import qsharp

# Reset to defaults
qsharp.init()

# With a specific target profile (required for Azure submission or resource estimation)
qsharp.init(target_profile=qsharp.TargetProfile.Base)

# With a Q# project (looks for qsharp.json in the given directory)
qsharp.init(project_root="./my_project")
```

### Target Profiles

| Profile                        | Use Case                                                              |
| ------------------------------ | --------------------------------------------------------------------- |
| `TargetProfile.Unrestricted`   | Full simulation (default)                                             |
| `TargetProfile.Adaptive_RIFLA` | Adaptive profile with integer, floating-point, loops, and arrays      |
| `TargetProfile.Adaptive_RIF`   | Adaptive profile with integer & floating-point computation extensions |
| `TargetProfile.Adaptive_RI`    | Adaptive profile with integer computation extension                   |
| `TargetProfile.Base`           | Minimal capabilities required to run a quantum program (Base Profile) |

## Q#

For Q# language syntax details, see [qsharp.md](./qsharp.md).

### Inline Simulation

`qsharp.eval()` executes top-level Q# statements on the sparse state simulator.
Compiler and quantum state persist across calls until `init()` is called.

```python
result = qsharp.eval("Message(\"Hello quantum!\")")
```

#### Inspecting Quantum State

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

#### Error Handling

```python
from qdk.qsharp import QSharpError

try:
    qsharp.eval("fail \"something went wrong\"")
except QSharpError as e:
    print(f"Q# error: {e}")
```

### Multishot Simulation

```python
qsharp.eval("operation CNOT_Measure() : (Result, Result) { use (q1, q2) = (Qubit(), Qubit()); H(q1); CNOT(q1, q2); (MResetZ(q1), MResetZ(q2)) }")
results = qsharp.run("CNOT_Measure()", 100)
# Returns a list of 100 results

# Reproducible results with explicit seed
results = qsharp.run("CNOT_Measure()", 100, seed=42)
```

### Loading Q# Files

```python
from pathlib import Path

# Load and evaluate a .qs file
code = Path("sample.qs").read_text()
qsharp.eval(code)

# Run an operation defined in that file
results = qsharp.run("Main()", 100)
```

### Using Q# Projects

```python
qsharp.init(project_root="./my_project")  # directory with qsharp.json

# Import Q# callables as Python objects
from qdk import code
result = code.Main()

# Namespaced callables
result = code.MyNamespace.MyOperation(42)
```

### `%%qsharp` Magic

In Jupyter notebooks, use the `%%qsharp` cell magic to write Q# code directly in a cell.
This is equivalent to calling `qsharp.eval()` with the cell contents.
Defined operations become available as callables via `qdk.code`.

```python
%%qsharp
operation BellPair() : (Result, Result) {
    use (q1, q2) = (Qubit(), Qubit());
    H(q1);
    CNOT(q1, q2);
    (M(q1), M(q2))
}
```

```python
from qdk import code
result = code.BellPair()
```

### Python / Q# Interop

#### The `qdk.code` Module

When Q# code is evaluated (via `qdk.qsharp.eval()`, `qdk.qsharp.init(project_root=...)`, or
`qdk.openqasm.import_openqasm()`), the resulting Q# callables become available as Python
objects under `qdk.code`. Namespaces in Q# map to submodules:

```python
from qdk.qsharp import init, eval
from qdk import code

code.Main                        # top-level callable
code.MyNamespace.MyOperation     # namespaced callable
code.qasm_import.MyImportedGate  # imported OpenQASM gate
```

#### Passing Arguments to Callables

```python
from qdk import code
result = code.GenerateRandomBits(5)  # pass Q# function arguments directly
```

#### Working with Q# Types in Python

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

## OpenQASM

For OpenQASM syntax details, see [openqasm.md](./openqasm.md).

### Multishot Simulation

```python
from qdk.openqasm import run, import_openqasm, ProgramType

# Run OpenQASM directly
results = run(source, shots=100, as_bitstring=True)

# With noise
results = run(source, shots=1000, noise=qsharp.DepolarizingNoise(0.01))

# Import as a standalone file (manages its own qubits)
import_openqasm(source, name="Bell", program_type=ProgramType.File)
from qdk.code.qasm_import import Bell
result = Bell()

# Import as an operation (qubits become parameters)
import_openqasm(source, name="MyGate", program_type=ProgramType.Operation)
qsharp.eval("{ use q = Qubit(); MyGate(q); Reset(q) }")
```

## Simulation

### Noisy Simulation

Run quantum programs with realistic noise models.

#### Built-in Noise Models

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

#### Qubit Loss

```python
# Simulate qubit loss (measurement returns Result.Loss)
results = qsharp.run("BellPair()", 100, qubit_loss=0.1)
for r in results:
    if r == qsharp.Result.Loss:
        print("Qubit lost!")
```

### Neutral Atom Device Simulation

#### Q#

```python
from qdk.simulation import NeutralAtomDevice
from qdk import qsharp

device = NeutralAtomDevice()

# Compile Q# to QIR first
qsharp.init(target_profile=qsharp.TargetProfile.Base)
qir = qsharp.compile("Main()")

# Noiseless Clifford simulation
results = device.simulate(qir, shots=1000, type="clifford")

# View device-level gate decomposition and scheduling
device.show_trace(qir)
```

#### OpenQASM

```python
from qdk.openqasm import compile
from qdk.simulation import NeutralAtomDevice

qir = compile(source, target_profile=qsharp.TargetProfile.Base)
device = NeutralAtomDevice()
results = device.simulate(qir, shots=1000, type="clifford")
```

#### With Noise Configuration

```python
from qdk.simulation import NoiseConfig

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

### Sparse Simulation (Default)

The default simulator used by `qsharp.run()` and `qsharp.eval()` is a sparse state simulator.
It efficiently represents quantum states by only tracking non-zero amplitudes, making it
suitable for programs where the state vector remains relatively sparse throughout execution.
No special configuration is required — it is used automatically when no noise model is specified.

`NoiseConfig` also works with the sparse simulator for per-gate noise control:

```python
from qdk.simulation import NoiseConfig

noise = NoiseConfig()
noise.rx.set_bitflip(0.005)
noise.rzz.set_pauli_noise("XX", 0.005)
results = qsharp.run("Main()", 100, noise=noise)
```

## Qiskit Integration

Requires `pip install "qdk[qiskit]"`. Three Qiskit backends are available.

### Local Simulation

```python
from qdk.qiskit import QSharpBackend

backend = QSharpBackend()
job = backend.run(qiskit_circuit, shots=1024)
counts = job.result().get_counts()
```

### Resource Estimation

```python
from qdk.qiskit import ResourceEstimatorBackend, estimate
from qdk.estimator import EstimatorParams, QubitParams

# Quick: convenience function
result = estimate(qiskit_circuit)

# With parameters
params = EstimatorParams()
params.qubit_params.name = QubitParams.GATE_NS_E3
result = estimate(qiskit_circuit, params)

# Or use the backend directly
backend = ResourceEstimatorBackend()
job = backend.run(qiskit_circuit, params=params)
result = job.result()
```

### Neutral Atom Simulation

```python
from qdk.qiskit import NeutralAtomBackend

backend = NeutralAtomBackend()
job = backend.run(qiskit_circuit, shots=1000)
counts = job.result().get_counts()

# With noise
from qdk.simulation import NoiseConfig
noise = NoiseConfig()
noise.cz.set_depolarizing(0.01)
job = backend.run(qiskit_circuit, shots=1000, noise=noise)
```

## Azure Quantum

### Q# Submission (requires `qdk[azure]`)

Compile Q# to QIR and submit to an Azure Quantum target.

```python
from qdk.azure import Workspace

workspace = Workspace(subscription_id="...", resource_group="...", name="...", location="westus")
target = workspace.get_targets("quantinuum.sim.h1-1e")

qsharp.init(target_profile=qsharp.TargetProfile.Base)
qir = qsharp.compile("Main()")
job = target.submit(qir, "my-job", shots=100)
job.wait_until_completed()
results = job.get_results()
```

### Qiskit Submission (requires `qdk[azure,qiskit]`)

Submit Qiskit circuits to Azure Quantum hardware.

```python
from qdk.azure.qiskit import AzureQuantumProvider

provider = AzureQuantumProvider(resource_id="...", location="westus")
backend = provider.get_backend("quantinuum.sim.h1-1e")
job = backend.run(qiskit_circuit, shots=100)
counts = job.result().get_counts()
```

### Cirq Submission (requires `qdk[azure,cirq]`)

Submit Cirq circuits to Azure Quantum hardware.

```python
from qdk.azure.cirq import AzureQuantumService

service = AzureQuantumService(resource_id="...", location="westus")
simulator = service.get_simulator("quantinuum.sim.h1-1e")
result = simulator.run(cirq_circuit, repetitions=100).measurements
```

## Compilation to QIR

Compile to Quantum Intermediate Representation for hardware submission.

### Q#

```python
qsharp.init(target_profile=qsharp.TargetProfile.Base)
qir = qsharp.compile("Main()")
# qir is a QirInputData object suitable for Azure Quantum submission
```

With arguments:

```python
from qdk import code
qir = qsharp.compile(code.RunExperiment, 100, qsharp.Pauli.Z)
```

### OpenQASM

```python
from qdk.openqasm import compile

qir = compile(source, target_profile=qsharp.TargetProfile.Base)
```

## Circuit Diagram Generation

### Q#

```python
# From a Q# expression
circuit = qsharp.circuit("GHZSample(3)")
print(circuit)  # text representation

# From an operation that takes a qubit array
circuit = qsharp.circuit(operation="PrepareCatState")
```

### OpenQASM

Import an OpenQASM program, then generate a circuit diagram via the Q# circuit API:

```python
import qdk
from qdk.openqasm import import_openqasm, ProgramType

import_openqasm(source, name="Bell", program_type=ProgramType.File)
circuit = qsharp.circuit(qdk.code.qasm_import.Bell)
print(circuit)
```

### Circuit Generation Methods

By default, circuit generation traces a single execution path through the program.
Programs with measurement-based conditionals require an explicit generation method.

```python
from qdk.qsharp import CircuitGenerationMethod

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
import qdk
qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
qsharp.eval("operation Foo() : Unit { use q = Qubit(); H(q); if M(q) == One { X(q); } Reset(q); }")
circuit = qsharp.circuit(qdk.code.Foo, generation_method=CircuitGenerationMethod.Static)
```

## Resource Estimation

Estimate the physical resources needed to run a quantum algorithm on fault-tolerant hardware.

### Q#

```python
result = qsharp.estimate("Main()")
```

### OpenQASM

```python
from qdk.openqasm import estimate

result = estimate(source, {"qubitParams": {"name": "qubit_gate_ns_e3"}})
```

### Basic Estimation

```python
# Access results
logical_qubits = result["physicalCounts"]["breakdown"]["algorithmicLogicalQubits"]
runtime_ns = result["physicalCounts"]["runtime"]
```

### With Parameters

```python
from qdk.estimator import EstimatorParams, QubitParams, QECScheme

params = EstimatorParams()
params.error_budget = 0.01
params.qubit_params.name = QubitParams.GATE_NS_E3
params.qec_scheme.name = QECScheme.SURFACE_CODE

result = qsharp.estimate("Main()", params)
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

## Visualizations (Jupyter)

Requires `pip install "qdk[jupyter]"`.

```python
from qdk.widgets import Circuit, Histogram, EstimateDetails, SpaceChart, EstimatesOverview, EstimatesPanel

# Circuit diagram
Circuit(qsharp.circuit("GHZSample(3)"))

# Histogram with ket labels
Histogram(qsharp.run("Main()", 1000), labels="kets")

# Resource estimation widgets
EstimateDetails(result)       # interactive result table
SpaceChart(result)            # physical qubit distribution
EstimatesOverview(result)     # compare multiple estimates
EstimatesPanel(result)        # full interactive panel
```
