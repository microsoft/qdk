# Pass extension points

This names the points at which a pass may be inserted into the progression from a circuit to generated code, states what each one sees that no other one does, describes the contracts a pass is written against and the pipeline that runs them, and says which of all that is stable.

The names exist in code rather than only here. Rust spells them `qsc_codegen::pass_anchor::PassAnchor` and Python spells them `qdk.circuit.PassAnchor`, and a test compares the set below against the set the code publishes, so the two cannot drift apart quietly.

## The set

| Anchor           | Stable name       | The program is | What only this anchor sees                                                 |
| ---------------- | ----------------- | -------------- | -------------------------------------------------------------------------- |
| `CircuitBuilt`   | `circuit_built`   | a circuit      | register names, moment structure, and a compile-time loop index            |
| `GraphLowered`   | `graph_lowered`   | a graph        | a graph still matching the circuit one for one                             |
| `GraphRewritten` | `graph_rewritten` | a graph        | the graph code generation and the raise will actually see                  |
| `CircuitRaised`  | `circuit_raised`  | a circuit      | the program on the way out, before the export refuses what it cannot write |

```python
import warnings

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
from qdk.circuit import PassAnchor, PassStage

assert [anchor.name for anchor in PassAnchor.all()] == [
    "circuit_built",
    "graph_lowered",
    "graph_rewritten",
    "circuit_raised",
]
assert PassAnchor.CircuitBuilt.stage == PassStage.Circuit
assert PassAnchor.GraphRewritten.stage == PassStage.Graph
assert PassAnchor.CircuitRaised.outbound
```

### `circuit_built`

A pass here runs on the circuit as the user built or imported it, before anything reads it.

Registers, moments, and a loop index that is still a compile-time value exist only at this anchor. Lowering does not carry them, and neither does the OpenQASM crossing. A pass that wants to reason about which qubits belong together, or about a loop before it is unrolled, has exactly one place to run, and this is it.

### `graph_lowered`

A pass here runs on the graph immediately after lowering, before any rewrite has touched it.

Provenance is exact at this anchor and at no later one, because a rewrite is free to retire the node an instruction produced. A pass that has to attribute a graph node back to the instruction it came from, or that wants to record something against the original program before it changes, runs here.

### `graph_rewritten`

A pass here runs on the graph after the built-in rewrites and before it is read for code generation or raised back to a circuit.

This is the anchor the transformation surface exists for. Regions, ports, ownership, effects, interfaces, and stable identity are all present, and this is the last point at which anything can change what is emitted. A pass that reduces, reorders, or re-synthesizes the program belongs here.

### `circuit_raised`

A pass here runs on the circuit that came back out of the graph, before it is exported.

It is the only anchor reached on the way out. The export refuses what the raise could not express, so this is the one place a program can be repaired before that refusal, and the one place the losses on the way out are visible as a circuit rather than as a graph.

## Why the set stops here

An anchor between the graph reading and the emitter would sit on a linear form that has lost what the graph carries and gained nothing a pass can use. It would be a weaker `graph_rewritten` rather than a different anchor.

An anchor in the runtime intermediate representation is deliberately absent. That representation already has its own pass set and its own profile checks, and publishing a second way in through this surface would give one transformation two owners.

The OpenQASM, Qiskit, and Cirq boundaries are conversions of a circuit rather than stages of one. A pass that wants to act there runs at `circuit_built` on the way in or at `circuit_raised` on the way out.

## Where the shape comes from

Each property of this set is one QDK already relies on somewhere else, rather than one borrowed from another toolchain.

| What the set does                                                | Where QDK already does it                                                                                                                                                                            |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Names a stage by what the program becomes, not by device concern | `PipelineStage` in the FIR transform crate names its cutpoints after monomorphization, defunctionalization, UDT erasure, and the rest                                                                |
| Defines a boundary by a property checkable at it                 | `InvariantLevel` in the same crate attaches a specific checkable property to each stage, so being at a boundary is a testable claim rather than a position in a list                                 |
| Separates what is decided now from what is deferred              | `StageDisposition` in the graph lowering crate splits generation from residual, and this surface already publishes that distinction as `PortStage`                                                   |
| Bounds an extension point and reports the bound                  | `StagingConfig` carries specialization and step budgets under hard caps, and the graph rewrite driver carries an application budget whose exhaustion is a reported outcome rather than a silent stop |
| Selects the transforms by target rather than fixing them         | the resource estimator pairs an architecture with a QEC strategy, and that pairing is what selects the trace transforms that run                                                                     |

Catalyst supplied the idea that a stage set should be organized by what the intermediate representation becomes, and that capability is added by inserting one pass at a named anchor. Its own five stages do not transfer, because three of them exist only because of a host stack QDK does not have.

## What is not named here

- Accepting passes from outside the repository, which needs a trust model that does not exist yet.

## The contracts a pass is written against

Two, and the reason there are two is structural rather than stylistic.

A **patch** says what changes about a node that already exists. It may replace the node, keeping its operands, or remove it, splicing each operand thread past it. The rewrite framework certifies a patch and applies it under an application budget, and reports what it did not reach rather than stopping quietly.

A **fragment** says what a node becomes, which may be several nodes. The native rewrite engine can create nodes, but the public patch decision surface only replaces or removes existing nodes. Caller-supplied expansion uses a separate native transaction path rather than claiming that a certificate proves its equivalence.

Both share the same two properties. A pass declares which operations it applies to and the traversal stays native, so a pass is consulted once per match rather than once per node, and what it costs at the boundary follows how much it changes rather than how large the program is. And nothing handed to a pass borrows the graph: a match is a plain record and a decision is a plain record, which is why the graph's transaction type, which carries a lifetime tied to the graph it edits, is absent from this surface rather than merely undocumented.

A node that produces a value cannot be stood in for by either contract, because neither a splice nor a chain can produce that value. That is refused by name rather than silently skipped.

See CIRCUIT-USAGE.md for worked examples of both.

## The pipeline

A pipeline is a set of passes registered at anchors. Running one walks a circuit through the whole progression once, stopping at each anchor to run whatever is registered there.

Order is fixed by two rules and nothing else. Anchors run in the order the progression reaches them, which is the order they are listed above. Inside one anchor, passes run in the order they were registered. A pipeline can be added to and cannot be reordered, replaced, or emptied, which is what makes an anchor a stable place to attach work while what runs there stays free to change.

A pass the compiler ships and a pass a caller writes are the same kind of thing. Both are registered against a named anchor, both appear in the same listing, and both are subject to the same check that the program at that anchor is the kind of program the pass expects. Registering a graph pass at a circuit anchor is refused by name rather than discovered part way through a run.

Today the pipeline ships two passes, both written in Rust, and both at the graph-rewritten anchor. The first cancels adjacent pairs of the same self-inverse operation on the same qubits. The second retires a gate that is diagonal in the computational basis when every qubit it holds runs straight into a Z-basis measurement or a reset, because a phase applied immediately before that basis collapses changes nothing anyone can observe. Neither is expressible through the bound contract, which matches on an operation name alone: whether a pair cancels depends on what sits between the two nodes and on which qubits each one holds, and whether a phase is removable depends on what consumes every one of the gate's operands. That is the point of having both surfaces. The compiler's own passes are not a privileged copy of the caller's contract, they are the same registration with more of the graph in view.

The second pass is conservative in two directions worth naming, because both look like gaps and are not. It reads a declaration in the operation contract registry rather than a matrix, so it acts only on operations this compiler has decided about. And it requires _every_ operand to reach a collapsing consumer: a two-qubit diagonal gate with one operand measured and one left live moves a phase onto the operand that survives, which is observable. An X-basis or Y-basis measurement is not a collapsing consumer either, for the same reason in reverse.

```python
import warnings

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
from qdk.circuit import PassAnchor, PassPatch, PassPipeline, QuantumCircuit

pipeline = PassPipeline()
assert [
    (entry.anchor.name, entry.name, entry.built_in) for entry in pipeline.listing()
] == [
    ("graph_rewritten", "cancel-inverse-pairs", True),
    ("graph_rewritten", "retire-diagonal-before-z-collapse", True),
]

pipeline.add_rewrite_pass(
    PassAnchor.GraphRewritten,
    "x-becomes-y",
    ["quantum.x"],
    lambda match: PassPatch.replace("quantum.y"),
)

circuit = QuantumCircuit(1)
circuit.h(0)
circuit.h(0)
circuit.x(0)

result = pipeline.run(circuit)

# The built-ins ran in registration order, then the registered pass ran on what
# was left. Nothing here is measured, so the second built-in had nothing to do.
assert [entry.name for entry in result.executed] == [
    "cancel-inverse-pairs",
    "retire-diagonal-before-z-collapse",
    "x-becomes-y",
]
assert "retire-diagonal-before-z-collapse" not in result.changed
assert "y q[0];" in result.circuit.to_qasm()
assert "h q[0];" not in result.circuit.to_qasm()
```

A run reports the passes that ran, in the order they ran, and how many times a pass was consulted in total. The circuit handed in is not changed. The result carries a new circuit and the graph as it stood when the last graph anchor finished with it.

## Native C and Go consumers

The [experimental C adapter](../circuit_capi/README.md) exposes the same four anchors and native scheduler without loading Python. Graph callbacks receive callback-duration match records and return owned keep/replace/remove/expand decisions. Circuit callbacks receive a borrowed working circuit and a checked replacement-offer scope. Their dependents are disposed child-first on every callback exit; unfinished construction is rejected rather than published.

Context state crosses as a `uintptr_t` cookie with explicit destructor ownership. The [Go consumer](../circuit_capi/tests/go/shim.go) uses `runtime/cgo.Handle`, not a retained Go pointer, and translates Go panics before they can cross the C ABI. Callbacks run synchronously on the calling thread, and active input/pipeline release or reentry is refused. This is in-process interoperability, not arbitrary plugin discovery or a sandbox/trust model.

Run `python3 source/circuit_capi/build.py --test` for the generated-header C and strict cgo gates. Native pipeline outputs preserve the existing graph-crossing limitations, including loss of named registers. No stable C ABI or full Go SDK is promised.

## What is stable

Nothing here is stable yet. `qdk.circuit` is experimental, importing it says so, and every name on this page may change in any release while that marker is there.

What follows is what the contract becomes when the marker is removed, stated now so the split is deliberate rather than whatever happens to be true later. It is two tiers, which is the shape Qiskit settled on for the same reason: the places you attach work have to outlive the work that happens there.

The stable tier is the shape of the extension surface.

- The anchor set: which anchors exist, what each is named, what the program is at each one, and the order a run reaches them.
- That a pass registered at an anchor runs at that anchor, and that passes at one anchor run in registration order.
- The two contracts: that a pass declares the operations it applies to, is handed a plain record for each match, and returns either a patch or a fragment.
- That a pass is consulted once per match rather than once per node.
- That a pipeline can be added to and not reordered, replaced, or emptied.
- That a request a contract cannot express is refused by name rather than quietly ignored.

The unstable tier is everything about what happens inside an anchor.

- Which passes ship registered by default, how many there are, what they are named, and what they do. A release may add one, remove one, or change what one does.
- The operation vocabulary the graph uses, and therefore the strings a pass matches on.
- The internal structure of the graph, including how many nodes stand for a given program.
- What a given circuit compiles to. A pipeline that ships a new pass produces different output for the same input, and that is not a break.

The practical reading: write a pass against an anchor and a contract, and do not write one against the output of the pipeline as it stands today.
