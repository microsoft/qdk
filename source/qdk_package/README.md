# qdk

The Quantum Development Kit (QDK) provides a single, cohesive Python entry point for compiling, simulating, and estimating resources for quantum programs (Q# and OpenQASM), with optional extras for visualization, cloud workflows, and interoperability with Qiskit and Cirq.

## Install

To install the core functionality, which include Q\# \& OpenQASM simulation, compilation, and resource estimation support:

```bash
pip install qdk
```

To include the Jupyter extra, which adds visualizations using Jupyter Widgets in the `qdk.widgets` submodule and syntax highlighting for Jupyter notebooks in the browser:

```bash
pip install "qdk[jupyter]"
```

To add the Azure Quantum extra, which includes functionality for working with the Azure Quantum service in the `qdk.azure` submodule:

```bash
pip install "qdk[azure]"
```

For Qiskit integration, which exposes Qiskit interop utilities in the `qdk.qiskit` submodule:

```bash
pip install "qdk[qiskit]"
```

For Cirq integration, which exposes Cirq interop utilities in the `qdk.cirq` submodule:

```bash
pip install "qdk[cirq]"
```

To easily install all the above extras:

```bash
pip install "qdk[all]"
```

## Quick Start

```python
from qdk import qsharp

result = qsharp.run("{ use q = Qubit(); H(q); return MResetZ(q); }", shots=100)
print(result)
```

## OpenQASM parsing and analysis

The preview `qdk.openqasm.parser` and `qdk.openqasm.semantic` modules expose
read-only syntax and semantic trees. Both entry points return diagnostics on a
result object, including diagnostics from resolved sources.

### Parse and navigate sources

`parser.parse` performs recovery-oriented syntactic parsing. Node and diagnostic
spans are global, half-open UTF-8 byte ranges. Resolve a span through the
immutable document owned by the result:

```python
from qdk.openqasm import parser

parsed = parser.parse(
    'OPENQASM 3.0; include "defs.inc"; qubit q;',
    path="memory://workspace/main.qasm",
    includes={"memory://workspace/defs.inc": "gate local q { x q; }"},
)
assert not parsed.has_errors
assert parsed.program.document is parsed.document

source_file = parsed.document.source_map.find("memory://workspace/defs.inc")
assert source_file is not None
position = parsed.document.source_map.position_at(source_file.id, 5)
assert parsed.document.source_map.byte_offset(source_file.id, position) == 5
assert source_file.path == "memory://workspace/defs.inc"
```

`parser.parse_program` is a strict-by-default convenience wrapper. It raises
`QASM3ParsingError` when the underlying parse result has diagnostics, unless
`permissive=True` is supplied. The exception retains the complete result.

### Analyze symbols and diagnostics

`semantic.analyze` resolves symbols, checks types, and evaluates constants. Its
program and result share the same source document. Diagnostics are result data,
not exceptions:

```python
from qdk.openqasm import semantic

analysis = semantic.analyze(
    'OPENQASM 3.0; include "stdgates.inc"; qubit q; h q; int value = missing;',
    path="main.qasm",
)
assert analysis.has_errors
assert analysis.program.document is analysis.document
assert any(diagnostic.code == "Qdk.Qasm.Lowerer.UndefinedSymbol" for diagnostic in analysis.diagnostics)

missing = analysis.diagnostics[-1].labels[0]
source_range = analysis.document.source_map.range_from_span(missing.span)
assert source_range.source_id == analysis.document.entry.id
```

Semantic types and constant values describe the current analysis result. Their
human-readable string forms are not a cross-release serialization format.

### Resolve includes

The `includes` argument accepts a `dict[str, str]`, a callback returning source
text or `None`, or `None`. Keys are platform-neutral logical identifiers and
should use `/` separators. Relative `.` and `..` path components are resolved
against the including source's logical parent. URI-like `scheme://` prefixes
are preserved, but QDK does not percent-decode, fetch, or otherwise interpret
them. Caller-provided key matching is exact and case-sensitive on every host:

```python
from qdk.openqasm import parser

resolved = parser.parse(
    'OPENQASM 3.0; include "./Case.inc"; include "case.inc";',
    path="memory://workspace/main.qasm",
    includes={
        "memory://workspace/Case.inc": "int upper = 1;",
        "memory://workspace/case.inc": "int lower = 2;",
    },
)
assert not resolved.has_errors
assert resolved.document.source_map.find("memory://workspace/Case.inc") is not None
assert resolved.document.source_map.find("memory://workspace/case.inc") is not None
```

`stdgates.inc`, `qelib1.inc`, and the QDK extension `qdk.inc` are built in and
do not invoke the callback. During semantic analysis, `qdk.inc` injects
`mresetz_checked(qubit) -> int`, which returns `0` for Zero, `1` for One, or `2`
for qubit loss, and `postselectz(bit, qubit) -> void`. These intrinsics are
unavailable without the include. There is no filesystem or network fallback
for other keys. A missing key, a
callback exception, or a callback result with the wrong type becomes a result
diagnostic and an unresolved source snapshot; it does not escape as the
callback's exception. A fresh resolver bridge is created for each `parse` or
`analyze` call, and returned results do not retain the callback.

### Visit syntax and semantic trees

`QASMVisitor` walks either tree. An optional context is propagated to callbacks
and `generic_visit`; one-argument callbacks remain supported:

```python
from qdk.openqasm import parser
from qdk.openqasm.parser import QASMVisitor

class GateNames(QASMVisitor):
    def visit_QuantumGate(self, node: object, context: list[str]) -> None:
        context.append(node.name.name)  # type: ignore[attr-defined]
        self.generic_visit(node, context)

names: list[str] = []
program = parser.parse("OPENQASM 3.0; qubit q; x q; y q;").program
GateNames().visit(program, names)
assert names == ["x", "y"]
```

### Write canonical source

`parser.dumps` (also exported as `qdk.openqasm.dumps` and `unparse`) emits the
current canonical format from a syntax program. It omits comments and original
formatting, preserves include directives without expanding them, and does not
accept semantic programs. The preview format can change between QDK releases:

```python
import io

from qdk.openqasm import parser

program = parser.parse_program("OPENQASM 3.0; qubit q; x q;")
canonical = parser.dumps(program)
assert canonical == "OPENQASM 3.0;\nqubit q;\nx q;\n"

stream = io.StringIO()
parser.dump(program, stream)
assert stream.getvalue() == canonical
```

`dumps` raises `QASMUnparseError` for recovered or unsupported syntax, invalid
strings, and non-finite float spellings. `dump` calls `stream.write` once and
propagates stream exceptions unchanged; it does not flush or close the stream.

To use widgets (installed via `qdk[jupyter]` extra):

```python
from qdk.qsharp import eval, run
from qdk.widgets import Histogram

eval("""
operation BellPair() : Result[] {
    use qs = Qubit[2];
    H(qs[0]);CX(qs[0], qs[1]);
    MResetEachZ(qs)
}
""")
results = run("BellPair()", shots=1000, noise=(0.005, 0.0, 0.0))
Histogram(results)
```

## Public API Surface

Submodules:

- `qdk.qsharp` – Q# interpreter functions: `init`, `eval`, `run`, `compile`, `circuit`, `estimate`, `dump_machine`, `dump_circuit`, `dump_operation`, and related types.
- `qdk.openqasm` – OpenQASM compilation, execution, parsing, semantic analysis,
  source navigation, visitors, and canonical serialization.
- `qdk.estimator` – resource estimation utilities.
- `qdk.simulation` – noise-aware simulation utilities: `NeutralAtomDevice`, `NoiseConfig`, `run_qir`, `DensityMatrixSimulator`, `StateVectorSimulator`, and related types.
- `qdk.code` – dynamic namespace populated at runtime with user-defined Q# and OpenQASM callables.
- `qdk.qre` – quantum resource estimation v3: `estimate`, `Application`, `Architecture`, `ISA`, `ISATransform`, and related types.
- `qdk.applications` – domain-specific quantum applications (e.g. `qdk.applications.magnets`).
- `qdk.widgets` – Jupyter widgets for visualization (requires the `qdk[jupyter]` extra).
- `qdk.azure` – Azure Quantum service integration (requires the `qdk[azure]` extra).
- `qdk.qiskit` – Qiskit interop: `QSharpBackend`, `NeutralAtomBackend`, and related types (requires the `qdk[qiskit]` extra).
- `qdk.cirq` – Cirq interop: `NeutralAtomSampler` (requires the `qdk[cirq]` extra).

### Top level exports

For convenience, the following helpers and types are also importable directly from the `qdk` root (e.g. `from qdk import code, Result`). Algorithm execution APIs (like `run` / `estimate`) remain under `qdk.qsharp` or `qdk.openqasm`.

| Symbol               | Type     | Origin                          | Description                                                            |
| -------------------- | -------- | ------------------------------- | ---------------------------------------------------------------------- |
| `code`               | module   | `qdk.code`                      | Exposes operations defined in Q\# or OpenQASM                          |
| `init`               | function | `qdk.qsharp.init`               | Initialize/configure the QDK interpreter (target profile, options).    |
| `set_quantum_seed`   | function | `qdk.qsharp.set_quantum_seed`   | Deterministic seed for quantum randomness (simulators).                |
| `set_classical_seed` | function | `qdk.qsharp.set_classical_seed` | Deterministic seed for classical host RNG.                             |
| `dump_machine`       | function | `qdk.qsharp.dump_machine`       | Emit a structured dump of full quantum state (simulator dependent).    |
| `Result`             | class    | `qdk.qsharp.Result`             | Measurement result token.                                              |
| `TargetProfile`      | class    | `qdk.qsharp.TargetProfile`      | Target capability / profile descriptor.                                |
| `StateDump`          | class    | `qdk.qsharp.StateDump`          | Structured state dump object.                                          |
| `ShotResult`         | class    | `qdk.qsharp.ShotResult`         | Multi-shot execution results container.                                |
| `PauliNoise`         | class    | `qdk.qsharp.PauliNoise`         | Pauli channel noise model spec.                                        |
| `DepolarizingNoise`  | class    | `qdk.qsharp.DepolarizingNoise`  | Depolarizing noise model spec.                                         |
| `BitFlipNoise`       | class    | `qdk.qsharp.BitFlipNoise`       | Bit-flip noise model spec.                                             |
| `PhaseFlipNoise`     | class    | `qdk.qsharp.PhaseFlipNoise`     | Phase-flip noise model spec.                                           |
| `Context`            | class    | `qdk.Context`                   | Isolated Q# and OpenQASM interpreter context for independent sessions. |

### Configuration Map

You can provide configuration at initialization time as a Python dictionary.

In Python, pass `qsharp_config: dict[str, int | float | str | bool]` to `Context(...)`.
If `qsharp_config` is omitted, the configuration map is empty. The map is immutable
after initialization. To use different configuration values, create a new `Context`.

In Q#, read values with `Std.Core.ConfigValue(name, defaultValue)`. In Q# code, config
values are immutable: in the same program, repeated calls with the same
`(name, defaultValue)` produce the same result.

Supported types: `int`, `float`, `str`, and `bool` (corresponding to `Int`, `Double`,
`String` and `Bool` in Q#). The type of each value in `qsharp_config` must match the
type of its corresponding default value.

Example:

```python
import qdk
context = qdk.Context(qsharp_config={"experiment_name": "baseline", "shots": 1000})
assert context.eval('Std.Core.ConfigValue("experiment_name", "")') == "baseline"
assert context.eval('Std.Core.ConfigValue("shots", 100)') == 1000
assert context.eval('Std.Core.ConfigValue("noise_level", 0.01)') == 0.01
```

## Telemetry

This library sends telemetry. Minimal anonymous data is collected to help measure feature usage and performance.
All telemetry events can be seen in the source file [telemetry_events.py](https://github.com/microsoft/qdk/tree/main/source/qdk_package/qdk/telemetry_events.py).

To disable sending telemetry from this package, set the environment variable `QDK_PYTHON_TELEMETRY=none`

## Support

For more information about the Microsoft Quantum Development Kit, visit [https://aka.ms/qdk](https://aka.ms/qdk).

## Contributing

Q# welcomes your contributions! Visit the Q# GitHub repository at [https://github.com/microsoft/qdk] to find out more about the project.
