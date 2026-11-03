---
description: Building, inspecting, compiling and saving QDK circuits.
---

# Building circuits

The `qdk.circuit` module is experimental. Importing it emits a warning, and the
API may change in any release until that marker is removed. Silence the warning
before the import if you accept that contract:

```python
import warnings

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
from qdk.circuit import QuantumCircuit
```

Filtering on `ExperimentalWarning` by category works for warnings raised after
import, but not for the import warning itself, because the class only becomes
importable once the module has run.

For native consumers, the [experimental C interface](../circuit_capi/README.md)
exposes the same existing circuit, graph and pass capabilities without loading
Python. Its compiled C and Go/cgo examples describe explicit ownership and
callback lifetimes; its Fortran examples cover construction, ownership/errors,
and adaptive QIR snapshots. Python remains a direct sibling adapter over Rust.

For runnable tutorials, see the [circuit API notebooks](../../samples/notebooks/circuit_api/README.md)
for Python, C, Go, and Fortran. They build Base and Adaptive circuits, transfer
native snapshots into Python, and simulate compiled QIR on CPU or an opt-in GPU.
Circuits do not have a direct `run` method; use `qdk.simulation.run_qir` on their QIR.

## Constructing a circuit

Gate methods take qubit indices and return the circuit, so calls chain:

```python
circuit = QuantumCircuit(2, 1)
circuit.h(0).cx(0, 1)
circuit.measure(0, 0)
```

Negative indices count from the end, matching normal Python sequence behavior.
Reserving capacity before a large build separates vector growth from per-gate
cost:

```python
circuit = QuantumCircuit(32)
circuit.reserve(1_000_000)
```

## Registers

Registers are named views over non-overlapping index ranges. They are laid out
end to end, and a duplicate name is rejected:

```python
from qdk.circuit import ClassicalRegister, QuantumCircuit, QuantumRegister

alice = QuantumRegister("alice", 2)
bob = QuantumRegister("bob", 3)
readout = ClassicalRegister("readout", 2)
circuit = QuantumCircuit.from_registers(alice, bob, readout)

placed = {r.name: r for r in circuit.quantum_registers}
circuit.cx(placed["alice"][1], placed["bob"][0])
circuit.measure(placed["alice"][0], readout[0])
```

Each kind of register is laid out in its own index space, so qubit and classical
bit numbering are independent. Registers cannot alias. Two registers of the same
kind never share a bit, which is checked at construction rather than diagnosed
later.

## Inspecting instructions

Indexing a circuit returns a read-only view. Mutating the view does not change
the circuit, and the operand lists it returns are copies:

```python
inspected = QuantumCircuit(2)
inspected.h(0)

view = inspected[0]
assert view.name == "h"
assert view.qubits == [0]
```

## Parameters

A parameter is symbolic at compile time and is bound before compilation. It is
owned by the circuit that created it:

```python
circuit = QuantumCircuit(1)
theta = circuit.parameter("theta")
circuit.rx(theta, 0)

assert circuit.parameters == [theta]
bound = circuit.assign_parameters([(theta, 0.5)])
assert bound.parameters == []
```

Binding returns a new circuit and leaves the original alone. Only parameters
that some instruction actually references are reported, so declaring one and
never using it does not oblige you to supply a value.

Parameters support arithmetic, so an angle can be computed rather than supplied
whole:

```python
import math

circuit = QuantumCircuit(1)
theta = circuit.parameter("theta")
circuit.rx(theta / 2, 0)
circuit.ry(2 * theta + math.pi, 0)

bound = circuit.assign_parameters([(theta, 1.0)])
assert bound[0].params == [0.5]
```

The arithmetic is resolved before compilation, so it never reaches the target as
a computation and never raises the capability a circuit demands.

A parameter is not a classical variable. Parameters disappear at binding time;
classical variables are read during execution.

## Control flow

Control-flow bodies are built with context managers. Instructions written
inside a body belong to that body, not to the enclosing circuit:

```python
circuit = QuantumCircuit(2, 1)
circuit.h(0)
circuit.measure(0, 0)

with circuit.if_test(0, 1):
    circuit.x(1)
```

An alternative branch comes from the builder, and the pair is stored as a
single construct:

```python
builder = circuit.if_test(0, 1)
with builder:
    circuit.x(0)
with builder.otherwise():
    circuit.z(0)
```

A condition can test a whole classical register rather than one bit. The
register is compared against the value as an unsigned integer, with its first
bit the least significant:

```python
readout = ClassicalRegister("readout", 2)
tested = QuantumCircuit.from_registers(QuantumRegister("q", 1), readout)
tested.measure(0, readout[0])

with tested.if_test(readout, 3):
    tested.x(0)
```

A value the register is too narrow to hold is rejected, because such a
condition could never be true and is more likely a mistake than an intent.

Loops and grouping follow the same shape:

```python
with circuit.while_loop(0, 1):
    circuit.h(0)
    circuit.measure(0, 0)

with circuit.for_loop([0, 1, 2]):
    circuit.x(0)

with circuit.box_("state preparation"):
    circuit.h(0)
    circuit.cx(0, 1)
```

A moment groups instructions that run at the same time. Unlike a box, which only
groups, a moment asserts simultaneity, so two instructions inside one may not
touch the same qubit:

```python
scheduled = QuantumCircuit(3)
with scheduled.moment():
    scheduled.h(0)
    scheduled.h(1)
with scheduled.moment():
    scheduled.cx(0, 1)
    scheduled.x(2)
```

An overlap is rejected at construction rather than left for a target to
discover, because simultaneity is the whole of what a moment claims.

A switch branches many ways on one value. The switch itself is the context
manager, and each case is opened from it, so the whole construct is written
once when the switch closes:

```python
selector = QuantumCircuit(1, 1)
selector.measure(0, 0)

with selector.switch(0) as selected:
    with selected.case(0):
        selector.x(0)
    with selected.case(1):
        selector.z(0)
    with selected.default():
        selector.h(0)

assert selector[1].name == "switch"
assert selector[1].cases == [[0], [1]]
```

A case can match several values, as `selected.case(1, 2, 3)`. A switch needs at
least one case, and it has at most one default. The values a case matches are
reported by `cases`, aligned with the leading entries of `blocks`, and the
default body is named separately by `default_block`.

A switch selects on a classical bit or a runtime loop index. It does not select
on a register, because a register condition is written as one test per bit,
which is a truth value rather than the number a switch compares against.

`break_loop` and `continue_loop` leave and restart the loop they sit in:

```python
searching = QuantumCircuit(1, 1)
with searching.while_loop(0, 1):
    searching.h(0)
    searching.measure(0, 0)
    with searching.if_test(0, 0):
        searching.break_loop()
```

Both are refused outside a loop body, naming the reason, because nothing can
lower a jump that has no loop to leave. A conditional nested inside a loop is
still inside it; a box on its own is not a loop.

A for loop hands its index to the body, so it can drive the angles inside:

```python
import math

steps = QuantumCircuit(1)
with steps.for_loop([k * math.pi / 4 for k in range(4)]) as angle:
    steps.rx(angle, 0)
    steps.h(0)
```

The index is a parameter, so it can be used anywhere an angle can, including in
arithmetic such as `angle / 2`. It belongs to the loop rather than to the
circuit, so it is not reported by `parameters` and `assign_parameters` does not
ask for a value for it.

An index used in an angle has to be known before the program runs, which means
the loop is unrolled during compilation. Pass `runtime=True` for an index the
program reads while it runs, which can be tested in a condition instead:

```python
counted = QuantumCircuit(1, 1)
with counted.for_loop([0, 1, 2], runtime=True) as step:
    with counted.if_test(step, 1):
        counted.x(0)
```

If the body raises, nothing is emitted. A failed build leaves no half-formed
construct in the circuit.

Use `body_of` to inspect what a construct contains:

```python
grouped = QuantumCircuit(2)
with grouped.box_("prep"):
    grouped.h(0)
    grouped.cx(0, 1)

assert [view.name for view in grouped.body_of(0)] == ["h", "cx"]
```

`body_of` reports every block of a construct in one list, which is enough to see
what it holds but not to tell a conditional's branches apart. A view names its
blocks, and `block` reads one of them, so the branches stay distinct:

```python
branching = QuantumCircuit(1, 1)
branching.measure(0, 0)
builder = branching.if_test(0, 1)
with builder:
    branching.x(0)
with builder.otherwise():
    branching.z(0)

conditional = branching[1]
then_block, else_block = conditional.blocks
assert [view.name for view in branching.block(then_block)] == ["x"]
assert [view.name for view in branching.block(else_block)] == ["z"]
```

Blocks are named rather than expanded, so reading an instruction costs the same
whatever it contains.

A view also reports the condition a construct branches on, as a tree:

```python
condition = conditional.condition
assert condition.kind == "binary"
assert condition.op == "=="
assert condition.operands[0].kind == "clbit"
assert condition.operands[0].clbit == 0
```

## Drawing a circuit

`print` draws the circuit as text:

```python
drawn = QuantumCircuit(2, 1)
drawn.h(0)
drawn.cx(0, 1)
drawn.measure(0, 0)

text = str(drawn)
assert "H" in text
```

`to_diagram` returns the same diagram type the package uses for Q# programs, so
a circuit built here displays the same way as one traced from Q# code:

```python
diagram = drawn.to_diagram()
assert "componentGrid" in diagram.json()
```

Gates are drawn under the diagram's own names rather than the names used to
build them. A controlled gate is drawn as its base gate with a control wire, so
`cx` appears as an `X` with a control, and an adjoint is flagged rather than
renamed, so `sdg` appears as an `S` marked adjoint.

Instructions that do not share a qubit share a column, and an instruction is
never drawn to the left of an earlier instruction on the same wire.

The diagram is a display format, not a program representation, so two things do
not survive the conversion. Classical bit indices appear as operation arguments,
because the diagram addresses measurement results per qubit and has no classical
register model. Control-flow bodies are drawn as a single placeholder spanning
every qubit rather than being expanded, because the diagram has no nesting that
matches the block model. Use `body_of` to inspect a body.

## Saving and restoring

`to_bytes` saves a circuit as a QDK-native packed Cap'n Proto envelope, and
`from_bytes` restores it:

```python
saved = QuantumCircuit(2, 1)
saved.h(0)
saved.cx(0, 1)
saved.measure(0, 0)

data = saved.to_bytes()
loaded = QuantumCircuit.from_bytes(data)

assert len(loaded) == len(saved)
assert str(loaded) == str(saved)
```

Save and restore is exact. Every circuit that can be built can be written and
read back with the same instructions, operands, angles, parameters, classical
variables, control flow, and registers. A restored circuit can be extended,
drawn, and have its parameters bound like any other:

```python
extended = QuantumCircuit.from_bytes(data)
extended.h(1)

assert len(extended) == 4
```

Use `to_text` for an S-expression envelope that can be inspected and diffed.
Both encodings represent the same model:

```python
text = saved.to_text()
assert text.startswith("QDKCIRCT(\n(qdk 1 ")
assert QuantumCircuit.from_text(text).to_bytes() == data
```

Restoring applies the same checks as building. A saved circuit that names a
qubit outside the circuit, refers to a block that does not exist, or carries a
different schema version is rejected with a `ValueError` rather than partly
restored.

A circuit cannot be saved while a control-flow block is still open, because
there is no complete circuit to save yet. Close the `with` block first.

The decoder bounds nesting and binary traversal. As before, callers accepting
untrusted circuits must limit input size. Size depends on the circuit; no fixed
bytes-per-instruction bound is promised.
Floating-point values retain their IEEE bit patterns, including signed zero
and NaN payloads. The current typed circuit schema is version 1.
This spike replaces the previous snapshot implementation outright; it has no
legacy reader, converter or compatibility promise.

`QuantumGraph` offers the same four methods. Graph snapshots additionally check
the contract-registry version, required features, content digest and graph
validity, retaining the graph decoder's 16 MiB encoded-input limit and a separate
64 MiB expanded-message limit. A graph digest covers canonical uncompressed
payload bytes, not packed transport bytes, so changing S-expression whitespace
does not invalidate it. Graph exports may describe unverified IR for inspection;
import still requires a valid graph. See the [typed format contract](../compiler/qsc_snapshot/README.md).

This follows [HUGR's envelope/model approach](https://github.com/Quantinuum/hugr/blob/ca20348350916739b9df4fc4e6c5f75ecf927211/specification/serialization.md),
not its graph vocabulary or wire schema. QDK snapshots cannot be loaded as
HUGR documents. JSON used for circuit display and resource-estimation APIs is
unaffected.

## Compiling to QIR

`to_qir` compiles a circuit for a named target profile:

```python
from qdk import TargetProfile

program = QuantumCircuit(2, 2)
program.h(0)
program.cx(0, 1)
program.measure(0, 0)
program.measure(1, 1)

qir = program.to_qir(TargetProfile.Base)

assert "define i64 @ENTRYPOINT__main()" in qir
```

The profile decides what the circuit is allowed to contain. `Base` takes a
straight-line program. Measurement feedback needs one of the adaptive profiles:

```python
feedback = QuantumCircuit(2, 2)
feedback.h(0)
feedback.measure(0, 0)
with feedback.if_test(0, 1):
    feedback.x(1)
feedback.measure(1, 1)

adaptive_ri = feedback.to_qir(TargetProfile.Adaptive_RI)
adaptive = feedback.to_qir(TargetProfile.Adaptive)

assert "__quantum__rt__read_result" in adaptive_ri
assert "__quantum__rt__read_result" in adaptive
assert '!"qir_major_version", i32 1' in adaptive_ri
assert '!"qir_major_version", i32 2' in adaptive
```

`Adaptive_RI` emits QIR v1, while full `Adaptive` emits QIR v2. The
[conversion tests](tests/test_circuit_conversion.py) compare the complete output
of this circuit against separate, shared
[QIR snapshots](../circuit_capi/tests/fixtures/) also used by the Go and Fortran
examples. These regression tests require no QIR runner and never rewrite the
expected files.

Asking for more than the profile offers is refused, and the refusal names both
the construct and the capability that is missing rather than failing generically:

```python
try:
    feedback.to_qir(TargetProfile.Base)
except ValueError as error:
    assert "'if'" in str(error) and "Adaptive" in str(error)
```

Gates with no QIR intrinsic are rewritten into ones that have, so a circuit does
not have to be written against the intrinsic set. An unbound parameter is
refused by name, because there is no value to emit.

## OpenQASM

`to_qasm` exports OpenQASM 3 and `from_qasm` reads it back:

```python
text = program.to_qasm()

assert text.startswith("OPENQASM 3.0;")

restored_from_qasm = QuantumCircuit.from_qasm(text)

assert restored_from_qasm.num_qubits == 2
assert restored_from_qasm.to_qasm() == text
```

Control flow crosses in both directions, so a circuit with conditionals, loops,
boxes, and switches survives the round trip rather than exporting one way only.
A program containing a switch declares `OPENQASM 3.1`, because the front end
refuses a switch under the earlier header; everything else declares `3.0`.

Some things do not survive the crossing, all of them losses on the way out:

- Register names. Export writes one qubit register and one classical register
  whatever the circuit was built from, so a restored circuit has no named
  registers.
- A moment comes back as a box, because OpenQASM cannot assert simultaneity.
- A compile-time loop index comes back as a runtime one, because OpenQASM has
  one kind of loop variable and it is read while the program runs.
- An angle comes back as the nearest angle OpenQASM can hold, because it stores
  an angle as a fixed-point fraction of a turn rather than as a float. The
  difference is around the last bit of a double, not enough to change a result
  and enough to fail an exact comparison.
- An operation OpenQASM cannot name comes back as the rewrite that was written
  for it. OpenQASM measures and prepares in the Z basis only, so `measure_x`,
  `measure_y`, `prepare_x`, and `prepare_y` are exported as the rotation that
  puts the basis there and back, and they return as those instructions rather
  than as themselves. `prepare_z` returns as `reset`, which is the same
  operation under the one spelling OpenQASM has for it.

The last two are worth seeing, because neither raises and neither is visible in
the exported text:

```python
turned = QuantumCircuit(1, 1)
turned.measure_x(0, 0)

assert "h q[0];" in turned.to_qasm()

rotated = QuantumCircuit(1)
rotated.rx(0.5, 0)

assert "rx(0.5)" in rotated.to_qasm()
assert QuantumCircuit.from_qasm(rotated.to_qasm()).to_bytes() != rotated.to_bytes()
```

Source that does not parse, and source using a construct a circuit cannot hold,
are both refused with the reason named.

## The graph

A circuit is one shape a program can take. The graph is the other, and it is
what passes will run on. `to_graph` lowers a circuit into it and `from_graph`
raises one back:

```python
graph = program.to_graph()

restored_from_graph = QuantumCircuit.from_graph(graph)

assert restored_from_graph.to_bytes() == program.to_bytes()
```

The two representations disagree about what a qubit is. A circuit addresses a
qubit by index and reuses that index freely; the graph carries a qubit as a
capability that flows along an edge and is used exactly once. Lowering turns an
index into a chain of capabilities, which is why the graph has more nodes than
the circuit had instructions:

```python
operations = [node.operation for node in graph.nodes]

assert "quantum.allocate" in operations
assert "quantum.h" in operations
```

A graph is read through plain records rather than live views, so nothing you
hold keeps the graph borrowed. `regions`, `nodes`, `ports`, and `edges` each
answer with a list, and `diagnostics` reports what the graph's own verification
says, with an empty list meaning every rule is satisfied:

```python
assert graph.diagnostics() == []
```

`GraphBuilder` builds one directly, for programs no circuit can express:

```python
from qdk.circuit import (
    EdgeKind,
    GraphBuilder,
    PortDirection,
    PortKind,
    PortStage,
    PortUsage,
    RegionKind,
    ValueType,
)

builder = GraphBuilder()
region = builder.add_region(RegionKind.ModuleOmega)
allocate = builder.add_node(region, "quantum.allocate")
produced = builder.add_port(
    allocate,
    PortDirection.Output,
    PortKind.Capability,
    ValueType.qubit(),
    PortStage.Residual,
    PortUsage.Affine,
)
release = builder.add_node(region, "quantum.release")
consumed = builder.add_port(
    release,
    PortDirection.Input,
    PortKind.Capability,
    ValueType.qubit(),
    PortStage.Residual,
    PortUsage.Affine,
)
builder.connect(produced, consumed, EdgeKind.AffineCapability)

by_hand = builder.finish()

assert len(by_hand.nodes) == 2
```

Each call is checked as it is made, so an edge between ports of different types
is refused where it is written rather than at the end. A builder produces one
graph and is spent afterwards.

The crossing back to a circuit is the only best-effort direction in the API,
because the graph expresses programs a circuit has no form for. Those are
refused with the operation named, so you can tell which side of the boundary
you landed on:

```python
foreign = GraphBuilder()
foreign_region = foreign.add_region(RegionKind.ModuleOmega)
foreign_node = foreign.add_node(foreign_region, "quantum.magic_state_distill")
foreign.add_port(
    foreign_node,
    PortDirection.Output,
    PortKind.ControlExit,
    ValueType.unit(),
    PortStage.Residual,
    PortUsage.Affine,
)

outside = foreign.finish()

try:
    QuantumCircuit.from_graph(outside)
except ValueError as error:
    assert "magic_state_distill" in str(error)
```

Lowering refuses unsupported constructs by name too. Symbolic angles and
supported if/else conditionals cross in both directions. Loops, delays, and
unsupported custom/unitary operations still have no graph form. Raising does
not recover named circuit registers.

Raising also answers where each recovered operation came from.
`from_graph_with_origins` returns the circuit together with one origin per
instruction:

```python
from qdk.circuit import OriginKind

raised = QuantumCircuit.from_graph_with_origins(graph)

assert raised.circuit.to_bytes() == program.to_bytes()
assert raised.origins[0].kind == OriginKind.Instruction
assert raised.origins[0].index == 0
```

An origin is not the raised program's own numbering. Each node has carried its
origin since it was created, so a pass that removes operations leaves the
survivors naming the instructions they came from: a gate written third and now
raised first still reports index two. A node carrying no circuit origin reports
`None` rather than a guess, which is what a graph built anywhere else looks like
from here.

A graph compiles on its own, without being raised first:

```python
assert graph.to_qir(TargetProfile.Base) == program.to_qir(TargetProfile.Base)
```

That equality is the point of the two paths existing, not the point of the graph
path existing. Compiling the graph reads the graph rather than the circuit it
came from, so a program a pass produced reaches QIR whether or not a circuit
could still express it. The profile is the same one the circuit surface names,
and a graph the path does not compile is refused with the operation named:

```python
try:
    outside.to_qir(TargetProfile.Base)
except ValueError as error:
    assert "magic_state_distill" in str(error)
```

OpenQASM from a graph is the raise followed by the circuit's own exporter, so it
offers nothing the raise does not and cannot disagree with it:

```python
assert graph.to_qasm() == QuantumCircuit.from_graph(graph).to_qasm()
```

## Writing a pass

A pass names the operations it applies to and is called once for each match.
The traversal stays on the native side, so what a pass costs at the boundary
follows how much it changes rather than how large the program is.

A pass that returns a patch says what changes about a node that already exists.
It may replace the node or remove it, and removing splices the operands past it:

```python
from qdk.circuit import PassPatch

noisy = QuantumCircuit(1)
noisy.x(0)
noisy.h(0)
noisy.x(0)

result = noisy.to_graph().rewrite(["quantum.x"], lambda match: PassPatch.remove())

assert result.outcome == "committed"
assert result.applications == 2
assert result.crossings == 2
assert result.graph.diagnostics() == []

remaining = QuantumCircuit(1)
remaining.h(0)
assert result.graph.to_qir(TargetProfile.Base) == remaining.to_graph().to_qir(
    TargetProfile.Base
)
```

Returning `None` declines a match, and a run is bounded: pass `budget` to cap
how many decisions are applied, and the result says what it did not reach
rather than stopping quietly.

A pass that returns a fragment says what a node becomes, which may be several
nodes. This is a separate call because a patch cannot create a node, so a
decomposition that grows the program has no patch form:

```python
wide = QuantumCircuit(1)
wide.x(0)
wide.t(0)

expanded = wide.to_graph().expand(
    ["quantum.x"], lambda match: ["quantum.h", "quantum.z"]
)

decomposed = QuantumCircuit(1)
decomposed.h(0)
decomposed.z(0)
decomposed.t(0)
assert expanded.graph.to_qir(TargetProfile.Base) == decomposed.to_graph().to_qir(
    TargetProfile.Base
)
```

Both calls leave the graph they were given alone and hand back a new one.
Nothing a pass receives borrows the graph, which is why a match is a plain
record rather than a live view.

Not every node can be stood in for. A node that produces a value, such as a
measurement, is refused by name, because neither a splice nor a chain can
produce that value:

```python
measured = QuantumCircuit(1, 1)
measured.measure(0, 0)

try:
    measured.to_graph().rewrite(
        ["quantum.measure_z"], lambda match: PassPatch.remove()
    )
    raise AssertionError("expected a refusal")
except ValueError as error:
    assert "quantum.measure_z" in str(error)
```

## Running passes in a pipeline

Calling `rewrite` or `expand` on a graph runs one pass at one point. A pipeline
registers passes at the named anchors instead, and walks a circuit through the
whole progression, from the circuit as you built it, down to the graph, and back
out to a circuit again.

Anchors run in the order the progression reaches them, and passes at one anchor
run in the order they were registered. A pipeline carries the compiler's own
passes unless you decline them. There are two of them today, both at the
graph-rewritten anchor: one cancels adjacent pairs of the same self-inverse
operation on the same qubits, and one retires a gate that is diagonal in the
computational basis when every qubit it holds runs straight into a Z-basis
measurement or a reset. The second only has something to do in a program that
measures, so it is listed but idle below:

```python
from qdk.circuit import PassAnchor, PassPipeline

pipeline = PassPipeline()
assert [entry.name for entry in pipeline.listing()] == [
    "cancel-inverse-pairs",
    "retire-diagonal-before-z-collapse",
]

pipeline.add_rewrite_pass(
    PassAnchor.GraphRewritten,
    "x-becomes-y",
    ["quantum.x"],
    lambda match: PassPatch.replace("quantum.y"),
)


def add_a_final_t(circuit):
    circuit.t(0)


pipeline.add_circuit_pass(PassAnchor.CircuitRaised, "add-a-final-t", add_a_final_t)

program = QuantumCircuit(1)
program.h(0)
program.h(0)
program.x(0)

run = pipeline.run(program)

assert [entry.name for entry in run.executed] == [
    "cancel-inverse-pairs",
    "retire-diagonal-before-z-collapse",
    "x-becomes-y",
    "add-a-final-t",
]

finished = QuantumCircuit(1)
finished.y(0)
finished.t(0)
assert run.circuit.to_qasm() == finished.to_qasm()
```

A pass at a circuit anchor is handed a `QuantumCircuit`. It may edit it and
return nothing, as above, or return a circuit of its own. A pass at a graph
anchor uses the same two contracts as `rewrite` and `expand`, registered with
`add_rewrite_pass` and `add_expand_pass`.

Where a pass runs is checked when it is registered, not part way through a run:

```python
try:
    pipeline.add_rewrite_pass(
        PassAnchor.CircuitBuilt, "nowhere", ["quantum.x"], lambda match: None
    )
    raise AssertionError("expected a refusal")
except ValueError as error:
    assert "circuit_built" in str(error)
```

The circuit you hand in is not changed, and the result carries the graph as well
as the circuit, so a pipeline can be the way you reach the graph rather than a
step after it. See PASS-EXTENSION-POINTS.md for what each anchor sees and for
which of this is meant to be stable.

## What is not here yet

Conditions are equality between a measurement result and a value. Richer
conditions over classical variables are representable in the underlying
storage but are not yet exposed.

`delay` carries a duration and a time unit, one of `dt`, `ns`, `us`, `ms`, or
`s`, with `dt` meaning the target's own time step:

```python
held = QuantumCircuit(1)
held.delay(100.0, 0, "ns")
```

A delay reaches OpenQASM, and no other path accepts it yet. Compilation to QIR
refuses it, and so do the Qiskit and Cirq converters.

Whether a circuit can be compiled for a given target profile depends on
capabilities that profile supports. Backwards branching, which any unbounded
loop needs, is available on the Adaptive and Unrestricted profiles and absent
from Base, Adaptive_RI, and Adaptive_RIF. A circuit can be built without regard
to profile and is checked when `to_qir` runs.
