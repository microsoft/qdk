---
title: "QHDG: A Hierarchical Dependence Graph for Hybrid Quantum Compilation"
description: "The Quantum Hierarchical Dependence Graph, its RVSDG foundations, QRVSDG influences, resource semantics, and transformation contracts."
---

## Abstract

Hybrid quantum programs combine copyable classical values, non-copyable quantum
resources, irreversible observations, and control decisions that may depend on
measurement. Representing these uniformly as ordinary values admits invalid
resource duplication. Serializing them through one undifferentiated state chain
instead obscures independence. A suitable intermediate representation must expose
dependencies without erasing the execution conditions and resource obligations
that make transformations sound.

The Quantum Hierarchical Dependence Graph (QHDG) separates typed value flow,
quantum-resource authority flow, hard-effect order, and explicit control and exit
reachability. Hierarchical regions compose these relations through typed
interfaces. Resource identities, ownership state, operation contracts, and
observation-relative rewrite obligations complement graph topology.

The design has two distinct conceptual antecedents. RVSDG supplies the regional
dependence framework, structured composition, construction and destruction, and
classical demand-oriented optimizations. QRVSDG adapts that framework to quantum
programs through linear quantum edges, quantum operations, measurement-aware
flow, quantum region annotations, and quantum-specific analysis and optimization.
QHDG refines this quantum adaptation with explicit authority, alias and cleanup
obligations, qualified effects, control/exit relations, staging, and checked
mutation. These refinements are not claims of global research novelty.

The following specification develops that lineage, defines QHDG's semantic
components, and gives transformation arguments and evaluation criteria. It does
not claim a complete operational semantics, a compiler-correctness theorem, or
new empirical performance results.

## 1 Introduction

An intermediate representation determines which facts a compiler can state
directly and which it must recover through analysis. Quantum compilation makes
this distinction important. Two references to the same qubit do not represent
two independently usable states. Measurement produces a classical result while
also changing quantum state. An unused classical result does not necessarily
make its measurement, allocation, failure, or cleanup removable.

QHDG represents computations as nodes with ordered operand and result ports.
Dependencies connect ports, while regions describe execution structure.
The representation distinguishes a classical value, a source-level resource
view, the authority to use a quantum resource, and the identity of that resource.
Its central design principle is:

> Represent classical values, quantum authority, hard effects, and control
> reachability independently, then preserve their joint observable behavior
> and resource obligations during transformation.

This principle develops a lineage rather than replacing it. The Regionalized
Value State Dependence Graph (RVSDG) [1] establishes the classical regional
framework. QRVSDG [2] makes the immediate quantum adaptation; its algorithm
addenda and later design study [3-6] investigate quantum transformations,
compound values, lifetime management, verification, and multi-level compilation.
QHDG adopts selected ideas and changes their formulation where aliasing, cleanup,
staging, or explicit reachability needs a separate contract.

The contributions of this design account are:

1. A separation of RVSDG foundations, QRVSDG adaptations, and QHDG refinements.
2. A definition of QHDG's four dependency relations and their composition through
   typed region interfaces.
3. An account of authority, observations, construction, destruction, and checked
   transformations that identifies semantic assumptions and open obligations.

### 1.1 Interpretation of the specification

A design requirement states what a conforming representation or transformation
must establish. A checking mechanism describes an executable way to discharge a
particular condition. A semantic assumption is supplied by an operation contract,
target model, or caller. An open obligation requires further semantics or proof.

These categories are not interchangeable. Structural verification does not prove
every operation identity; a declared region kind does not supply its complete
semantics; and a proposed analysis is not an empirical result. The predecessor
papers likewise contain both definitions and research proposals. Their
contributions are attributed without importing every proposed feature or
performance expectation into QHDG.

The path accounting, authority-transition judgment, and observation model below
are proposed normative contracts. Their notation specifies what an implementation
and its proofs must establish; it does not assert a completed type system,
checker, or metatheory.

## 2 Conceptual lineage

### 2.1 Foundations inherited from RVSDG

RVSDG defines an acyclic hierarchical multigraph whose regions contain arguments,
nodes, edges, and results [1, Section 4]. Simple nodes denote primitive operations;
structural nodes contain regions. Typed edges connect producing outputs or
arguments to consuming inputs or results. Each input has one origin, while
ordinary outputs may have multiple users.

Its structural vocabulary includes gamma conditionals, theta tail-controlled
loops, lambda procedures, delta global definitions, phi recursive environments,
and an omega translation-unit boundary. Region interfaces explain how operands
enter nested computations and how results return. Recurrence and recursive
environments are represented structurally without ordinary graph cycles.

RVSDG also supplies the following foundations:

- Dependence rather than incidental sequential order as the organizing principle
- Value edges for data and state edges for ordering side effects
- Guarding of side-effecting computation by regional execution structure
- Construction from source control/data flow and destruction by control-flow
  recovery
- Common Node Elimination (CNE) and Dead Node Elimination (DNE), including
  region-aware treatment of demand
- Normalization of structural control and independent-operation order

Independent state domains are already part of RVSDG. Its motivation explicitly
allows multiple independent states, and its evaluation describes separate
memory and nontermination states [1, Sections 2.4 and 7.1]. QHDG must therefore
not be distinguished by claiming that RVSDG requires one global state token.
The distinction concerns the explicit meaning and obligations assigned to
quantum authority, hard effects, and control.

### 2.2 Quantum adaptations inspired by QRVSDG

QRVSDG retains the regional framework and adds quantum-specific meaning [2].
Its primary adaptations are:

- Linear quantum edges, in contrast to freely shared classical values
- Quantum gate, preparation, measurement, reset, and barrier operations
- Measurement-result types and classical predicates derived from measurements
- Quantum resources threaded through conditional and loop interfaces
- Unitary, hybrid, and error-corrected region annotations
- Entanglement and quantum-liveness analyses
- Quantum-aware restrictions on CNE, DNE, code motion, and gate identities

QRVSDG's effect domains distinguish concerns such as quantum allocation pools,
classical memory, and input/output. This is a quantum adaptation of RVSDG state
dependence, not the invention of independent states. Similarly, quantum feedback
through gamma and theta specializes existing control constructs rather than
introducing hierarchical conditionals or loops.

The compound-type work extends linearity through arrays and products containing
quantum data. It considers destructive extraction, replacement, and swap-based
access to preserve ownership without general lifetime annotations [2,3].
QHDG's aggregate-aware use constraints therefore have a QRVSDG antecedent.
QRVSDG also discusses affine restrictions on quantum-capturing callable values;
the term _affine_ alone is not a QHDG contribution.

The later design study and addenda investigate dynamic allocation, qubit zones,
transport, QEC, layout, noise, scheduling, and layered lowering [4-6].
These provide design context. They are not automatic requirements that every
QHDG graph carry physical placement, a noise model, or pulse-level timing.

### 2.3 QHDG refinements of the quantum adaptation

QHDG develops the precursor ideas through explicit contracts:

- Quantum edges carry authority in a resource lineage rather than implying that
  a source reference is an independently owned quantum state.
- Owner paths, resource domains, scoped borrows, epochs, and cleanup states
  describe obligations that scalar edge linearity cannot express.
- Resource-qualified hard effects are distinct from authority flow.
- Control and exit reachability can be stated independently when value,
  authority, and effect dependencies do not express it.
- Generation and residual stages constrain information flow across staging
  boundaries.
- Rewrite boundaries, semantic obligations, transactional publication, and
  invalidation footprints make mutation requirements explicit.

These are refinements relative to the precursor specifications, not claims to
invent borrowing, effects, staging, or verified rewriting. QRVSDG's broader
investigations already consider borrowing and runtime ownership, effect
handlers, extension systems, and subgraph-replacement infrastructure.
The distinction is the particular QHDG formulation and how its obligations
compose.

### 2.4 Attribution summary

| Design concern             | RVSDG foundation [1]                                                  | QRVSDG adaptation [2-6]                                             | QHDG formulation                                                            |
| -------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Hierarchical composition   | Regions and structural nodes                                          | Quantum interpretation of the same hierarchy                        | Typed bindings for all four relations and explicit exits                    |
| Value and state dependence | Typed values and possibly independent states                          | Classical values, quantum edges, and qualified effect domains       | Value, authority, hard-effect, and control/exit relations                   |
| Quantum use constraints    | Single-definition inputs, but output fan-out is allowed               | Linear quantum outputs and quantum-containing aggregates            | Structural single use plus separate authority and cleanup obligations       |
| Callable resources         | Lambda bodies, captures, and apply                                    | Quantum subroutines, functors, and restrictions on quantum captures | Resource obligations remain distinct from callable identity                 |
| Aggregate access           | Typed values and dependency interfaces                                | Linear extraction/replacement and swap-based access                 | Alias views, authority partitions, borrow epochs, and join obligations      |
| Conditional feedback       | Gamma alternatives                                                    | Measurement-selected quantum branches                               | Path-sensitive authority use and explicit effect/exit composition           |
| Iteration                  | Structural theta recurrence in an acyclic graph                       | Quantum loop variables and feedback algorithms                      | Declared carried bindings and explicit recognized backedges where needed    |
| Region properties          | Structural node kinds                                                 | Unitary, hybrid, and error-corrected annotations                    | Semantic admission conditions distinct from structural region kind          |
| Deadness and congruence    | Region-aware DNE and CNE                                              | Quantum liveness, measurement safeguards, and gate reasoning        | Demand across relations and checked obligation-specific patches             |
| Scheduling                 | Dependence order and control-flow recovery                            | Quantum scheduling and layered hardware lowering                    | Semantic partial order separated from physical schedule and lifetime proofs |
| Staging                    | Multiple abstraction levels, not a generation/residual stage calculus | Parametric quantum programs and multi-level lowering                | Explicit generation/residual port stages                                    |
| Verification               | Structural invariants and transformation arguments                    | Linear checks and proposed compositional/ZX reasoning               | Per-edit certificates with explicit trust and discharge boundaries          |

This table separates documented inspiration from refinement. It does not assert
that QHDG realizes every precursor construct or that any predecessor's proof or
benchmark transfers to QHDG.

## 3 The Quantum Hierarchical Dependence Graph

### 3.1 Graph signature

Write the conceptual graph as:

$$
G = (N, R, P, E_v, E_q, E_h, E_c, I, X, \Sigma, \Omega, \Pi).
$$

| Component | Meaning                                                    |
| --------- | ---------------------------------------------------------- |
| $N$       | Computation and structural nodes                           |
| $R$       | Hierarchical regions with parent and owning-node relations |
| $P$       | Ordered input and output ports belonging to nodes          |
| $E_v$     | Copyable typed value dependencies                          |
| $E_q$     | Quantum capability dependencies                            |
| $E_h$     | Hard-effect ordering dependencies                          |
| $E_c$     | Explicit control and exit dependencies                     |
| $I$       | Typed region-interface bindings                            |
| $X$       | Declared region exits                                      |
| $\Sigma$  | Node, source-symbol, and resource identities               |
| $\Omega$  | Operation meanings and semantic contracts                  |
| $\Pi$     | Literal/symbolic payloads and provenance                   |

Ownership state $A$ and explicit demand roots $D_0$ accompany the graph.
They are semantic metadata, not extra interchangeable edge classes. The tuple
organizes the definition; it does not prescribe a storage layout.

Each node belongs to a region and names an operation. Ordered ports matter:
`CX(a,b)` and `CX(b,a)` have the same resource set and different meanings.
An edge names an operand position, not merely a pair of operations.
The regional node/port foundation is inherited from RVSDG; quantum resource
threading develops QRVSDG's quantum value flow.

### 3.2 Port types, usage, and stages

A port has the attributes:

$$
p = (\operatorname{node}, \operatorname{region}, \operatorname{direction},
     \operatorname{kind}, \operatorname{type}, \operatorname{stage},
     \operatorname{usage}, \operatorname{resource?}).
$$

Direction is input or output. Kind is value, capability, hard effect, or control
exit. Usage distinguishes copyable, affine, and linear use. Type describes the
carried value, including aggregate types that may contain quantum resources.
A complete executable interface supplies each required input through its
declared origin or boundary binding.

Copyable usage permits duplication or omission of a classical value. Linear
usage requires one use in an execution context. Affine usage permits at most
one use, but does not erase associated resource obligations.

The type discipline must distinguish authority-bearing values from resource
references. Write $\operatorname{Cap}(S,e)$ for authority over resource domain
$S$ at epoch $e$, and $\operatorname{View}(S)$ for an inert reference to those
resource identities. These are semantic type constructors, not a prescribed
encoding. Capabilities and aggregates containing them are non-copyable and flow
through $E_q$. Views carry no quantum state or access permission and may flow as
copyable classical values through $E_v$.

An array, tuple, record, or closure is non-copyable if it contains authority,
even when that authority is nested. An aggregate containing only views may be
copyable. Thus a source-level qubit reference can be copyable without making
the capability authorizing its use copyable. The compositional restriction on
authority-containing aggregates develops the QRVSDG compound-type adaptation.

Stages distinguish generation-time computation from residual computation in
the generated program. A generation value may feed an appropriate residual
consumer; a residual value cannot feed a generation-time consumer through an
ordinary edge. A dynamic feedback dependency must remain residual unless a
valid transformation changes the computation.

Hierarchy and staging are different. A nested region need not be statically
evaluated, and a known gate parameter does not make the gate's action on a
runtime qubit a generation-time computation.

### 3.3 Four dependency relations

For an ordinary edge $(p_o,p_i)$, the endpoints must exist, have output/input
direction, agree in type, and have kinds matching the relation. A cross-region
edge requires the corresponding typed interface binding.

| Relation | Interpretation                                                 | What it does not establish              |
| -------- | -------------------------------------------------------------- | --------------------------------------- |
| $E_v$    | A consumer needs a copyable classical value                    | Ownership of a quantum state            |
| $E_q$    | A consumer receives a capability and may produce its successor | Separability or absence of entanglement |
| $E_h$    | A hard semantic ordering must be respected                     | A universal physical schedule           |
| $E_c$    | Execution or exit reachability must be represented explicitly  | Ordinary data dependence                |

The first three relations develop RVSDG value/state dependence through
QRVSDG's classical/quantum/effect distinctions. The fourth makes additional
reachability obligations explicit rather than assuming data dependence always
encodes them. Not every pair of nodes requires all four relations.

A legal schedule respects applicable dependencies and regional execution
semantics. It is not an arbitrary topological sort of a flattened graph.
Iteration and residual control cycles require their own contracts.

### 3.4 Structural single use

An ordinary, unbound quantum-capability output must have one effective use in
an execution context: neither independent simultaneous consumers nor silent
loss. An interface-bound port is not dropped merely because it has no local
outgoing edge.

Alternatives require path-sensitive accounting. For a produced capability $p$,
let $\mathcal{P}(p)$ contain feasible paths from its production to the boundary
being checked, restricted to executions that reach that production. Let
$u_\pi(p)$ count its consuming uses on path $\pi$, including a transfer through
an interface or an explicitly permitted discharge such as release. Mere absence
of an edge is not a discharge. At every reachable exit the linear requirement is:

$$
\forall \pi \in \mathcal{P}(p), \qquad u_\pi(p)=1.
$$

A maximum use count of at most one checks no-fanout; it does not check
no-dropping. For a nonempty finite set of alternative exit paths, both
$\max_\pi u_\pi(p)\leq 1$ and $\min_\pi u_\pi(p)\geq 1$ are needed. Counts
$(1,0)$ fail the second condition even though their maximum is one. An
untouched branch must forward the capability, or perform a permitted
ownership transition that explicitly accounts for it.

For affine values the structural condition is only $u_\pi(p)\leq 1$.
If the value carries authority, a zero-use path must still discharge or export
the corresponding resource obligation under the ownership policy. Every return,
abort, and other reachable exit must be checked, not just a successful join.
Only proven-infeasible paths may be omitted; unknown feasibility is handled
conservatively.

These are local path and interface contracts, not a requirement that every
nonterminating execution eventually releases all resources. Iteration must
maintain the authority invariant on each backedge; a resource still live in
an infinite execution must not be silently lost. Termination and progress
observability are separate requirements in Section 7.1.

This strengthens SSA rather than following from it. RVSDG's single origin per
input does not prohibit fan-out from an output. QRVSDG supplies the quantum
single-use adaptation; QHDG combines it with authority and cleanup accounting.
Single-use edges alone do not prove disjoint aggregate paths, valid borrows,
or safe release.

### 3.5 Region interfaces and kinds

A region records its kind, parent, owning node where applicable, and interface.
Interfaces describe arguments, results, captures, carried values, and exits.
A binding relates an outer port to its corresponding inner port. Crossings
through several nesting levels must preserve the intervening contracts.

| Region kind           | Semantic role                                             |
| --------------------- | --------------------------------------------------------- |
| Module                | Imports, exports, and program-level composition           |
| Callable              | Body, arguments, results, and captures                    |
| Conditional           | Mutually exclusive alternatives                           |
| Iteration             | Repeated computation with carried state                   |
| Recursion             | Recursive composition                                     |
| Handler               | Scoped handling and exit behavior                         |
| Stage                 | Generation/residual boundary                              |
| Residual control flow | Explicit control not represented by the structured subset |

The module/callable/conditional/iteration/recursion roles develop RVSDG's
structural vocabulary and its QRVSDG interpretation. This is not a one-to-one
claim that every RVSDG structural node has an identically implemented QHDG
counterpart. In particular, RVSDG's delta definitions are antecedents for
global data and metadata, not a reason to require a separate QHDG delta kind.

Exits distinguish return, abort, branch, and backedge. Each construct needs
compatible signatures and ownership obligations at its boundaries. Naming
a construct is not a complete specification of its execution or destruction.

### 3.6 Conditional composition

A conditional has an owning node and sibling regions for its alternatives.
The owner groups the alternatives and identifies the predicate-controlled
construct. Parentage alone cannot group them: unrelated conditionals may have
the same parent.

```text
                          predicate b
                              |
                         conditional
                         /           \
      then region: q -> X -> q'     else region: q -> carry -> q'
                         \           /
                       continuation(q')
```

Only one alternative executes. The untouched branch explicitly returns the
capability required by the common interface. Classical values can similarly
be relayed through nested interfaces.

The gamma structure comes from RVSDG; measurement-controlled quantum
alternatives come from QRVSDG. QHDG adds the obligation to reconcile authority,
cleanup, effects, and exits at the join, rather than treating matching wire
counts as sufficient.

### 3.7 Iteration and explicit backedges

RVSDG theta expresses recurrence through a structural loop interface while
retaining acyclic ordinary regions [1]. QRVSDG applies that pattern to quantum
loop variables and feedback algorithms [2]. QHDG can additionally represent
recognized recurrence edges through carried bindings and declared backedge
exits.

An accepted backedge must be justified by the enclosing iteration contract,
including its source exit and target carried port. Ordinary undeclared
dependency cycles remain invalid. A residual control-flow region similarly
needs explicit control semantics; it is not permission for arbitrary cyclic
value dependencies.

These declarations describe recurrence, not a static execution count.
A condition and body may have different execution multiplicities. A local
rewrite must not cancel an apparent inverse pair across a backedge or retire
a port that the loop interface still names without updating and justifying
the loop contract.

### 3.8 Region properties are not region kinds

QRVSDG distinguishes unitary, hybrid, and error-corrected region annotations
[2, Section 4.3]. These describe semantic properties, whereas a gamma or theta
describes execution structure. A callable may be unitary; an iteration may
contain measurement and classical feedback.

QHDG uses the same distinction when admitting transformations. Unitary
classification needs evidence about operations, resource boundaries, effects,
and exits. A structural region name does not authorize adjoint derivation,
gate reordering, or unitary synthesis. QHDG need not reproduce the precursor's
annotation taxonomy as a new set of structural kinds.

## 4 Quantum authority, aliases, and cleanup

### 4.1 Values, views, and resources

QRVSDG's linear state threads make quantum use explicit. Its aggregate access
policy emphasizes extraction/replacement and swaps rather than general
lifetime-annotated borrowing [2,3]. QHDG distinguishes an additional layer:
source references can be alias views, while authority over the underlying
resource is tracked separately.

Copying a view does not clone quantum state or mint fresh authority. Two views
can identify the same resource. Conversely, two disjoint authority domains
can still refer to entangled subsystems. Ownership and separability answer
different questions.

Using a $\operatorname{View}(S)$ to address a quantum operation requires
resolving it against a current capability or authorized borrow covering $S$.
View copying creates no such permission. Views cannot be used to manufacture
capabilities, read unknown quantum state as classical data, or bypass a
conflicting borrow.

Allocation dependencies and lifetime provenance must survive view construction.
A view that outlives a release may remain inert data, but cannot authorize
access to a later allocation that reuses the same physical location. Resource
identities distinguish those lifetimes. Unresolved or stale access must be
rejected or follow an explicit checked policy; it must not default to a fresh
independent wire.

### 4.2 Authority records and partitions

An authority record is:

$$
a = (\operatorname{path}, \operatorname{resources},
     \operatorname{epoch}, \operatorname{cleanup}).
$$

An owner path can select fields, indices, dynamic indices, or slices. Resource
identities describe underlying resources; paths describe how a program reaches
them. Epochs distinguish successive ownership states even when the path has
the same spelling.

Live ownership domains in one execution state must not overlap. Borrow
permissions refer to that ownership; they are not additional overlapping
owners. Alternative execution states are checked separately, so passing the
same resource to mutually exclusive branches does not violate this invariant.
Splitting authority requires
pairwise-disjoint parts whose union is exactly the parent domain.
The split is a partition, not a cloning operation. Joining must reconstruct
compatible authority and cleanup information.

This is a QHDG resource-accounting formulation of a problem already exposed by
QRVSDG's linear aggregate operations. It does not attribute aggregate linearity
itself to QHDG.

### 4.3 Borrowing and unknown aliasing

A borrow identifies shared or exclusive access to an authority's path,
resource domain, and epoch. Exclusive access must not conflict with another
live borrow. A stale epoch cannot authorize current access.

Shared access permits only operations whose contracts establish that they do
not transform or observe quantum state or change resource lifetime, such as
inspecting classical descriptor metadata. A gate, measurement, or reset
requires owning access with no conflicting borrow, or a valid exclusive
borrow. Exclusive borrowing suspends conflicting use by the owner; retaining
the owner record does not permit simultaneous access.

Unknown aliasing requires an explicit policy: prove disjointness, perform an
approved runtime check, escape to a representation that can preserve the
semantics, or reject the operation. Merely naming a runtime-check alternative
does not implement or prove it.

Repeated uses of one immutable binding can identify the same wire without
establishing that two different bindings hold distinct wires. Fresh allocation
may establish distinct identity; unresolved indexing must not silently become
an independent capability chain.

### 4.4 Affine use does not authorize silent deletion

The mathematical distinction is at-most-once use for an affine value and
exactly-once use for a linear value. QRVSDG sometimes describes a
quantum-capturing callable as affine while also imposing an exactly-once
application condition. QHDG separates usage from the obligations attached to
the captured resources.

An authority can be transferred, consumed, or discharged according to policy,
but an optimizer cannot erase a live quantum obligation by calling its token
affine. Structural no-dropping checks and cleanup obligations remain necessary.
This is a compiler resource policy, not a claim that physical partial trace
or destructive measurement is impossible.

### 4.5 Cleanup and exits

Cleanup distinguishes known computational-basis zero, pending restoration,
restoration fragments, unknown live state, escape, and release. Operations,
measurement, reset, split, and join update these facts conservatively.

An unused result does not prove safe release. A dirty borrowed resource may
need restoration rather than reset. A failure exit may owe cleanup even when
there is no normal continuation. An explicit release node closes a resource
thread only when the corresponding lifetime and cleanup policy permits it.

QRVSDG supplies preparation, reset, destructive measurement, and lifetime
analysis as antecedents [2,5]. QHDG makes restoration and exit-sensitive
obligations explicit. Complete policies for dirty restoration, entangled
resources, runtime alias checks, and failure cleanup remain open obligations,
not consequences of a linear edge count.

### 4.6 Operation-level authority transitions

Let $A$ be live ownership state, $B$ its active borrow permissions, and
$\bar{\kappa}_{\mathrm{in}}$ and $\bar{\kappa}_{\mathrm{out}}$ ordered
capability versions. Write the proposed transition judgment:

$$
\Omega; A; B \vdash
\operatorname{op}(\bar{\kappa}_{\mathrm{in}})
\Longrightarrow
(\bar{\kappa}_{\mathrm{out}}; A'; B').
$$

The judgment connects an operation contract to both graph ports and authority
state. It is not satisfied by disjoint authority records alone. Admission
requires the following conditions:

1. Each capability input resolves to its current live version, resource domain,
   owner path or borrow permission, and authority epoch. Operand aliasing must
   satisfy the operation contract; in particular, distinct operands of a gate
   requiring distinct qubits cannot resolve to the same resource.
2. Access permissions authorize every touched resource. State transformation,
   measurement, reset, transfer, and release cannot bypass conflicting borrows.
   The owning record and a borrowed permission do not create independent
   capability lineages for the same resource.
3. Each consumed capability version is retired and cannot be consumed again.
   Successor versions are fresh, retain the corresponding resource identities,
   and have the contract's ordered input-to-output correspondence. Identity
   permutation requires an explicit transfer map; matching only resource sets
   is insufficient.
4. Ownership changes that invalidate permissions advance the relevant epoch.
   A borrow at the old epoch must end, or an explicit transition must retire it
   and issue its replacement. A gate's fresh capability version is distinct
   from an ownership epoch: not every gate needs to change the owner epoch.
5. Cleanup facts are transformed by the contract, unaffected authority is
   preserved, and all successor authority and borrow invariants hold. No
   operation obtains a clean or restored state merely by relabeling its output.

For the touched domain, let $S_{\mathrm{in}}$ and $S_{\mathrm{out}}$ denote
locally held resources before and after the operation. Let $F$, $L$, and $X$
be newly allocated, permitted released, and explicitly escaped domains.
Resource accounting requires:

$$
S_{\mathrm{in}} \mathbin{\dot\cup} F =
S_{\mathrm{out}} \mathbin{\dot\cup} L \mathbin{\dot\cup} X.
$$

The dotted unions require disjoint accounting, not disjoint quantum states.
$F$ contains fresh lifetime identities, $L$ requires a valid cleanup/release
policy, and $X$ requires a receiving external obligation rather than silent
loss. These sets record ownership transfer, not conservation of a resource
through arbitrary physical channels.

| Transition                                           | Required resource and permission behavior                                                                                      |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Gate, non-destructive measurement, reset             | Preserve resource identities and produce fresh capability versions; update quantum and cleanup state according to the contract |
| Allocation                                           | Introduce fresh identities and initial authority; initialize cleanup facts explicitly                                          |
| Transfer or split/join                               | Preserve total resource domain, retire superseded versions, and establish the declared path/partition map                      |
| Release or destructive measurement with deallocation | Consume the capability, satisfy the release policy, and leave no locally usable authority or borrow for that lifetime          |
| Escape                                               | Transfer the obligation to a named boundary contract; do not retain independent local authority                                |

For example, `H` consuming resource $r$ may produce a successor for $r$, not
silently for $s$. Two uses of the same input capability version are invalid,
even if each resulting graph edge is locally well typed. Two shared views do
not authorize concurrent `X` and `Z` on their common resource.

Region composition lifts this judgment over each reachable alternative.
Every exit must account for its live domain, permissions, and cleanup.
Branches rejoining one continuation must agree on its required authority
interface or provide explicit alternative outcomes; a join cannot silently
intersect resource sets and discard the difference. Iteration must establish
the carried invariant initially and re-establish it on each backedge.

The judgment specifies necessary admission obligations. Concrete operation
transfer functions, complete exit policies, and a preservation proof remain
to be supplied; the notation alone is not a soundness theorem.

## 5 Effects and observational semantics

### 5.1 Qualified hard effects

An effect event has a domain, mode, resource qualifier, and observability.
Domains can distinguish quantum state, measurement results, allocation,
classical memory, output, failure, host interaction, and noise.

Independent domains develop RVSDG's independent states and QRVSDG's qualified
effect tokens. QHDG further separates hard-effect order from quantum authority
and states the resource overlap used by an ordering decision.

A conservative event-level conflict predicate is:

$$
\operatorname{conflict}(e,f) =
(\operatorname{domain}(e)=\operatorname{domain}(f))
\land \operatorname{overlap}(e,f)
\land \neg(\operatorname{read}(e)\land\operatorname{read}(f)).
$$

Equal resource qualifiers overlap; an unknown qualifier overlaps resources
within its domain. This predicate is not a complete alias or commutativity
theorem. Effects that cross nominal domains must be modeled by appropriate
contracts and dependencies rather than assumed independent by their labels.

### 5.2 Reachability-qualified ordering

Hard-effect order is qualified by execution reachability. Effects in mutually
exclusive branches do not execute in sequence merely because both regions
exist. Conflicting events on a possible execution path need the appropriate
order; the enclosing conditional composes its alternatives through its
interface.

RVSDG already guards side-effecting computations through regions. QRVSDG
applies that structure to quantum feedback. QHDG's explicit control/exit
relation supplements this foundation when other relations do not encode all
reachability obligations. It is not a claim that predecessor graphs lack
control semantics.

### 5.3 Observations include joint behavior

Let $\llbracket G \rrbracket_{\Omega,A}$ denote intended graph behavior under
operation contracts $\Omega$ and admissible initial ownership $A$.
For observation set $O$, rewrite preservation requires:

$$
G \rightsquigarrow G'
\quad\Longrightarrow\quad
\operatorname{Obs}_O(\llbracket G \rrbracket_{\Omega,A}) =
\operatorname{Obs}_O(\llbracket G' \rrbracket_{\Omega,A})
$$

for all legal inputs and applicable enclosing contexts, with compatible
interfaces and resource obligations. Relevant observations include classical
results, joint measurement distribution, final live quantum state, ordered
output, failure, termination, and resource lifetimes. Classical results and
quantum state must be observed jointly, not as independently preserved
marginals.

For a first-order, finite-outcome fragment, make this requirement explicit with
a trace-indexed quantum instrument. Let $t$ be an observable event trace
(including ordered output and resource events), $c$ the classical results,
and $k$ a terminal outcome such as return or failure. Each
$\mathcal{I}^{G}_{t,c,k}$ is a completely positive, trace-nonincreasing map
from the quantum input to the live quantum output for that outcome. Their
sum is trace-nonincreasing, and trace-preserving for a fragment guaranteed to
terminate within the modeled outcomes.

For input state $\rho_{Q_{\mathrm{in}}F}$, where $F$ is an arbitrary untouched
reference system that may be entangled with the input, the terminal
classical-quantum state is:

$$
\omega_G(\rho) =
\sum_{t,c,k} |t,c,k\rangle\langle t,c,k|
\otimes
\bigl(\mathcal{I}^{G}_{t,c,k}\otimes\operatorname{id}_F\bigr)
(\rho_{Q_{\mathrm{in}}F}).
$$

Outcome-dependent quantum interfaces are interpreted as tagged output spaces.
The map on $F$ is the identity, not a claim that $F$ is separable from the
program. Preservation compares the observation projection of this joint
object for every admissible input and reference system. Hiding an outcome
sums its instrument branches; hiding a quantum output takes its partial trace.
Neither operation substitutes equality of separately computed marginals for
equality of the retained joint state and trace associations.

For example, a fair classical bit returned with the matching quantum state
`|0>` or `|1>` has the same classical and quantum marginals as an independent
fair bit returned with a maximally mixed qubit. Measuring the qubit in Z and
comparing with the bit gives agreement probability one in the first case
and one half in the second. They are not equivalent returned interfaces.

This terminal instrument is a precise contract for the stated fragment, not
a complete model of nontermination. Its subnormalized mass records completion
probability, but does not describe the observable prefixes or outputs of an
infinite execution. General preservation additionally needs a trace semantics
for those executions and the progress obligations in Section 7.1. Absence of
a terminal result is not permission to erase a computation.

QRVSDG's measurement-aware reasoning is an important precursor [2,3].
Equal marginal distributions do not justify merging measurement instances:
two independent fair bits agree with probability one half; replacing both by
one sampled bit makes agreement certain.

Likewise, an unrecorded measurement can change later behavior. Prepare a Bell
pair with `H(a); CX(a,b)`, then consider an intermediate Z measurement of `a`
whose result is unused. Undoing the preparation and measuring both qubits
distinguishes the measured state from the unmeasured one. Classical-result
deadness therefore does not imply quantum-channel deadness.

Entanglement analysis can conservatively bound possible dependence, but
ownership disjointness does not prove separability. Measurement may leave
classical correlations even when a particular entanglement abstraction splits
a class. That abstraction is not a complete observational semantics.

### 5.4 Phase and physical models

Equality of isolated measurement distributions is also too weak for arbitrary
reusable unitary identities. Control can turn a global phase into a relative
phase. A transformation relying on exact inverse cancellation must state
whether it preserves exact phase, not silently use equality up to phase.

The first-order instrument model identifies unitaries that induce the same
channel, including ones differing only by global phase. It therefore cannot
alone justify substitution under a coherent-control constructor. An operation
that supports coherent control must additionally preserve its declared unitary
meaning, including the required phase convention, under that constructor.
Classical measurement-selected control does not supply this stronger guarantee.

An ideal noiseless identity need not preserve physical execution under a noise
model. QRVSDG's layered design separates logical optimization from physical
mapping, scheduling, and calibration concerns [5,6]. QHDG retains that boundary:
an observation contract and target model determine what a transformation may
ignore.

The preservation equation is a specification obligation. A complete semantics
for probabilistic traces, divergence, noise, and contextual equivalence, and
a soundness theorem for all admitted rewrites, remain open.

## 6 Construction and destruction

### 6.1 Construction from circuit and source form

RVSDG supplies construction from control/data flow, and QRVSDG adapts it to
quantum assembly and quantum runtime calls [1-3]. QHDG construction similarly
maps source operations into semantic dependencies rather than preserving every
incidental sequence edge.

For numbered circuit wires, construction maintains the current capability of
each resource. A resource-transforming operation consumes its operands' current
capabilities and produces successors or explicitly discharges them under its
contract. Measurement that retains the resource also produces classical result
flow. Later predicates depend on the result producer, not merely on a reused
register number.

```text
allocate(q) -> H -> measure_z -> successor capability -> permitted release(q)
                         |
                         +-- classical result -> conditional predicate
```

The diagram distinguishes quantum and classical outputs. It does not imply
that measurement releases the qubit. Conditional construction creates owned
alternatives with compatible interfaces; iteration construction declares
carried state and recurrence. Unknown aliasing, unmodeled effects, or
unsupported control must lead to an explicit boundary or refusal.

Source construction must also preserve parameter expressions, global phase,
resource declarations, evaluation that may fail, and provenance. An opaque
operation can retain an original computation without claiming a pure or
unitary classification. Any such opacity constrains which analyses and
destruction routes are sound.

Capability threading must establish the authority-transition judgment, not
only compatible port types. Views retain their allocation/lifetime dependencies,
and each exit forwards or discharges its obligations. Potentially divergent
computation also needs the control/progress dependencies that prevent later
observable work from being executed prematurely.

### 6.2 Symbolic construction and staging

Quantum bodies can be represented before parameters or resource counts are
fully specialized. QRVSDG's parametric subroutines and hybrid loops motivate
this use. A redundancy removed from a symbolic body may otherwise be
rediscovered in each later specialization.

QHDG's explicit stage labels add a different obligation: distinguish
generation-time evaluation from residual computation. Classical parameter
folding is not permission to execute a dynamic measurement or branch during
generation. A body can remain symbolic without being a generation-time
program.

### 6.3 Destruction as a semantic projection

Destruction selects a legal order and materializes target control, values,
runtime operations, and resource behavior. This task is inherited from RVSDG
control-flow recovery and its QRVSDG quantum extension, not introduced by QHDG.
For a supported target projection $S_T$, the intended obligation is:

$$
\operatorname{Obs}_O(\llbracket G \rrbracket) =
\operatorname{Obs}_O(\llbracket S_T(G) \rrbracket).
$$

Targets and return paths have different expressive limits:

- Circuit form may admit only a subset of classical and control constructs.
- A structured target requires legal recovery of branches and loops.
- A residual control-flow target needs block, exit, and carried-value handling.
- A source-preserving return path may support only edits expressible in the
  original source form.
- A physical target adds placement, connectivity, timing, and runtime constraints.

Shared operation names do not make these projections interchangeable.
A projection that can remove or reorder existing instructions is not thereby
able to synthesize every operation or new region expressible in the graph.
Preservation must be established for the actual projection.

### 6.4 Dependence order is not hardware time

QRVSDG's destruction and scheduling proposals expose quantum dependence and
place hardware-sensitive work at later lowering boundaries [2,5,6]. QHDG
likewise treats a semantic partial order as input to scheduling, not its result.

A longest dependency path can bound ideal logical depth under a stated cost
model. It does not account automatically for durations, routing, shared control
hardware, classical feedback latency, noise, or calibration. Nor does one
arbitrary topological order establish the lifetime facts needed for resource
reuse. Scheduling and lifetime-sensitive optimization need additional
contracts.

## 7 Analyses and transformations

### 7.1 Demand as a least fixed point

RVSDG DNE marks computations contributing to region results and follows
structural interfaces [1, Section 6.2]. QRVSDG extends the question to quantum
liveness, effects, and entanglement [2,3]. QHDG demand must additionally retain
explicit resource, reachability, and cleanup obligations.

Let $D_{\mathrm{base}}$ contain producer demands, required effects, resource
and cleanup obligations, and required exits. Define a further set $T_O$ of
potentially executed computations whose termination or progress behavior must
be retained under observation contract $O$:

$$
D_0 = D_{\mathrm{base}} \cup T_O.
$$

When termination is observable, an executable loop, recursive invocation,
unknown call, or residual control-flow cycle is in $T_O$ unless its totality
or unreachability is established. This concerns execution sites, not the mere
definition of an uncalled function. Unknown totality is not proof of
termination.

Excluding termination from $O$ is not sufficient by itself to remove divergence:
blocking later output, failure, cleanup, or another retained observation can
still be observable. Such computation remains demanded unless removing it is
proved harmless to every retained observation.

Retaining a node is also insufficient if its continuation can run too early.
Construction must encode the dependency of later observable work on successful
completion, using the appropriate control or hard-effect relation. For example,
`while true { }; emit(1)` must not become `emit(1)`, nor may emission move before
the loop. A proven finite pure loop with no demanded result or other observable
obligation may instead be removable. A demand mark never authorizes speculative
execution of a region that its original guard does not select.

Let $\operatorname{pred}(D)$ follow all edge relations, including the required
guards and completion dependencies. Let $\operatorname{iface}(D)$ follow
corresponding demanded results, arguments, and carried bindings across region
interfaces, whether or not a binding is also encoded as an ordinary edge.
Let $\operatorname{owners}(D)$ retain nodes whose regions contain demanded
computation. A conservative graph-level formulation is:

$$
D^* = \mu D.\,
  \bigl(D_0 \cup \operatorname{pred}(D) \cup \operatorname{iface}(D)
        \cup \operatorname{owners}(D)\bigr).
$$

The lattice is the powerset of finite node identities with union as join.
A monotone worklist reaches a fixed point because each node is inserted at
most once. Backedges require no special widening for this finite-set analysis;
that fact says nothing about whether the represented program terminates.
Demanding a branch computation also demands the node that makes the branch
reachable; an edge-only walk can miss that relation.

Dead-node elimination may retire only nodes outside demand, and the resulting
patch must still preserve interfaces, progress dependencies, and the
operation-level authority and exit obligations. More precise
region-scoped summaries require explicit interface demand inputs. Unused
measurement results alone are insufficient evidence of deadness.

### 7.2 Classical congruence and quantum restrictions

RVSDG CNE redirects outputs of congruent computations [1, Section 6.1].
QRVSDG preserves that mechanism for suitable classical results but restricts
diversion of quantum outputs and nondeterministic measurement instances [2,3].
QHDG follows this distinction.

For pure classical computations in one execution region, congruence requires
equal operation meaning and payloads, corresponding input producers, and
compatible signatures. Effects, totality, and the requested observation
contract must permit sharing. The duplicate's $k$th consumer connects to the
canonical $k$th output.

Equal operation categories are insufficient: two symbolic constants naming
different parameters are different values. Nor may `U(q); U(q)` become a
single `U(q)` by ordinary CNE. The second application consumes the successor
of the first, not an unchanged duplicable input.

Sharing a subroutine description is different from merging its quantum
invocations. The description may be classical while each invocation consumes
and transforms distinct resource authority.

### 7.3 Inverse cancellation

Gate-identity reasoning is a QRVSDG quantum adaptation, not a consequence of
classical CNE [2,5]. For declared inverses $U$ and $V$:

```text
before:  p[k] -> U.in[k] ... U.out[k] -> V.in[k] ... V.out[k] -> c[k]
after:   p[k] -------------------------------------------------> c[k]
```

A QHDG cancellation obligation requires:

1. Reciprocal inverse meaning and the required exact-phase policy.
2. Matching operand counts and positions, with no hidden intermediate use.
3. Effects and observations compatible with removal.
4. A common execution context and preserved region interfaces.
5. No unsupported cancellation across recurrence or exit boundaries.
6. Retirement of exactly the pair and an exact positional bypass.

`CX(a,b); CX(b,a)` illustrates why equality of resource sets is insufficient.
A checker must reconstruct the permitted bypass rather than merely accept a
rule's proposed connections. Operation declarations remain semantic
assumptions unless independently established.

### 7.4 Motion and loop invariance

RVSDG supplies regional code-motion patterns. QRVSDG adapts them to unitary
gates and quantum loop variables [2-4]. QHDG requires resource and effect
evidence before applying those patterns.

Disjoint-resource gates may exchange order when their contracts and requested
observations permit it. Changing an ordering relation must not change which
quantum operand either operation consumes.

A loop-invariant angle is not proof that its gate is loop invariant.
Repeatedly applying a fixed `X` still changes a qubit according to iteration
parity. Hoisting quantum operations requires an argument about carried state,
execution multiplicity, effects, and exits. Structured recurrence helps state
that argument; it does not supply it automatically.

### 7.5 Diagonal operations before collapse

A computational-basis diagonal unitary immediately before a suitable Z
measurement or reset may be removable under a stated observation contract.
The basis qualification matters: the same deletion can change an X- or
Y-basis observation.

Every affected quantum output must reach a permitted collapsing consumer
without an intervening observation. Collapsing one operand of a multi-qubit
diagonal gate is insufficient; another live operand can retain an observable
phase. This is an obligation-specific quantum rewrite, not ordinary DNE.

### 7.6 Finite optimization islands

QRVSDG's unitary regions, quantum rewriting, and equality-saturation proposals
motivate a boundary around stronger quantum optimization [2,5]. QHDG uses
finite admissible islands: pure classical computation or statically
ownership-closed unitary computation.

Admission needs evidence about resource and effect boundaries. Dynamic
aliasing, unresolved restoration, allocation/release, measurement/reset/noise,
unknown calls, and exit changes cannot be admitted merely by naming the
island unitary. A published patch must stay inside its certified boundary.

An island is an admissibility condition, not a delivered synthesis algorithm
or a completeness theorem for arbitrary quantum equivalence.

## 8 Checked transformation and assurance

### 8.1 Identity and transactional publication

Node identity, source-symbol identity, resource identity, and storage position
serve different purposes. A reused storage position must not make an old
reference identify an unrelated node. Retired durable identities must not be
reissued.

A transaction constructs a candidate graph, checks preconditions and structural
validity, then publishes atomically. Abandoning the candidate leaves the
original unchanged. A successful edit reports a footprint over affected
nodes, ports, regions, interfaces, resources, ownership paths, effects,
exits, and provenance.

QRVSDG's rewriting and verification investigations motivate explicit edit
boundaries [5]. QHDG's transactional protocol specifies how a particular edit
is admitted and published. Atomicity is a structural safety property, not
proof that the edit preserves behavior.

### 8.2 Match, claim, patch, discharge

```text
find match -> state certificate/obligation -> build port patch
           -> validate boundary and discharge obligation
           -> verify candidate -> publish footprint
```

A certificate records matched identities, revisions, boundary ports,
resource/effect/exit information, observations, assumptions, an obligation,
evidence, and invalidations. A patch states exact creations, replacements,
retirements, and edge changes.

Checking must bind the certificate to the actual input and proposed patch,
restrict the touched entities, preserve operand threading, and recompute
obligation-specific evidence. Naming a proof method or supplying an evidence
string does not make an arbitrary edit sound.

In particular, path maxima do not discharge per-exit resource obligations;
disjoint authority sets do not discharge an operation's transfer judgment;
and equal output marginals do not establish joint classical-quantum
preservation. Demand-based edits must justify their termination/progress roots
and preserve interface and continuation dependencies.

### 8.3 Distinct assurance levels

| Mechanism                      | What it establishes                                                    | Limitation                                          |
| ------------------------------ | ---------------------------------------------------------------------- | --------------------------------------------------- |
| Structural verification        | Implemented graph invariants                                           | Not complete program semantics                      |
| Obligation-specific discharge  | Particular demand, congruence, inverse, motion, or collapse conditions | Depends on the obligation and operation assumptions |
| Assumed rewrite                | An explicit caller trust decision                                      | No independent semantic check                       |
| External differential evidence | Agreement on an executed validation domain                             | Not universal equivalence or a per-edit oracle      |
| Content digest                 | Equality with expected serialized content                              | Not authenticity or behavioral equivalence          |

QRVSDG proposes compositional and ZX-based verification [2, Section 8.1].
QHDG's certificates can record or consume appropriate evidence, but they do
not replace those semantic arguments. Neither a verification proposal nor
a certificate schema establishes a complete correctness theorem.

### 8.4 Analysis validity and determinism

Cached analyses depend on graph content, scope, revision, and other analyses.
A footprint must invalidate affected results and their dependents.
Equal revision numbers on different graph states do not establish equal
analysis inputs.

Deterministic serialization and candidate ordering support reproducibility.
They do not normalize all isomorphic graphs to one identity or select a unique
semantically equivalent program. Likewise, RVSDG's structural normalization
must not be confused with rewrite termination or a universal normal form for
quantum circuits.

Budgets and repeat detection can bound execution of a rewrite engine.
The obligations and limitations of those mechanisms must be explicit; they
are not proofs of optimization completeness.

## 9 Evaluation criteria

An evaluation should distinguish representation quality, semantic assurance,
optimization utility, and compilation cost. Results established for RVSDG or
projected in QRVSDG are motivation, not measurements of QHDG.

A defensible study reports:

- Accepted and rejected program subsets, including why a projection declines
- Matched and committed rewrites, final graph shape, and semantic checks
- Construction, analysis, matching, discharge, mutation, verification, and
  destruction costs separately
- Node, port, edge, region, and interface counts, allocation, and peak memory
- Sparse edits, cancellation-dense computation, nested branches, loops, symbolic
  parameters, aggregate resources, and refusal-heavy cases
- Preserved observation sets, target assumptions, and external oracle limits
- Identical semantics and checking obligations for data-structure comparisons
- Logical depth separately from routed duration, physical fidelity, and resource
  lifetime costs

A certificate or footprint describes a locality contract; it does not by
itself establish an incremental algorithm. Whole-graph cloning, matching,
verification, or invalidation can dominate a seemingly local rewrite.
Performance and memory claims require attributable measurements, not schema
inspection or an asymptotic expectation borrowed from another representation.

No new speedup, memory advantage, scaling law, or benchmark result is claimed
here.

## 10 Limitations and open obligations

QHDG's principal weakness is the gap between its explicit contracts and a
complete, compositional account of how to establish them. The preceding
requirements rule out important unsound shortcuts, but do not yet supply all
transfer rules, executable checks, or preservation proofs. More graph metadata
can expose an obligation without discharging it.

The following areas distinguish design gaps from demonstrated implementation
defects. Each identifies a failure risk, required refinement, and evidence
needed for closure. None is marked complete by the notation, a certificate
schema, or passing examples alone. The practical goal is a defensible guarantee
for a declared program subset and observation contract, not an unqualified
claim that arbitrary hybrid compilation is correct.

### 10.1 Semantic completeness and contextual equivalence

The instrument in Section 5.3 covers a first-order, finite-outcome fragment.
It does not define infinite observable execution, general recursion, or
phase-sensitive callable substitution. Without those definitions, two
transformations can agree on terminating outputs yet differ in progress,
failure, output prefixes, or behavior under coherent control.

Required refinements are:

- Define execution and observation semantics for each admitted construct,
  including probabilistic traces, divergence, failure, and lifetime events.
  State how hidden outcomes and resources are projected and how observation
  contracts compose across enclosing contexts.
- Separate channel equivalence from exact-unitary equivalence. State which
  contexts may coherently control a rewritten operation and how its phase
  convention is preserved.
- Define which effects are observable under each target model. Keep ideal
  logical identities separate from claims about noise, timing, routing, or
  physical resource reuse.

Closure requires a preservation statement with explicit inputs, contexts,
observations, and assumptions for the supported fragment, together with its
proof obligations. Counterexamples must exercise lost classical-quantum
correlations, unused measurements, coherent control of phase-different
unitaries, and output blocked by divergence. Finite differential tests provide
supporting evidence, not a replacement for the contextual argument.

### 10.2 Executable authority, alias, and cleanup rules

Sections 3.4 and 4 specify necessary resource invariants, but feasible-path
quantification and the authority judgment are not algorithms. Dynamic indices,
slices, branch-dependent allocations, and repeated loop allocations complicate
both identity and accounting. An unsound approximation can duplicate authority,
reuse a stale view, or discard an obligation on a less common exit. An overly
conservative one can reject useful source programs.

Required refinements are:

- Supply operation transfer functions for capability versions, resource
  identities, owner epochs, borrows, partitions, and cleanup facts. Specify
  fresh lifetime identity across repeated execution of an allocation site,
  not only distinct static allocation nodes.
- Define a decidable conservative checking discipline for alternative paths
  and carried invariants. Unknown feasibility must not remove an exit from
  checking; unknown aliasing must select an explicit supported policy.
- Specify runtime alias-check success and failure behavior, including effects,
  cleanup, and lowering. A runtime check must establish the precondition it
  guards, not merely defer an unspecified obligation.
- Define restoration and release policies for dirty or entangled resources,
  escape, and abnormal exits. Restoration must preserve the required
  correlations with an untouched reference, not just a local marginal state
  or a cleanup label.

Closure requires preservation of ownership and permission invariants by each
admitted transition and by region composition. Positive and rejection tests
must cover a branch that drops authority, overlapping slices, repeated gate
operands, stale borrows after transfer, allocation-location reuse, failed alias
checks, and cleanup on abort. The distinction between a fresh capability
version and a changed ownership epoch must survive those checks.

### 10.3 Composition of dependencies and regional execution

Separating four relations improves expressiveness but increases consistency
obligations. Independently well-formed value, authority, effect, and control
relations do not establish that their combination has the intended execution.
An omitted completion dependency can permit output before divergence; an
incorrect cross-domain independence assumption can permit invalid motion.
Adding every possible ordering edge avoids some risks but defeats the purpose
of exposing independence.

Required refinements are:

- Define how the four relations jointly enable an operation and how region
  guards, exits, and completion constrain that execution. Check combined
  ordering constraints, not only each relation in isolation; distinguish
  declared recurrence from an accidental dependency cycle.
- Specify callable, recursive, handler, stage, and residual-control interfaces,
  including captured authority, invocation multiplicity, effect summaries,
  abnormal exits, and carried invariants. Unknown calls need conservative
  contracts, not assumed purity or totality.
- Give compositional rules for effect conflicts that cross nominal domains
  and for joins whose alternatives carry different cleanup or escape facts.
  State which summaries are exact and which are conservative approximations.
- Establish conditions for lifting local rewrite arguments through enclosing
  regions. Interfaces, reachability, and execution multiplicity must be
  preserved; adjacency in a displayed graph is not sufficient.

Closure requires a regional composition argument connecting local checks to
whole-program obligations, plus nested-control counterexamples. These should
include an untouched branch, early abort, a loop whose condition and body have
different execution counts, and an apparent inverse pair separated by a
backedge. Unsupported compositions must be rejected explicitly rather than
accepted because their node kinds exist.

### 10.4 Trusted checking, mutation, and analysis validity

Section 8 separates assurance levels, but a certificate is only as strong as
the facts its checker reconstructs. Operation declarations, observation
policies, proof discharge, and analysis results can all introduce assumptions.
Transactional publication prevents partial edits; it does not prevent an
atomically published semantic error. Likewise, an incomplete invalidation
footprint can let a later rewrite consume evidence that was valid before an
earlier edit.

Required refinements are:

- Enumerate the trusted components and assumptions for each assurance level:
  operation meanings, primitive identity laws, structural checks, semantic
  discharge, and any external evidence checker. Keep caller-assumed rewrites
  visibly distinct from independently checked ones.
- Bind evidence to the actual graph state, patch, ordered boundary,
  operation contracts, observation policy, and analysis inputs. Contract or
  policy changes must not leave old certificates usable.
- Define and check footprint completeness, transitive invalidation, identity
  retirement, and rollback behavior. Specify how evidence obtained before
  another mutation is detected as stale.
- Require each rewrite family to reconstruct its permitted edit and semantic
  side conditions. A registered corpus name, proof-method label, or digest
  must not stand in for discharged evidence.

Closure requires a per-rule assurance account and adversarial rejection tests
for swapped operand positions, undeclared boundary changes, stale certificates,
missed invalidations, and invalid candidates. Compare incremental analyses with
fresh recomputation after edit sequences. State which trusted assumptions
remain outside verification; the existence of a test corpus does not reduce
that trusted boundary by itself.

### 10.5 Construction, staging, and target coverage

A sound internal rewrite is insufficient if construction omitted source
semantics or destruction cannot express its result. Section 6 distinguishes
target projections, but a list of possible targets is not a compatibility
contract. In particular, a return path that edits existing instructions need
not support synthesized operations, new regions, or changed resource
lifetimes.

Required refinements are:

- Publish a support matrix for each producer, transformation family, and
  return path. Include control constructs, symbolic parameters, aggregates,
  allocation, captures, effects, exits, and generation/residual boundaries.
  Distinguish representable, verifiable, optimizable, and lowerable features.
- Specify admission and refusal behavior at each boundary. Opaque retention
  or escape is valid only when the chosen return path preserves its semantics;
  otherwise decline before publishing an unusable transformation.
- Establish source-to-graph and graph-to-target preservation separately.
  Preserve evaluation order where observable, failure, global phase, lifetime
  declarations, and provenance. Staging must not execute residual measurement
  or feedback during generation.

Closure requires no-optimization round trips before optimized round trips,
followed by producer-to-target tests for every claimed supported combination.
Use an independently interpreted source or target semantics where possible;
agreement between two paths that share the same mistaken lowering is weak
evidence. Unsupported combinations need explicit refusal tests. A proof for
one projection must not be reported as coverage for another.

### 10.6 Analysis precision, optimization utility, and cost

Conservative handling of aliases, effects, entanglement, and unknown calls is
necessary for soundness but may leave few useful optimization opportunities.
Conversely, a more precise analysis can cost more than the work it enables.
Section 9 defines measurement categories without establishing a practical
coverage or scaling result.

Required refinements are:

- Define abstract domains and sound transfer, join, and loop-summary rules
  for imported entanglement and other semantic analyses. State what they
  cannot prove, especially separability, classical correlations, and cleanup.
  Any widening or precision limit must preserve conservative results.
- Measure rejection reasons and missed opportunities on representative hybrid
  programs, not only successful rewrites on straight-line circuits. Separate
  unsupported semantics from conservative-analysis refusals and exhausted
  optimization budgets.
- Attribute construction, analysis, matching, discharge, cloning, verification,
  invalidation, and destruction costs. Compare incremental work with full
  recomputation under identical semantic and checking obligations.
- Set workload-specific acceptance thresholds before measurement and report
  memory, repeatability, graph shape, and total compilation cost. Include
  sparse edits, nested control, aggregate resources, and refusal-heavy inputs.

Closure requires reproducible evidence of useful coverage and acceptable cost
for the declared use cases. Rewrite counts alone do not show end-to-end value;
local edit size alone does not show incremental scaling. Greater precision,
additional caching, or a new storage layout should be justified by those
measurements rather than treated as an automatic improvement.

### 10.7 Priorities and readiness criteria

The work should proceed in dependency order:

1. Fix the initial supported fragment, operation meanings, observation contract,
   and producer/target combinations. Identify unsupported features explicitly.
   This bounds the claim that subsequent checks and proofs must establish.
2. Make authority transfer, cleanup, dependency composition, and boundary
   preservation precise for that fragment. Establish conservative admission
   and refusal before widening optimization coverage.
3. Discharge each enabled rewrite's obligations against those contracts.
   Exercise counterexamples, multi-edit invalidation, and end-to-end
   construction/optimization/destruction, not only isolated graph identities.
4. Measure precision, utility, and cost, then refine analyses and mutation
   mechanisms without weakening assurance. Extend recursion, handlers,
   dynamic ownership, and other features only with their own closure evidence.

A supported fragment is ready for a stated use only when its claim can be
traced through semantics, operation and region contracts, checking or explicitly
trusted assumptions, boundary preservation, and reproducible validation.
Every unresolved item needs a bounded impact and an enforced exclusion or
conservative policy. Maintain that status per feature and rewrite family,
including the responsible owner and links to the contract, checker, proof or
assumption, tests, and remaining counterexamples. A global "verified" label
would obscure these different assurance levels.

Passing regression tests is necessary implementation evidence, not proof of
arbitrary program equivalence. Any formal result must identify the fragment
and actual construction, rewrite, or lowering function it covers, and explain
its connection to the implementation. A model-only theorem does not by itself
verify an independently implemented compiler path.

QRVSDG's zones, transport, QEC, noise, and physical/pulse layering [4-6] remain
valuable integration directions. They are not prerequisites for a deliberately
logical, noiseless fragment, nor are they covered by its guarantees. Add them
through explicit target semantics and observation policies, with separate
preservation and evaluation evidence. Open proof work cannot be discharged by
renaming a quantum state edge an authority edge.

### 10.8 Priority research checklist

The first research package should establish execution and resource semantics
for a minimal hybrid fragment, rather than attempt to prove the entire proposed
representation at once. The following items remain open. Closing an item
requires the stated evidence and a technical review, not merely a description
of an intended checker.

- [ ] R1: Specify the supported semantic core and legal execution. Choose the
      initial operations, region kinds, producer/target combinations, and excluded
      features. Define how value, authority, effect, and control dependencies
      jointly enable execution, including guards, completion, and recurrence.
      State the observation contract and distinguish joint classical-quantum
      preservation from phase-sensitive callable equivalence. Close with precise
      semantics for that fragment, explicit preservation obligations, and
      counterexamples covering measurement correlations, failure, divergence, and
      invalid speculation (Sections 10.1 and 10.3).
- [ ] R2: Make resource accounting executable. Supply operation and interface
      transfer rules for views, capabilities, borrows, epochs, allocation identity,
      and cleanup. Resolve the initial policy for unknown aliases and abnormal
      exits. Close with a conservative checking discipline, an invariant-preservation
      argument, and accepted/rejected examples for overlapping resources, stale
      permissions, branch-local obligations, loop-carried authority, and restoration
      of dirty or entangled resources (Section 10.2).
- [ ] R3: Establish one complete producer-to-target path. Record which features
      are representable, verifiable, optimizable, and lowerable. Establish
      no-optimization round-trip preservation before enabling a small rewrite set.
      Close with the support matrix, boundary-preservation arguments, independently
      grounded semantic comparisons, and explicit refusal tests. Agreement between
      paths sharing the same lowering does not suffice as independent evidence
      (Section 10.5).
- [ ] R4: Define the trusted checking and mutation boundary. Identify which
      operation laws and analysis facts are proved, checked, or assumed. Bind
      certificates to the actual graph, patch, ordered interface, contracts, and
      observation policy. Close with a per-rule assurance account, rejection of
      wrong-path or stale evidence, and agreement between incremental analysis and
      fresh recomputation after edit sequences (Section 10.4).
- [ ] R5: Audit related-work and inherited guarantees in parallel. Compare
      RVSDG, Sea of Nodes, HUGR, and QHDG by their actual execution, resource, and
      transformation contracts. Preserve the RVSDG/QRVSDG lineage in Section 2;
      shared dependency-graph ideas do not establish direct derivation from Sea of
      Nodes. Close with primary-source-backed distinctions and qualified claims:
      graph acyclicity does not prove rewrite termination, and structural
      normalization is not a universal quantum-equivalence procedure (Section 8.4).

R1 and R2 must be developed together: execution determines when authority is
consumed, while resource rules constrain legal execution. R3 begins with an
unoptimized path; its checked optimization claims depend on R4. R5 can proceed
alongside this work. Broader optimization coverage, storage tuning, and
physical/noise/QEC extensions should not delay agreement on the initial
semantic contract. Tests support these arguments but do not replace them.

The Sea of Nodes comparison has a concrete engineering source. In "Managing
the effect and control chains manually is hard," Mercadier [7] describes
Turbofan's difficulty maintaining the two chains across branches and links to
a merged correction [8]. That patch changes the inputs of a
`TransitionElementsKind` node from `receiver, effect, control` to
`receiver, this_effect, this_control`, using the current path's chains rather
than the outer inputs. The change is titled "fix missing transitions causing
deopt-loop." The account also acknowledges that better Turbofan helpers could
have simplified chain management.

This is implementation evidence for investigating wrong-path dependencies,
mutation helpers, and checking in R1 and R4, not evidence that QHDG has the same
defect or must adopt a CFG representation. The QHDG-specific question is whether
its region interfaces and checked edits prevent analogous errors without
unnecessarily serializing independent work.

## 11 Conclusion

QHDG is a typed hierarchical dependence representation for hybrid
quantum-classical programs. RVSDG supplies its regional framework, value/state
dependence, structural control, and demand-oriented transformation patterns.
QRVSDG supplies the immediate quantum adaptation: linear quantum flow,
measurement-aware composition, quantum region properties, and quantum
analysis and optimization concerns.

QHDG develops that lineage by separating copyable values, resource authority,
hard-effect order, and explicit control/exit reachability. Authority state,
cleanup, staging, observation contracts, and checked mutation refine how those
relations can be transformed safely.

The resulting distinction is not hierarchy versus no hierarchy, or multiple
states versus a mandatory single token. It is a choice of explicit semantic
relations and obligations. Their value depends on how precisely they constrain
construction, analysis, optimization, and destruction, and on the evidence
that those stages preserve the intended behavior.

## References

1. Nico Reissmann, Jan Christian Meyer, Helge Bahmann, and Magnus Sjalander.
   [RVSDG: An Intermediate Representation for Optimizing Compilers](https://arxiv.org/abs/1912.05036).
   Original regional dependence model; see especially Sections 2.4, 4, 5, 6,
   and 7.1. A [local copy of the paper](qrvsdg/1912.05036v2.pdf) is also available.
2. [QRVSDG: A Regionalized Value State Dependence Graph for Hybrid Quantum-Classical Compilation](qrvsdg/qrvsdg_paper.md).
   Design manuscript; especially Sections 4.1-4.4, 5, 6, 8.1, and 10.3.
3. [QRVSDG Addendum: Consolidated Algorithms for Classical and Quantum Compilation](qrvsdg/qrvsdg_addendum_algorithms.md).
   Companion algorithm specification, including classical inheritance and
   quantum construction, destruction, CNE, and DNE.
4. [QRVSDG Addendum: Optimization Algorithms from Classical Compiler IRs](qrvsdg/qrvsdg_addendum_optimization_algorithms.md).
   Proposed adaptations of classical optimization patterns and compound-aware
   transformations.
5. [QRVSDG Addendum: Quantum Optimizations from HUGR and Qiskit](qrvsdg/qrvsdg_addendum_quantum_optimizations.md).
   Quantum rewrite, verification, allocation, and scheduling proposals.
6. [Toward a Universal Intermediate Representation for Hybrid Quantum-Classical Programs: Design Principles, Qubit Zone Management, and Multi-Level Compilation](qrvsdg/qrvsdg_ir_design_paper.md).
   Design study; especially Sections 5, 6, and 8 on zones, layered integration,
   and compilation boundaries.
7. Darius Mercadier.
   [Land ahoy: leaving the Sea of Nodes](https://v8.dev/blog/leaving-the-sea-of-nodes#managing-the-effect-and-control-chains-manually-is-hard).
   V8, 2025-03-25. The linked section, "Managing the effect and control chains
   manually is hard," gives the engineering account and points to the fix in
   reference [8]; it is not an evaluation of QHDG.
8. V8 project.
   [Fix missing transitions causing deopt-loop](https://chromium.googlesource.com/v8/v8/+/c573bd216aa4f7649e9b7e54a3134f33920ac983%5E%21/).
   Commit `c573bd216aa4f7649e9b7e54a3134f33920ac983`,
   [reviewed change 1749902](https://chromium-review.googlesource.com/c/v8/v8/+/1749902),
   merged 2019-08-13. Corrects effect/control inputs in
   `src/compiler/js-native-context-specialization.cc`.

## Draft status

**AI-assisted design synthesis for human technical review. Definitions and
examples are original exposition with conceptual influences attributed above.
No completed formal proof, new empirical result, or publication acceptance
is implied.**
