---
title: "Intermediate Representations for Hybrid Quantum-Classical Programs: A Comparative Assessment"
description: "A source-grounded comparison of dependence graphs, SSA and regional IRs, quantum dialects, circuit representations, and interchange formats."
---

## Abstract

Hybrid quantum-classical programs combine classical computation, quantum
operations, measurement-dependent control, and resource-lifetime obligations.
Their representations serve different purposes: exposing optimization
opportunities, enforcing resource constraints, exchanging programs between
tools, or executing workflows on heterogeneous systems. Comparing these
representations as if they were interchangeable circuit formats obscures
important differences.

This assessment compares the IR families examined in the accompanying design
work: CFG/SSA and QIR; Sea of Nodes and regional dependence graphs; MLIR and
quantum dialects; HUGR; circuit representations in Qiskit, TKET, and Cirq;
OpenQASM and Quil; specialized ZX representations; and the local QRVSDG and
QHDG proposals. Workflow and execution representations are considered at their
own abstraction levels. The comparison uses common criteria for control,
quantum-resource use, effects, hierarchy, transformation assurance, and
interoperability.

The evidence supports conditional suitability statements, not an overall
winner. Several representations can encode the same computation while
exposing different facts to an optimizer. A representation's extensibility
does not establish implemented support, and a verifier does not establish
arbitrary semantic equivalence. The assessment distinguishes published
specifications, inspected implementations, research prototypes, and proposed
contracts. It reports no new performance experiment or compiler-correctness
proof.

## 1 Scope and evidence method

### 1.1 Comparison units

An IR is evaluated together with the contracts that give it meaning. A generic
SSA framework, a quantum dialect implemented in that framework, a textual
interchange language, and a workflow graph are not equivalent comparison units.
For example, MLIR supplies extensible operations and regions; Quake and
Catalyst assign quantum meanings to particular operations. QIR specifies an
LLVM-based interface and backend profiles. Tierkreis orchestrates asynchronous
jobs rather than prescribing the instruction semantics of a quantum device
[1,2,5,8,9,18].

The comparison therefore separates:

- Classical representation foundations and extensible compiler frameworks
- Quantum-aware optimization representations and their resource contracts
- Circuit-oriented representations with embedded classical control
- Interchange languages, execution targets, and orchestration layers
- Local design proposals and the narrower implementation observed in this branch

PDG and VSDG are included as dependence-graph antecedents, not as independently
evaluated quantum implementations. XACC, ScaffCC, and ProjectQ provide
framework context rather than additional entries with assumed uniform IR
semantics. QTX, QAT, and Torque are not scored under those names: this assessment
did not establish an unambiguous, separately inspected IR specification for
each. That is an evidence boundary, not a claim that the projects or names do
not exist.

### 1.2 Source and version discipline

Sources were checked on 2026-11-06. Earlier repository research was used to
identify subjects and sources, but its rankings and capability claims were
re-evaluated. Technical claims below cite primary papers, official
specifications, or implementation source. A source must support the nearby
claim; a reachable homepage alone is not sufficient.

Versioned papers and pinned repository sources are identified in the
references. Unversioned official documentation describes the retrieved
specification, not every historical release. The QDK implementation snapshot
is commit `1b68d48b365b05dd92a04159ed5ecea2dd377fb2`; the companion QHDG
working document was consulted on the same retrieval date [21,22].

Evidence descriptions have distinct meanings:

- A specification defines a contract, not necessarily every implementation's behavior.
- Inspected source establishes a concrete representation or code path, not its universal correctness.
- A research prototype supports the claims evaluated in its paper, not unmeasured production workloads.
- A proposal states intended contracts; those contracts must not be reported as completed checks or proofs.
- Not established means the inspected evidence is insufficient, not that the capability is impossible.

Appendix A records what the citations were used to verify and the important
access and version limits. No toolchain was rebuilt or benchmarked for this
assessment.

## 2 Common comparison criteria

### 2.1 Hybrid execution is more than a circuit with parameters

Three different interactions must be distinguished:

1. Generation-time classical computation constructs or specializes a circuit.
2. In-execution classical feedback consumes measurements while quantum state
   remains live.
3. Near-time orchestration processes completed jobs and prepares later jobs.

A system may support one without supporting all three in the same execution
environment. OpenQASM 3 explicitly distinguishes real-time from near-time
interaction; QIR profiles qualify backend feedback capabilities; Tierkreis
addresses asynchronous workflows [2,15,18]. A representation of a conditional
does not establish a hardware feedback-latency guarantee.

### 2.2 Evaluation questions

| Criterion                 | Questions applied to every relevant representation                                                                                                             |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Control and feedback      | Are branches, repetition, calls, and measurement-dependent decisions represented? Which execute during generation, during a quantum run, or between jobs?      |
| Quantum use and lifetime  | Are operands resource references, state-like values, or capabilities? What prevents duplication, stale access, conflicting aliases, or invalid release?        |
| Effects and reachability  | What preserves measurement effects, classical memory, output, failure, and progress? How are guards and execution multiplicity represented?                    |
| Structure and composition | Are nesting, aggregate values, callable boundaries, and symbolic computation explicit? Which obligations cross those boundaries?                               |
| Transformation assurance  | What does structural verification check? Which rewrites need semantic laws, analyses, assumptions, or external evidence?                                       |
| Interoperability          | What can enter and leave the representation? Are restrictions imposed by the IR, a profile, a conversion pass, or a particular target?                         |
| Practical evidence        | Which capabilities have inspected implementations or published evaluation? Are compilation cost and optimization benefit measured under comparable conditions? |

These are not equally weighted scores. The necessary evidence depends on the
use case. A circuit-routing tool need not encode an entire host application;
an interchange format need not prescribe the optimizer's internal graph.

### 2.3 Representation invariants are not quantum equivalence

SSA gives each value one definition, but ordinarily permits multiple uses.
It does not alone enforce quantum single use. HUGR specifies non-copyable port
constraints, while QSSA describes a separate no-cloning analysis [7,10].
Likewise, an acyclic graph does not prove termination of a rewrite engine that
can repeatedly replace that graph.

Structural normalization also has a limited meaning. The RVSDG paper discusses
normalization of independent-operation order and structural control, not a
canonical representative of every semantically equivalent program [3,
Section 2.3]. Neither equal graph shape nor equal marginal measurement
statistics is a general quantum-equivalence test.

A complete transformation argument must state its observations and context.
Classical outputs and remaining quantum state may be correlated. An unused
measurement can still alter later behavior. Equality up to global phase for
an isolated unitary does not automatically permit substitution under coherent
control. These obligations apply regardless of the graph or instruction
format; the companion QHDG account makes them explicit as open or
operation-specific contracts [21, Sections 5 and 10].

## 3 Classical foundations and compiler frameworks

### 3.1 CFG/SSA and LLVM

LLVM organizes typed SSA computation within functions and basic blocks.
Its verifier checks well-formedness, including dominance, while its operation
semantics and memory rules govern transformations [1]. Control-flow structure
and data dependencies coexist; basic-block instruction order is part of the
representation, but analyses and transformations can establish legal motion.

For hybrid programs, this provides a direct home for classical arithmetic,
branches, calls, memory, and runtime interfaces. QIR uses that foundation
rather than defining an unrelated classical instruction language [2].
Quantum-specific invariants must be supplied by the quantum interface and its
analyses. A pointer to a qubit is a classical handle, and copying that handle
does not copy quantum state or establish permission for arbitrary simultaneous
uses.

The relevant trade-off is between explicit control and recoverable quantum
dependencies. LLVM offers an established low-level compilation boundary;
quantum optimizers must understand which calls touch which resources and what
their effects mean. Opaque quantum calls do not make quantum-aware optimization
impossible: the QIR specification explicitly includes a generic transformation
and analysis phase [2]. Incorrect effect attributes would be a contract error,
not evidence that the representation cannot support such passes.

### 3.2 Sea of Nodes

Sea of Nodes represents computation through dependencies rather than assigning
every operation an early basic-block position. Turbofan's implementation
distinguishes value, control, and effect edges, and later schedules operations.
The V8 account also notes that multiple effect chains are possible, although
Turbofan used a single chain with branch splits and merges [4].

This organization exposes opportunities to share or move computations without
first preserving an incidental instruction order. For a quantum adaptation,
resource use, measurement effects, and execution guards would still need
explicit contracts. The classical representation does not itself supply
quantum linearity, borrowing, or cleanup semantics.

There is concrete engineering evidence for the maintenance cost of explicit
relations. V8's effect/control-chain discussion links a merged correction that
changes a transition node from outer `effect, control` inputs to the current
`this_effect, this_control` inputs. The same account discusses scheduling
complexity and acknowledges the role of implementation helpers [4]. These
observations concern that implementation and its workloads; they do not prove
that every Sea of Nodes implementation, or every dependence graph, is
unsuitable for hybrid quantum programs.

### 3.3 PDG, VSDG, and RVSDG

The RVSDG paper places its design in a lineage that includes program dependence
graphs combining data and control dependencies, and value/state dependence
graphs that make state ordering explicit [3, Section 3]. Those foundations
identify recurring questions about aliases, effects, and execution conditions.
They do not determine a quantum resource discipline.

RVSDG adds hierarchical regions and structural nodes for alternatives,
iteration, functions, recursive environments, and program-level composition.
Ordinary regional dependence graphs remain acyclic while structural interfaces
express recurrence. State edges constrain effects, including independent
state domains; one mandatory global state token is not an RVSDG requirement
[3, Sections 2.4 and 4].

For hybrid compilation, this makes structured feedback and symbolic
subprograms natural subjects for analysis. Quantum single-use rules,
measurement semantics, and resource cleanup are additions, not consequences
of the classical graph definition. Construction from control-flow form and
destruction back to an executable target also require algorithms and
preservation arguments. The paper supplies classical construction,
destruction, optimization, and prototype evaluation; those results do not
automatically transfer to a quantum extension [3].

### 3.4 MLIR

MLIR is an extensible framework rather than one fixed quantum IR. Operations
consume and produce typed values and may contain nested regions. Its language
reference distinguishes SSA-CFG regions from graph regions; in graph regions,
operation order need not be semantically significant and cycles can be
represented [5]. Describing MLIR as inherently flat or restricted to
structured-control-flow trees is inaccurate.

For hybrid programs, different dialects can preserve quantum operations,
classical arithmetic, aggregates, control, and lower-level interfaces in one
module. Progressive lowering can change only the portions whose contracts
are ready for a lower abstraction. Traits, interfaces, verifiers, and passes
provide mechanisms for enforcing dialect-specific requirements [5].

The qualification is that extensibility is not a delivered quantum semantics.
Quantum value usage, alias handling, measurement effects, and legal
cross-dialect rewrites must be specified and implemented by the relevant
dialects and passes. A pass designed for one region kind or quantum operand
form cannot be presumed valid for every MLIR program.

## 4 Quantum-aware regional and value representations

### 4.1 QIRO and QSSA

QIRO's paper, _Enabling Dataflow Optimization for Quantum Programs_, separates
an input dialect using memory semantics from an optimization dialect using
value semantics. The latter exposes quantum and classical dependencies by
having operations consume and produce state-like values. The work includes
an MLIR prototype and evaluated optimizations [6].

QSSA is a distinct research representation. Its inspected preprint describes
a side-effect-free operation model, corresponding quantum inputs and outputs,
and a static analysis for no-cloning. It evaluates optimizations on specified
benchmark collections [7]. The analysis, not the word SSA alone, supplies the
additional usage check.

Both approaches are relevant when explicit quantum def-use chains simplify
analysis and classical/quantum co-optimization. Their published results support
the studied models and workloads. They do not, by themselves, establish a
general policy for every source language's aliases, dynamic collections,
borrowed ancillas, or abnormal-exit cleanup. Nor do their reported results
rank them against later implementations under a shared experiment. The
appropriate comparison is the exposed dependencies and supported conversion
boundary, not an assumed universal performance advantage.

### 4.2 CUDA-Q and Quake

Quake documents both reference-oriented and value-oriented quantum forms.
Reference operands identify resources modified by operations; value-oriented
wires make the succession of quantum operations explicit. The documentation
allows the forms to coexist, including supported mixed-form operations [8].

Its motivating alias example is significant: two gates on a reference cannot
be cancelled merely because no intervening operation names that same scalar
reference. An intervening measurement through a containing register can affect
the same resource. Explicit wire threading can expose that dependency, while
reference form requires sufficient resource analysis [8].

CUDA-Q's compiler IR documentation places Quake alongside classical CC and
upstream MLIR operations, not in an isolated circuit-only language. It also
describes a conversion to linear values that leaves dynamic registers,
runtime-indexed elements, and unsupported cases in reference form [8].
Consequently, a useful suitability claim must name the accepted form and
conversion subset. Neither the presence of wire values nor the availability
of a conversion pass establishes unrestricted alias elimination or a complete
hybrid-program correctness theorem.

### 4.3 Catalyst

Catalyst's architecture combines program capture through PennyLane/JAX,
an MLIR compiler core, lowering to LLVM with QIR-style operations, and a
runtime interface to quantum devices. Automatic differentiation and hybrid
workflow compilation are explicit parts of its design [9].

The inspected quantum operation definitions expose successor quantum values:
the generic unitary gate has quantum inputs and outputs, and projective
measurement returns both a classical result and a successor qubit [9].
This is concrete representation evidence, rather than an inference from the
framework's use of MLIR.

Catalyst is relevant where hybrid numerical computation, differentiation, and
quantum execution must remain connected through compilation. Capture
restrictions, dialect conversions, runtime behavior, and device capabilities
are separate parts of that suitability. In particular, compiling a host-side
feedback program does not alone establish coherence-time execution on every
device. The architecture document itself warns that its runtime discussion may
lag implementation, so architectural goals are not treated here as a complete
current backend-support matrix [9].

### 4.4 HUGR and the HUGR-based TKET implementation

HUGR specifies a typed port graph with a hierarchy tree, dataflow, static
references, order dependencies, and control-flow edges. Its core includes
nested dataflow, conditional and iterative constructs, functions, and CFG
structures. The type system distinguishes copyable data from values whose
ports require single use; compound copyability depends on component types
[10].

This supplies explicit structure for mixed quantum-classical programs and
quantum data that must not be freely duplicated or discarded. The specification
does not fix the meanings of arbitrary extension operations, and translations
to other representations are separate responsibilities [10]. Thus core graph
validation and extension-level semantic correctness are different claims.

The repository named `tket2` contains a HUGR-based compiler layer: its inspected
Rust documentation identifies a circuit interface, quantum extensions,
composable passes, rewriting utilities, and interoperability with legacy
`pytket` circuit serialization [13]. This is distinct from the legacy circuit
implementation discussed in Section 5.2.

HUGR is relevant when a shared hierarchical representation and extensible
quantum types are needed across tools. The remaining assessment must examine
the selected extensions, conversions, and passes. A linear port constraint is
not, by itself, a proof of arbitrary gate identities, safe dirty-resource
restoration, or complete target coverage. Conversely, the absence of QHDG's
particular authority vocabulary does not demonstrate that such policies cannot
be represented using HUGR.

### 4.5 QRVSDG and QHDG

The local QRVSDG manuscript proposes a quantum adaptation of RVSDG with linear
quantum edges, measurement-result flow, quantum operations, regional
annotations, and quantum-aware analyses and transformations [20]. Its
hierarchical formulation is relevant to structured feedback and reusable
quantum subprograms. Its proposed algorithms and integration directions must
be distinguished from independently validated implementations or formal
results.

QHDG's companion specification develops that lineage by separating copyable
values, quantum authority, hard-effect order, and explicit control/exit
reachability. It adds contracts for views, ownership epochs, borrowing,
cleanup, staging, and checked mutation [21]. These expose obligations that
need not be visible in a scalar wire chain.

Explicit obligations also create implementation and consistency work. The
QHDG specification acknowledges incomplete operation transfer rules,
compositional semantics, proof coverage, and empirical evaluation. Its
recognized backedges and residual control constructs need their own contracts;
classical RVSDG invariants cannot be imported indiscriminately [21].

The branch contains concrete QHDG graph, checking, analysis, and rewrite
machinery, but its compiler integrations are narrower than the complete
design. The inspected FIR producer handles a callable-body specialization and
has explicit refusal cases. Its return leg removes supported statements and
rejects introduced graph nodes. The RIR return leg uses graph-derived order
while retaining original instruction sites and block membership, and likewise
rejects introduced nodes [22].

QHDG is therefore an implemented optimization substrate accompanied by a
broader proposed contract, not an established replacement compiler for every
construct named in the design. Its suitability depends on the actual
producer, enabled transformation, and return path. No comparative superiority
over HUGR, MLIR dialects, or circuit IRs follows from the number of explicit
metadata categories.

## 5 Circuit-oriented representations

### 5.1 Qiskit DAGCircuit

Qiskit's `DAGCircuit` represents inputs, outputs, and operations connected by
qubit and classical-bit dependencies. Its API also exposes classical variables
and other circuit metadata. Separately, `ControlFlowOp` encapsulates inner
circuits through its `blocks` interface [11].

This provides a circuit-centered substrate for analysis and transformation
without requiring every operation to be reduced to a flat static gate list.
It is inaccurate to conclude from the word DAG that Qiskit cannot represent
nested control or that it must unroll every loop.

The relevant qualification is the scope of each operation, pass, conversion,
and backend. Nested block circuits are not automatically interchangeable with
an arbitrary whole-program IR containing host memory, recursion, or external
services. Passes must preserve control semantics and any data crossing block
boundaries. The inspected APIs establish the representation mechanisms, not
universal support by every transpilation route.

### 5.2 Legacy TKET circuits

The TKET paper describes a language-agnostic circuit compiler with optimization
and routing for NISQ devices. Current inspected circuit source has ordered
subcircuit boundaries, classical conditional operations, and circuit boxes
[12]. A `CircBox` contains a circuit as an operation; a `Conditional` decorates
an operation with a condition on classical bits.

This is a useful representation for circuit transformations, decomposition,
and hardware mapping. It should not be characterized as having no hierarchy
or no classical control. At the same time, a boxed circuit and a bit-conditioned
operation do not by themselves establish arbitrary whole-program recursion,
host-side memory semantics, or a general resource-borrowing discipline.

The published TKET benchmarks concern their stated compiler versions and
workloads. They are evidence that an implementation was evaluated, not a
current ranking against all representations in this assessment. The
HUGR-based implementation in the `tket2` repository has a different
representation contract and is considered separately [12,13].

### 5.3 Cirq

Cirq represents a circuit as an ordered sequence of moments, each grouping
operations on disjoint quantum operands in an abstract time slice. Its
documentation explicitly distinguishes that abstraction from a required
physical schedule [14].

Cirq also supports classically controlled operations and `CircuitOperation`,
which encapsulates a circuit with mappings and repetition, including a
`repeat_until` condition. Describing it as necessarily flat or incapable of
feedback would omit documented mechanisms [14].

The fit is circuit construction, transformation, simulation, and supported
execution with explicit quantum operands. The host Python program, the
contained circuit, and the target's executable subset remain different
objects. A compact nested circuit is not evidence that arbitrary Python
control or state has been captured for execution on the quantum controller.

## 6 Interchange, execution, and specialized representations

### 6.1 QIR profiles as an execution boundary

QIR is an LLVM-based representation with quantum and runtime interfaces.
Its specification separates language-specific compilation, generic
transformation, and target-specific compilation. It does not require every
backend to implement all expressible programs or one universal gate set [2].

The inspected Adaptive Profile requires measurement-based forward branching
and nested adaptive control. Loops, broader classical computation, IR-defined
functions, dynamic allocation, and arrays are optional capabilities.
The associated memory specification treats arrays and runtime allocation as
independent capabilities [2].

QIR is consequently suitable as a contract between producers and target
toolchains when the profile and instruction set are agreed. It is not accurate
to equate the restrictions of one profile with the limits of LLVM or QIR as a
whole. Nor does an IR accepted by an LLVM parser necessarily satisfy a quantum
backend's profile, resource, or instruction semantics.

### 6.2 OpenQASM 3 and Quil

OpenQASM 3 explicitly targets real-time quantum-classical interaction, with
classical control, external functions, timing, gate modifiers, and calibration
integration. Its language specification includes measurement-dependent loops,
early termination, and timing constructs such as durations and stretches
[15].

These features make it relevant for interchange and for expressing execution
and timing intent. Imperative syntax does not prevent an implementation from
building a dependence graph or an SSA IR. Conversely, source-level timing and
control syntax do not establish that every device or exporter implements those
constructs. Source round-tripping must preserve the supported semantics rather
than merely emit syntactically valid text.

Quil's cited abstract-machine account combines a quantum state, classical
memory, measurement, and program control. Its `DEFCIRCUIT` facility is described
as macro-like substitution, not general callable semantics [16]. This supports
a different instruction-oriented interface for hybrid execution. The cited
version is not used to make claims about every later Quil extension or current
backend. Neither textual language alone supplies a universal optimizer
correctness argument.

### 6.3 ZX diagrams and PyZX

ZX diagrams represent linear maps between qubit systems and support
diagrammatic equational reasoning. PyZX supplies automated rewriting,
circuit-oriented optimization, and equality-validation tooling [17].

This makes ZX a relevant representation for suitable quantum optimization
regions, including reasoning that is not naturally expressed as local
instruction cancellation. Its role need not be that of the entire compiler
IR: a host representation can retain classical control and resource boundaries
while extracting an eligible quantum fragment.

The fragment's input/output order, scalar or phase convention, and extraction
back to an executable circuit remain part of the contract. Diagrammatic
expressiveness must not be confused with a complete account of arbitrary
host effects, divergence, dynamic resource lifetimes, or hardware scheduling.
This assessment does not claim that extensions of graphical calculi cannot
address those subjects; it limits the comparison to the inspected
circuit-oriented evidence.

### 6.4 Tierkreis and WebAssembly

Tierkreis's inspected documentation describes computational graphs and workers
for asynchronous hybrid jobs on different execution platforms [18].
This addresses orchestration: data dependencies determine when a task can
consume another task's output. It does not establish that a workflow edge can
carry a live quantum state or that a remote task invocation satisfies a
coherence-time feedback deadline.

WebAssembly supplies a low-level execution format with structured control,
functions, modules, and imported interfaces [19]. A quantum integration can
assign meaning to imported functions or external resources, but that interface
is additional to the inspected core execution model.

Both can complement a quantum compiler. They are not substitutes for a
quantum optimization IR solely because they contain graphs, loops, or calls.
In particular, WebAssembly's _linear memory_ is a byte-addressed memory model,
not a linear quantum-resource type discipline.

### 6.5 Other compiler frameworks

XACC documents an extensible hybrid compilation framework whose language
frontends and hardware backends are connected by a quantum IR. ScaffCC is a
Scaffold compiler and scheduler built using LLVM. ProjectQ describes a
compiler framework with extensible backends and compilation plugins
[23,24,25].

These systems are relevant implementation precedents. Their frameworks must
be assessed through a selected IR, frontend, passes, and backend rather than
assigned one universal score for all hybrid programs. Their existence also
shows why the choice is not limited to replacing every stage with a single
new representation.

## 7 Comparative synthesis

### 7.1 Representation and assurance matrix

The matrix summarizes the cited contracts. It is not a feature-count ranking.
An entry's qualification is part of its capability statement.

| Representation                       | What it exposes directly                                                  | Hybrid-program fit                                                   | Principal qualification                                                                          |
| ------------------------------------ | ------------------------------------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| LLVM CFG/SSA and QIR [1,2]           | Classical control/data flow and quantum runtime interfaces                | Compilation and interchange with declared backend capabilities       | Quantum effects, resource meaning, and profile admission need specific contracts                 |
| Sea of Nodes [4]                     | Value, control, and effect dependencies before final scheduling           | Dependency-oriented optimization with an added quantum discipline    | Relation maintenance and scheduling obligations remain; V8 evidence is implementation-specific   |
| RVSDG [3]                            | Hierarchical dependence and structural control                            | Symbolic subprograms and structured feedback after quantum extension | Classical SSA/state invariants do not supply quantum use or cleanup rules                        |
| MLIR [5]                             | Extensible typed operations, nested SSA-CFG and graph regions             | Combining quantum/classical abstractions and progressive lowering    | Dialects and passes must establish the actual quantum semantics                                  |
| QIRO and QSSA [6,7]                  | Quantum def-use structure with research-specific contracts                | Dataflow-oriented quantum/classical optimization                     | The supported model and analyses, not SSA alone, justify resource use and rewrites               |
| Quake [8]                            | Reference and value quantum forms alongside classical dialects            | Quantum kernels with form-specific optimization                      | Unconverted dynamic cases remain references; pass admission matters                              |
| Catalyst [9]                         | Captured hybrid computation, quantum values, differentiation and lowering | Compiled numerical/quantum workflows                                 | Capture, runtime, and device support are distinct constraints                                    |
| HUGR and HUGR-based TKET [10,13]     | Typed ports, hierarchy, structured/CFG control, quantum extensions        | Shared hybrid representation and circuit compilation                 | Extension semantics and target conversions are separate assurance obligations                    |
| Qiskit, legacy TKET, Cirq [11,12,14] | Circuit resources plus documented nested/control mechanisms               | Circuit optimization, mapping, simulation and supported feedback     | Do not assume either universal flatness or arbitrary host-program capture                        |
| OpenQASM 3 and Quil [15,16]          | Quantum/classical instruction and control syntax                          | Portable program exchange and execution interfaces                   | Language capability differs from accepted backend/exporter subsets                               |
| ZX/PyZX [17]                         | Quantum linear-map structure and diagrammatic identities                  | Optimization and validation of suitable quantum fragments            | Whole-program effects and extraction contracts remain outside the selected fragment              |
| Tierkreis and WebAssembly [18,19]    | Workflow dependencies or a general execution/import model                 | Orchestration and runtime integration                                | Neither inspected core contract supplies a quantum ownership or feedback-timing guarantee        |
| QRVSDG proposal [20]                 | Regional quantum-flow and analysis contracts                              | Proposed structured hybrid compilation                               | Proposal and algorithm descriptions are not independently established implementation coverage    |
| QHDG design and branch [21,22]       | Separate authority/effect/control relations and checked graph machinery   | Producer-specific optimization with explicit obligations             | Full semantics, proof coverage, general return paths, and comparative costs remain unestablished |

### 7.2 Measurement feedback and repeat-until-success

Consider a program that measures a qubit, branches on the result, applies
conditional operations, and repeats until a classical condition is satisfied.
Representing this program requires more than a measurement-result field:
the continuation, carried quantum resources, and repeated execution must have
defined semantics.

LLVM control flow, suitable MLIR dialect compositions, HUGR control constructs,
and quantum extensions of regional dependence graphs provide different ways
to state those dependencies. Circuit APIs can also carry control and repetition,
as the Qiskit and Cirq documentation demonstrates. At a QIR boundary, backward
branching is an optional Adaptive capability, so acceptance must be checked
for the chosen backend [2,3,5,10,11,14,20,21].

The decision therefore depends on which representation preserves the required
classical and quantum state through the actual toolchain. It cannot be made
from the labels "DAG," "SSA," or "hierarchical" alone.

### 7.3 Symbolic circuit families and numerical workflows

A parameterized body can be retained once and specialized or executed many
times. Regional representations, callable or nested circuit objects, and MLIR
operations provide ways to retain structure. QIRO, Quake, and Catalyst expose
different forms of classical/quantum dependency suitable for optimization
before all details become concrete [3,5,6,8,9,10,14].

For an outer numerical optimizer, it matters whether the classical computation
is captured in the compiled program, executed by the host, or scheduled as a
workflow task. Catalyst and Tierkreis address different parts of that problem.
No benefit should be attributed to an IR simply because its surrounding
framework performs useful work at another layer [9,18].

### 7.4 Dynamic collections, aliases, and borrowed resources

Suppose two expressions may select the same qubit, or an operation acts on an
entire register between two operations on one element. Reference-based
representations require analysis or contracts connecting those accesses.
Explicit value or capability flow can make dependencies easier to inspect,
but a conversion must first establish the identities and legal access pattern.
Quake's motivating example and documented residual reference forms make that
boundary explicit [8].

HUGR's non-copyable types and QSSA's usage analysis provide particular checking
mechanisms; QHDG proposes a richer authority/borrow/cleanup account [7,10,21].
These should be compared by the source operations and invariants they actually
cover, not by assuming that one mechanism automatically solves every alias or
lifetime problem. Optional QIR array and allocation support is likewise an
interface capability, not a universal proof of source-level ownership [2].

### 7.5 Device mapping, timing, noise, and fault tolerance

Circuit optimization and routing, timing intent, physical scheduling, and
fault-tolerant resource allocation are different tasks. TKET's compiler work
and OpenQASM's timing specification address concrete parts of this space,
while Cirq moments explicitly need not be a device schedule [12,14,15].

An unordered pair of logical operations is not necessarily executable
simultaneously on hardware with shared controls or connectivity constraints.
An ideal identity is not automatically equivalent under a fixed noisy
implementation. Conversely, the absence of pulse or QEC fields from a logical
IR is not a defect if those concerns are deliberately handled by a separately
specified lowering. The comparison must name the layer and target assumptions
before treating physical detail as a requirement.

## 8 Assurance, empirical limits, and future evaluation

### 8.1 What evidence would support a stronger comparison

A useful future experiment would declare a common supported subset and
observation contract before collecting results. Candidate workloads should
include static circuits, measurement branches, bounded and unbounded feedback,
symbolic callable bodies, resource collections, and intentional refusal cases.
Not every tool must accept every case; explicit rejection is a result, not a
reason to silently replace the workload.

For accepted programs, the experiment should distinguish:

- Construction and no-optimization round-trip preservation
- Structural verification and semantic rewrite validation
- Classical outputs jointly with retained quantum state and observable effects
- Source-program support from profile and device restrictions
- Construction, analysis, optimization, checking, and lowering costs
- Compile time and peak memory from gate count, routing cost, and device duration
- Analysis precision and refusal rate from successful rewrite count

Versioned inputs, target assumptions, pass configurations, and independent
semantic checks are prerequisites for attributing differences to a
representation. Comparing an unchecked local rewrite with a transaction that
clones and verifies an entire graph would measure different assurance
policies as well as storage and algorithm choices.

### 8.2 Limits of this assessment

No controlled cross-tool experiment was run. Existing prototype evaluations
are reported only as evidence of their own studies; their speedups and
benchmark rankings are not transferred across projects or releases.
Source inspection establishes narrower facts than execution testing.

The local QHDG and QRVSDG documents are treated as design artifacts, not
independent corroboration of each other. Likewise, an official project's
architectural goals are not automatically evidence that all its targets
implement them. The assessment does not certify complete operational
semantics, proof coverage, source compatibility, or production readiness
for any entry.

## 9 Conclusion

The studied representations make different facts explicit at different
compilation and execution boundaries. CFG/SSA and QIR provide classical
control and interface contracts; dependence graphs expose ordering freedom;
regional and extensible frameworks preserve compositional structure;
quantum value disciplines expose resource succession; circuit representations
support domain-focused transformations; and interchange, workflow, and
execution formats connect distinct parts of a hybrid application.

Suitability depends on the program subset, resource policy, observations,
transformation requirements, and target path. No inspected source establishes
one representation as uniformly preferable across those dimensions. A
multi-representation pipeline can be appropriate, but its conversions become
part of the correctness and cost argument. The defensible choice is the one
whose required contracts and boundaries are supported by evidence for the
intended workload, not the one with the broadest unverified feature list.

## References

1. LLVM Project.
   [LLVM Language Reference Manual](https://llvm.org/docs/LangRef.html).
   Introduction and Well-Formedness; living specification retrieved 2026-11-06.
2. QIR Alliance. Quantum Intermediate Representation specification, revision
   `b1dd5bef0c4e0a6b987c86e67291ca9848b2ae7c`.
   [Model and profiles](https://github.com/qir-alliance/qir-spec/blob/b1dd5bef0c4e0a6b987c86e67291ca9848b2ae7c/specification/README.md),
   [Adaptive Profile](https://github.com/qir-alliance/qir-spec/blob/b1dd5bef0c4e0a6b987c86e67291ca9848b2ae7c/specification/profiles/Adaptive_Profile.md),
   and [Arrays and Dynamic Allocation](https://github.com/qir-alliance/qir-spec/blob/b1dd5bef0c4e0a6b987c86e67291ca9848b2ae7c/specification/Memory_Management.md).
   Model, mandatory/optional capabilities, and memory-management scope inspected.
3. Nico Reissmann, Jan Christian Meyer, Helge Bahmann, and Magnus Sjalander.
   [RVSDG: An Intermediate Representation for Optimizing Compilers](https://arxiv.org/abs/1912.05036v2).
   Version 2, 2020; especially Sections 2.3-2.4 and 3-6.
4. Darius Mercadier.
   [Land ahoy: leaving the Sea of Nodes](https://v8.dev/blog/leaving-the-sea-of-nodes#managing-the-effect-and-control-chains-manually-is-hard).
   V8, 2025-03-25. Dependency kinds, effect/control maintenance, and scheduling;
   the linked maintenance example is backed by the
   [merged V8 correction](https://chromium.googlesource.com/v8/v8/+/c573bd216aa4f7649e9b7e54a3134f33920ac983%5E%21/),
   commit `c573bd216aa4f7649e9b7e54a3134f33920ac983`, 2019-08-13.
5. LLVM Project.
   [MLIR Language Reference](https://mlir.llvm.org/docs/LangRef/).
   High-Level Structure, Regions, SSA Control Flow Regions, Graph Regions,
   and Type System; retrieved 2026-11-06.
6. David Ittah, Thomas Haner, Vadym Kliuchnikov, and Torsten Hoefler.
   [Enabling Dataflow Optimization for Quantum Programs](https://arxiv.org/abs/2101.11030v2).
   Version 2, 2021. QIRO's two-dialect model and prototype/evaluation scope;
   related journal DOI: `10.1145/3491247`.
7. Anurudh Peduri and Siddharth Bhat.
   [QSSA: An SSA-based IR for Quantum Computing](https://arxiv.org/abs/2109.02409v1).
   Inspected preprint, version 1, 2021. The related CC 2022 publication,
   DOI `10.1145/3497776.3517772`, lists Peduri, Bhat, and Tobias Grosser;
   publication metadata was verified separately. Technical claims here use
   the accessible preprint, not an assumed identical final version.
8. NVIDIA.
   [Quake Dialect](https://nvidia.github.io/cuda-quantum/latest/specification/quake-dialect.html)
   and [CUDA-Q compiler IRs](https://nvidia.github.io/cuda-quantum/latest/using/extending/compiler/cudaq_ir.html).
   Reference/value motivation, mixed forms, and Linear-value Quake IR;
   retrieved 2026-11-06.
9. PennyLane/Catalyst contributors. Revision
   `b6fcbd45c9de49d39157ee6eab362d3785a54e51`.
   [Architecture](https://github.com/PennyLaneAI/catalyst/blob/b6fcbd45c9de49d39157ee6eab362d3785a54e51/doc/dev/architecture.rst)
   and [quantum operation definitions](https://github.com/PennyLaneAI/catalyst/blob/b6fcbd45c9de49d39157ee6eab362d3785a54e51/mlir/include/Quantum/IR/QuantumOps.td).
   Compilation Stack, Compiler and Runtime sections; `CustomOp` and `MeasureOp`.
10. Quantinuum.
    [HUGR Specification](https://quantinuum.github.io/hugr/specification/),
    [Structure and Semantics](https://quantinuum.github.io/hugr/specification/hugr.html),
    and [Type System](https://quantinuum.github.io/hugr/specification/type-system.html).
    Core/non-goals, edges and ports, dataflow/control constructs, and copyability;
    retrieved 2026-11-06.
11. IBM/Qiskit.
    [DAGCircuit](https://quantum.cloud.ibm.com/docs/en/api/qiskit/qiskit.dagcircuit.DAGCircuit)
    and [ControlFlowOp](https://quantum.cloud.ibm.com/docs/en/api/qiskit/qiskit.circuit.ControlFlowOp).
    Circuit graph and inner-block interfaces; retrieved 2026-11-06.
12. Seyon Sivarajah et al.
    [tket: A Retargetable Compiler for NISQ Devices](https://arxiv.org/abs/2003.10611v3),
    version 3, 2020. Implementation details additionally checked at TKET revision
    `dbf6e1a49dcb96863bbd7c877d625bc8998a5d83`:
    [circuit/subcircuit interface](https://github.com/Quantinuum/tket/blob/dbf6e1a49dcb96863bbd7c877d625bc8998a5d83/tket/include/tket/Circuit/Circuit.hpp),
    [conditional operations](https://github.com/Quantinuum/tket/blob/dbf6e1a49dcb96863bbd7c877d625bc8998a5d83/tket/include/tket/Circuit/Conditional.hpp),
    and [circuit boxes](https://github.com/Quantinuum/tket/blob/dbf6e1a49dcb96863bbd7c877d625bc8998a5d83/tket/include/tket/Circuit/Boxes.hpp).
13. Quantinuum.
    [HUGR-based TKET Rust documentation](https://github.com/Quantinuum/tket2/blob/42e3094a573b2d2d60f36d81827b88c66ea9307b/tket/tket-rs/README.md).
    Revision `42e3094a573b2d2d60f36d81827b88c66ea9307b`; circuit layer,
    quantum extension, pass system, and legacy serialization interoperability.
14. Google/Cirq.
    [Circuits](https://quantumai.google/cirq/build/circuits),
    [CircuitOperation](https://quantumai.google/reference/python/cirq/CircuitOperation),
    and [Classical control](https://quantumai.google/cirq/build/classical_control).
    Moments, nested circuits/repetition, and measurement-conditioned operations;
    retrieved 2026-11-06.
15. Andrew W. Cross et al.
    [OpenQASM 3: A broader and deeper quantum assembly language](https://arxiv.org/abs/2104.14722v2).
    Version 2; ACM Transactions on Quantum Computing, 2022. Supplemented by the
    official [classical instructions](https://openqasm.com/language/classical.html)
    and [circuit timing](https://openqasm.com/language/delays.html) specifications,
    retrieved 2026-11-06.
16. Robert S. Smith, Michael J. Curtis, and William J. Zeng.
    [A Practical Quantum Instruction Set Architecture](https://arxiv.org/abs/1608.03355v2).
    Version 2, 2017; quantum abstract machine and
    [Quil circuit, measurement, and program-control semantics](https://arxiv.org/html/1608.03355v2#S3.SS5).
17. Aleks Kissinger and John van de Wetering.
    [PyZX: Large Scale Automated Diagrammatic Reasoning](https://arxiv.org/abs/1904.04735v2).
    Version 2, 2020; EPTCS 318, pages 229-241,
    DOI `10.4204/EPTCS.318.14`. Linear-map diagrams and circuit-oriented tooling.
18. Quantinuum.
    [Tierkreis documentation](https://quantinuum.github.io/tierkreis/).
    Introduction and workflow execution model; retrieved 2026-11-06.
19. WebAssembly Community Group.
    [WebAssembly Core Specification: Overview](https://webassembly.github.io/spec/core/intro/overview.html).
    Instructions, functions, linear memory, modules, and imported interfaces;
    retrieved 2026-11-06.
20. [QRVSDG: A Regionalized Value State Dependence Graph for Hybrid Quantum-Classical Compilation](qrvsdg/qrvsdg_paper.md).
    Local design manuscript; quantum edges, structural interfaces, analyses,
    and transformation proposals. Not treated as independent experimental
    evidence for its comparative claims.
21. [QHDG: A Hierarchical Dependence Graph for Hybrid Quantum Compilation](qhdg.md).
    Companion working design, consulted 2026-11-06; Sections 1.1, 3-8, and 10.
    Proposed contracts and explicit open obligations are distinguished from
    completed implementations.
22. Microsoft QDK, branch snapshot
    `1b68d48b365b05dd92a04159ed5ecea2dd377fb2`.
    [QHDG crate surface](../src/lib.rs),
    [FIR producer](../../qsc_codegen/src/fir_qhdg.rs),
    [FIR return leg](../../qsc_codegen/src/fir_qhdg/return_leg.rs),
    [RIR producer](../../qsc_codegen/src/rir_qhdg.rs), and
    [RIR return leg](../../qsc_codegen/src/rir_qhdg/return_leg.rs).
    Source inspection of representation modules and producer-specific
    admission/return contracts; not a new execution test.
23. Eclipse XACC.
    [Project overview](https://github.com/eclipse/xacc/blob/d1edaa7ae53edc7e335f46d33160f93d6020aaa3/README.md).
    Revision `d1edaa7ae53edc7e335f46d33160f93d6020aaa3`;
    extensible hybrid compilation framework and IR-mediated integration.
24. ScaffCC contributors.
    [ScaffCC overview](https://github.com/epiqc/ScaffCC/blob/66a79944ee4cd116b27bc1a69137276885461db8/README.md).
    Revision `66a79944ee4cd116b27bc1a69137276885461db8`;
    Scaffold compilation/scheduling and LLVM-based implementation.
25. Damian S. Steiger, Thomas Haner, and Matthias Troyer.
    [ProjectQ: An Open Source Software Framework for Quantum Computing](https://arxiv.org/abs/1612.08091v2).
    Version 2, 2018; Quantum 2, 49. Compiler framework and extension mechanisms.

## Appendix A: Citation verification and interpretation

Verification checked source identity and the passages supporting the claims,
not merely HTTP availability. Repository citations use commit-pinned files
where practical; paper citations identify the inspected arXiv version.
Official living documentation is retrieval-dated. The following table records
the scope of that verification.

| References | Verified source locations and claims                                                                                                      | Interpretation limit                                                                                                   |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| [1,2,5]    | LLVM well-formedness; QIR model, optional capabilities and arrays/allocation; MLIR nested and graph regions                               | Framework and profile contracts do not prove every quantum transformation or backend                                   |
| [3]        | Paper identity, abstract, and Sections 2-6 on normalization, state domains, regions and construction/destruction                          | Classical representation and prototype results do not establish quantum-extension results                              |
| [4]        | Exact V8 maintenance passage and the merged patch's changed effect/control inputs                                                         | A concrete implementation failure, not a universal argument against dependency graphs                                  |
| [6,7]      | Version-specific titles, authors, abstracts, operation models and reported evaluation scope; QSSA publication metadata checked separately | Abstract-level findings are not a revalidation of all proofs or measurements; the QSSA publisher page was inaccessible |
| [8,9]      | Quake reference/value semantics and conversion limits; Catalyst architecture and quantum input/output definitions                         | Current capture, pass and device coverage is not exhaustively audited                                                  |
| [10,13]    | HUGR core structure/types/non-goals and the HUGR-based TKET implementation's own description                                              | Extensible operations still need semantic contracts and target-specific evidence                                       |
| [11,12,14] | Qiskit inner-block interface; TKET conditional and circuit-box source; Cirq nesting, repetition and classical control                     | Nested representations do not imply every pass/backend supports every construct                                        |
| [15,16]    | OpenQASM real-time scope, control and timing; Quil abstract machine and circuit/measurement/control sections                              | Language expressiveness is distinct from implementation acceptance and later extensions                                |
| [17,18,19] | PyZX linear-map/circuit scope; Tierkreis asynchronous workflows; WebAssembly core concepts                                                | Specialized optimization, orchestration and execution are different comparison layers                                  |
| [20,21,22] | Local design claims compared with current branch modules and explicit producer/return restrictions                                        | No complete formalization, implementation-conformance audit or new benchmark is claimed                                |
| [23,24,25] | Primary framework descriptions and ProjectQ paper identity/abstract                                                                       | Used for framework context, not detailed capability or performance rankings                                            |

Some documentation URLs returned navigation shells or unavailable pages.
For TKET, the concrete circuit claims therefore cite inspected source files
rather than treating a documentation landing page as evidence. QSSA's
accessible preprint and separately verified publication metadata are explicitly
distinguished. Unverified secondary summaries and ambiguous project names are
not used to fill missing capability cells.

## Draft status

AI-assisted comparative synthesis for human technical review. Interpretive
suitability statements are bounded by the cited sources and versions. No
preferred architecture, new empirical result, completed formal proof, or
publication acceptance is implied.
