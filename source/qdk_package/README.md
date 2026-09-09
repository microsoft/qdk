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

## OpenQASM parsing and analysis

The preview `qdk.openqasm.parser` and `qdk.openqasm.semantic` modules expose
read-only syntax and semantic trees. `parse` is recovery-oriented and returns
diagnostics on its result rather than raising; `analyze` additionally resolves
symbols, checks types, and evaluates constants. Node, symbol, and diagnostic
spans are global, half-open UTF-8 byte ranges resolved through the immutable
document the result owns.

```python
from qdk.openqasm import analyze, dumps, parse

parsed = parse(
    'OPENQASM 3.0; include "defs.inc"; qubit q; local q;',
    path="memory://workspace/main.qasm",
    includes={"memory://workspace/defs.inc": "gate local q { x q; }"},
)
assert not parsed.has_errors
assert parsed.program.document is parsed.document
assert dumps(parsed.program).startswith("OPENQASM 3.0;")

analysis = analyze("OPENQASM 3.0; int value = missing;")
assert analysis.has_errors
diagnostic = analysis.diagnostics[0]
source_range = analysis.document.source_map.range_from_span(diagnostic.labels[0].span)
assert source_range.source_id == analysis.document.entry.id
```

Both trees compare and hash structurally, ignoring source position and the
document a node came from, so the same construct written twice compares equal.
Resolved types and constant values are structured nodes rather than strings, so
dispatch over them with `isinstance`. `QASMVisitor` walks either tree, and
`dumps` writes canonical source for a whole syntactic program. Most class names
appear in both layers, so `parser.SyntaxNode` and `semantic.SemanticNode` answer
which tree a value came from.

These APIs are in preview and may change between QDK releases. Run
`help(qdk.openqasm.parser)` and `help(qdk.openqasm.semantic)` for the full
contracts, including include resolution, the shared-class exception, the
canonical format's guarantees, and the visitor's context protocol.

## Public API Surface

### Error Correction Preview

`qdk.ec` analyzes codes and gadgets and can derive checks and readout equations.
It requires Python 3.11 or newer and qodec 0.1.x. Install the local qodec Python
bindings first while that version is unpublished, then install `qdk[ec]`.

Use `qodec.gadgets.Circuit(instruction_set, source, format=...)` for circuits.
Parsed invocations are `circuit.calls`; each call has positional `operands`
and named classical `arguments`. The instruction definitions are in
`circuit.instruction_set.instructions`.

Gadget readouts are typed objects with `name`, `position`, `is_flag`, and
`equation`. Constructors and setters accept these values directly, preserving
names and equations while recomputing positions and roles. Parity sequences
and named mappings are also accepted. Checks, readouts, and readout equations
are immutable tuple snapshots; assign new collections to change the gadget.
`str(readout)` gives the authored form, and `repr(readout)` includes its equation.
Bundle fixtures use schema version 7;
`Qodec.load` takes an explicit manifest or bundle file path, not a directory.

The `qdk.ec` public API is unchanged by this qodec migration.

`ec.audit` checks code algebra, complete Clifford maps (including implicit
identities), gadget actions, and check, flag, and readout equations. It also
owns protocol completeness, code-list shapes and capacities, reference bounds,
parameter uses, and circuit-call validity. These declaration findings use
`qodec/invalid-structure`; qodec itself enforces only preservation preconditions
such as unambiguous resolution and bindings that can survive a round trip.
This replaces `gadget/reference-out-of-bounds` and
`gadget/missing-source-instruction`; update any rule filters using those IDs.
Invalid prerequisites block dependent gadget analyses, including shared
definitions, while unrelated gadgets remain analyzable. Direct analysis calls
still check their mathematical preconditions.

Algebraically incorrect drafts can be constructed, loaded, and saved by
qodec, as can unequal code lists, incomplete protocols, and malformed circuit
text. Parsed accessors can fail on a loadable draft. Entirely omitted readout
lists are allowed for later derivation and reported as informational; partially
supplied lists also persist but are audit errors.

Parity verification assumes valid noiseless input
codewords with arbitrary incoming Pauli frames. A measurement readout that
omits a required logical-frame correction is an error even if it works for a
zero-frame input. The authored C4 example currently has such omissions; the
audit reports them without changing the protocol.

Readout references are solved as binary linear equations, including cycles
with unique consistent solutions. Output stabilizer signs must be determined
by valid declared constraints, not merely mentioned in them. Empty equations
are zero but provide no constraint. Unsupported parity analysis produces
warnings rather than claiming a result. These checks do not establish fault
tolerance or verify a particular fault model.

`gadget/missing-check` is informational. It reports an independent noiseless
measurement check that is available but not implied by valid declared checks,
readout definitions, and verified zero-valued flags. Candidates combine circuit
bits and incoming stabilizer signs, under the same arbitrary-frame contract.
Equivalent XOR bases are accepted; duplicate and invalid checks do not hide
omissions. Each finding includes a copyable equation. Unsupported discovery
produces an informational notice, never an invented equation. Authors may
intentionally leave checks for derivation, so these findings are not promoted
by `promote_warnings=True` and do not claim inadequate fault tolerance.

Input and output frames are not symmetric requirements. Input signs are supplied
boundary information; output stabilizer signs must be determined from that
information and the circuit results. A gadget need not remeasure or reconstruct
every input sign. There is therefore no `gadget/incomplete-input-frame` rule.
Missing dependence on an input sign is caught by check/readout verification;
an omitted available syndrome relation is a missing check. Pure input-to-output
transport remains the responsibility of `gadget/incomplete-output-frame`.

The registry has 14 built-in rule IDs. This adds no public Python exports:
`missing-check` follows `missing-observable` and `missing-flag`; `no-checks`
would miss incomplete nonempty check lists.

### Submodules

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

In Python, pass `qdk_config: dict[str, int | float | str | bool]` to `Context(...)`.
If `qdk_config` is omitted, the configuration map is empty. The map is immutable
after initialization. To use different configuration values, create a new `Context`.

In Q#, read values with `Std.Core.ConfigValue(name, defaultValue)`. In Q# code, config
values are immutable: in the same program, repeated calls with the same
`(name, defaultValue)` produce the same result.

Supported types: `int`, `float`, `str`, and `bool` (corresponding to `Int`, `Double`,
`String` and `Bool` in Q#). The type of each value in `qdk_config` must match the
type of its corresponding default value.

Example:

```python
import qdk
context = qdk.Context(qdk_config={"experiment_name": "baseline", "shots": 1000})
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
