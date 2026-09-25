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
It requires Python 3.11 or newer and `qodec>=0.2.0.dev0,<0.3`. Install the local
qodec Python bindings first while that version is unpublished, then install
`qdk[ec]`.

On Linux aarch64 and Windows ARM64, `qdk[ec]` installs `qodec` without its
`parsers` extra because `stim` publishes no wheels for those platforms. Features
that parse `format="stim"` circuits, including `ec.build_qodec`, raise
`ValueError` there unless you build and install `stim` from source.

The default audit reports no errors or warnings for a qodec returned by
`ec.build_qodec(code)`. Both `strategy="flagged-css/v1"` and `strategy="bare-css/v1"`
include incoming-frame corrections in logical readouts and stabilizer transport
relations for supported transversal gates. `strict=False` allows unsupported
instructions to be omitted and recorded in the build metadata; it does not
allow inconsistent retained gadgets. A failed final audit raises `ValueError`.

Both strategies attempt `prepare_z_all`, `prepare_x_all`, `syndrome`,
`measure_z_all`, `measure_x_all`, `h_all`, and `cx_all`. The `_all` suffix means
all logical qubits in a block; `cx_all` pairs corresponding logical qubits of
two blocks. `syndrome` measures the stabilizers while preserving the logical
state. The names describe logical actions, not their circuit implementations.

Audit cleanliness establishes noiseless consistency, not fault tolerance.
The bare strategy intentionally uses no flag qubits and can lose code distance.
Evaluate gadget distance separately before choosing an implementation.
`ec.filled(artifact)` returns a new gadget or qodec with derived checks and
readout equations, leaving the input unchanged. It retains verified direct
readout and check equations, including the builder's boundary-frame relations.

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

`Diagnostic` has an optional keyword-only field, `source_location`. It contains
the actual source file and 1-based line when qodec retained a reliable location.
Reports print `filename:line` on the line after the severity and rule, before
the existing layer and artifact context. Printed paths under the current user's
home directory use `~/`; other paths remain absolute. The stored
`SourceLocation.path` remains absolute in either case.
Equation failures point to the equation; other findings fall back to the
containing artifact. Source metadata does not affect diagnostic equality.
Constructed or modified models may have no source locations. `qdk.ec` has
12 top-level exports.

Use `ec.CodeProfile(code)` to analyze a `qodec.Code`, just as
`ec.GadgetProfile(gadget)` analyzes a gadget. Both profiles snapshot their input
at construction; later edits to the original do not change the profile.
Code names, descriptions, and persistence remain with qodec.

`CodeProfile` has 17 named public members:

- `stabilizers`: a tuple of independent Pauli copies in declaration order.
  Use `paulimer.PauliGroup(profile.stabilizers)` when a group is needed.
- `x` and `z`: read-only tuples of physical logical-X and logical-Z
  representatives, indexed by logical qubit in the order of `qodec.Code.x`
  and `qodec.Code.z`.
- `gauge_x` and `gauge_z`: read-only tuples of physical gauge-X and gauge-Z
  generators, with matching indexes identifying gauge pairs. The derived gauge
  basis is fixed within the snapshot but is not canonical.
- `support`, `length`, and `logical_qubit_count`: physical labels, their count,
  and the number of logical X/Z pairs. `length` is not the largest physical
  label plus one.
- `syndrome_of(error)`: zero-based positions in `stabilizers` that anticommute
  with the physical error.
- `logical_effect_of(error, *, including_phase=True)`: a Pauli on zero-based
  logical-qubit indexes. By default, a nonzero syndrome or a gauge component
  raises `ValueError` rather than dropping phase. Use `including_phase=False`
  for the phase-free logical-basis component of any supported physical error.
- `is_logical(error)`: true only for zero syndrome and a nonidentity logical
  effect. Identity, stabilizers, gauge-only changes, and detectable errors are
  false, regardless of global phase. The name describes a nontrivial logical
  error, not every operator that preserves the code space.
- `representative_of(pauli)`: expand a logical Pauli to physical labels,
  preserving phase.
- `encoding_clifford(*, supported_by=None)`: compute an encoding with every
  physical support label appearing once in the requested order.
- `distance` and `distance_bounds`: exact and bounded distance searches.
- `is_equivalent_to` and `why_not_equivalent_to`: the same comparison policy
  with `including_signs=True` and `strict_basis=True` by default. Set
  `strict_basis=False` to compare logical groups instead of ordered bases.
  The explanation is empty exactly when the predicate is true.

All five operator collections return independent Pauli copies. X and Z name
the logical or gauge role, not the physical factors: `profile.x[0]` can contain
physical Y or Z factors. Logical and gauge indexes start at zero; neither is a
physical qubit label.

All physical-error entry points reject labels outside `support`. Logical
Pauli inputs reject indexes outside `range(logical_qubit_count)`. Profiles
are unhashable; `==` compares the ordered operator snapshot rather than
algebraic equivalence. Code construction and relocation remain with qodec
and the internal analysis algorithms.

`CodeProfile.distance()` and `distance_bounds()` default to single-qubit
`"XYZ"` errors, each with unit cost. Supplying `errors` restricts those Pauli
kinds or replaces them with an explicit sequence of allowed Pauli errors,
including correlated errors. Both methods return `Distance[Pauli]` rather than
tuples. `result.witness.product` is the combined Pauli; `result.witness.factors`
is the tuple of selected unit-cost errors.

Both searches accept `logical_observable=Pauli("Z_0")` to restrict failure to
flipping logical Z on logical qubit zero; logical X and Y errors qualify.
The observable must be nonidentity and Hermitian. `None` permits any logical
failure. This is an observable-flip constraint, not an exact error-class query.

The former `coset_representative` argument is replaced, not just renamed.
To translate a physical representative from that parity query, obtain its
phase-free logical effect, then exchange X and Z on each logical qubit
(leaving Y unchanged) to form `logical_observable`. For two unencoded qubits,
the old `coset_representative=Pauli("X_1")` becomes
`logical_observable=Pauli("Z_1")`.

`is_logical` replaces `is_non_trivial_logical_error`; the three other overlapping
error predicates are removed. `logical_effect_of` replaces `logical_action_of`;
`including_phase=False` replaces `unsigned_logical_action_of`. The existing
`logical_effect_of` name follows the walkthrough's error queries rather than
adding another spelling of “action.” `including_phase` follows `Pauli.phase`;
the positive option avoids the ambiguous “unsigned” method name. Public group
views and anti-stabilizer accessors are removed; internal encoding machinery
is unchanged. `length` retains the existing code-length terminology rather
than adding a synonymous `physical_qubit_count`.

The public `logical_basis` and `gauge_basis` properties are replaced, not
retained as aliases. `x` and `z` borrow qodec's names rather than introducing
`logical_x` and `logical_z`. `gauge_x` and `gauge_z` extend those axis names with
the existing gauge prefix, rather than reversing it to `x_gauge` and `z_gauge`.
The interleaved bases remain internal to the analysis algorithms.

`Distance` has read-only `lower_bound`, `upper_bound`, `is_exact`, `value`,
`witness`, and `witnesses` properties. `None` means positive infinity in both
bounds. `(3, None)` means at least three with no finite upper bound established;
`(None, None)` proves no allowed logical failure exists. `value` requires equal
bounds and raises `ValueError` otherwise. `witness` returns the retained
`Distance.Witness` without searching, or raises `LookupError` if none is available.

```python
distance = ec.CodeProfile(code).distance()
print(distance)
if distance.upper_bound is not None:
    error = distance.witness.product
    factors = distance.witness.factors
```

Strings contain only the value or interval: `2`, `[2, 4]`, `[3, ∞]`, or `∞`.
Comparisons against integers and other distances use the certified bounds.
For `[3, 5]`, `result > 2` is true and `result == 2` is false, but `result > 4`
raises `ValueError` because it is unresolved. Comparisons never search.
`bool(result)` raises `TypeError`; use an explicit comparison or `is_exact`.
String formatting supports alignment, while numeric formatting uses `value`.
Distance equality compares numerical values, not witness choices; results are
unhashable. Witness equality and hashing use the ordered factors, not the product.

Each access to `witnesses` returns a fresh lazy iterator. Its first item is the
retained `witness`; further items enumerate distinct selections of original
allowed-factor positions at the same finite upper-bound cost. Different selections
may have the same product. Enumeration uses the snapshotted binary problem,
does not rerun circuit simulation or alter the bounds, and may be combinatorially
expensive. Exhaustion means all selections at that cost were enumerated;
interruptions and errors propagate. No finite upper bound gives an empty iterator.

Neither type has a public constructor or supports implicit iteration or tuple
unpacking. Use `len(witness.factors)` for cost, never the product's Pauli or fault
weight. `Distance` is generic without a restriction on factor types; the profiles
produce Pauli and FaultEvent factors.

`GadgetProfile.distance()` and `distance_bounds()` apply the same search to
circuit faults:

```python
profile = ec.GadgetProfile(gadget)
distance = profile.distance()
bounds = profile.distance_bounds()

faults = [ec.FaultEvent.after(0, ec.Pauli({0: "X", 1: "X"}))]
distance = profile.distance(faults=faults)
if distance.upper_bound is not None:
    (combined_effect,) = profile.effects_of([distance.witness.product])

measurement_call = next(
    index for index, call in enumerate(gadget.circuit.calls)
        if any(isinstance(action, qc.actions.Observe)
            for action in gadget.circuit.instruction_set.instructions[call.mnemonic].action)
)
readout_fault = ec.FaultEvent.after(measurement_call, readout_flips=0)
effect, = profile.effects_of([readout_fault])
```

The default includes post-call Pauli errors and flips of the readout bits produced
by that call, including every combination except the identity event. A call with
`n` qubits and `r` readouts contributes `4**n * 2**r - 1` events. Calls without
readouts retain 3 faults for one qubit and 15 for two. Instruction support is
expanded from block operands. This is a set of possible faults, not a probability
distribution; every event at one call costs one, including correlated quantum
and readout errors.

A readout flip changes the reported bit without changing the surviving quantum
state. For a non-destructive Pauli measurement it is equivalent to applying an
anticommuting Pauli before and after measurement. For destructive measurement,
the trailing Pauli has no surviving quantum output to affect. Use the same
constructor for quantum, readout, and correlated errors:

```python
ec.FaultEvent.after(7, ec.Pauli("X_0"))
ec.FaultEvent.after(7, readout_flips=0)
ec.FaultEvent.after(7, ec.Pauli("X_0"), readout_flips=[0, 2])
```

The first argument always indexes `Circuit.calls` from zero. `readout_flips`
accepts one integer or a sequence of indexes into that call's own readouts;
`0` and `[0]` mean the same thing. These are not absolute `Circuit.readouts`
indexes or gadget logical-readout positions. Booleans are rejected. Call and
readout bounds are checked when the event is applied to a circuit, including
readout faults on a call that has no readouts.

Pass `faults=...` to replace the default set; `faults=[]` means no allowed faults.
An explicit event may span several call positions and still counts as one allowed
factor. The reported distance counts witness factors, not `FaultEvent.weight`,
which sums Pauli support weights and the number of flipped readout bits.
`FaultEvent` is immutable; its `repr` shows equivalent `after(...)`
expressions with call-local indexes. Witness products combine Pauli errors and
XOR readout flips at each call for replay with `effects_of`. `FaultEvent()` is
the identity; the mapping constructor remains available for post-call Pauli
errors.

Inspect an event through `event.locations`, a tuple of `FaultEvent.Location`
values stored in increasing call-index order. Each location provides
`after_call`, `error`, and `readout_flips`. `after_call` is the zero-based index
in `Circuit.calls()` after which the fault is applied. The Pauli acts on circuit
qubits, and readout flips use that call's local indexes. Each affected call
appears once, with its combined
changes; canceled changes are omitted. The identity event has no locations.
Products merge the ordered locations without sorting again.

```python
calls = gadget.circuit.calls()
for location in fault.locations:
    call = calls[location.after_call]
    print(call, location.error, location.readout_flips)
```

Locations are immutable values returned by events, not separately constructed
faults. Equality and hashing compare their fields, not circuit ownership.
`error` returns a defensive Pauli copy, and `readout_flips` is a `frozenset`;
readout-only faults have an identity Pauli. Event construction also copies
input Paulis, so later mutations do not change the event or its hash.

`FaultEffect` is an immutable set of changed gadget quantities, expressed as
qodec references:

```python
effect = ec.FaultEffect(["checks[1]", "checks[3]", "out[0].z[1]"])
assert "checks[1]" in effect
assert "readouts[0]" not in effect
assert effect ^ effect == ec.FaultEffect()
print(effect)
```

This prints `["checks[1]", "checks[3]", "out[0].z[1]"]`. Checks and readouts use
the gadget's declaration indices, including flag readouts. Output references
identify logical X/Z signs or stabilizer signs in each output encoding, after
applying frame corrections. A flipped Z sign corresponds to an X error, not an
X-sign change. These are evaluated changes, not mutations to the model fields
or to `Gadget.frames` equations.

Construction expands final selectors and deduplicates canonical targets;
iteration yields typed `qodec.Reference` values in deterministic numeric order.
qodec owns their normalized equality and hashing. Effects store those references
directly; only the distance adapter assigns solver indices.
Membership requires a single selected target. The effect is hashable; XOR takes
the symmetric difference for effects on the same analyzed snapshot. Empty means
no recorded change, not no fault. Bounds are checked by analysis or model
resolution, not by the standalone set constructor.

The read-only `effect.checks`, `effect.readouts`, and `effect.frames` properties
return `tuple[qodec.Reference, ...]` in the effect's iteration order. `frames`
includes output X/Z and stabilizer signs. For example, `bool(effect.checks)` asks
whether any check flipped. These are ordinary tuples: use Reference values for
membership, not strings. XOR and string-aware membership belong to `FaultEffect`.
The tuples are computed on access, not stored as additional state.

For bare circuits, readouts index the full record and checks are discovered.
Output encodings follow increasing circuit-layout slot order, excluding slots
prepared by the circuit but including unused slots. Each encoding represents
one qubit, so its logical index is zero. Effects describe the profile's snapshot,
not later edits to its source.

`FaultEvent` has four named members: `after`, `weight`, `locations`, and the nested
`Location` type. `Location` has three properties: `after_call`, `error`, and
`readout_flips`. `Location` names a self-contained change after one call;
`Change` alone would not identify its position in the circuit. There is no new
top-level export.
`after` keeps the existing call-location convention, and the `readout_flips`
keyword describes recorded-bit changes. A separate `flip_readout`
method would duplicate this constructor; a boolean shortcut would need an
extra rule for calls with multiple readouts.

Detection means a nonzero **declared check** syndrome. The search also requires
the combined fault to commute with every stabilizer of every output encoding.
This ensures that its residual preserves the output codespace; a lone detectable
data error must not count as a logical error merely because a logical probe flips.
Individual fault factors need not preserve the codespace: their output syndromes
may cancel in combination. Syndromes on different output blocks cannot cancel
each other. This is a constraint on what constitutes a logical error, not a new
declared detector: `out[k].stabilizers[i]` targets remain distinct from
`checks[j]` targets in a `FaultEffect`.

Among combinations satisfying both constraints, failure means changing the
**realized logical action**. Indicators come from its prepared-state stabilizers,
preserved logical mappings, and logical measurement signs. A logical Z fault on
a prepared logical zero is harmless; a logical X fault changes the prepared state.
Output errors and measurement-dependent signs are evaluated together, so their
changes can cancel. A nonidentity output Pauli alone does not establish failure.
No logical measurement is required; readout-free gadgets use their output
encodings. With no quantum outputs the codespace constraint is empty, so destructive
measurements are assessed through their logical readouts. Flag readouts are not
automatically detectors or logical failures. The effect evaluator
uses complete check/readout equations, including output signs and uniquely defined
readout dependencies; circuit-internal faults do not change incoming frame signs.
Distance computes check, output-code, and action changes in one fault-propagation
pass. Independent readout equations are evaluated directly; dependent equations
are reduced once with all fault columns. Physical action probes are cached on
the profile and shared by both distance methods.
A bare circuit uses its discovered checks and the action on its identity-encoded
boundary: the qubits it does not prepare.
No decoder or additional output recovery is assumed. Full noiseless validation remains an
explicit `ec.audit(protocol)` call, not an implicit part of every distance search.
Distance still rejects missing information that would make the result partial,
including declared but unbound logical readouts, and requires an action that can
be interpreted against the boundary codes. These guards are calculation
preconditions, not a validity certificate.

Both methods accept keyword-only `upper_bound` and `solver` like the code methods.
Both accept `solver="highs"`, `"enumeration"`, or `"mwpf"`. HiGHS is the default
for both `distance()` and `distance_bounds()` and is included in `qdk[ec]`:

```bash
pip install 'qdk[ec]'
```

```python
distance = profile.distance()
bounds = profile.distance_bounds()
```

MWPF remains available with `solver="mwpf"` after a separate `pip install mwpf`.
It is not included in any QDK extra.

The same selection works on `CodeProfile`. No solver classes or interfaces
are exported. Enumeration search enumerates subsets; HiGHS minimizes fault count
subject to the binary check and logical constraints. Enumeration and HiGHS use
`upper_bound` as an inclusive search cutoff; MWPF ignores it. The proven
distance-1/2 shortcuts can answer without invoking a backend.

Internally all backends return bounds and a witness. `distance()` returns only
when the optimum is proved; it raises `RuntimeError` if a cutoff or solver limit
leaves a gap. `distance_bounds()` can return that gap. Public results translate
internal sentinels into `None` bounds; no witness is fabricated for infinity.
HiGHS limits with no feasible witness, backend failures, invalid witnesses, or
missing bound certificates raise rather than claiming a result. Only logical
searches proved impossible are skipped. This applies to both code and gadget
distance. The solver algorithms are internal and may change without a public
interface change.
The default fault set itself grows exponentially with instruction support.
Invalid call indices or parity references, unbound logical readouts, and ambiguous
readout dependencies raise rather than silently omitting effects. Conditional or
selected circuits and circuit instruction flags are not supported.

`GadgetProfile` has 10 named members. Both distance methods return
`Distance[FaultEvent]`. Names follow `CodeProfile.distance` and
`distance_bounds`; separate `gadget_distance_*` free functions would duplicate
the profile's ownership. `faults` follows `effects_of(faults)`, rather than `errors`,
because the inputs include circuit locations. The existing `fault_effects` property
uses a compact X/Z and readout-flip propagation basis, not the full unit-cost
circuit fault set.

`ec.audit` evaluates audit rules for code algebra, complete Clifford maps
(including implicit identities), gadget actions, and check, flag, and readout
equations. It also owns protocol completeness, code-list shapes and capacities,
reference bounds, parameter uses, and circuit-call validity. These declaration findings use
`qodec/invalid-structure`; qodec itself enforces only preservation preconditions
such as unambiguous resolution and bindings that can survive a round trip.
This replaces `gadget/reference-out-of-bounds` and
`gadget/missing-source-instruction`; update any rule filters using those IDs.
Use `gadget/unsupported-action-step` for unsupported instruction steps. It
replaces `gadget/unsupported-action-atom`; "step" follows the action model and
avoids confusion with physical atoms. The registry has 17 rule IDs.
Invalid prerequisites block dependent gadget analyses, including shared
definitions, while unrelated gadgets remain analyzable. Direct analysis calls
still check their mathematical preconditions.

The returned `Report` contains errors, warnings, and informational notes as
`Diagnostic` objects. An empty report prints `audit: ok`; otherwise, the report
prints errors and warnings followed by counts for all three severities.
`report.ok` means there are no errors; warnings do not make it false. Reports
do not record successful rule evaluations, so an empty report is not a record
that every audit rule ran and passed.

Repeated circuit labels within an encoding, or shared by two encodings on the
same boundary, are `qodec/invalid-structure` errors. The message identifies both
support positions. Input and output boundaries are checked separately; reusing
a label across them is normal. An overlap blocks that gadget's dependent analyses
without suppressing unrelated gadgets. qodec loading and saving are unchanged.

Three informational rules describe declaration redundancy:

- `gadget/vacuous-check`: an explicit check is empty or all its reference terms
    cancel. It is valid, but cannot detect a fault. An omitted checks list has no
    entry to report.
- `gadget/redundant-check`: a nonvacuous check is the exact XOR of earlier
    independent checks. The finding lists those check positions. It compares formal
    reference equations, not noiseless values, and does not substitute readouts.
    Distinct measurements that agree noiselessly can still detect faults differently.
- `code/redundant-stabilizer`: a valid code has a generator that is the product of
    earlier independent generators, or is +I. The finding shows the dependency and
    the rank of the full stabilizer list. Inconsistent signs remain algebra errors.

These informational notes preserve authored declarations and positions, remain
INFO under `promote_warnings=True`, and can be disabled by rule ID. They add no
top-level Python exports. The names use the existing check and stabilizer terms:
`duplicate-*` would miss combinations of earlier declarations, while `invalid-*`
would wrongly label harmless redundancy. Audit rules that depend on a fault
model remain deferred until an explicit fault set and detection requirement
are supplied.

Algebraically incorrect drafts can be constructed, loaded, and saved by
qodec, as can unequal code lists, incomplete protocols, and malformed circuit
text. Parsed accessors can fail on a loadable draft. Entirely omitted readout
lists are allowed for later derivation, but audit reports missing observable
and flag equations as errors. Each error identifies the missing readout, whether
the list is empty or partially supplied. Missing observable errors include a
verified readout equation when the analysis can derive one. Extra readouts are
structural errors. An omitted flag equation is undefined; an explicit `[]`
equation declares zero. The audit does not invent flag equations: noiseless
consistency alone does not determine which faults a flag should detect.

Checks, logical readout equations, and frame transformations are verified on
valid noiseless input codewords with arbitrary incoming Pauli frames. A
measurement readout that omits a required logical-frame correction is an error
even if it works for a zero-frame input. The authored C4 example currently has
such omissions; the audit reports them without changing the protocol.

Flags instead must be zero for every valid logical input state with a trivial
incoming Pauli frame. A raw stabilizer measurement can therefore serve as a
rejection flag: it is zero on a noiseless codeword but may fire for an incoming
error. Flag equations must still be well-defined and consistent under arbitrary
incoming frames; only the zero-value requirement is restricted to a trivial
frame.

Readout messages distinguish an incorrect equation from a result the circuit
does not provide. A recoverable mismatch shows only the declared and verified
equations. Otherwise, a short explanation identifies missing information,
a required constant inversion, or contradictory/undetermined definitions.
Readout messages do not include counterexamples. Check and flag failures retain
term values when needed to demonstrate firing without a fault, and label the
required noiseless value as zero. Flag counterexamples use a trivial incoming
frame; check counterexamples may use an arbitrary incoming frame.

Readout references are solved as binary linear equations, including cycles
with unique consistent solutions. A group of contradictory equations is reported
once, with verified observable equations where available. Readouts that depend
on the conflict are reported as unverified; independent readouts remain
analyzable. Undetermined observables also include verified equations when
available. Output stabilizer signs must be determined
by valid declared constraints, not merely mentioned in them. Empty equations
are zero but provide no constraint. Unsupported parity analysis produces
warnings rather than claiming a result. These audit rules do not establish fault
tolerance or verify a particular fault model.

`gadget/missing-check` is informational. It reports an independent noiseless
measurement check that is available but not implied by valid declared checks,
readout definitions, and flags verified to be zero under arbitrary incoming
frames. Candidates combine circuit bits and incoming stabilizer signs, under
the same arbitrary-frame contract.
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

The registry has 17 built-in rule IDs. This adds no public Python exports:
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
