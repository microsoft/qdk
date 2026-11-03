# QRVSDG Addendum: Quantum Optimizations from HUGR and Qiskit

**Companion to:** *QRVSDG: A Regionalized Value State Dependence Graph for Hybrid Quantum-Classical Compilation*

**Source Inspiration:**

- HUGR (Hierarchical Unified Graph Representation) / TKET2: Badger rewriting, linear type verification, constant folding, graph rewrite system, extension/plugin system
- Qiskit Transpiler: basis translation, gate cancellation, unitary synthesis/consolidation, routing/layout, scheduling, optimization levels, template optimization
- [1] Reissmann, Meyer, Bahmann, Själander. "RVSDG: An Intermediate Representation for Optimizing Compilers." ACM TECS 19(6), 2020.
- [2] QRVSDG paper (this work).
- [3] Quantinuum, "HUGR: Hierarchical Unified Graph Representation," 2024.
- [4] Javadi-Abhari et al., "Quantum computing with Qiskit," 2024.

## Overview

This addendum extends the QRVSDG algorithm set with nine core quantum optimization algorithms (XIV–XXII) adapted from quantum-specific optimization frameworks, plus dynamic/runtime and reasoning algorithms (XXIII–XXV), a quantum loop unrolling algorithm (XXVI), and an extension path for Algorithm XIII-B. Each algorithm maps a concrete pass from HUGR/TKET2 or Qiskit's transpiler to the QRVSDG's hierarchical data-flow representation.

Numbering note: this addendum uses an independent local namespace (XIV-XXVI). The optimization algorithms addendum separately uses XIV-XXXIX; cross-document references should be disambiguated by document context.

The algorithms are organized in seven groups:

1. **Verification and Infrastructure** (Algorithms XV, XVII): Type checking and subgraph replacement machinery.
2. **Rewriting and Synthesis** (Algorithms XIV, XVI, XVIII, XXII): Equality saturation, constant folding, basis translation, template optimization.
3. **Gate-Level Optimization** (Algorithm XIX): Cancellation and commutation.
4. **Physical Compilation** (Algorithms XX, XXI): Unitary consolidation and scheduling.
5. **Dynamic Qubit Management** (Algorithms XXIII, XXIV, XIII-B): Lifetime analysis, qubit reuse, and dynamic QIR destruction.
6. **Assertion-Based Reasoning** (Algorithm XXV): Hoare-style optimization.
7. **Quantum Loop Optimization** (Algorithm XXVI): Selective quantum-aware loop unrolling.

Additionally, a **Dynamic Qubit Allocation** section formalizes how dynamic allocation maps to the QRVSDG IR.

## Notation

All notation from the consolidated algorithms addendum applies. Additional notation:

| Symbol | Meaning |
|--------|---------|
| $\mathcal{E}$ | ECC (Equivalence Circuit Class) rule set |
| $\mathcal{G}_\mathcal{R}$ | E-graph for region $\mathcal{R}$ |
| $\mathcal{B}$ | Target basis gate set |
| $\mathcal{L}$ | Equivalence library (decomposition graph) |
| $\mathcal{I}$ | Inverse gate pairs $\{(g, g^{-1})\}$ |
| $\mathcal{K}$ | Commutation rules |
| $\mathcal{T}$ | Template library $\{(P_i, R_i, \Delta_i)\}$ |
| $B$ | Saturation budget |
| $d(n)$ | Gate duration (from δ-node) |
| $t_{\text{start}}(n)$ | Scheduled start time of node $n$ |
| $\text{Array}(\tau, n)$ | Homogeneous compound type: $n$ elements of type $\tau$ |
| $\text{Tuple}(\tau_1, \ldots, \tau_k)$ | Heterogeneous compound type: ordered sequence of typed fields |
| $\text{Struct}(\text{name}, \{f_i : \tau_i\})$ | Named record type with labeled fields |
| $\text{contains\_quantum}(\tau)$ | Predicate: true iff $\tau$ transitively contains a Qubit-typed component |
| $\text{linear}(\tau)$ | Predicate: true iff $\tau$ contains quantum data requiring single-use constraints |
| $\text{contents}(\text{arr})$ | Identity-to-container mapping: qubit identities within compound value |
| $\text{SROA}$ | Scalar Replacement of Aggregates: decomposes compound types to scalar edges |
| $\text{X-DFE}$ | Dead Field Elimination: compound-aware variant of DNE |
| $\text{VIII-CE}$ | Compound Elimination: destruction-time lowering of compound operations |

---

## Dynamic Qubit Allocation and IR Mapping

### Allocation Models

**Static allocation** (current default): all qubits allocated at program start with a fixed, compile-time-known count. In QIR: `dynamic_qubit_management = false`, `required_num_qubits` declared as entry-point attribute. Qubits identified by constant integer indices.

**Dynamic allocation**: qubits allocated and deallocated during execution, allowing the qubit count to vary over time. The maximum number of simultaneously live qubits may be less than the total allocated over the program's lifetime. Hardware requirements: qubit pool manager, dynamic reset capability, mid-circuit measurement, classical control.

### Current IR Summary

| Feature | QIR | OpenQASM 3 | HUGR | Qiskit | QRVSDG |
|---------|-----|------------|------|--------|--------|
| Static allocation | Yes (default) | Yes (only) | N/A (extensions) | Yes (only) | Yes (default) |
| Dynamic allocation flag | `dynamic_qubit_management` | No | Via extensions | No | Via Algorithm XIII |
| Reset/reuse | Adaptive Profile | `reset q;` | Extension op | `circuit.reset()` | Reset node |
| Qubit scoping | Global | Global | Region-scoped (linear) | Global | Region-scoped (Prep in regions) |
| Linear typing | No | No | Yes | No | Yes |

### QRVSDG Representation of Dynamic Allocation

The existing Prep + MeasureD + Reset model already captures dynamic allocation semantics:

- **Allocation:** Prep node inside any region (θ-body, γ-region, λ-body).
- **Deallocation:** MeasureD consumes the qubit; Measure + Reset enables reuse.
- **Scoping:** Linear typing bounds qubit lifetimes—each qubit must be consumed exactly once within its region's scope.

**Key insight:** Prep inside a θ-node body represents dynamic allocation: each loop iteration creates a new qubit.

**Recommended extension — explicit Dealloc node:**

$$\text{Dealloc} : (\text{Qubit}, \text{Eff}) \to (\text{Eff})$$

Explicitly releases a qubit back to the pool without measuring it. Semantically defined as tracing out the qubit and returning it to the allocation pool. Differs from MeasureD in that it does not produce a classical Bit. Can be lowered to MeasureD-with-discarded-result during destruction. Useful for uncomputation patterns: allocate ancilla → entangle → unentangle → deallocate.

### Prep Inside Structural Nodes

**Prep inside θ-nodes (loops):** Each iteration allocates a fresh ancilla, uses it, and deallocates it. The number of simultaneously live qubits depends on program point, not total iterations. This is classic dynamic allocation.

**Prep inside γ-nodes (conditionals):** A qubit is allocated only in one branch. The qubit must be consumed within that branch unless all branches produce matching Qubit exit variables.

**Prep inside λ-nodes (functions):** Function-scoped allocation. The caller does not see the internal ancilla—the λ-node's signature shows only input/output qubits.

### Qubit Lifetime Analysis

A qubit's lifetime is the interval from Prep to MeasureD/Dealloc/region exit. In the QRVSDG, the linear typing constraint means each Qubit output has exactly one user. Following the chain:

$$\text{Prep} \to q_0 \to \text{UGate}_1 \to q_1 \to \text{UGate}_2 \to q_2 \to \cdots \to q_n \to \text{MeasureD/Dealloc}$$

**Formal lifetime bound:** For a Prep node $p$ in region $\mathcal{R}$: if the chain terminates within $\mathcal{R}$, lifetime is contained within $\mathcal{R}$. If the chain flows out as a result, lifetime extends to the enclosing region.

**Register allocation applicability:** Classical algorithms (graph coloring, linear scan) apply directly to qubit lifetime intervals for mapping virtual qubits to physical indices. Key constraint: qubits cannot be spilled (no-cloning). If the chromatic number exceeds device qubit count, the program cannot execute.

### Impact on `required_num_qubits`

**Static:** `required_num_qubits` = total Prep count.

**Dynamic:** `required_num_qubits` = $\max_t |Q_{\text{live}}(t)|$ (peak live count), computed from qubit lifetime analysis. For programs with Prep inside θ-nodes with unbounded iteration count, peak live count is still bounded by the maximum within a single iteration body.

### Region Annotation Constraints

**Unitary regions ($R_U$) cannot dynamically allocate.** A Prep node inside $R_U$ would violate the qubit-count-preserving invariant $|A_q| = |R_q|$. Dynamic allocation is restricted to hybrid regions ($R_H$). Error-corrected regions ($R_{EC}$) may internally allocate syndrome qubits via nested hybrid sub-regions.

---

## Group 1: Verification and Infrastructure

### Algorithm XV: Linear Type Verification

*Source: HUGR linear type enforcement. Formalizes the QRVSDG linear typing invariant verification.*

HUGR enforces linear types on quantum wires: a source port with non-`CopyableType` must have exactly one edge. This structural invariant is checked during HUGR validation. The QRVSDG specifies $\forall o \in O_q(n) : |\text{users}(o)| = 1$ but has not formalized a complete verification algorithm for complex nested structures.

**Input:** QRVSDG $G$. **Output:** Valid / Invalid with error location.

**1. Recursive verification:** Process the ω-region. For each region $\mathcal{R} = (A, N, E, Res)$:

**2. Edge-level check:** For every edge $e \in E$:
- If $\text{type}(e) = \text{Qubit}$: verify $|\text{users}(\text{origin}(e))| = 1$.
- If $\text{contains\_quantum}(\text{type}(e))$: verify $|\text{users}(\text{origin}(e))| = 1$ (compound-level check sufficient per Compound Linearity Sufficiency Theorem, see [2] §4.1).

**3. Per-node check:** For each node $n \in N$:

- **Simple node (UGate, Measure, Prep, Reset, Barrier):**
  - Verify each quantum output $o \in O_q(n)$ has exactly one user.
  - Verify each quantum input $i \in I_q(n)$ has exactly one origin.

- **Compound-type node (ArrayConstruct, ArrayDestruct, ArraySwap, ArrayApplyGate, ArrayMultiGate, TupleConstruct, TupleExtract, StructConstruct, StructExtract):**
  - For each output $o$ where $\text{contains\_quantum}(\text{type}(o))$: verify $|\text{users}(o)| = 1$.
  - Verify the closed-set invariant: (1) construction consumes all element inputs, (2) extraction outputs satisfy $|\text{users}(o)| = 1$, (3) no operation produces a duplicate of a sub-element.
  - Flag any user-defined node type that consumes or produces compound types containing quantum data as a potential theorem violation requiring manual review.

- **γ-node:** For each entry variable $ev = (i, A)$ where $\text{type}(i) = \text{Qubit}$:
  - Verify external input $i$ has one origin.
  - Verify each region argument $a^{R_j}$ has exactly one user within $R_j$.
  - For each exit variable $ex = (R, o)$ where $\text{type}(o) = \text{Qubit}$: verify each region result $r^{R_j}$ has one origin and output $o$ has one user.
  - Recursively verify each γ-subregion.

- **θ-node:** For each loop variable $lv = (i, a, r, o)$ where $\text{type}(i) = \text{Qubit}$:
  - Verify input $i$, argument $a$, result $r$, and output $o$ each satisfy linearity.
  - Verify result-to-argument feedback preserves linearity.
  - Recursively verify the θ-region.

- **λ-node:** For each quantum argument and context variable: verify linearity. For unitary λ-nodes ($\rho = R_U$): verify $|A_q| = |R_q|$ (qubit conservation). Recursively verify the λ-region.

- **φ-node:** Standard linearity checks on recursion variables. Recursively verify the φ-region.

- **ω-node:** Recursively verify the ω-region.

**4. Transitive consumption check:** For each quantum argument $a$ in any region: trace the chain of consumers. Verify the chain terminates at a region result, Measure/MeasureD, or Reset. If the chain dead-ends, report a no-deleting violation.

**Region annotation interaction:**
- $R_U$: additional $|A_q| = |R_q|$ check.
- $R_H$: MeasureD legally terminates chains; Measure continues them.
- $R_{EC}$: LogicalQubit-typed edges obey the same linearity as Qubit edges. Logical qubit count preservation: $|A_{\text{logical}}| = |R_{\text{logical}}|$. Verify Encode/Decode boundary nodes have matching physical qubit counts per [2] §4.3.

**Conjugate node verification:** Verify $\mathcal{R}_{\text{outer}}$ region is annotated $R_U$ (required for adjoint generation). Verify through-qubits thread linearly through all three phases (outer, inner, adjoint). Verify ancilla-qubit outputs satisfy linearity. See [2] §4.2.1.

**Complexity:** $O(N + E)$.

---

### Algorithm XVII: Quantum Subgraph Replacement

*Source: HUGR Grs (SimpleReplace/Replace). Adapts HUGR's subgraph rewriting for QRVSDG with linear typing.*

HUGR provides `SimpleReplace` (replaces convex subgraphs of leaf nodes with replacement HUGRs of matching signature) and `Replace` (general replacement supporting container nodes with child rewiring). The key constraint is **convexity**: every node on every path between two nodes in the replacement set must also be in the set.

**Input:** QRVSDG $G$, convex subgraph $S$ in region $\mathcal{R}$, replacement subgraph $S'$ with matching signature.

**Preconditions:**
- $S$ is convex within $\mathcal{R}$.
- Quantum input/output signatures match: $I_q(S) = I_q(S')$, $O_q(S) = O_q(S')$.
- Classical signatures match.
- $S'$ satisfies internal linear typing (verified by Algorithm XV).

**Procedure:**

**1. Disconnect inputs:** For each input $i$ of $S$ connected to external origin $o_{\text{ext}}$: record $(o_{\text{ext}}, i)$.

**2. Disconnect outputs:** For each output $o$ of $S$:
- Quantum: record single user $(o, u_{\text{ext}})$.
- Classical: record all users.

**3. Insert replacement:** Add $S'$ to region $\mathcal{R}$.

**4. Rewire inputs:** Connect external origins to corresponding inputs of $S'$.

**5. Rewire outputs:** Connect $S'$'s outputs to recorded external users.

**6. Remove:** Delete all nodes in $S$ and their internal edges.

**7. Post-verify:** Run Algorithm XV on the modified region.

**Linearity constraints for patterns:**
- Patterns must respect qubit wire identity: matching qubit wire $q_0$ through gates $G_1, G_2$ must match the entire chain.
- Overlapping matches on the same qubit wire are mutually exclusive.
- Non-overlapping matches on independent qubit wires can be applied simultaneously.

**Complexity:** $O(|S| + |S'|)$ for replacement. Pattern matching: $O(|P| \cdot |R|)$ per pattern.

---

## Group 2: Rewriting and Synthesis

### Algorithm XIV: Region-Scoped Equality Saturation

*Source: TKET2 Badger + QUESO/GUOQ. Extends Section 8.2 of the QRVSDG paper.*

TKET2's Badger uses pre-computed Equivalence Circuit Classes (ECCs) generated by Quartz superoptimization. Each ECC contains circuits implementing the same unitary. The optimizer uses `PatternMatcher` for subgraph matching, `RewriteStrategy` (greedy or exhaustive), backtracking search with priority queue, and circuit hashing to avoid revisiting states.

The QRVSDG's equality saturation framework provides a more principled approach than Badger's backtracking search: region-scoped e-graphs control exponential blowup, and ILP-based extraction with linearity constraints ensures no-cloning compliance.

**Input:** QRVSDG $G$, ECC rule set $\mathcal{E}$, cost function $\mathcal{C}$, saturation budget $B$.

**1. Rule compilation:** For each ECC $E_i = \{C_1, \ldots, C_k\} \in \mathcal{E}$:

- For each circuit $C_j$, extract the QRVSDG subgraph representation (sequence of UGate nodes with quantum/classical edges).
- Generate bidirectional rewrite rules: $\forall (C_j, C_l) : C_j \Leftrightarrow C_l$.
- Verify each rule preserves linear typing: $|I_q| = |O_q|$ and qubit identity mapping is consistent.

**2. Region enumeration:** Collect all regions $\mathcal{R}$ with $\rho(\mathcal{R}) = R_U$ (unitary regions), ordered leaf-to-root in the hierarchy.

**3. Per-region saturation:** For each unitary region $\mathcal{R}$:

**(a)** Initialize e-graph $\mathcal{G}_\mathcal{R}$ from the current subgraph of $\mathcal{R}$.

**(b)** Repeat until saturation or budget $B$ exhausted:
- Match all rewrite rule LHS patterns against $\mathcal{G}_\mathcal{R}$.
- For each match, add the RHS as an equivalent e-class member.
- Rebuild e-graph (merge congruent e-classes).

**(c)** Extract optimal circuit from $\mathcal{G}_\mathcal{R}$ using cost function $\mathcal{C}$ with ILP linearity constraints:
- $\forall q \in \text{Qubit edges}: q$ produced exactly once and consumed exactly once.
- Minimize $\sum_{n \in \text{selected nodes}} \mathcal{C}(n)$.

**(d)** Replace the region's subgraph with the extracted optimal circuit.

**4. Cross-region opportunities:** After all unitary regions are optimized, run CNE (Algorithm IX) and DNE (Algorithm X).

**Region annotation interaction:**
- $R_U$: full ECC saturation (all gates unitary, rules preserve reversibility).
- $R_H$: restrict saturation to unitary subcircuits between measurements.
- $R_{EC}$: saturate logical operation independently from syndrome/correction circuits.
- **Conjugate nodes**: The outer region ($R_U$) and inner region are saturated independently. After saturation, check for Conjugate identity elimination ($U \cdot I \cdot U^\dagger = I$) and Conjugate fusion (adjacent Conjugate nodes with congruent outer regions). Conjugate-specific rewrite rules may be added to the ECC rule set: $C(U, V_1) \cdot C(U, V_2) \Leftrightarrow C(U, V_1 \cdot V_2)$.

**Complexity:** Per region: $O(|\mathcal{E}| \cdot |\mathcal{G}|^p)$ where $p$ is largest pattern size, bounded by budget $B$. ILP extraction is NP-hard in general but practical for region-sized instances.

---

### Algorithm XVI: Quantum Constant Folding

*Source: HUGR constant folding. Extends classical constant folding with quantum state propagation.*

HUGR supports constant folding through `Const` nodes and `LoadConstant` operations. For quantum, this extends to folding known initial states through gates, folding measurement outcomes of known states, and folding gate compositions.

**Input:** QRVSDG $G$.

**1. State annotation:** For each Prep node, annotate its quantum output edge with known state vector ($|0\rangle$).

**2. Forward propagation:** Process nodes in topological order within each region:

- **UGate node with all inputs annotated (state known, parameters constant):**
  - Compute $|\psi'\rangle = U(\text{params}) |\psi\rangle$.
  - If $|\psi'\rangle$ is a computational basis state, annotate output.
  - If $U|\psi\rangle = e^{i\phi}|\psi\rangle$ (eigenstate): gate is a no-op up to global phase. Mark for removal.

- **Measure node with annotated quantum input:**
  - If input is $|b\rangle$: replace Bit output with constant $b$, post-measurement Qubit with $|b\rangle$.
  - Propagate constant bit into γ-node predicates.

- **γ-node with constant predicate:** Replace with selected region's contents.

**3. Gate identity elimination:** For each UGate identified as identity on the specific input state: bypass the node.

**4. Rotation merging:** For consecutive same-axis rotation gates with constant parameters:
- $\theta_{\text{merged}} = \theta_1 + \theta_2 \pmod{2\pi}$.
- If $\theta_{\text{merged}} = 0$: remove both (identity).
- Otherwise: replace with single merged rotation.

**Region annotation interaction:**
- $R_U$: full gate composition folding; state propagation from region arguments if known.
- $R_H$: folding crosses measurement boundaries—measurement folding simplifies γ-node predicates.

**Complexity:** $O(N)$ single topological pass.

---

### Algorithm XVIII: Basis Translation

*Source: Qiskit BasisTranslator. Decomposes UGate nodes into a target basis set.*

Qiskit's `BasisTranslator` searches an `EquivalenceLibrary` for decomposition paths using BFS/Dijkstra. For example, decomposing Toffoli (CCX) into $\{CX, R_z, H\}$ requires 6 CX gates and several single-qubit gates.

**Input:** QRVSDG $G$, target basis set $\mathcal{B} = \{g_1, g_2, \ldots\}$, equivalence library $\mathcal{L}$.

**1. Identify non-basis gates:** Collect all UGate nodes $n$ where $\text{op}(n) \notin \mathcal{B}$.

**2. For each non-basis gate $n$ with $\text{op}(n) = g$:**

**(a) Look up decomposition:** Find the shortest path in $\mathcal{L}$ from $g$ to a sequence using only gates in $\mathcal{B}$.

**(b) Construct replacement subgraph $S'$:**
- Create the decomposition as UGate nodes connected by quantum edges.
- Ensure input/output quantum signature matches $n$'s signature.
- Propagate classical parameter edges through the decomposition.

**(c) Apply subgraph replacement:** Use Algorithm XVII to replace $n$ with $S'$.

**3. Linear typing preservation:** Each decomposition preserves qubit count ($|I_q(n)| = |I_q(S')|$, $|O_q(n)| = |O_q(S')|$) and qubit identity mapping. No fan-out of quantum edges within the decomposition.

**Step 1B: Compound Gate Expansion.** For each $\text{ArrayMultiGate}$ node $m$ where $\text{gate}(m) \notin \mathcal{B}$:

**(a)** Look up decomposition of $\text{gate}(m)$ in library $\mathcal{L}$: $\text{gate}(m) \to (g_1, \ldots, g_r)$ with qubit-operand mappings.

**(b)** For each sub-gate $g_i$ in the decomposition:
- If $g_i$ is a single-qubit gate on target index $j$: emit $\text{ArrayApplyGate}(j, g_i)$.
- If $g_i$ is a multi-qubit gate on indices $\text{indices}_i$: emit $\text{ArrayMultiGate}(g_i, \text{indices}_i)$.

**(c)** Chain emitted nodes through the array edge: $\text{arr} \to n_1 \to n_2 \to \cdots \to n_r$.

**(d)** Apply Algorithm XVII (subgraph replacement) to replace the original $\text{ArrayMultiGate}$ node $m$ with the chained sequence.

**Example:** $\text{ArrayMultiGate}(\text{CNOT}, (2, 5))$ with CNOT decomposition to CZ basis:

$$\text{ArrayApplyGate}(5, H) \to \text{ArrayMultiGate}(\text{CZ}, (2, 5)) \to \text{ArrayApplyGate}(5, H)$$

This preserves compound abstraction and supports late-lowering. Serialization through array threading is not artificial for typical multi-qubit gate decompositions, as sub-gates are genuinely sequential on overlapping qubit sets.

**4. Post-optimization:** Run CNE (Algorithm IX) and DNE (Algorithm X).

**Region annotation interaction:**
- $R_U$: all decompositions must use only unitary gates.
- $R_H$: decompositions may introduce measurement-based implementations.
- $R_{EC}$: basis translation operates at logical gate level.

**Complexity:** $O(N \cdot D)$ where $N$ is non-basis gate count and $D$ is maximum decomposition depth.

---

### Algorithm XXII: Template-Based Optimization

*Source: Qiskit TemplateOptimization. Applies circuit template rewrites via subgraph matching.*

Qiskit's `TemplateOptimization` replaces subcircuit patterns with equivalent shorter implementations. Templates include multi-gate cancellation patterns, algebraic identities, and device-specific optimizations.

**Input:** QRVSDG $G$, template library $\mathcal{T} = \{(P_i, R_i, \Delta_i)\}$ where $P_i$ is pattern, $R_i$ is replacement, $\Delta_i$ is cost delta.

**1. Template compilation:** For each template $(P_i, R_i, \Delta_i)$:
- Convert $P_i$ to QRVSDG subgraph pattern with typed ports.
- Verify $R_i$ has matching quantum/classical signature.
- Verify $R_i$ satisfies internal linear typing.
- Record $\Delta_i = \text{cost}(P_i) - \text{cost}(R_i)$.

**2. Pattern matching:** For each region $\mathcal{R}$:
- Match all templates $P_i$ against nodes in $\mathcal{R}$.
- A match preserves node types, operators, edge connectivity, and qubit wire identity.

**3. Conflict resolution:** When matches overlap:
- Two matches conflict if they share any node.
- Select non-conflicting subset maximizing $\sum \Delta_i$ (greedy approximation).
- QRVSDG linearity means qubit-wire matches are mutually exclusive, reducing conflicts.

**4. Application:** For each selected match, apply Algorithm XVII (subgraph replacement).

**5. Iterate** until no more profitable templates match.

**QRVSDG normalized form advantage:** Strong normalization means fewer template variants are needed. Commuting gates are unordered, so a single template pattern matches all orderings.

**Complexity:** $O(|\mathcal{T}| \cdot |R|^p)$ per region. Conflict resolution: $O(M \log M)$.

---

## Group 3: Gate-Level Optimization

### Algorithm XIX: Quantum Gate Cancellation

*Source: Qiskit CXCancellation, InverseCancellation, CommutativeCancellation. Adapts three cancellation strategies.*

Qiskit provides three cancellation passes of increasing generality. The QRVSDG's demand-dependence automatically exposes many commutation opportunities that Qiskit requires `CommutationAnalysis` to find, but explicit cancellation algorithms are still needed for same-qubit-chain patterns.

**Input:** QRVSDG $G$, inverse pairs $\mathcal{I} = \{(g, g^{-1})\}$, commutation rules $\mathcal{K}$.

**Part A — Adjacent Inverse Cancellation:**

**1.** For each quantum edge chain within a region:

**2.** For each consecutive pair $(n_i, n_{i+1})$ on the same qubit wire:
- If $(\text{op}(n_i), \text{op}(n_{i+1})) \in \mathcal{I}$ and classical parameters match:
  - Single-qubit gates: bypass both, connecting $n_i$'s input to $n_{i+1}$'s output's users.
  - Multi-qubit gates (e.g., CX·CX): bypass on all qubit wires simultaneously.

**3.** Repeat until fixed point.

**Part B — Commutation-Aware Cancellation:**

**4.** Build commutation database from $\mathcal{K}$ and gate properties:
- Diagonal gates commute with each other ($R_z$, CZ, $S$, $T$, $Z$).
- Gates on disjoint qubit sets always commute (implicit in QRVSDG).
- Single-qubit gates commute through multi-qubit gates on non-shared qubits.

**5.** For each node $n_i$, search forward along the qubit chain for $n_j$ where $(\text{op}(n_i), \text{op}(n_j)) \in \mathcal{I}$:
- Verify all intervening nodes commute with $n_i$.
- If so, cancel both.

**Part C — Rotation Merging:**

**6.** For consecutive same-axis rotation gates: $R_z(\theta_1) \cdot R_z(\theta_2) = R_z(\theta_1 + \theta_2)$.
- If sum is $0 \pmod{2\pi}$: cancel both.
- Otherwise: replace with single merged rotation.

**QRVSDG-specific advantages:**
- Demand-dependence exposes many commutation opportunities without explicit analysis.
- Inter-procedural cancellation: if λ-node body ends with gate $G$ and all callers follow with $G^{-1}$, cancel across boundary.
- γ-node boundary cancellation: gates hoisted via Algorithm XI may cancel with adjacent operations.
- **Conjugate boundary cancellation:** The adjoint phase of a Conjugate node auto-generates $U^\dagger$. A gate $G$ immediately after the Conjugate on a through-qubit cancels with the last gate of the adjoint phase if $G = U_{\text{last}}$. Similarly, adjacent Conjugate nodes with identical outer regions yield $U^\dagger \cdot U = I$ cancellations at the junction (Conjugate fusion, see [2] §6.8).

**Extended commutation rules for QRVSDG:**

| Rule | Gates | Condition |
|------|-------|-----------|
| Axis commutation | $R_\alpha(\theta_1), R_\alpha(\theta_2)$ | Same rotation axis |
| Diagonal commutation | Any diagonal pair | Both diagonal in computational basis |
| Control commutation | $R_z$ on control, CNOT | $R_z$ on CNOT's control qubit |
| Target commutation | $X$ on target, CNOT | $X$ on CNOT's target qubit |

**Part D — Compound Commutation Rules:**

When the IR contains compound-type operations (before SROA decomposition), the following commutation rules enable compound-level cancellation and reordering in Phase 2 of the compilation pipeline, before SROA:

| Rule | Operations | Condition | Action |
|------|-----------|-----------|--------|
| Index-disjointness | $\text{ArrayApplyGate}(j, g_1)$, $\text{ArrayApplyGate}(k, g_2)$ | $j \neq k$ | Always commute (independent qubits) |
| Partial overlap | $\text{ArrayMultiGate}(g_1, I_1)$, $\text{ArrayApplyGate}(j, g_2)$ | $j \notin I_1$ | Commute |
| Same index | $\text{ArrayApplyGate}(j, g_1)$, $\text{ArrayApplyGate}(j, g_2)$ | Standard gate commutation for $g_1, g_2$ | Apply scalar commutation rules on qubit $j$ |
| Overlapping multi-gate | $\text{ArrayMultiGate}(g_1, I_1)$, $\text{ArrayMultiGate}(g_2, I_2)$ | $I_1 \cap I_2 \neq \emptyset$ | Apply gate-level commutation on overlapping qubits |
| Disjoint multi-gate | $\text{ArrayMultiGate}(g_1, I_1)$, $\text{ArrayMultiGate}(g_2, I_2)$ | $I_1 \cap I_2 = \emptyset$ | Always commute |

The index-disjointness rule is the primary optimization lever: for an $n$-qubit array, operations on distinct indices are automatically independent, enabling cancellation and reordering without decomposing compound structure. Parts A-C self-inverse cancellation and rotation merging also apply at compound level when operations target the same index (same-index rule delegates to standard gate algebra).

**Complexity:** Part A: $O(N)$ per pass, $O(N^2)$ worst case. Part B: $O(N^2)$. Part C: $O(N)$. Part D: $O(N)$ per pass (index comparison is constant-time).

---

## Group 4: Physical Compilation

### Algorithm XX: Region-Local Unitary Consolidation

*Source: Qiskit Collect2qBlocks + ConsolidateBlocks + UnitarySynthesis. Consolidates gate sequences into optimal decompositions.*

Qiskit's pipeline: `Collect2qBlocks` collects maximal 1q/2q gate sequences → `ConsolidateBlocks` computes overall unitary → `UnitarySynthesis` re-synthesizes using optimal decomposition (e.g., KAK for 2q unitaries, yielding $\leq 3$ CX gates). This often produces shorter circuits than individual gate cancellation.

**Input:** QRVSDG $G$, target basis $\mathcal{B}$, maximum consolidation size $k$ (typically 2 qubits).

**1. Block identification:** Within each unitary region ($R_U$) or unitary subchain of a hybrid region ($R_H$):

**(a)** Identify maximal connected subgraphs of UGate nodes acting on at most $k$ qubits with no intervening measurements or classical control.

**(b)** For each block $B = \{n_1, \ldots, n_m\}$ with qubit set $Q_B$ where $|Q_B| \le k$:
- Verify $B$ is convex in the region's dependency graph.
- Verify $B$ contains no structural nodes.

**2. Unitary computation:** For each block $B$:
- Compute $U_B = U_{n_m} \cdots U_{n_1}$ (matrix product in topological order).
- Requires all gate parameters to be compile-time constants.

**3. Optimal resynthesis:** Decompose $U_B$ into $\mathcal{B}$:
- 1-qubit: Euler decomposition ($R_z \cdot R_y \cdot R_z$) — at most 3 gates.
- 2-qubit: KAK decomposition — at most 3 CX gates + single-qubit rotations.
- $k > 2$: Quantum Shannon Decomposition or optimal synthesis.

**4. Subgraph replacement:** Use Algorithm XVII to replace $B$ with resynthesized circuit.

**5. Cross-region considerations:**
- Do not consolidate across λ-node boundaries (function calls).
- Each γ-subregion consolidated independently.
- If parameters are loop-invariant (Algorithm XII), the consolidated block's description is loop-invariant.

**Interaction with equality saturation:** Complementary: consolidation uses numerical unitary computation for global optimality within a block; equality saturation explores known rewrite rules exhaustively.

**Complexity:** Block identification: $O(N)$. Unitary computation: $O(4^k \cdot m)$ per block. KAK: $O(1)$ for $k = 2$.

---

### Algorithm XXI: QRVSDG-Aware Scheduling

*Source: Qiskit ALAPSchedule, ASAPSchedule, PadDynamicalDecoupling. Extends demand-dependence with hardware timing.*

The QRVSDG's demand-dependence automatically provides ASAP scheduling: a topological sort yields a valid schedule where each node executes as soon as inputs are available. The longest path gives the critical path. This algorithm adds hardware-aware timing, crosstalk avoidance, and dynamical decoupling.

**Input:** QRVSDG $G$, hardware timing model $T$ (from δ-node), scheduling policy $P \in \{\text{ASAP}, \text{ALAP}\}$.

**1. Duration annotation:** For each UGate node $n$, look up duration $d(n)$ from δ-node calibration data.

**2. ASAP schedule (forward pass):**
- For each node $n$ in topological order: $t_{\text{start}}(n) = \max_{i \in I(n)} \{t_{\text{end}}(\text{origin}(i))\}$ where $t_{\text{end}}(m) = t_{\text{start}}(m) + d(m)$.

**3. ALAP schedule (backward pass):**
- For each node in reverse topological order: $t_{\text{start}}(n) = \min_{u \in \text{users}(O(n))} \{t_{\text{start}}(u)\} - d(n)$.

**4. Slack computation:** $\text{slack}(n) = t_{\text{ALAP}}(n) - t_{\text{ASAP}}(n)$. Zero-slack nodes are on the critical path.

**5. Crosstalk-aware scheduling:** If δ-node provides a crosstalk map:
- For each pair of simultaneously scheduled gates with crosstalk: add Order edge or adjust schedule.
- Prefer delaying non-critical-path gates.

**6. DD insertion:** For each qubit wire with idle period $> d_{\text{DD}}$:
- Insert dynamical decoupling sequence (e.g., X-X) as UGate nodes with Order edges.

**What demand-dependence already provides:**
- ASAP schedule: inherent in topological sort.
- Parallelism: nodes with no path between them execute simultaneously.
- Critical path: longest dependency chain determines minimum depth.

**What additional scheduling requires:**
- Gate durations (from δ-node).
- Crosstalk constraints (not captured by data dependencies).
- Hardware alignment (post-QRVSDG constraint).
- Dynamical decoupling during idle windows.

**Complexity:** $O(N + E)$ for ASAP/ALAP. Crosstalk: $O(N^2)$.

---

## QRVSDG Optimization Levels

Mapping Qiskit's optimization levels 0–3 (plus an exhaustive Level 4) to QRVSDG pass pipelines using the combined algorithm set:

**Level 0 — Minimal (fast compilation):**

```
Construction → Basis Translation (XVIII) → [Routing] → Destruction
```

No QRVSDG-specific optimization. Implicit benefits of demand-dependence (automatic parallelism, canonical form) are still present.

**Level 1 — Light:**

```
Construction → CNE (IX) → DNE (X) → Basis Translation (XVIII) →
Adjacent Gate Cancellation (XIX-A) → [Routing] → Destruction
```

**Level 2 — Standard:**

```
Construction → CNE (IX) → DNE (X) →
Gate Hoisting (XI) → Loop-Invariant ID (XII) →
Gate Cancellation (XIX-A+B) → Rotation Merging (XIX-C) →
Constant Folding (XVI) → CNE (IX) → DNE (X) →
Basis Translation (XVIII) → [Routing] → Destruction
```

**Level 3 — Aggressive:**

```
Construction → CNE (IX) → DNE (X) →
Gate Hoisting (XI) → Loop-Invariant ID (XII) →
Unitary Consolidation (XX) → Basis Translation (XVIII) →
Region-Scoped Equality Saturation (XIV) →
Gate Cancellation (XIX-A+B+C) →
Constant Folding (XVI) → CNE (IX) → DNE (X) →
[Noise-Aware Routing] → Scheduling (XXI) → Destruction
```

**Level 4 — Exhaustive (verification/research):**

```
Level 3 + ZX-calculus simplification + formal verification (hierarchical ZX proofs) +
repeated equality saturation with increasing budgets +
cross-function optimization (inter-procedural gate cancellation, function specialization)
```

---

## SROA Placement and Optimization-Level Parameterization

SROA (Scalar Replacement of Aggregates) decomposes compound types into individual scalar edges, enabling the full set of scalar-level quantum optimizations. Its placement is parameterized by optimization level.

**Level 0:** Skip SROA. Algorithm VIII-CE handles compound lowering during destruction. Fastest compilation. Compound types survive to the destruction phase, which emits per-element operations as part of target code generation.

**Level 1:** Run SROA after construction, then enable basic scalar-level optimizations (XIX-A gate cancellation). Modest IR expansion.

**Level 2:** Run SROA after compound-level passes (Phase 2). Full compound-then-scalar pipeline: compound-level optimization exploits array-level patterns first, then SROA decomposes for scalar passes. For arrays with $n > 50$ elements, consider deferring SROA and relying on compound-level passes only (the $n > 50$ threshold requires empirical calibration against representative benchmarks).

**Level 3+:** Run SROA for all compounds. Provenance-based reconstruction during destruction produces idiomatic target output (e.g., `qubit[n]` declarations in OpenQASM 3, array-typed QIR). Edge-annotated provenance metadata carries the original compound grouping through scalar optimization.

**Passes requiring SROA:** Seven passes fundamentally require individual gates and are ineffective without SROA: XIX (Gate Cancellation, Parts A-C on individual wires), XX (Unitary Consolidation), XIV-quantum (Equality Saturation), XXII (Template Optimization), XXV (Quantum Hoare Optimization), XXI (Scheduling), and XXVI (Loop Unrolling boundary analysis). If any of these passes are in the pipeline, SROA must run first.

**SROA expansion cost:** $O(n)$ per compound operation, where $n$ is the array size. Each $\text{Array}(\text{Qubit}, n)$ edge becomes $n$ individual $\text{Qubit}$ edges. For small arrays ($n \le 10$), the cost is modest. For large arrays ($n \ge 50$), the expansion is significant (up to $50\times$ IR element increase). This is analogous to LLVM's SROA cost for large structs.

---

## Equality Saturation Placement Decision

Algorithm XIV-quantum (Region-Scoped Equality Saturation) runs in Phase 6 of the compilation pipeline, after SROA (Phase 3). This placement is deliberate.

**Rationale:** Scalar ECC rules (approximately 5K to 50K rules from Quartz/TKET2 pre-computed equivalence classes) provide vastly more optimization power than the approximately 20 to 30 compound-level rules that could be defined (self-inverse cancellation, index-disjoint commutation, rotation merging at compound level). No existing quantum compiler (TKET2, Qiskit, MLIR-based compilers) performs compound-level equality saturation; all operate exclusively at the individual gate level.

Algorithm XIX's compound extension (Part D) handles the most valuable compound-level patterns in Phase 2 without requiring e-graph infrastructure: index-disjointness commutation enables reordering, and same-index self-inverse cancellation enables gate removal. These pattern-matching transformations are sufficient for compound-level optimization.

**Reserved future option:** If SROA deferral is introduced for large arrays ($n > 50$), compound-level saturation with approximately 20 rules can be added to Phase 2. The compound rule set (self-inverse, rotation merging, commutation, construct/destruct identity, decomposition equivalences) is small enough to be specified manually and systematically derived from the scalar gate algebra plus compound node semantics.

---

## Dynamic Index Strategy

### Phase 1: Static Indices Only (Current)

All array indices are operator parameters (compile-time constants). Since array size $n$ is statically known at construction time, source-language loops over array elements are unrolled during construction, producing $n$ static $\text{ArrayApplyGate}$ nodes. No θ-node is created for such loops.

**Target algorithm expressibility:** All target algorithms are expressible with static indices:

| Algorithm | Index Pattern | Representation |
|-----------|--------------|----------------|
| VQE | Sequential parameter application | $n$ static $\text{ArrayApplyGate}$ nodes from unrolled loop |
| QAOA | Alternating cost/mixer layers | Static per-layer gate application |
| QPE | Controlled rotations by qubit index | Static power-of-two indexed gates |
| Grover | Uniform operations + oracle | Per-qubit static application |
| Fixed-code QEC | Syndrome extraction per stabilizer | Static per-stabilizer gate patterns |

Data-dependent qubit access (measurement-outcome-driven indices) uses γ-node conditional branches with static $\text{ArrayApplyGate}$ in each region. The γ-node predicate selects the branch; the array index within each branch is a compile-time constant.

### Phase 2: Dynamic Variants (Deferred)

When parametric sizes ($N$ as a type parameter) make unrolling impossible, dynamic index variants are required:

$$\text{ArraySwapDyn} : (\text{Array}(\tau, n), \text{Int}, \tau) \to (\text{Array}(\tau, n), \tau)$$

$$\text{ArrayApplyGateDyn}(\text{gate}) : (\text{Array}(\text{Qubit}, n), \text{Int}) \to \text{Array}(\text{Qubit}, n)$$

The index is an input edge (runtime value) rather than an operator parameter.

**CNE handling:** Dynamic indices are handled correctly by CNE (Algorithm IX). The index is another input edge; standard congruence comparison applies (two dynamic operations are congruent iff they have the same operator and all input edges, including the index edge, originate from congruent sources).

**Entanglement tracking with dynamic indices:** For single-qubit gates ($\text{ArrayApplyGateDyn}$), no precision loss in entanglement tracking (the gate acts on one qubit regardless of which index is selected). For multi-qubit dynamic gates, a conservative full-array merge approximation is applied: all qubits in the array are merged into a single entanglement class, since the compiler cannot statically determine which qubits are affected.

---

## 9-Phase Compilation Pipeline

The compilation pipeline organizes all QRVSDG algorithms into nine phases with formalized ordering constraints and iteration groups. This pipeline subsumes the per-level pass sequences defined in the optimization levels section and provides the canonical ordering for compound-type-aware compilation.

### Iteration Group Protocol

An iteration group is an ordered sequence of passes $[P_1, \ldots, P_k]$ with a maximum iteration count $M$:

```
for iteration in 1..M:
    changed = false
    for P in [P_1, ..., P_k]:
        changed = changed || P.run(IR)
    if not changed: break
```

Convergence is guaranteed by the monotonicity of each constituent pass (each pass reduces or preserves IR size/complexity).

### Phase 0: Construction

Algorithms I (Inter-Procedural Translation), II (Demand Annotation), III (Control Tree Translation), VII (Quantum Extensions).

Output: well-formed QRVSDG with compound types, region annotations, and linear typing.

### Phase 1: Early Cleanup

- **IterGroup A** [max 2 iterations]: IX (Quantum CNE) → X (Quantum DNE).
- XVII-classical (Vacuum): remove trivially empty nodes.

Purpose: eliminate redundancies introduced during construction before optimization passes encounter them.

### Phase 2: Compound-Level Optimization

- **IterGroup D** [max 2 iterations]: XI (Gate Hoisting) → XV-classical (Suffix Factoring) → IX (CNE).
- X-DFE (Dead Field Elimination): eliminate dead fields from compound types without decomposing.
- **IterGroup A** [max 2 iterations]: IX (CNE) → X (DNE).

This phase exploits compound-level structure: array-level loop invariance (more efficient than per-element), compound-level gate commutation (Algorithm XIX Part D), and dead field elimination (avoids creating intermediate dead scalar values). Algorithm XIX Part D compound rules (index-disjointness, same-index cancellation) apply here.

### Phase 3: Compound Decomposition

- SROA (Scalar Replacement of Aggregates).
- X (DNE): clean up dead scalar values from SROA.

No iteration: SROA is a one-directional transformation. At Level 0, this phase is skipped entirely (VIII-CE handles compound lowering during destruction).

### Phase 4: Scalar-Level Quantum Optimization

- XII (Loop-Invariant Quantum ID): scalar-level, catches patterns invisible at compound level.
- **IterGroup C** [max 2 iterations]: XXXVIII (Classical Constant Folding) → XXXIX (Basis State Constant Propagation) → XIV-classical (Extended DCE).
- **IterGroup B** [max 3 iterations]: XIX (Gate Cancellation A+B+C) → XXV (Quantum Hoare Optimization).
- **IterGroup A** [max 2 iterations]: IX (CNE) → X (DNE).

The primary scalar optimization phase. IterGroup C resolves classical constants and propagates basis states, enabling branch elimination. IterGroup B performs iterative gate cancellation and assertion-based removal.

### Phase 5: Basis and Synthesis

- XVIII (Basis Translation): decompose to target basis gate set.
- **IterGroup B** [max 2 iterations]: XIX (Gate Cancellation) → XXV (Post-basis cleanup).
- XX (Unitary Consolidation) → XXII (Template-Based Optimization).

Basis translation may introduce cancellable pairs (e.g., Toffoli decomposition produces CX pairs). IterGroup B cleans up post-translation. Consolidation and template optimization operate on the final gate set.

### Phase 6: Advanced Optimization (Level 3+)

- XIV-quantum (Region-Scoped Equality Saturation): budget-based, region-scoped e-graph saturation with ILP extraction.
- XXVI (Selective Quantum Loop Unrolling).
- **IterGroup B** [max 2 iterations]: XIX → XXV.
- **IterGroup A** [max 2 iterations]: IX → X.

Equality saturation explores the full space of ECC equivalences on the post-basis-translation IR. Loop unrolling exposes boundary gate cancellations. Both are followed by cleanup passes.

### Phase 7: Physical Compilation

- XXIII (Qubit Lifetime Analysis).
- XXIV (Qubit Reuse Optimization).
- [Routing]: SWAP insertion (external to QRVSDG algorithm set).
- XXI (Scheduling): hardware-aware timing with crosstalk avoidance and dynamical decoupling.

### Phase 8: Verification

- XV-quantum (Linear Type Verification): full verification of the optimized IR.

XV-quantum can also run at any intermediate point for debugging.

### Phase 9: Destruction

- IV (Inter-Procedural CFR) / VIII (Quantum Destruction to OpenQASM 3) / XIII (QIR Destruction).
- If SROA was skipped (Level 0): VIII-CE (Compound Elimination) runs before target emission.

### Hard Ordering Constraints

The following 12 constraints MUST be respected. Violation produces incorrect output.

| # | Constraint | Reason |
|---|-----------|--------|
| H1 | Construction (I/II/III/VII) → all optimizations | IR must exist |
| H2 | All optimizations → Destruction (IV/VIII/XIII) | Optimize before emitting |
| H3 | XXIII (Lifetime Analysis) → XXIV (Qubit Reuse) | XXIV consumes XXIII's interference graph |
| H4 | X-DFE → SROA | Dead fields eliminated before decomposition |
| H5 | SROA → X (standard DNE) | SROA may expose dead scalar values |
| H6 | XII (Loop-Invariant) → XXIII/XXIV | Hoisting invariant qubits improves reuse |
| H7 | XVIII (Basis Translation) → XIX (Gate Cancellation) | Basis translation may introduce cancellable pairs |
| H8 | XVIII → XX (Unitary Consolidation) | Consolidation operates on basis gates |
| H9 | XX → XXI (Scheduling) | Schedule after final gate selection |
| H10 | All gate optimizations → XXI (Scheduling) | Schedule is the last optimization |
| H11 | [Routing] → XXI (Scheduling) | Schedule after SWAP insertion |
| H12 | XXIV → XIII-B (Dynamic QIR Destruction) | Dynamic destruction needs lifetime info |

### Soft Ordering Constraints

The following 7 constraints SHOULD be respected for better optimization results, but violation does not produce incorrect output.

| # | Constraint | Reason |
|---|-----------|--------|
| S1 | IX (CNE) → X (DNE) | CNE creates dead nodes for DNE |
| S2 | XI (Gate Hoisting) → XIX (Gate Cancellation) | Hoisted gates may cancel with adjacent operations |
| S3 | XI → XII (Loop-Invariant) | Hoisting may expose loop-invariant patterns |
| S4 | XVI (Constant Folding) → XIV-classical (DCE) | Folded constants resolve γ-node predicates |
| S5 | XXXVIII (Classical Const Folding) → XXV (QHO) | Folded parameters enable rotation merging |
| S6 | XXXIX (BSCP) → XIV-classical (DCE) | Basis state tracking resolves deterministic measurements |
| S7 | XXVI (Loop Unrolling) → XIX (Gate Cancellation) | Unrolling exposes boundary gate cancellations |

### Optimization-Level Pipeline Variants

**Level 0:** Phase 0 → Phase 1 → skip Phases 2-6 → Phase 7 (XXIII/XXIV only if dynamic allocation) → Phase 9 (with VIII-CE).

**Level 1:** Phase 0 → Phase 1 → Phase 3 → Phase 4 (XIX-A only, single iteration) → Phase 5 (XVIII only) → Phase 7 → Phase 9.

**Level 2:** Phase 0 → Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 7 → Phase 8 → Phase 9.

**Level 3+:** Phase 0 → Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7 → Phase 8 → Phase 9.

---

## Routing Integration

Routing maps logical qubits to physical hardware qubits by inserting SWAP gates. It belongs post-optimization and pre-destruction in the QRVSDG pipeline:

```
QRVSDG Construction → QRVSDG Optimization → Routing → QRVSDG Destruction → Target Output
```

**QRVSDG-informed routing advantages:**

1. **Entanglement tracking** (Definition 3): if $E(q_1) \cap E(q_2) = \emptyset$, qubits $q_1$ and $q_2$ can be placed independently, partitioning the routing problem.

2. **Hierarchical structure**: route each λ-node independently, specializing per call site.

3. **θ-node awareness**: routing must be consistent across loop iterations (initial layout must equal loop-back layout, or additional SWAPs must restore it).

4. **γ-node routing**: each branch may have different two-qubit gate patterns; route branches independently, then reconcile qubit permutation at exit.

SWAP insertion creates UGate nodes preserving linear typing ($\text{SWAP} : (\text{Qubit}, \text{Qubit}) \to (\text{Qubit}, \text{Qubit})$).

---

## QRVSDG Gate Extension Model

*Source: HUGR Extension System. Formalizes custom gate set extensibility.*

HUGR's extension system allows third-party definition of types and operations (OpDefs) with polymorphic signatures, `try_lower` decomposition, and fallback implementations.

**QRVSDG Extension Model:**

**1. Gate Extension definition:**

```
Extension(name, gates: {
  gate_name: (n_q: Nat, n_p: Nat, properties: {
    unitary: Bool, self_adjoint: Bool, diagonal: Bool, ...
  }),
  ...
})
```

**2. Lowering/Decomposition:** Each extension provides `decompose(gate, target_extension)` → maps to basis translation (Algorithm XVIII).

**3. Rewrite Rules:** Extensions register ECC rewrite rules for their gates, integrating with equality saturation (Algorithm XIV).

**4. Optimization Hints:** Extensions declare algebraic properties: `commutes_with(other_gate)` (for commutation analysis), `inverse()` (for cancellation), `matrix()` (for unitary synthesis and constant folding).

**5. Calibration Integration:** δ-node stores extension-specific calibration data (gate fidelities, durations).

---

## Group 7: Quantum Loop Optimization

### Algorithm XXVI: Selective Quantum-Aware Loop Unrolling

*Extends: Algorithm XXXVII (θ-Node Loop Unrolling, classical optimization addendum) with quantum-specific classification and boundary gate cancellation analysis.*

Quantum programs have a distinctive property: unrolling exposes inter-iteration gate cancellations (e.g., $R_x(\beta) \cdot R_{zz}(\gamma)$ at QAOA layer boundaries) that flat circuit optimizers cannot exploit without unrolling. However, not all quantum loops benefit: variational optimization loops (VQE, QAOA outer loop), QEC syndrome cycles, and repeat-until-success protocols have runtime-dependent iteration counts and must never be unrolled. This algorithm extends the classical θ-node unrolling (Algorithm XXXVII) with quantum-specific loop classification and boundary gate analysis.

**Additional parameters** (beyond Algorithm XXXVII's parameters):

| Parameter | Symbol | Default | Description |
|-----------|--------|---------|-------------|
| Min cancellation benefit | $B_{\min}$ | 1 gate | Minimum boundary gate cancellations to justify partial unrolling |

**Phase 0 — Quantum θ-Node Classification (prepended before Algorithm XXXVII Phase 1):**

**0.** For each θ-node $\theta$, classify by quantum characteristics before trip count analysis:

| Classification | Condition | Action |
|---|---|---|
| **Repeat-until-success** | Predicate result is an MResult from a Measure node within the loop body | Skip unrolling (iteration count is probabilistic) |
| **QEC syndrome cycle** | Loop body contains SyndromeExtract or Encode/Decode nodes, or is within an $R_{EC}$ region | Skip unrolling (cycle count is runtime-determined by decoder) |
| **Variational optimization** | Predicate depends on a classical convergence check consuming measurement-derived values from within the loop body | Skip unrolling (convergence is runtime-dependent) |
| **Fixed quantum iteration** | Predicate depends only on compile-time-known classical values (e.g., QAOA layer count $p$, Grover iteration count $\lfloor\pi/4\sqrt{N}\rfloor$) | Proceed to Algorithm XXXVII |

Only loops classified as **fixed quantum iteration** proceed to the classical unrolling algorithm.

**Phase 2B — Boundary Gate Cancellation Analysis (inserted after Algorithm XXXVII Phase 2):**

**2B.** For each candidate θ-node with quantum loop variables, analyze the iteration boundary for gate cancellation potential:

**(a)** Identify the *last* UGate on each quantum loop variable's chain (just before the loop-back result $r$) and the *first* UGate on the same variable's chain (just after the loop argument $a$).

**(b)** For each quantum loop variable $lv$: if $G_{\text{last}} = G_{\text{first}}^{-1}$ (the last gate's adjoint equals the first gate), record a boundary cancellation. Also check if $G_{\text{last}}$ and $G_{\text{first}}$ commute and are same-axis rotations — these merge rather than cancel ($R_z(\theta_1) \cdot R_z(\theta_2) = R_z(\theta_1 + \theta_2)$).

**(c)** Count total cancellation potential: $B = \sum_{\text{quantum loop variables}} |\text{cancellable or mergeable pairs}|$. If $B < B_{\min}$ and the loop would only be partially unrolled, skip (the size increase is not justified by quantum benefit).

**Phase 5B — Quantum Post-Unrolling Optimization (appended to Algorithm XXXVII Phase 5):**

**6B.** Run quantum gate cancellation (Algorithm XIX from this addendum) on the unrolled body. Boundary cancellations identified in Phase 2B are now adjacent gates.

**7B.** If the unrolled body contains Conjugate nodes at iteration boundaries: check for Conjugate fusion ($U V_1 U^\dagger \cdot U V_2 U^\dagger = U (V_1 V_2) U^\dagger$). See [2] §6.8.

**8B.** Run entanglement partition refinement ([2] §4.4) on the unrolled body — the expanded circuit may reveal separable sub-circuits not visible in the looped form.

**Quantum constraints:**

- **Quantum state is never duplicated by unrolling.** Each iteration's qubit state flows linearly into the next. Unrolling materializes the iteration chain as explicit nodes, preserving $|\text{users}(o)| = 1$.
- **Measurement-dependent loops must not be unrolled.** Phase 0 classification catches this.
- **Region annotation preservation.** Unrolling does not change the region annotation: if the θ-body is $R_H$, the unrolled sequence is still within an $R_H$ context.

**Complexity:** Phase 0: $O(|\theta\text{-nodes}|)$. Phase 2B: $O(|LV_q| \cdot |body|)$. Phases 6B–8B: same as parent algorithms applied to the unrolled body.

**Example (QAOA p=3 layers):**

```
Phase 0 classification: QAOA layer loop has p=3 (constant) → fixed quantum iteration

Phase 2B: boundary analysis on quantum loop variable q:
  Last gate of body: Rx(βₗ)
  First gate of body: Rzz(γₗ₊₁) on different qubit pair
  → Rx and Rzz commute (different qubits) → demand-dependence handles this
  But: Rx(βₗ) · Rx(βₗ₊₁) on same qubit merge to Rx(βₗ + βₗ₊₁)
  B = 1 per qubit × n qubits ≥ B_min → proceed

After full unrolling (using Algorithm XXXVII Phase 4):
  cost(γ₁, q) → mixer(β₁, q) → cost(γ₂, q) → mixer(β₂, q) → cost(γ₃, q) → mixer(β₃, q)

Phase 6B: rotation merging at layer boundaries
  Rx(β₁) · Rx(β₂) → Rx(β₁ + β₂) if angles are compile-time constants
```

---

## Algorithm Summary Table

| # | Name | Source | Group | Extends |
|---|------|-------|-------|---------|
| XIV | Region-Scoped Equality Saturation | TKET2 Badger | Rewriting | Section 8.2 |
| XV | Linear Type Verification | HUGR | Verification | Novel |
| XVI | Quantum Constant Folding | HUGR | Rewriting | Novel |
| XVII | Quantum Subgraph Replacement | HUGR Grs | Infrastructure | Novel |
| XVIII | Basis Translation | Qiskit | Rewriting | Novel |
| XIX | Quantum Gate Cancellation (A+B+C) | Qiskit | Gate-Level | Novel |
| XX | Region-Local Unitary Consolidation | Qiskit | Physical | Novel |
| XXI | QRVSDG-Aware Scheduling | Qiskit | Physical | Novel |
| XXII | Template-Based Optimization | Qiskit | Rewriting | Novel |
| XXIII | Qubit Lifetime Analysis | QRVSDG (this work) | Dynamic Qubit Management | Novel |
| XXIV | Qubit Reuse Optimization | QRVSDG (this work) | Dynamic Qubit Management | XXIII |
| XXV | Quantum Hoare Optimization (QHO) | Qiskit HoareOptimizer | Assertion-Based Reasoning | Novel |
| XXVI | Selective Quantum Loop Unrolling | QRVSDG (this work) | Quantum Loop Optimization | XXXVII (classical addendum) |
| XIII-B | Dynamic QIR Destruction Path | QRVSDG (this work) | Dynamic Qubit Management | XIII |

Combined with existing Algorithms I–XIII and the optimization algorithms addendum, this gives a comprehensive set of algorithms for QRVSDG construction, optimization, scheduling, and destruction.

---

## Dynamic Qubit Allocation Algorithms

### Algorithm XXIII: Qubit Lifetime Analysis

*Source: Classical register allocation (liveness analysis). Prerequisite for Algorithm XXIV.*

**Input:** QRVSDG $G$.

**Output:** Lifetime map $\mathcal{L}$, interference graph $\mathcal{I} = (V_\mathcal{I}, E_\mathcal{I})$, peak live count $P$.

**Phase 1 — Linear Chain Tracing:**

**1.** For each region $\mathcal{R}$ (leaf-to-root), collect Prep nodes $\text{Preps}(\mathcal{R})$.

**2.** For each Prep node $p$, follow the unique linear chain:

$$p \xrightarrow{q_0} n_1 \xrightarrow{q_1} n_2 \xrightarrow{q_2} \cdots \xrightarrow{q_{k-1}} n_k$$

At each node, classify: UGate/Barrier/Reset → continue; Measure → continue via Qubit output; MeasureD/Dealloc → terminal; region result → continue in enclosing region.

For compound-type nodes (classified as pass-through for lifetime analysis): the identity-to-container mapping, maintained for entanglement tracking, provides position tracking for chain tracing.
- $\text{ArrayConstruct}$: the qubit enters the array at a known position; chain continues through the array edge and resumes when the qubit is extracted via $\text{ArraySwap}$ or $\text{ArrayDestruct}$.
- $\text{ArraySwap}$: the chain passes through; the extracted qubit's chain continues independently from the swap output.
- $\text{ArrayDestruct}$: all qubit chains resume from individual outputs.
- $\text{ArrayMultiGate}$ / $\text{ArrayApplyGate}$: the qubit remains in the array; chain continues through the array output edge.

For structural nodes: γ-node entry → lifetime spans entire γ-node; θ-node loop variable → lifetime spans all iterations; θ-node body-scoped Prep → lifetime is per-iteration; λ-node argument → lifetime determined by caller.

θ-node compound considerations: loop variable qubits inside arrays span all iterations (no intra-loop reuse possible). Body-scoped ancillae (per-iteration Prep/MeasureD within the θ-body) can reuse across iterations. Algorithm XII (loop-invariant hoisting) as a pre-pass can move invariant elements out of loops to improve reuse opportunity.

Record $\text{chain}(p)$, $\text{terminal}(p)$, and $\text{scope}(p)$.

**Phase 2 — Topological Lifetime Intervals:**

**3.** Compute topological ordering $\sigma_\mathcal{R}$ for each region.

**4.** Assign $\text{start}(p) = \sigma(p)$, $\text{end}(p) = \sigma(\text{terminal}(p))$.

**Definition (Simultaneously Live).** Two qubit chains $L_1, L_2$ in the same region are simultaneously live unless one is fully ordered after the other:

$$\text{not simultaneously live} \iff \text{end}_1 \prec \text{start}_2 \quad \text{or} \quad \text{end}_2 \prec \text{start}_1$$

where $\prec$ is the topological precedence relation (directed path exists).

**Phase 3 — Interference Graph Construction:**

**5.** Build $\mathcal{I} = (V_\mathcal{I}, E_\mathcal{I})$ where $V_\mathcal{I}$ = qubit lifetimes, edges connect simultaneously-live qubits.

Structural interference rules:
- γ-node: qubits in different γ-regions never interfere (only one branch executes).
- θ-node: per-iteration qubits do not interfere across iterations, but DO interfere with loop-variable qubits.
- λ-node: internal qubits interfere based on λ-body's internal order.

**Phase 4 — Peak Live Count:**

**6.** $P = \chi(\mathcal{I})$ (chromatic number). Use greedy Welsh-Powell heuristic for upper bound. For chordal interference graphs (common), $\omega = \chi$.

**Complexity:** $O(N + E + Q^2)$. No asymptotic increase from compound types: compound-type nodes are pass-through classifications with constant per-node overhead.

**Example (RUS Protocol):**

```
θ-node (RUS loop):
  Chain C_t (target): loop variable, spans all iterations
  Chain C_a (ancilla): Prep → CNOT → Measure → MeasureD (per-iteration)

C_t and C_a interfere within one iteration.
C_a does NOT interfere with itself across iterations.
Peak live count = 2  (target + one ancilla per iteration)
```

---

### Algorithm XXIV: Qubit Reuse Optimization

*Source: Chaitin's register allocation adapted for quantum. Uses Algorithm XXIII output.*

**Input:** QRVSDG $G$, interference graph $\mathcal{I}$ from Algorithm XXIII, hardware constraints $\delta$.

**Output:** Modified QRVSDG $G'$ with qubit reuse, updated `required_num_qubits`.

**Phase 1 — Graph Coloring:**

**1.** If $\chi(\mathcal{I}) > Q_{\text{hw}}$ (from δ-node): emit error — program cannot execute.

**2.** Apply Chaitin's algorithm: simplify nodes with degree $< K$ onto stack; select colors from stack. **No spilling** — no-cloning prevents saving qubits to memory.

**Alternative:** Linear scan allocation in $O(Q \log Q)$.

**Phase 2 — Reset Node Insertion:**

**3.** For each pair of chains $(C_i, C_j)$ assigned the same color where $\text{end}(C_i) \prec \text{start}(C_j)$:

| Terminal of $C_i$ | Action |
|---|---|
| MeasureD | Change to Measure + Reset; remove $C_j$'s Prep |
| Dealloc | Replace with Measure(discard Bit) + Reset; remove $C_j$'s Prep |

**Phase 3 — QRVSDG Update:**

**4.** Merge chains per physical qubit. Set `required_num_qubits` = colors used.

**5.** Verify with Algorithm XV (Linear Type Verification).

**Compound type considerations:**

Qubits inside arrays CAN participate in reuse after extraction via $\text{ArraySwap}$ or $\text{ArrayDestruct}$. Full array destruction is NOT required for individual qubit reuse.

The interference graph uses individual qubit identities as vertices. Compound membership is irrelevant to interference: qubits in the same array may have different lifetimes and independently participate in reuse.

Reset node insertion for reuse is local to the extracted qubit's chain. The containing array is unaffected.

Compound membership can inform register allocation heuristics: qubits in the same array may benefit from being assigned to nearby physical qubits (spatial locality), since compound operations often involve multi-qubit gates across array elements.

**Complexity:** Greedy coloring: $O(Q^2)$. Linear scan: $O(Q \log Q)$.

**Example (Teleportation Chain):**

```
Without reuse: 7 qubits (sender + 3 Bell pairs)
With reuse:    3 qubits (sender + 1 Bell pair at a time)
Width reduction: 7 → 3 physical qubits
```

---

### Dealloc Node Specification

Dealloc is a core QRVSDG node (see [2] §4.2 for integration context), not merely a proposed extension. It is the primary mechanism for explicit qubit deallocation in hybrid regions.

**Signature:** $\text{Dealloc} : (\text{Qubit}, \text{Eff}) \to (\text{Eff})$

**Semantics:** Traces out the qubit's quantum state and returns it to the allocation pool. The qubit edge is consumed (linear typing satisfied). No classical output (unlike MeasureD which produces a Bit).

**DeallocRegister (compound variant):**

$$\text{DeallocRegister} : (\text{Array}(\text{Qubit}, n), \text{Eff}) \to (\text{Eff})$$

Releases all $n$ qubits in the array back to the allocation pool. Semantically equivalent to $\text{ArrayDestruct}$ followed by $n$ individual $\text{Dealloc}$ nodes. The compound variant avoids intermediate decomposition when deallocation is the only remaining operation on the array.

**Region constraints:** Neither Dealloc nor DeallocRegister is permitted in $R_U$ (violates qubit-count preservation: $|A_q| = |R_q|$). Both are permitted in $R_H$ and $R_{EC}$.

**Lifetime analysis connection:** Dealloc and DeallocRegister mark the end of a qubit's (or qubit array's) lifetime. Algorithm XXIII treats these as terminal nodes in the linear chain. Algorithm XXIV uses the resulting lifetime intervals for reuse optimization.

**Entanglement interaction:** Deallocating an entangled qubit is physically valid but changes entangled partners' density matrix. Compiler should emit a warning for potentially entangled qubits (may indicate a bug).

**Optimization interactions:**
- DNE (X): Prep → Dealloc with no intervening gates is dead code, remove both.
- CNE (IX): Dealloc nodes are never congruent (consume unique qubit states).
- BSCP (XXXIX): Dealloc of known basis state is semantically equivalent to MeasureD with discarded result.
- Vacuum (XVII): Prep immediately followed by Dealloc can be removed entirely.

**Construction from source languages:**
- QIR: `__quantum__rt__qubit_release()` → Dealloc
- Q#: `use q = Qubit()` scope exit → Reset + Dealloc
- OpenQASM 3: no direct mapping (global qubits only)
- Qiskit: no direct mapping (no explicit deallocation)

**Destruction to targets:**
- QIR (dynamic): `__quantum__rt__qubit_release()`
- QIR (static): lower to MeasureD + discard Bit, or omit at program end
- OpenQASM 3: omit (all qubits persist to program end)

---

### Algorithm XIII-B: QIR Destruction with Dynamic Qubit Management

*Extends: Algorithm XIII. Alternative code path when `dynamic_qubit_management = true`.*

**Input:** Optimized QRVSDG $G$, target QIR profile, lifetime analysis $\mathcal{L}$ from Algorithm XXIII.

**Step 1 — Profile Compatibility:**
- Base Profile: emit error (requires `dynamic_qubit_management = false`).
- Adaptive Profile: emit warning (currently incompatible, but infrastructure exists for future extension).
- Future Dynamic Profile: full support.

**Step 2 — Module Setup:**

```llvm
!2 = !{i32 1, !"dynamic_qubit_management", i1 true}
declare ptr @__quantum__rt__qubit_allocate()
declare void @__quantum__rt__qubit_release(ptr)
```

Set `required_num_qubits` to peak live count $P$ from Algorithm XXIII.

**Step 3 — Node Translation:**

| QRVSDG Construct | QIR (static, XIII) | QIR (dynamic, XIII-B) |
|---|---|---|
| Prep | `inttoptr (i64 k to ptr)` | `call ptr @__quantum__rt__qubit_allocate()` |
| MeasureD | `__quantum__qis__mz__body` | `mz__body` + `qubit_release` |
| Dealloc | N/A | `qubit_release(ptr %q)` |
| UGate | `qis__*__body(inttoptr ...)` | `qis__*__body(ptr %q)` |
| Prep in θ-body | Index from counter | `qubit_allocate()` per iteration |
| Prep in γ-region | Index (wastes if branch not taken) | `qubit_allocate()` only in taken branch |
| `required_num_qubits` | Total Prep count | Peak live count $\chi(\mathcal{I})$ |

**Step 4 — Structural Node Translation:**

γ-nodes: `qubit_allocate()` inside conditional branch basic block.

θ-nodes: `qubit_allocate()` inside loop body; `qubit_release()` before loop back-edge.

λ-nodes: internal allocations invisible to caller.

---

## Compound Structural Node Patterns

This section documents structural node patterns specific to compound quantum values from a quantum optimization perspective. For formal compound type definitions, see [2] §4.1; for compound node operator signatures, see [2] §4.2.2.

### γ-node (Conditional) Extract-Operate-Reinsert Mandate

Within each γ-node region, the pattern for operating on elements of a compound linear value is:

1. $\text{ArraySwap}(j)$ to extract element at index $j$ from the array entry variable.
2. Operate on the extracted qubit (UGate, Measure, etc.).
3. $\text{ArraySwap}(j)$ to reinsert the operated qubit back into the array.
4. Produce the restored compound as the region result.

All γ-node regions must produce the same output type (γ-node matching-signature constraint from [1] §4.1). This means every region must restore the array to the same type, even if one region does not operate on the element.

**Quantum-specific consideration:** entanglement created within one γ-region does not persist to other regions (only one branch executes). However, entanglement that enters via the γ-node entry variable spans the entire γ-node in Algorithm XXIII's lifetime analysis.

### θ-node (Loop) Per-Iteration Element Access

For per-element operations on a loop-variable array:

1. $\text{ArraySwap}(i_{\text{iter}})$ to extract the element at the current iteration's index.
2. Operate on the extracted qubit.
3. $\text{ArraySwap}(i_{\text{iter}})$ to reinsert.
4. Advance the index.

Array size is invariant across iterations (the θ-node loop variable type is fixed). The iteration index $i_{\text{iter}}$ is a classical loop variable incremented per iteration.

**Quantum-specific consideration:** θ-node loop variable qubits (including those inside arrays) maintain entanglement across iterations. No intra-loop reuse is possible for loop-variable qubits (Algorithm XXIII). Linearity at the θ-node boundary requires the result array type to match the argument array type.

---

## ILP Extraction Formulation for Quantum E-Graphs

*Extends Algorithm XIV (Region-Scoped Equality Saturation), Step 3c.*

### Decision Variables

For each e-class $c$: $a_c \in \{0,1\}$ (active). For each e-node $n$ in $c$: $x_n \in \{0,1\}$ (selected). Optional: $d_n \in \mathbb{R}_{\ge 0}$ (depth), $D$ (circuit depth).

### Objective Function

**Gate count:** $\min \sum_n x_n \cdot w(n)$

**Depth:** $\min D$ subject to $D \ge d_n$ for all selected nodes.

**Noise-weighted:** $w(n) = -\log(\text{fidelity}(\text{op}(n), \text{qubits}(n)))$ from δ-node.

### Constraints

**C1 (E-class selection):** $\forall c : \sum_{n \in c} x_n = a_c$

**C2 (Child activation):** $\forall n, \forall c' \in \text{children}(n) : x_n \le a_{c'}$

**C3 (Root activation):** $\forall c \in \text{roots} : a_c = 1$

**C4 (Qubit production uniqueness):** $\forall c_q \in \text{qubit e-classes} : \sum_{n : c_q \in \text{outputs}(n)} x_n = 1$

**C5 (Qubit consumption uniqueness):** $\forall c_q \in \text{qubit e-classes} : \sum_{n \in \text{parents}(c_q)} x_n = 1$

**C6 (Qubit conservation):** For $R_U$: $\sum_{c \in \text{input qubit}} a_c = \sum_{c \in \text{output qubit}} a_c$

**C7 (Acyclicity):** Pre-filter cyclic e-nodes (fix $x_n = 0$), or use topological ordering variables with big-M: $t_{n_2} - t_{n_1} \ge 1 - M(2 - x_{n_1} - x_{n_2})$.

### Scalability

| Circuit | E-classes | E-nodes | ILP Variables | Solve Time |
|---|---|---|---|---|
| 5q VQE layer | ~40 | ~80 | ~120 | < 1s |
| 10q QAOA | ~200 | ~800 | ~1000 | 1–5s |
| 20q QFT | ~800 | ~5000 | ~5800 | 10–60s |
| Region-scoped (50 gates) | ~100 | ~500 | ~600 | < 5s |

Region scoping is the dominant scalability lever: 20 regions of 50 gates are much faster than one flat 1000-gate ILP.

**Warm-starting:** Greedy extraction provides an initial feasible solution, yielding 2–10× ILP solver speedup.

---

## Incremental Linear Type Verification

*Algorithm XV-Inc: Extends Algorithm XV for post-replacement verification.*

**Input:** QRVSDG $G$ (previously verified), replacement subgraph $S'$, boundary edges $B$.

**Precondition:** Algorithm XVII preconditions hold ($I_q(S) = I_q(S')$, $O_q(S) = O_q(S')$).

**Step 1:** Verify $S'$ internally (Algorithm XV restricted to $S'$ nodes). $O(|S'|)$.

**Step 2:** Verify boundary inputs: each $B_{\text{in}}$ qubit edge has $|\text{users}| = 1$. $O(|B|)$.

**Step 3:** Verify boundary outputs: each $B_{\text{out}}$ qubit edge has $|\text{users}| = 1$. $O(|B|)$.

**Step 4:** If $\rho(\mathcal{R}) = R_U$: verify no non-unitary operations in $S'$.

**Complexity:** $O(|S'| + |B|)$ vs. $O(N + E)$ full re-verification.

**Failure cases (requiring full re-verification):** signature change, annotation change, structural node creation/destruction, cross-region rewrites, overlapping bulk replacements.

**Per-pass verification strategy:**

| Pass | Strategy |
|---|---|
| Subgraph replacement (XVII) | Incremental (XV-Inc) |
| Equality saturation (XIV) | Full per-region |
| Gate cancellation (XIX) | Incremental per pair |
| Basis translation (XVIII) | Incremental per decomposition |
| DNE (X) | Full re-verification |

---

## ECC Import Pipeline

*Importing TKET2/Quartz pre-computed Equivalence Circuit Classes for Algorithm XIV.*

### Translation Pipeline

```
TKET2 ECC JSON → Parse circuits → Convert to QRVSDG subgraph patterns →
Verify linearity (Algorithm XV) → Generate bidirectional rules → Register as e-graph rules
```

### Gate Type Mapping

| TKET2 Gate | QRVSDG UGate |
|---|---|
| CX | UGate("CX", n_q=2, n_p=0) |
| Rz | UGate("Rz", n_q=1, n_p=1) |
| H | UGate("H", n_q=1, n_p=0) |
| T / Tdg | UGate("T"/"Tdg", n_q=1, n_p=0) |

### Scale Estimates

| ECC Set | Max Gates | Max Qubits | Est. ECCs | Est. Rules | Memory |
|---|---|---|---|---|---|
| nam_6_3 | 6 | 3 | ~500–2000 | ~5K–50K | ~0.5–1 MB |
| nam_6_4 | 6 | 4 | ~5K–20K | ~100K+ | ~5–10 MB |

### QRVSDG Advantages for ECC Matching

- **Strong normalization** reduces pattern variants needed (one pattern matches all orderings of commuting gates).
- **Region scoping** limits matching scope per-region.
- **Hierarchical application** (leaf regions first) enables cross-region optimization after per-region saturation.

---

## Correctness Framework

All algorithms preserve three foundational invariants:

**Invariant 1 — Semantic Preservation:** $\llbracket P \rrbracket(\sigma) = \llbracket P' \rrbracket(\sigma)$ for all valid input states.

**Invariant 2 — Linear Typing:** $\forall o \in O_q(n) : |\text{users}(o)| = 1$ maintained at every step.

**Invariant 3 — Region Annotation:** $\rho(\mathcal{R})$ preserved or correctly weakened.

### Proof Sketch Classes

| Class | Algorithms | Key Lemma |
|---|---|---|
| Cleanup | DCE (XIV-Opt), Vacuum (XVII-Opt) | Dead quantum chain: no live downstream measurement → no observable effect |
| Value Optimization | CSE (XVI-Opt), GVN (XXI-Opt), SCCP (XXIII-Opt), BSCP (XXXIX-Opt), Gate Fusion (XXV-Opt), Constant Folding (XVI) | Quantum congruence non-diversion: congruent outputs are distinct states |
| Code Motion | Code Folding (XV-Opt), LICM (XXII-Opt), Push/Pull-Through (XXVI/XXVII-Opt) | Gate motion: unitary commutation across structural boundaries |
| Function-Level | Inlining (XVIII-Opt), Tail Call (XXIV-Opt) | Inlining equivalence: apply-node semantics equals body substitution |
| Quantum-Specific | Equality Saturation (XIV), Gate Cancellation (XIX), Basis Translation (XVIII), Unitary Consolidation (XX) | ECC unitary equivalence: $U_{C_i} = U_{C_j}$; $G \cdot G^{-1} = I$ |

---

## Gate Set Parameterization Framework

### GateSet Definition

```
GateSet = {
  gates: Map<Name, GateSpec>,
  inverse_pairs: Set<(Name, Name)>,
  commutation_rules: Set<CommutationRule>,
  decomposition_rules: Map<Name, List<DecompositionSequence>>,
  identity_rules: Set<IdentityRule>,
  rotation_axes: Map<Name, Axis>
}

GateSpec = {
  qubit_count: Nat,
  parameter_count: Nat,
  is_self_adjoint: Bool,
  is_diagonal: Bool,
  matrix: Option<Matrix>,
  cost: CostMetric
}
```

### Standard Gate Sets

| Gate Set | Gates | Use Case |
|---|---|---|
| Clifford+T | {H, S, T, CNOT, X, Y, Z, Sdg, Tdg} | Fault-tolerant compilation |
| IBM | {CX, Rz, SX, X, I} | IBM Eagle/Heron hardware |
| Google | {CZ, Rz, √X, I} | Sycamore/Willow hardware |
| IonQ | {Rxx, Rz, Ry, I} | Trapped ion hardware |
| Nam | {CX, Rz, H} | Optimization-friendly research |
| Universal | Full set | Development/simulation |

### Algorithm-Gate Set Interactions

| Algorithm | Gate Set Usage |
|---|---|
| GVN (XXI-Opt) | `canonical_op` mapping ($R_z(\pi/4) \to T$ if $T \in$ GateSet) |
| Gate Fusion (XXV-Opt) | `rotation_axes` for same-axis merging, `identity_rules` |
| Basis Translation (XVIII) | `decomposition_rules` for targeted decomposition |
| Gate Cancellation (XIX) | `inverse_pairs`, `commutation_rules` |
| Template Optimization (XXII) | Templates parameterized by gate set |
| Constant Folding (XVI) | `matrix` for state propagation |
| Equality Saturation (XIV) | ECC rules generated per gate set |

### Gate Set Algebra

- `compose(gs1, gs2)`: Union with consistency check on conflicting GateSpecs.
- `restrict(gs, subset)`: Restriction with auto-generated decomposition rules for removed gates.
- `cost_model(gs, δ_node)`: Instantiate error rates from hardware calibration.

---

## Noise-Aware Algorithm Variants

### Noise Cost Function

$$\text{cost}_{\text{noise}}(n) = -\log(\text{fidelity}(\text{op}(n), \text{qubits}(n)))$$

Total circuit fidelity: $F \approx \prod_n \text{fidelity}(n) \cdot \prod_q (1 - \text{readout\_error}(q)) \cdot e^{-\sum_q t_{\text{idle}}/T_2(q)}$

### Per-Algorithm Variants

| Algorithm | Noise-Aware Modification |
|---|---|
| CNE (IX) | Select higher-fidelity congruent implementation |
| DNE (X) | Always remove dead gates (reduces noise) |
| Gate Hoisting (XI, XV-Opt, XXVI-Opt) | Consider fidelity difference at source vs. target location |
| Gate Fusion (XXV-Opt) | Compare merged fidelity vs. separate gate fidelities |
| LICM (XXII-Opt) | Classical hoisting: no noise impact; quantum templates: check coherence budget |
| Inlining (XVIII-Opt) | Inline if boundary cancellation reduces noise more than added gate count |
| Basis Translation (XVIII) | Choose decomposition minimizing total error, not gate count |
| Gate Cancellation (XIX) | Prioritize cancellations on high-error qubits |
| Unitary Consolidation (XX) | Compare KAK fidelity vs. original gate sequence fidelity |
| Equality Saturation (XIV) | Noise-weighted cost in ILP extraction |
| Scheduling (XXI) | Already noise-aware: crosstalk avoidance + DD insertion |

### δ-Node Update Protocol

Calibration data has a timestamp. When stale, re-run only noise-dependent phases (routing + scheduling), keeping the structural optimization result. This enables compilation amortization.

---

### Algorithm XXV: Quantum Hoare Optimization (QHO)

*Source: Qiskit HoareOptimizer [9]. Strictly subsumes BSCP.*

**Input:** QRVSDG $G$, assertion library $\mathcal{A}$.

**Phase 1 — Forward Assertion Propagation:** Process nodes topologically: Prep → $|0\rangle$ assertion; UGate → update assertions; controlled gate with $|0\rangle$ control → identity (remove); Measure of known state → constant Bit.

**Phase 2 — Backward Assertion Propagation:** From measurement nodes, propagate backward through invertible gates: $G^{-1}(\text{output assertion})$ = input assertion.

**Phase 3 — Gate Removal:** Remove all provably redundant gates.

**Relationship to SCCP/BSCP:**

$$\text{SCCP (Alg XXIII) + BSCP (Alg XXXIX)} \subset \text{QHO} \subset \text{Full Quantum Hoare Logic}$$

SCCP: classical constants only, $O(N_c+E_c)$, deterministic. BSCP: adds basis state tracking, $O(N+E)$, deterministic. QHO: arbitrary assertions, $O(N \cdot T_{\text{Z3}})$, practical. Full: entanglement predicates, undecidable.

**Recommendation:** Use SCCP + BSCP at Level 1–2; QHO at Level 3+ for ancilla-heavy algorithms and uncomputation patterns.

---

## QRVSDG Demand-Dependence as LightCone

The QRVSDG's demand-dependence structure already provides Qiskit's LightCone optimization. DNE (Algorithm X) is the operational equivalent: it removes all nodes whose outputs do not transitively reach a measurement, function return, or ω-region result.

| Aspect | Qiskit LightCone | QRVSDG DNE |
|---|---|---|
| Representation | Must explicitly compute backward reachability | Light cone is implicit in graph structure |
| Incrementality | Must recompute after each pass | Maintained automatically |
| Scope | Intra-circuit only | Inter-procedural (unused λ-nodes also removed) |

---

## Scope Note

Benchmark methodology, numbering governance discussions, and multi-level pipeline governance are maintained in the research artifact:

- [.copilot-tracking/research/2026-03-13/qrvsdg-extensions-research.md](.copilot-tracking/research/2026-03-13/qrvsdg-extensions-research.md)

This addendum remains focused on formal algorithm definitions, mappings, constraints, and complexity.

---

### Algorithm XVI-Q-B: Simplify Parametric Gates (Angle-to-Named-Gate Simplification)

*Source: Novel. Lightweight quantum-specific pass that replaces parametric rotation gates with equivalent named gates when the angle parameter evaluates to a recognized constant.*

Parametric gates like $R_z(\theta)$, $R_x(\theta)$, $R_y(\theta)$ are frequently emitted by synthesis and decomposition passes with concrete angle values that correspond to well-known named gates. This algorithm performs a single forward pass replacing such gates with their canonical named equivalents, improving readability and enabling downstream pattern-matching passes (InverseCancellation, TemplateOptimization) that operate on gate names.

**Input:** Region $\mathcal{R}$ containing UGate nodes with concrete `ParamExpr` parameters.

**Algorithm:**

For each Simple node $n$ in topological order within $\mathcal{R}$:

**1.** If $n$ is a parametric rotation gate (e.g., $R_z(\theta)$, $R_x(\theta)$, $R_y(\theta)$) with all parameters concrete (no `RegionArg` leaves):

**2.** Evaluate all parameters to numeric values. Let $\theta$ be the primary angle.

**3.** Check $\theta \pmod{2\pi}$ against the recognition table:

| $\theta \pmod{2\pi}$ | Replacement |
|---|---|
| $0$ | Identity (remove gate) |
| $\pi/4$ | T (for $R_z$) |
| $\pi/2$ | S (for $R_z$), SX (for $R_x$) |
| $\pi$ | Z (for $R_z$), X (for $R_x$), Y (for $R_y$) |
| $3\pi/2$ | Sdg (for $R_z$) |
| $7\pi/4$ | Tdg (for $R_z$) |

**4.** If a match is found: replace $n$ with the named gate. Preserve all quantum input/output port wiring. If the replacement is identity, bypass $n$ (connect inputs directly to output consumers).

**Complexity:** $O(N_p)$ where $N_p$ is the number of parametric gate nodes. Constant-time per gate (table lookup after evaluation).

**Relationship to other passes:** GVN (Algorithm XXI) parameter equivalence detection and 1-qubit decomposition resynthesis partially cover this pattern. A dedicated standalone pass is recommended as a post-synthesis cleanup because it is cheaper ($O(N_p)$ table lookup vs. full value numbering) and exposes named gates for downstream pattern-matching passes.

---

### Algorithm XXI-Q-Lite: Commutativity-Based Depth Reduction

*Source: Qiskit `CommutationAnalysis` + scheduling. Lightweight depth optimization using gate commutativity to reorder operations for reduced circuit depth.*

Circuit depth is the primary metric for NISQ-era quantum computing, directly affecting decoherence-limited fidelity. This algorithm exploits the commutativity of quantum gates to reorder operations within a region, reducing the longest path (critical path depth) without changing the circuit's unitary.

**Input:** $R_U$ region $\mathcal{R}$ with UGate nodes.

**Algorithm:**

**Phase 1 — Commutativity Analysis:**

For each pair of UGate nodes $(n_1, n_2)$ in $\mathcal{R}$ where $n_1$ precedes $n_2$ in topological order and both operate on overlapping qubit sets:

**1.** Check commutativity: $[U_1, U_2] = 0$ using the commutation rules:

- Diagonal gates commute with all diagonal gates (Rz, T, S, Z, CZ).
- Single-qubit gates on disjoint qubits always commute.
- CNOT commutes with Z-rotations on the control qubit and X-rotations on the target.
- General case: compute $U_1 U_2 - U_2 U_1$ numerically if matrices are available.

**Phase 2 — Dependency Relaxation:**

**2.** For each commuting pair $(n_1, n_2)$ currently connected by a data dependency through a shared qubit:

- If removing the ordering constraint (allowing $n_2$ before $n_1$) reduces the critical path length: relax the constraint. In the QRVSDG, this means the two nodes become unordered within the region (no direct data dependency between them), exposing parallelism.

**Phase 3 — Depth-Optimal Scheduling:**

**3.** With relaxed dependencies, compute an ASAP (as-soon-as-possible) schedule:

- Assign each node to the earliest time step where all its non-relaxed dependencies are satisfied.
- The resulting schedule minimizes circuit depth under the relaxed dependency graph.

**Complexity:** Phase 1: $O(G^2)$ for $G$ gates in the region (pairwise commutativity). Phase 2: $O(G)$. Phase 3: $O(G + E)$ topological scheduling. Total: $O(G^2)$ dominated by commutativity analysis.

**Incremental adoption:** Phase 3 (ASAP scheduling) can be implemented independently using the QRVSDG's existing topological order as the dependency graph. Phases 1–2 (commutativity-based dependency relaxation) layer on top, requiring a commutativity analysis module that provides pairwise commutativity predicates for the gate set.

---

## References

[1] N. Reissmann, J. C. Meyer, H. Bahmann, and M. Själander, "RVSDG: An Intermediate Representation for Optimizing Compilers," *ACM Transactions on Embedded Computing Systems*, vol. 19, no. 6, pp. 1–28, 2020.

[2] QRVSDG: A Regionalized Value State Dependence Graph for Hybrid Quantum-Classical Compilation. (This work.)

[3] Quantinuum, "HUGR: Hierarchical Unified Graph Representation," 2024. https://github.com/quantinuum/hugr. Specification: https://quantinuum.github.io/hugr/specification/index.html.

[4] A. Javadi-Abhari, M. Treinish, et al., "Quantum computing with Qiskit," arXiv:2405.08810, 2024.

[5] M. Xu, Z. Li, O. Padon, et al., "Quartz: Superoptimization of Quantum Circuits," *PLDI 2022*, ACM, 2022.

[6] M. Xu, A. Hirth, et al., "QUESO: Quantum Equality Saturation for Circuit Optimization," *PLDI 2023*, ACM, 2023.

[7] M. Willsey, C. Nandi, et al., "egg: Fast and Extensible Equality Saturation," *POPL 2021*, ACM, 2021.

[8] S. Sivarajah, S. Dilkes, et al., "tket: A Retargetable Compiler for NISQ Devices," *Quantum Science and Technology*, vol. 6, no. 1, 2021.

[9] T. Haener, T. Hoefler, and M. Troyer, "Assertion-Based Optimization of Quantum Programs," *Proceedings of the ACM on Programming Languages* (OOPSLA), 2020. arXiv:1810.00375.
