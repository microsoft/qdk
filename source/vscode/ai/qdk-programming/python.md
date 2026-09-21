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
| `qre`     | `pip install "qdk[qre]"`     | Quantum Resource Estimation v3 and pandas result tables                  |
| `azure`   | `pip install "qdk[azure]"`   | Azure Quantum workspace connectivity and job submission                  |
| `qiskit`  | `pip install "qdk[qiskit]"`  | Qiskit interop — run Qiskit circuits on QDK simulators and Azure Quantum |
| `cirq`    | `pip install "qdk[cirq]"`    | Cirq interop — run Cirq circuits on QDK simulators and Azure Quantum     |
| `all`     | `pip install "qdk[all]"`     | All of the above                                                         |

### Version Alignment

These QDK packages are versioned together and must be kept in sync:

- `qdk`
- `qsharp-widgets`
- `qsharp-jupyterlab`

They share the same version number (e.g., `1.26.1234`). **Never mix versions** across these packages.

Third-party dependencies (`azure-quantum`, `qiskit`, `cirq-core`, `pyqir`) have their own versioning. The `qdk` package pins compatible ranges, so installing via `qdk` ensures compatibility.

When upgrading:

```bash
pip install --upgrade "qdk[jupyter,azure]"
```

Always upgrade via the `qdk` package to keep versions aligned.

The legacy `qsharp` Python package is deprecated and no longer receives updates. Install
`qdk` and import Q# APIs from `qdk.qsharp` instead.

## Package Layout

```text
qdk                          # top-level package — pip install qdk
├── qdk.Context              # isolated compiler and simulator state
├── qdk.qsharp               # eval, run, compile, estimate, circuit, ...
├── qdk.code                  # dynamic namespace for Q# callables (see below)
├── qdk.openqasm              # run, compile, estimate, import_openqasm
├── qdk.stim                  # experimental Stim-like compilation and simulation
├── qdk.qre                   # requires qdk[qre]: current resource estimation API
├── qdk.estimator             # deprecated resource estimation API
├── qdk.simulation            # run_qir, NeutralAtomDevice, NoiseConfig, LossPolicy
├── qdk.test_utils            # Q# test discovery and operation test helpers
├── qdk.widgets               # requires qdk[jupyter]: Circuit, BlochSphere, Histogram, ...
├── qdk.azure                 # requires qdk[azure]: Workspace, Target, Job
├── qdk.azure.qiskit          # requires qdk[azure,qiskit]: AzureQuantumProvider
├── qdk.azure.cirq            # requires qdk[azure,cirq]: AzureQuantumService
├── qdk.qiskit                # requires qdk[qiskit]: QSharpBackend, NeutralAtomBackend
└── qdk.cirq                  # requires qdk[cirq]: NeutralAtomSampler
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

# With compile-time values available through Std.Core.ConfigValue in Q#
qsharp.init(qdk_config={"size": 10, "angle": 2.0})
```

`qdk_config` values may be `int`, `float`, `str`, or `bool`. Q# reads them with
`Std.Core.ConfigValue(name, defaultValue)`; the default value determines the expected type.

### Isolated Contexts

The module-level APIs use one global compiler and simulator context. Create `qdk.Context`
instances when independent state, projects, target profiles, or configuration maps are needed.

```python
import qdk

context = qdk.Context(qdk_config={"experiment": "baseline"})
context.eval("operation Main() : Result { use q = Qubit(); X(q); MResetZ(q) }")

assert context.run("Main()", 2) == [qdk.Result.One, qdk.Result.One]
assert context.code.Main() == qdk.Result.One
```

Contexts expose `eval`, `run`, `compile`, `circuit`, `logical_counts`, `dump_machine`,
`import_openqasm`, and `import_circuit`. Callables under `context.code` belong to that context
and cannot be passed to another context.

```python
# Import a .qsc visual circuit as a self-contained callable.
visual_circuit = context.import_circuit("circuit.qsc", name="MyCircuit")
result = visual_circuit()

# Or import an operation that accepts its qubits from Q# code.
operation = context.import_circuit(
    "circuit.qsc",
    name="MyOperation",
    program_type=qdk.ProgramType.Operation,
)
```

### Target Profiles

| Profile                      | Use Case                                                              |
| ---------------------------- | --------------------------------------------------------------------- |
| `TargetProfile.Unrestricted` | Full simulation (default)                                             |
| `TargetProfile.Adaptive`     | The QIR Adaptive Profile with all QDK-supported extensions.           |
| `TargetProfile.Adaptive_RIF` | Adaptive profile with integer & floating-point computation extensions |
| `TargetProfile.Adaptive_RI`  | Adaptive profile with integer computation extension                   |
| `TargetProfile.Base`         | Minimal capabilities required to run a quantum program (Base Profile) |

## Q\#

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
code = Path("sample.qs").read_text(encoding="utf-8")
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

## Testing Q# from Python

`run_tests` discovers and runs Q# operations marked with `@Test` in the global or an isolated
context. It raises `RuntimeError` when any test fails.

```python
from qdk import qsharp
from qdk.test_utils import run_tests

qsharp.eval("""
import Std.Diagnostics.Fact;

@Test()
operation AdditionTest() : Unit {
    Fact(2 + 2 == 4, "assertion failed");
}
""")

run_tests(seed=42, regex="AdditionTest")
```

Use `ArithmeticOpTester` to run in-place Q# arithmetic operations on classical integer inputs:

```python
from qdk.test_utils import ArithmeticOpTester

tester = ArithmeticOpTester("Std.Arithmetic.IncByLE", [8, 8])
assert tester.run([5, 7]) == [5, 12]
```

`qdk.test_utils.dump_operation_on_state` returns the state vector produced by an operation
with signature `(Qubit[] => Unit)`.

## OpenQASM

For OpenQASM syntax details, see [openqasm.md](./openqasm.md).

### Parse, analyze, and navigate source

Parsing and semantic analysis return diagnostics as result data. Their nodes
and diagnostics use global, half-open UTF-8 byte spans resolved through the
result's immutable source document:

```python
from qdk.openqasm import parser, semantic

parsed = parser.parse(
    'OPENQASM 3.0; include "defs.inc"; qubit q;',
    path="memory://workspace/main.qasm",
    includes={"memory://workspace/defs.inc": "gate local q { x q; }"},
)
assert not parsed.has_errors

included = parsed.document.source_map.find("memory://workspace/defs.inc")
assert included is not None
position = parsed.document.source_map.position_at(included.id, 5)
assert parsed.document.source_map.byte_offset(included.id, position) == 5

analysis = semantic.analyze(
    'OPENQASM 3.0; include "stdgates.inc"; qubit q; h q; int value = missing;'
)
assert analysis.has_errors
assert any(d.code == "Qdk.Qasm.Lowerer.UndefinedSymbol" for d in analysis.diagnostics)
```

### Resolved types, constant values, and equality

Resolved types are structured nodes, not strings, and constant values are data,
not renderings. Nodes, types, and values all compare and hash structurally, with
source position excluded, so nodes work as `set` members and `dict` keys:

```python
from qdk.openqasm import semantic

analysis = semantic.analyze(
    "OPENQASM 3.0; array[int[8], 2, 3] grid; const angle turn = pi/2;"
)
grid, turn = analysis.program.statements

assert isinstance(grid.type, semantic.ArrayType)
assert grid.type.base_type.size == 8
assert grid.type.dimensions == [2, 3]
assert isinstance(turn.init_expr.const_value, semantic.Angle)

source = "OPENQASM 3.0; qubit q;"
assert semantic.analyze(source).program == semantic.analyze(source).program
```

A resolved type carries no `span` and is not a `QASMNode`, so it does not appear
in `children()`. `parser.dumps` rejects a semantic program with a message naming
both the expected and the received type.

### Include resolver contract

Resolver keys are platform-neutral logical identifiers. Use `/` separators.
Relative `.` and `..` components are normalized against the including source;
URI-like schemes are preserved but not decoded or fetched. Caller keys match
exactly and case-sensitively on every host:

```python
from qdk.openqasm import parser

result = parser.parse(
    'OPENQASM 3.0; include "./Case.inc"; include "case.inc";',
    path="memory://workspace/main.qasm",
    includes={
        "memory://workspace/Case.inc": "int upper = 1;",
        "memory://workspace/case.inc": "int lower = 2;",
    },
)
assert not result.has_errors
```

`stdgates.inc`, `qelib1.inc`, and the QDK extension `qdk.inc` are built in and
do not invoke the resolver. During semantic analysis, `qdk.inc` makes two QDK
intrinsics available: `mresetz_checked(qubit) -> int`, which measures and resets
a qubit and returns `0` for Zero, `1` for One, or `2` for qubit loss; and
`postselectz(bit, qubit) -> void`, which post-selects a computational-basis
result. These names are unavailable without `qdk.inc`.

Other keys have no filesystem or network fallback. Missing keys, wrong callback
return types, and callback exceptions become diagnostics and unresolved source
entries. Results do not retain resolver callbacks.

### Visit and serialize syntax

`QASMVisitor` propagates optional context through syntax and semantic trees.
Canonical serialization accepts syntax programs only and may change between
preview releases:

```python
from qdk.openqasm import parser
from qdk.openqasm.parser import QASMVisitor

class GateNames(QASMVisitor):
    def visit_QuantumGate(self, node: object, context: list[str]) -> None:
        context.append(node.name.name)  # type: ignore[attr-defined]
        self.generic_visit(node, context)

names: list[str] = []
program = parser.parse_program("OPENQASM 3.0; qubit q; x q; y q;")
GateNames().visit(program, names)
assert names == ["x", "y"]
assert parser.dumps(program) == "OPENQASM 3.0;\nqubit q;\nx q;\ny q;\n"
```

`parser.dumps` raises `QASMUnparseError` for recovered or unsupported syntax,
invalid strings, and non-finite floats. `parser.dump` writes once to a text
stream, propagates writer exceptions, and does not flush or close the stream.

### OpenQASM Multishot Simulation

```python
from qdk.openqasm import run, import_openqasm, ProgramType

# Run OpenQASM directly
results = run(source, shots=100, as_bitstring=True)

# With noise
results = run(source, shots=1000, noise=qsharp.DepolarizingNoise(0.01))

# Use the scalable stabilizer simulator
results = run(source, shots=1000, type="clifford", num_qubits=100)

# Import as a standalone file (manages its own qubits)
import_openqasm(source, name="Bell", program_type=ProgramType.File)
from qdk import code
result = code.qasm_import.Bell()

# Import as an operation (qubits become parameters)
import_openqasm(source, name="MyGate", program_type=ProgramType.Operation)
qsharp.eval("{ use q = Qubit(); MyGate(q); Reset(q) }")
```

## QDK-Stim (Experimental)

`qdk.stim` compiles a Stim-like language to QIR and runs it on the QDK's Clifford, CPU,
or GPU simulators. It includes Stim instructions, non-Clifford operations, and QDK-specific
instructions for post-selection and qubit-loss handling. The API is experimental and may change.

```python
import qdk.stim as stim

source = "H 0\nT 0\nH 0\nM(0.01) 0"

# M(0.01) applies 1% symmetric readout noise to the measurement result.
results = stim.run(source, shots=1000, seed=42, type="clifford")

# Compile separately when another QIR-consuming simulator or service will run it.
qir, noise = stim.compile(source)
```

The QDK-specific `SELECT`, `REQUIRE`, and `NOTLEAKED` instructions support post-selection
and repeat-until-success patterns. `PEEK_LOSS` and `LOSS_ERROR` support qubit-loss modeling.

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

### Direct QIR Simulation

`run_qir` accepts a `QirInputData` object, QIR text, or LLVM bitcode. It supports Base and
Adaptive Profile programs, including mid-circuit measurements, branching, and loops.

```python
from qdk import qsharp
from qdk.simulation import run_qir

qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
qir = qsharp.compile("Main()")

results = run_qir(qir, shots=1000, seed=42, type="cpu")
```

Set `type` to `"cpu"`, `"gpu"`, or `"clifford"`. If omitted, `run_qir` tries the GPU and
falls back to the CPU. For lower-level, gate-by-gate simulation, `qdk.simulation` also exports
the experimental `DensityMatrixSimulator` and `StateVectorSimulator` classes.

### Neutral Atom Device Simulation

#### Q\# Programs

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

#### OpenQASM Programs

```python
from qdk.openqasm import compile
from qdk.simulation import NeutralAtomDevice

qir = compile(source, target_profile=qsharp.TargetProfile.Base)
device = NeutralAtomDevice()
results = device.simulate(qir, shots=1000, type="clifford")
```

### Per-Gate Noise and Loss Policies

```python
from qdk.simulation import LossPolicy, NoiseConfig

noise = NoiseConfig()

# Single-qubit gate noise
noise.sx.set_pauli_noise("L", 0.001)
noise.sx.set_bitflip(0.01)
noise.sx.set_depolarizing(0.002)

# Two-qubit gate noise
noise.cz.set_depolarizing(0.01)
noise.cz.set_pauli_noise("IL", 0.003)
noise.cz.on_loss = LossPolicy.PROPAGATE

# Movement noise
noise.mov.z = 1e-3
noise.mov.set_pauli_noise("L", 0.0005)

results = device.simulate(qir, shots=1000, noise=noise, type="clifford")
```

Loss fault strings use `L` for a lost qubit, such as `L`, `IL`, or `XL`. `LossPolicy`
controls what a multi-qubit gate does when an operand is already lost: `SKIP`, `PROPAGATE`,
`DEGRADE`, `RESIDUAL_S_DAGGER`, or `APPLY_ANYWAY`. The older `NoiseTable.loss` property is
deprecated; use loss fault strings with `set_pauli_noise`.

### Stabilizer Simulation

Select the stabilizer simulator with `type="clifford"`. Stabilizer branching allows it to
run programs containing a small number of non-Clifford operations, such as T gates and arbitrary
rotations. Runtime and memory grow exponentially with the number of non-Clifford operations.

```python
results = qsharp.run("Main()", 1000, type="clifford")
```

The same `type="clifford"` option is available on `qdk.openqasm.run`; direct QIR simulation
uses `qdk.simulation.run_qir(..., type="clifford")`.

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

### Resource Estimation (Deprecated)

`qdk.qiskit.estimate` and `ResourceEstimatorBackend` use the deprecated resource estimator.
Use `qdk.qre` for new resource-estimation workflows.

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

### Q\# Compilation

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

### OpenQASM Compilation

```python
from qdk.openqasm import compile

qir = compile(source, target_profile=qsharp.TargetProfile.Base)
```

## Circuit Diagram Generation

### Q\# Circuits

```python
# From a Q# expression
circuit = qsharp.circuit("GHZSample(3)")
print(circuit)  # text representation

# From an operation that takes a qubit array
circuit = qsharp.circuit(operation="PrepareCatState")

# Include source locations on circuit operations
circuit = qsharp.circuit("GHZSample(3)", source_locations=True)
```

### OpenQASM Circuits

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

Circuit generation honors the Q# `@CircuitRenderingOptions` attribute. Use `hideBox=true` to
render an operation's contents without its wrapper, and `inputSizes=[...]` to set the displayed
sizes of qubit-array arguments. Common rotation angles are rendered as fractions of pi automatically.

## Resource Estimation

Install `qdk[qre]` to estimate physical resources and explore Pareto-optimal tradeoffs between
physical qubits and runtime.

```python
from qdk.qre import estimate
from qdk.qre.application import QSharpApplication
from qdk.qre.models import GateBased, RoundBasedFactory, SurfaceCode

application = QSharpApplication("Main()")
architecture = GateBased(error_rate=1e-4, gate_time=50, measurement_time=100)

results = estimate(
    application,
    architecture,
    SurfaceCode.q() * RoundBasedFactory.q(),
    max_error=0.01,
)

print(results)
for result in results:
    print(result.qubits, result.runtime, result.error)
```

Application adapters are available for Q#, OpenQASM, QIR, and Cirq as `QSharpApplication`,
`OpenQASMApplication`, `QIRApplication`, and `CirqApplication`. Built-in architecture models
include `GateBased`, `Majorana`, and `NeutralAtom`. `estimate` returns an `EstimationTable`;
use `results.as_frame()` for a pandas DataFrame or `qdk.qre.plot_estimates(results)` to plot
the Pareto frontier.

The older `qsharp.estimate`, `qdk.openqasm.estimate`, `qdk.estimator`, and Qiskit resource
estimator APIs are deprecated and will be removed in a future release.

## Visualizations (Jupyter)

Requires `pip install "qdk[jupyter]"`.

```python
from qdk.widgets import BlochSphere, Circuit, Entanglement, Histogram

# Interactive single-qubit state and gate explorer
BlochSphere("H T H")
BlochSphere("H Rx(1.5708) S'")

# Circuit diagram
Circuit(qsharp.circuit("GHZSample(3)"))

# Histogram with ket labels
Histogram(qsharp.run("Main()", 1000), labels="kets")

# Orbital entanglement diagram from entropy and mutual-information data
Entanglement(
    s1_entropies=[0.2, 0.4],
    mutual_information=[[0.0, 0.1], [0.1, 0.0]],
    labels=["1", "2"],
)
```

`EstimateDetails`, `SpaceChart`, `EstimatesOverview`, and `EstimatesPanel` remain available
for results from the deprecated resource estimator. Use `qdk.qre.plot_estimates` for QRE v3.
