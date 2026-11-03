# QRVSDG Addendum: Optimization Algorithms from Classical Compiler IRs

**Companion to:** *QRVSDG: A Regionalized Value State Dependence Graph for Hybrid Quantum-Classical Compilation*

**Source Inspiration:**

- Binaryen (WebAssembly optimizer): DCE, Code Folding, CSE, Vacuum, Reorder Functions, SimplifyLocals, Duplicate Function Elimination, Monomorphize, Directize, Outlining, DAE, Signature Refining, GUFA
- LLVM: GVN, LICM, SCCP, Function Inlining, Tail Call Optimization, Instruction Combining
- RVSDG-native patterns: Node Push-Through, Node Pull-Through, Node Splitting, Region Flattening
- [1] Reissmann, Meyer, Bahmann, Själander. "RVSDG: An Intermediate Representation for Optimizing Compilers." ACM TECS 19(6), 2020.
- [2] QRVSDG paper (this work).

## Overview

This addendum extends the QRVSDG algorithm set (I–XIII from the consolidated algorithms addendum) with twenty-six new optimization algorithms (XIV–XXXIX) adapted from classical compiler IR passes. Each algorithm is presented with its classical origin, QRVSDG mapping, quantum-specific constraints, formal specification, complexity analysis, and a concrete quantum example.

Numbering note: this addendum uses an independent local namespace (XIV-XXXIX). Cross-document references to similarly numbered algorithms in the quantum optimizations addendum should be disambiguated by document context.

The algorithms are organized into six groups:

1. **Cleanup** (Algorithms XIV, XVII): Removing dead code and structural artifacts.
2. **Value Optimization** (Algorithms XVI, XXI, XXIII, XXV, XXXVIII, XXXIX): Eliminating redundant computations, folding constants, and propagating basis states.
3. **Code Motion** (Algorithms XV, XXII, XXVI, XXVII): Moving operations across structural boundaries.
4. **Function-Level** (Algorithms XVIII, XIX, XX, XXIV): Optimizing inter-procedural structure.
5. **Structural** (Algorithms XXVIII, XXIX, XXXVII): Restructuring, simplifying, and unrolling the graph.
6. **Inter-Procedural Whole-Program** (Algorithms XXX–XXXVI): Cross-function deduplication, specialization, resolution, extraction, elimination, refinement, and whole-program analysis.
7. **Compound Type** (Algorithms X-DFE, SROA): Field-sensitive dead code elimination and scalar replacement for compound types.

## Notation

All notation from the consolidated algorithms addendum applies. Additional notation:

| Symbol | Meaning |
|--------|---------|
| $\text{vn}(n)$ | Value number assigned to node $n$ |
| $\text{canonical\_op}(n)$ | Canonical operator mapping for node $n$ |
| $d(n)$ | Duration of node $n$ (from δ-node calibration data) |
| $\rho(\mathcal{R})$ | Region annotation: $R_U$, $R_H$, or $R_{EC}$ |
| $\text{CLL}(\lambda)$ | Set of all apply-nodes (callers) of λ-node $\lambda$ |
| $T_{\text{size}}$ | Inlining size threshold |
| $\text{Array}(\tau, n)$ | Homogeneous fixed-size array of $n$ values of type $\tau$ |
| $\text{Tuple}(\tau_1, \ldots, \tau_k)$ | Heterogeneous product type |
| $\text{Struct}(\text{name}, \text{fields})$ | Named product with labeled fields |
| $\text{linear}(\tau)$ | Predicate: true if $\tau$ contains quantum data |
| $\text{contains\_quantum}(\tau)$ | Predicate: true if $\tau$ contains Qubit or LogicalQubit at any depth |
| $\text{contents}(\text{arr})$ | Identity-to-container mapping: qubit identities within compound value |
| $\text{ArraySwap}(j)$ | Element exchange at static index $j$ |
| $\text{ArrayMultiGate}(\text{gate}, \text{indices})$ | In-place gate application to array elements |
| $\text{ArrayApplyGate}(j, \text{gate})$ | Shorthand for ArrayMultiGate(gate, $(j)$) |
| $\text{ArrayTransport}(Z_{\text{src}} \to Z_{\text{dst}})$ | Bulk zone transition |
| VIII-CE | Algorithm VIII-CE: Compound Elimination pre-destruction pass |
| X-DFE | Algorithm X-DFE: Dead Field Elimination |
| SROA | Scalar Replacement of Aggregates |

**Extension note:** Algorithms XXX–XXXVI (Group 6) reference zone-annotated qubit types $\text{Qubit}\langle Z \rangle$ as proposed in the IR design paper [qrvsdg_ir_design_paper.md]. These annotations are an extension beyond the core type system defined in the QRVSDG paper [2] §4.1 and are applicable only when zone information is available.

**Consistency note:** The QRVSDG paper [2] defines multiple independent effect domains ($\text{Eff}_{\text{qpool}}$, $\text{Eff}_{\text{cmem}}$, $\text{Eff}_{\text{io}}$); this addendum uses `Eff` generically to refer to the relevant domain-specific token. The paper also defines Conjugate structural nodes (§4.2.1), LogicalQubit types with Encode/Decode boundary nodes (§4.3), and canonical form CNE via phase polynomials and ZX-calculus (§6.1). Algorithms in this addendum that reference `Eff` threading, R_EC regions, or CNE congruence should be read in the context of these expanded definitions.

---

## Group 1: Cleanup Algorithms

### Algorithm XIV: Extended Dead Code Elimination (Quantum DCE)

*Source: Binaryen DCE, LLVM ADCE. Extends Algorithm X with unreachable code analysis.*

Binaryen's DCE removes unreachable code blocks after `unreachable`, `return`, or `br` instructions. LLVM's ADCE (Aggressive DCE) assumes values dead until proven alive and also removes dead control flow. Algorithm X (Quantum DNE) handles dead *nodes* (unused qubit chains, unreachable branches). Extended DCE adds three capabilities: unreachable γ-region elimination with quantum cleanup, dead θ-node iteration analysis, and post-measurement dead chain conversion.

**Phase 1 — Constant Predicate Analysis:**

For each γ-node $\gamma$ in the QRVSDG:

**1.** Analyze the predicate input $p$ of $\gamma$:

- If $\text{origin}(p)$ is a constant: the predicate is statically known. Mark regions $R_j$ where $j \neq v$ (the selected region index) as *unreachable*.
- If $\text{origin}(p)$ is a Measure node whose quantum input is a Prep node (known $|0\rangle$ state) with no intervening gates: the measurement outcome is deterministic (always 0). Mark unselected regions as unreachable.

For each θ-node $\theta$:

**2.** Analyze the predicate result $r_0$:

- If $r_0$ is provably false on the first iteration (derived from a constant or single-pass condition): mark $\theta$ as *single-iteration* (degenerate loop).

**Phase 2 — Unreachable Region Cleanup:**

For each unreachable γ-region $R_j$:

**3.** For each entry variable $ev = (i, A)$ where $\text{type}(i) = \text{Qubit}$:

- Remove the argument $a^{R_j}$ and all quantum nodes consuming it within $R_j$.
- If *all* non-selected regions reference this entry variable identically (passthrough) and the exit variable maps directly to the entry argument in $R_v$: remove the entry/exit variables entirely, connecting the input directly to the output.

**4.** Remove all nodes within unreachable regions. Remove the unreachable regions themselves if all non-selected regions are unreachable (γ-node becomes passthrough of the selected region's body).

For each single-iteration θ-node:

**5.** Replace the θ-node with its body:

- Connect each loop variable input $i$ to the corresponding argument $a$.
- Connect each result $r$ (except predicate $r_0$) to the corresponding output $o$.
- Inline the θ-region's nodes into the parent region.

**Phase 3 — Post-Measurement Dead Chain:**

**6.** For each Measure node $m$ where the post-measurement Qubit output has no live downstream consumer:

- If $m$'s Bit output is live: replace $m$ with MeasureD (destructive), removing the dead qubit output.
- If $m$'s Bit output is also dead: remove $m$ entirely (handled by Algorithm X).

**Phase 4 — Conjugate Identity Elimination:**

**7.** For each Conjugate node $C$: if $\mathcal{R}_{\text{inner}}$ is empty (no operations) or all operations are identity-equivalent (detected by canonical form comparison or by checking that all through-qubit results directly connect to through-qubit arguments):

- The entire Conjugate computes $U \cdot I \cdot U^\dagger = I$. Remove $C$: connect each through-qubit input directly to its output. Remove ancilla-qubit Prep/Dealloc chains that were internal to $C$.

**Phase 5 — Identity Gate-Call Elimination:**

**8.** For each Simple node $n$ whose operator is a gate name recognized as a structural identity (e.g., `id`, `noop`, `barrier` with no scheduling effect, or a gate whose unitary matrix equals $I$ up to global phase):

- Bypass $n$: for each qubit input-output pair $(i^k, o^k)$, redirect all consumers of $o^k$ to $\text{origin}(i^k)$. Mark $n$ dead.
- This extends Algorithm X's sweep phase: whereas DNE removes nodes whose outputs are *unused*, identity-gate-call elimination removes nodes whose outputs are *equivalent to their inputs* regardless of liveness.
- An implementation may combine this phase with DNE's sweep phase, using a gate identity table to recognize identity gate names during the same traversal.

**Quantum constraints:**

- Removing unreachable regions must verify that quantum state edges entering dead regions are not entangled with live qubits via prior multi-qubit gates. If entangled, the dead region's input edges must be properly terminated.
- Prep nodes in unreachable regions can be fully removed.
- Measurement nodes with observable side effects (output recording) must not be removed even if the quantum output is dead.

**Complexity:** Phase 1: $O(|\gamma| + |\theta|)$. Phase 2: $O(N_{\text{dead}})$. Phase 3: $O(|M|)$. Total: $O(N + E)$ combined with Algorithm X.

**Example:**

```
Quantum teleportation of known |0⟩ state:

Before DCE:
  Prep q_a |0⟩, Prep q_b |0⟩
  H(q_a), CNOT(q_a, q_b)       — create Bell pair
  CNOT(|0⟩, q_a), H(|0⟩)      — Bell measurement
  Measure(|0⟩) → m₁ = 0        — deterministic outcome
  Measure(q_a) → m₂
  γ-node(m₁):
    R₀: Identity(q_b)          ← always selected (m₁ = 0)
    R₁: Z(q_b)                 ← unreachable

After DCE:
  R₁ removed, γ-node on m₁ eliminated (passthrough)
  Remaining: γ-node(m₂) only
```

**Compound type note:** DNE (Algorithms VI and X) is conservatively correct for compound types. The mark rule "mark all inputs alive" keeps dead fields alive when the compound output is alive. This is sound but suboptimal: if only one field of a 3-field tuple is extracted downstream, the other two fields remain alive under DNE. For example, given `Tuple(Matrix, Eigenvalue, Metadata)` where only Eigenvalue (field 1) is used after the node, DNE marks all three fields alive because the compound output has a user. Algorithm X-DFE (Dead Field Elimination) addresses this suboptimality with field-sensitive marking. See Group 7 below.

---

### Algorithm XVII: Quantum Vacuum

*Source: Binaryen Vacuum, LLVM simplifycfg. Removes structurally unnecessary constructs.*

Binaryen's Vacuum removes obviously unneeded code: empty blocks, trivial control flow, nop-like constructs. LLVM's `simplifycfg` removes blocks with no predecessors and merges trivial blocks. The QRVSDG analog cleans structural overhead that does not contribute to computation.

Process nodes bottom-up (inner regions first):

**1. Identity γ-node removal:**

For each γ-node $\gamma$: if for every exit variable $ex = (R, o)$:

$$\forall j \in [0..k] : \text{origin}(r^{R_j}) = a^{R_j}_{\text{corresponding entry variable}}$$

then $\gamma$ is an identity. Remove it: for each entry/exit pair, connect $\text{origin}(i_\text{entry})$ directly to $\text{users}(o_\text{exit})$.

**2. Passthrough variable pruning:**

For each γ-node $\gamma$: for each entry/exit variable pair where the identity condition holds for that variable (but not all variables):

- Remove the entry variable and exit variable from the γ-node interface.
- Connect the input directly to the output, bypassing the γ-node for that value.

For each θ-node $\theta$: for each loop variable $lv = (i, a, r, o)$ where $\text{origin}(r) = a$:

- Remove $lv$ from the θ-node interface.
- Connect $\text{origin}(i)$ directly to $\text{users}(o)$.

**3. Identity quantum node removal:**

For each UGate node $n$ representing a known identity operation (e.g., $I$, or removable Barrier):

- For each qubit $k$: connect $\text{origin}(i^k_n)$ to $\text{users}(o^k_n)$.
- Remove $n$.

**4. Trivial Prep-Measure chain:**

For each Prep node $p$ immediately followed by a MeasureD node $m$ (i.e., $\text{origin}(i_m) = o_p$, no intervening gates):

- Replace $m$'s Bit output with a constant 0 node.
- Remove both $p$ and $m$.

**5. Identity Conjugate removal:**

For each Conjugate node $C$: if $\mathcal{R}_{\text{inner}}$ has no operations (all through-qubit arguments connect directly to results), remove $C$ and connect inputs to outputs. This differs from Algorithm XIV Phase 4 in that Vacuum catches the purely structural case (empty inner region) without canonical form analysis.

**6. Conjugate passthrough variable pruning:**

For each Conjugate node $C$: for each through-qubit where the outer region applies a gate $G$ and the inner region does not modify that qubit: the through-qubit experiences $G \cdot I \cdot G^\dagger = I$ on that wire. Remove the through-qubit from the Conjugate interface and connect directly.

**Quantum constraints:**

- Quantum passthrough detection requires that in every γ-region, the entry argument's qubit chain leads directly to the exit result with no intervening gates. Due to linear typing, this means $\text{origin}(\text{result}) = \text{argument}$ in each region.
- Prep immediately followed by MeasureD produces deterministic Bit(0); this is safe to fold.

**Complexity:** $O(N + |EV| + |LV|)$.

**Example:**

```
Before Vacuum:
  θ-node (single-iteration loop):
    lv₁: (params_in, a, r_params, params_out)  — a = r (passthrough)
    lv₂: (q_in, a_q, r_q, q_out)               — modified by gates
    predicate: false

After Vacuum:
  θ-node replaced by body (single-iteration)
  lv₁ removed (passthrough): params connected directly
  Gates from body inlined into parent region
```

---

## Group 2: Value Optimization Algorithms

### Algorithm XVI: Quantum Local/Global CSE

*Source: Binaryen LocalCSE, LLVM GVN. Extends Algorithm IX with region-scoped and cross-region classical CSE.*

**Compound type note:** CNE (Algorithms V and IX) is correct unchanged for compound types. Congruence is determined by operator equality and input congruence; the type of outputs does not affect the congruence check. Compound-type operators (ArrayConstruct, TupleConstruct, StructConstruct, and their extraction counterparts) define specific operations with unique operator identities, so congruence testing naturally distinguishes them. Quantum compound outputs are never diverted (the no-cloning constraint from Algorithm IX prevents diversion of linear-typed outputs). No modifications to the CNE algorithm are required. The Compound Linearity Sufficiency Theorem (§4.1 of the QRVSDG paper [2]) is the formal justification: compound-level linearity checking is sufficient, so CNE need not inspect internal fields.

Binaryen's LocalCSE finds repeated computations within a linear execution path and reuses the first computation's result. Algorithm IX (Quantum CNE) already performs structural CSE. This algorithm adds region-local classical optimization, cross-region classical extraction, and quantum gate chain identity detection.

**Phase 1 — Region-Local Classical CSE (within each region):**

For each region $\mathcal{R}$, process nodes in topological order:

**1.** Maintain a hash table $H$ mapping (operator, origin-tuple) → node for classical simple nodes.

**2.** For each classical simple node $n$: compute key = (op($n$), origins($I_n$)).
- If $H[\text{key}]$ exists as node $n'$: divert all edges from $O_n$ to $O_{n'}$. Mark $n$ dead.
- Else: $H[\text{key}] \gets n$.

**3.** Clear $H$ at region boundaries (each γ-subregion, θ-body processed independently).

**Phase 2 — Cross-Region Classical CSE (γ-nodes):**

For each γ-node $\gamma$, after processing all subregions independently:

**4.** Compare classical computations across regions. For each pair of nodes $n_j \in R_j, n_k \in R_k$ with same operator and all inputs originating from congruent entry variable arguments:

- These nodes compute the same value in every region. Hoist the computation before the γ-node (as an additional entry variable) and replace $n_j, n_k$ with the entry argument.

**Phase 3 — Quantum Gate Chain Identity Detection:**

For each qubit state chain within a region:

**5.** Walk the chain node-by-node. Track the last gate on each qubit.

**6.** If the current gate is the inverse of the immediately preceding gate on the same qubit(s), mark both as dead:
- $HH \to I$, $XX \to I$, $YY \to I$, $ZZ \to I$
- $\text{CNOT} \cdot \text{CNOT} \to I$ (same control and target)
- $R_z(\alpha) \cdot R_z(\beta) \to R_z(\alpha + \beta)$ (rotation merging)

**Quantum constraints:**

- No-cloning blocks quantum CSE: two UGate nodes on *different* qubit states cannot be merged even if structurally congruent—they perform physically different operations. Classical CSE is unrestricted.
- Boundary with Algorithm XXV: Algorithm XVI focuses on local identity detection and classical redundancy elimination; advanced gate fusion/template-level simplification remains centralized in Algorithm XXV.

**Complexity:** Phase 1: $O(N_c)$. Phase 2: $O(|EV| \cdot k)$ per γ-node. Phase 3: $O(N_q)$. Total: $O(N)$.

---

### Algorithm XXI: Quantum Global Value Numbering

*Source: LLVM GVN. Extends Algorithm IX with hash-based value numbering for deeper equivalence detection.*

LLVM's GVN assigns hash-based value numbers to expressions, detecting redundancies beyond structural identity. For quantum circuits, GVN detects algebraic identities ($HXH \equiv Z$), parameter equivalence ($R_z(\pi/4) \equiv T$), and commutation-based equivalence.

**1. Value Number Assignment:** Process nodes in topological order within each region:

For classical nodes: assign $\text{vn}(n) = \text{hash}(\text{op}(n), \text{vn}(\text{origin}(i^0_n)), \ldots, \text{vn}(\text{origin}(i^{|I_n|-1}_n)))$.

For UGate nodes: assign $\text{vn}(n) = \text{hash}(\text{canonical\_op}(n), \text{vn}(\text{quantum\_inputs}(n)), \text{vn}(\text{classical\_inputs}(n)))$, where $\text{canonical\_op}$ maps known aliases:
- $R_z(\pi/4) \to T$, $R_z(\pi/2) \to S$, $R_z(\pi) \to Z$

**2. Extended Identity Detection:** For each node $n$, check if $\text{vn}(n)$ matches known identity patterns:
- Self-inverse: $G(G(q))$ for $G \in \{H, X, Y, Z, \text{CNOT}\}$ → cancel both.
- Rotation merge: adjacent $R_z(\alpha)$, $R_z(\beta)$ → $R_z(\alpha + \beta)$.
- Commuting group simplifications.

**3. Classical Redundancy Elimination:** For classical nodes with matching value numbers, divert edges as in Algorithm IX.

**4. Cross-Region Value Numbering (γ-nodes):** After processing all γ-regions, compare value numbers of exit variable origins across regions. If all regions produce the same value number for an exit variable, the computation can be hoisted before the γ-node.

**Quantum constraints:**

- Full unitary equivalence checking is $O(4^n)$ for $n$ qubits. Practical GVN uses polynomial-time heuristics.
- Even when value-equivalent, quantum outputs cannot be merged (no-cloning). GVN for quantum primarily enables gate identity cancellation, parameter deduplication, and cross-function equivalence detection.

**Complexity:** $O(N \cdot \alpha(N))$ plus $O(1)$ hash computation per node.

---

### Algorithm XXIII: Sparse Conditional Constant Propagation (SCCP)

*Source: LLVM SCCP. Propagates known classical values through the QRVSDG.*

LLVM's SCCP assumes values constant until proven otherwise and assumes blocks dead until proven reachable. This algorithm handles the purely classical domain: classical simple node evaluation, γ-node/θ-node predicate resolution, and classical constant replacement. Quantum basis state tracking is handled separately by Algorithm XXXIX (BSCP).

**1. Initialize:**

- For each constant classical node: mark output as *known value*.
- Mark all γ-region reachability as *unknown*.

**2. Propagate (worklist-based):**

Process nodes from a worklist. For each node $n$:

- **Classical simple node**: If all inputs are known constants, evaluate and mark outputs as known.

- **γ-node with constant predicate**: Mark selected region $R_v$ as reachable, all others unreachable. Propagate known values into $R_v$.

- **θ-node**: If predicate is known false on first iteration: loop executes once. Propagate through single iteration.

**3. Replace:** For each node with all outputs known, replace with constants. Remove dead code via Algorithms X/XIV.

**Complexity:** $O(N_c + E_c)$ where $N_c$ and $E_c$ are classical nodes and edges (lattice height bounded: unknown → known constant).

**Example:**

```
Before SCCP:
  a = lit(3)
  b = lit(7)
  c = a + b                 — c = 10 (known)
  d = c > 5                 — d = true (known)
  γ-node(d):
    R₀: expensive_path()    — unreachable (d = true ≠ false)
    R₁: fast_path()         — always taken

After SCCP:
  c = 10, d = true (constants)
  γ-node eliminated, R₁ inlined
```

---

### Algorithm XXXIX: Basis State and Constant Propagation (BSCP)

*Source: Novel. Extends Algorithm XXIII (SCCP) with quantum basis state tracking through Clifford gates and measurement outcome resolution.*

SCCP (Algorithm XXIII) handles classical constant propagation but treats quantum nodes as opaque. BSCP extends it to track quantum basis states ($|0\rangle$, $|1\rangle$) through Clifford gates, resolve deterministic measurement outcomes to classical constants, and use those constants to drive γ-node predicate resolution — bridging the quantum-classical boundary.

**1. Initialize:**

- Run SCCP (Algorithm XXIII) to establish classical constants.
- For each Prep node: mark quantum output as *known basis state* $|0\rangle$ (or specified initial state).

**2. Propagate (worklist-based):**

Process nodes from a worklist, extending SCCP's classical worklist with quantum-aware rules:

- **UGate node (single-qubit, Clifford)**: If quantum input is a known basis state and the gate maps basis states to basis states: compute output state. Example: $X|0\rangle = |1\rangle$ (known), $H|0\rangle = |+\rangle$ (not a basis state — mark unknown for measurement).

- **UGate node (parameterized)**: If all classical parameters are known (from SCCP): if gate evaluates to identity ($R_z(0) = I$), mark output as input's state and mark gate for removal.

- **Measure node**: If quantum input is a known basis state $|b\rangle$: mark Bit output as constant $b$, mark post-measurement Qubit output as $|b\rangle$. Feed constant bit back into SCCP's classical worklist for γ-node predicate resolution.

- **γ-node with constant predicate** (from measurement): Mark selected region $R_v$ as reachable, all others unreachable. Propagate known basis states and classical values into $R_v$.

**3. Replace:** For each quantum node with known basis state output feeding only measurement, replace the measurement with a constant. For each identity gate ($R_z(0)$), remove and connect input to output. Remove dead code via Algorithms X/XIV.

**Quantum constraints:**

- Beyond $|0\rangle$ and $|1\rangle$, quantum states cannot be represented as classical constants. BSCP is limited to basis state tracking through Clifford gates and classical parameter folding.
- Measurement of superposition is random — BSCP cannot propagate through such measurements.
- Multi-qubit gates (CNOT, CZ) with one known basis-state input can be partially resolved: e.g., $\text{CNOT}(|0\rangle, q) \to (|0\rangle, q)$ (identity on target when control is $|0\rangle$).

**Complexity:** $O(N + E)$ (lattice height bounded: unknown → known basis state / known constant).

**Example:**

```
Before BSCP:
  Prep q₀ |0⟩
  X(q₀)                    — q₀ is now |1⟩ (known basis state)
  Measure(q₀) → m₀         — m₀ = 1 (known, bridges to classical)
  γ-node(m₀):
    R₀: correction(q₁)     — unreachable (m₀ = 1 ≠ 0)
    R₁: identity(q₁)       — always taken

After BSCP:
  m₀ = 1 (constant, from basis state tracking)
  γ-node eliminated, R₁ inlined
```

---

### Algorithm XXV: Quantum Instruction Combining (Gate Fusion)

*Source: LLVM instcombine. Combines adjacent quantum gates into fewer equivalent operations.*

LLVM's instcombine forms fewer, simpler instructions from redundant or combinable patterns. The quantum analog is gate fusion/merging along qubit state chains.

For each qubit state chain in the QRVSDG (within each region):

**1. Adjacent same-axis rotation merging:**

For consecutive $R_\alpha(\theta_1)$ and $R_\alpha(\theta_2)$ on the same qubit (same rotation axis $\alpha$):
- Replace with $R_\alpha(\theta_1 + \theta_2)$.
- If $\theta_1 + \theta_2 = 0 \pmod{2\pi}$: remove both.
- If $\theta_1 + \theta_2 = k \cdot \pi/4$ for small $k$: replace with named gate ($T$, $S$, $Z$).

**2. Self-inverse cancellation:**

For consecutive identical self-inverse gates $G \cdot G$ on the same qubit(s):
- If $G \in \{H, X, Y, Z, \text{CNOT (same operands)}\}$: remove both.

**3. Known product rules:**

For consecutive gates matching known product identities:
- $X \cdot R_z(\theta) \cdot X = R_z(-\theta)$
- $H \cdot R_z(\theta) \cdot H = R_x(\theta)$
- $S \cdot H \cdot S^\dagger = H \cdot Z$

Replace matched patterns with their simplified form.

**4. Template matching (optional):**

For sequences matching known circuit templates: Toffoli decomposition → single Toffoli node, 3 CNOT → SWAP node.

**5. Classical parameter combining:**

For consecutive classical operations feeding gate parameters: apply standard constant folding and algebraic simplification.

**Quantum constraints:**

- Unitarity preservation: any combination must produce a valid unitary.
- Gate set constraints: fusion may produce gates outside the native set, requiring subsequent decomposition.

**Complexity:** $O(N_q \cdot T)$ where $T$ is template count.

**Example:**

```
Before gate fusion:
  Rz(π/4, q₀)    — T gate
  Rz(π/4, q₀)    — another T gate
  H(q₀)
  H(q₀)

After gate fusion:
  Rz(π/2, q₀)    — merged: T·T = S
  (H·H removed: identity)

Net result: 4 gates → 1 gate (S)
```

---

### Algorithm XXXVIII: Classical Constant Folding

*Source: LLVM ConstantFolding, Binaryen OptimizeInstructions (constant sub-pass). Collapses pure classical expressions with all-literal inputs into literal results.*

This algorithm specifies two tiers of constant folding. **Tier 1 (gate parameter folding)** targets gate parameter expression trees containing literals ($\pi$, $e$, $\tau$, numeric constants) and arithmetic/trigonometric operations. **Tier 2 (general classical folding)** extends to all classical integer, floating-point, and boolean instruction folding as described below. An implementation may support Tier 1 alone as a useful subset.

LLVM's `ConstantFolding` evaluates instructions whose operands are all compile-time constants and replaces them with the computed literal. Binaryen's `OptimizeInstructions` contains a similar sub-pass for WebAssembly constant expressions. While SCCP (Algorithm XXIII) and BSCP (Algorithm XXXIX) subsume this via full lattice-based analyses, a dedicated lightweight constant folding pass is valuable as a cheap fixpoint-free cleanup that runs in $O(N_c)$ and can be invoked repeatedly between other passes without the overhead of their worklists.

For each region $\mathcal{R}$, process classical simple nodes in reverse-postorder (topological order):

**1. Unary constant folding:**

For each classical unary simple node $n$ with operator $\text{op}$ and single input $i$:

- If $\text{origin}(i)$ is a literal node producing constant value $c$:
  - Evaluate $v = \text{op}(c)$ at compile time.
  - Create a new literal node $\ell$ producing $v$ in $\mathcal{R}$.
  - Divert all edges from $O_n$ to $O_\ell$.
  - Mark $n$ dead.

Applicable operators: arithmetic negation ($-x$), bitwise NOT ($\lnot x$), logical NOT ($!x$), type casts (e.g., `Int → Float`), trigonometric functions on constant angles ($\sin c$, $\cos c$), and any other pure single-input classical operation.

**2. Binary constant folding:**

For each classical binary simple node $n$ with operator $\text{op}$ and inputs $i_0, i_1$:

- If $\text{origin}(i_0)$ is a literal producing $c_0$ and $\text{origin}(i_1)$ is a literal producing $c_1$:
  - Evaluate $v = \text{op}(c_0, c_1)$ at compile time.
  - If the evaluation is well-defined (no division by zero, no overflow in fixed-width semantics):
    - Create a new literal node $\ell$ producing $v$ in $\mathcal{R}$.
    - Divert all edges from $O_n$ to $O_\ell$.
    - Mark $n$ dead.

Applicable operators: `+`, `-`, `*`, `/`, `%`, `**`, bitwise (`&`, `|`, `^`, `<<`, `>>`), comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`), logical (`&&`, `||`), `min`, `max`, `atan2`, and any other pure two-input classical operation.

**3. Partial constant folding (identity/annihilator):**

For each classical binary simple node $n$ where exactly one input is a literal:

- $x + 0 \to x$, $x - 0 \to x$, $0 + x \to x$
- $x \cdot 1 \to x$, $1 \cdot x \to x$, $x \cdot 0 \to 0$, $0 \cdot x \to 0$
- $x / 1 \to x$
- $x \mathbin{\&} 0 \to 0$, $x \mathbin{|} 0 \to x$, $x \oplus 0 \to x$
- $x \mathbin{\&\&} \text{false} \to \text{false}$, $x \mathbin{||} \text{true} \to \text{true}$

Divert edges from $O_n$ to the surviving operand's origin (or a new literal for annihilator cases). Mark $n$ dead.

**4. Recursive cleanup:**

After folding, some predecessor nodes may become dead (their only user was the folded node). Invoke Algorithm X (DNE) or simply re-traverse; the topological processing order naturally propagates constants forward without a separate fixpoint.

**Quantum interaction:**

- This pass operates exclusively on classical simple nodes. Quantum nodes (UGate, Prep, Measure) are never folded.
- Folded literals feeding UGate parameter inputs enable downstream Algorithm XXV (Gate Fusion) to merge parameterized rotations: e.g., after folding $\pi/8 + \pi/8 \to \pi/4$, a $R_z(\pi/8 + \pi/8)$ node becomes $R_z(\pi/4)$, which Algorithm XXV can recognize as a $T$ gate.
- Folded comparison results feeding γ-node predicates enable Algorithm XIV (Extended DCE) to eliminate unreachable branches without running full SCCP.

**Complexity:** $O(N_c)$ where $N_c$ is the number of classical simple nodes, since each node is visited once in topological order and evaluation is $O(1)$ per node.

**Example:**

```
Before constant folding:
  a = lit(π)                 — literal
  b = lit(4)
  c = a / b                  — π/4
  d = lit(2)
  e = c * d                  — π/2
  Rz(e, q₀)                  — Rz(π/2, q₀)

After constant folding:
  e = lit(π/2)               — folded: (π/4) * 2 = π/2
  Rz(e, q₀)                  — Rz(π/2, q₀) = S gate
  (a, b, c, d dead → removed by DNE)

Partial folding example:
  f = x * lit(1)             — identity: collapses to x
  g = y + lit(0)             — identity: collapses to y
  h = z && lit(false)        — annihilator: collapses to lit(false)
```

---

## Group 3: Code Motion Algorithms

### Algorithm XV: Quantum Code Folding (Suffix Factoring)

*Source: Binaryen Code Folding. Dual of Algorithm XI Push-Out. Factors common gate suffixes out of γ-nodes.*

Binaryen's Code Folding merges identical tails (suffixes) of if-else branches. Algorithm XI (Push-Out) factors common *prefixes*; this algorithm factors common *suffixes*. Together they extract all common boundary operations from γ-nodes.

For each γ-node $\gamma$, examine each exit variable $ex = (R, o)$ where $\text{type}(o) = \text{Qubit}$:

**1.** For each region $R_j$, identify the last operation $n_j$ producing the result $r^{R_j}$ of this exit variable.

**2.** If the same gate operation is the last producer in *all* regions:

$$\forall j, j' : \text{op}(n_j) = \text{op}(n_{j'}) \wedge \text{classical\_inputs}(n_j) \cong \text{classical\_inputs}(n_{j'})$$

then the gate can be folded out:

**(a)** Remove $n_j$ from each region $R_j$. Update each region's result $r^{R_j}$ to point to $n_j$'s quantum input origin.

**(b)** Create the gate node $n_{\text{folded}}$ after the γ-node, consuming the exit variable's output $o$.

**(c)** Divert all users of $o$ to $n_{\text{folded}}$'s quantum output. Update $o$ to feed $n_{\text{folded}}$'s input.

**3.** Repeat for each exit variable. Apply iteratively to peel multiple suffix layers.

**Correctness conditions:**
- The gate appears as the last operation on the qubit chain in *every* region.
- Classical parameters are congruent across all regions and available in the parent scope.
- For multi-qubit gates: all qubit operands have matching suffix gates.
- Linear typing is preserved.

**Complexity:** $O(|\gamma| \cdot |EX_q| \cdot k)$ where $k$ is the number of γ-regions.

**Example:**

```
Before (QEC syndrome correction):
  γ-node(syndrome):
    R₀: I(q) → H(q)    R₁: X(q) → H(q)
    R₂: Z(q) → H(q)    R₃: Y(q) → H(q)

After suffix folding:
  γ-node(syndrome):
    R₀: I(q)   R₁: X(q)   R₂: Z(q)   R₃: Y(q)
  H(q_out)    — folded out
```

---

### Algorithm XXII: Quantum Loop-Invariant Code Motion (Extended LICM)

*Source: LLVM LICM. Extends Algorithm XII (Invariant Value Redirection) with sinking and quantum-specific invariant analysis.*

Algorithm XII (INV) redirects invariant values from θ- and γ-nodes per [1] Section 7.1. Extended LICM builds on this foundation, adding hoisting and sinking capabilities beyond basic invariant value redirection.

LLVM's LICM hoists loop-invariant computations out of loops and sinks loop-invariant stores to exit blocks. Algorithm XII identifies loop-invariant values; this algorithm adds sinking, quantum circuit template hoisting, and self-contained chain detection.

For each θ-node $\theta$:

**Phase 1 — Invariant Identification (from Algorithm XII):**

**1.** Mark loop variable $lv = (i, a, r, o)$ as invariant if $\text{origin}(r) = a$.

**2.** Mark node $n$ as invariant if all inputs originate from invariant loop variables, constants, or other invariant nodes.

**Phase 2 — Classical Hoisting (from Algorithm XII):**

**3.** For each invariant classical node $n$: move $n$ before the θ-node. Update dependencies.

**Phase 3 — Classical Sinking:**

**4.** For each node $n$ in the θ-region whose outputs are consumed *only* by loop variable results that map to θ-outputs (not to next-iteration arguments or the predicate):

- $n$ is a post-loop computation. Move $n$ after the θ-node.
- Update the loop variable: $r$ receives $n$'s input; $n$ consumes $o$ outside the loop.

**Phase 4 — Quantum Circuit Template Hoisting:**

**5.** For each sequence of UGate nodes within the θ-region that depend only on invariant loop variables and constants:

- The gate sequence constitutes a *fixed circuit template*.
- Annotate for single-compilation: the circuit description is computed once and the runtime dispatches the pre-compiled circuit each iteration.
- This is a metadata annotation, not a graph transformation (quantum state must still flow through each iteration).

**Phase 5 — Self-Contained Quantum Chain Detection:**

**6.** For each Prep node $p$ in the θ-region: trace the qubit state chain from $p$ through gates and measurement.

- If the chain terminates at MeasureD within the θ-body (before feeding back to a loop variable): the chain is self-contained.
- Mark self-contained chains as independently schedulable per iteration.

**Phase 6 — Conjugate Loop Invariance:**

**7.** For each Conjugate node $C$ within the θ-region: if all classical parameters to $\mathcal{R}_{\text{outer}}$ originate from loop-invariant values (entry-port connects to exit-port or constants):

- The outer region's circuit structure is loop-invariant. Annotate for single-compilation: the outer and adjoint circuit descriptions are computed once.
- The inner region may still depend on non-invariant parameters (this is the typical case for variational algorithms where the outer region prepares a basis and the inner region contains parameterized rotations).

**Quantum constraints:**

- Quantum state is never hoistable across measurement: even if the same circuit applies each iteration, quantum state must be re-prepared. Only the *classical description* can be pre-compiled.
- Parameter-dependent circuits (gates with non-invariant parameters) cannot be hoisted.

**Complexity:** $O(N + E)$.

**Example:**

```
VQE θ-node:
  lv_H: Hamiltonian (invariant)    lv_θ: parameters (non-invariant)

  Body:
    n₁: decompose(H)           — invariant → hoist
    n₂: build_circuit(n₁, θ)   — depends on non-invariant θ
    Prep → ansatz → Measure     — self-contained chain every iteration

After LICM:
  decompose(H) hoisted before θ-node
  Prep-ansatz-Measure chain marked self-contained
```

---

### Algorithm XXVI: Quantum Node Push-Through

*Source: RVSDG-native push operations. Generalizes Algorithm XI to all structural node types.*

From Reissmann's RVSDG work, pushing simple nodes through structural nodes moves a node from inside a structural node's region to outside (or vice versa). This generalizes code motion across structural boundaries.

**Push through γ-nodes:** Already specified by Algorithm XI Push-Out.

**Push through θ-nodes:**

For each θ-node $\theta$, for each UGate node $n$ in the θ-region:

**1.** If all inputs of $n$ originate from loop-invariant loop variable arguments or constants: move $n$ before the θ-node. Update the θ-node's inputs to reflect $n$'s output. Create a passthrough loop variable for $n$'s output value.

**2.** Quantum gate push-through θ: If $n$ operates on a quantum loop variable $lv = (i, a, r, o)$ where $n$ is the *first* operation on $a$ and $n$'s classical parameters are all loop-invariant: move $n$ before the θ-node. $n$ consumes $\text{origin}(i)$, and $i$ is updated to $n$'s output.

This is safe only if $n$ operates identically on the initial input and the loop-back value; since the loop-back quantum state may differ, this push-through is valid only if $n$ commutes with the entire loop body's operations on that qubit.

**Push backward into θ-nodes:**

**3.** For each UGate node $n$ immediately after a θ-node, consuming output $o$ of loop variable $lv$: if $n$ should be applied each iteration, move $n$ into the θ-region at the end of the loop body, just before result $r$.

**Push through Conjugate nodes:**

**4.** For each UGate node $n$ immediately before a Conjugate node $C$ on a through-qubit:

- Push $n$ into $\mathcal{R}_{\text{outer}}$ as the first operation on that qubit. The adjoint $n^\dagger$ is auto-appended to the adjoint phase. Net effect on the through-qubit: $n^\dagger \cdot U \cdot V \cdot U^\dagger \cdot n$ — this changes the Conjugate's semantic. Only safe if $n$ was intended to be part of the preparation.
- **Preferred alternative:** Push $n$ into $\mathcal{R}_{\text{inner}}$ instead (does not alter the outer region or adjoint). This is always safe and enables cancellations with inner-region gates.

**5.** For each UGate node $n$ immediately after a Conjugate node $C$ on a through-qubit:

- If $n = (U_{\text{last}})^\dagger$ (the adjoint of the last gate of the outer region on that qubit), the two cancel: the adjoint phase ends with $U_{\text{last}}^\dagger$ and $n$ follows. Remove both.

**Complexity:** $O(N)$.

---

### Algorithm XXVII: Quantum Node Pull-Through

*Source: RVSDG-native pull operations. Extends Algorithm XI Pull-In to all structural node types.*

The reverse of Push-Through: pulling simple nodes from outside a structural node's region into the region. Beneficial when a node's result is only used within one branch (reduces noise exposure in NISQ).

**Pull into γ-nodes (extends Algorithm XI Pull-In):**

For each UGate node $n$ immediately before a γ-node $\gamma$:

**1.** Determine which γ-regions use $n$'s quantum output:
- If used in *all* regions: keep outside.
- If used in *one* region $R_v$: pull $n$ into $R_v$.
- If used in *some* but not all: pull $n$ into each using region, creating region-specific copies (valid because only one region executes at runtime).

**Pull into θ-nodes:**

For each UGate node $n$ immediately before a θ-node whose quantum output feeds a loop variable input:

**2.** If the gate should be applied only on the first iteration (initialization): keep outside.

**3.** If the gate should be applied every iteration: move $n$ into the θ-body at the beginning.

**Noise-motivated Pull-In:**\n\n**4.** For NISQ targets: if a gate before a γ-node introduces noise on a qubit used in only one branch, pulling the gate into that branch reduces noise exposure. Use δ-node gate fidelity data for this decision.

**Pull into Conjugate inner region:**

**5.** For each UGate node $n$ immediately before a Conjugate node $C$ on a through-qubit: pulling $n$ into $\mathcal{R}_{\text{inner}}$ (as the first operation on that qubit in the inner region) is always safe and preferred over pushing into the outer region. This enables cancellations between $n$ and inner-region gates without altering the conjugation structure.

**6.** For each UGate node $n$ immediately after a Conjugate node $C$ on a through-qubit: pulling $n$ backward into $\mathcal{R}_{\text{inner}}$ (as the last operation) is safe and may enable gate fusion with existing inner-region operations.

**Complexity:** $O(|\gamma| \cdot |EV_q|)$.

---

## Group 4: Function-Level Algorithms

### Algorithm XVIII: Quantum Function Inlining

*Source: LLVM inline, Binaryen inlining heuristics. Inlines λ-node bodies at apply-node call sites.*

LLVM's inline pass performs bottom-up inlining using cost-benefit heuristics. Binaryen adds sizing constraints: an always-inline threshold for trivially small functions, a flexible threshold for lightweight functions at high optimization levels, a single-caller threshold, and a post-inlining combined size limit to prevent function bloat. Function inlining is uniquely powerful for quantum: it exposes gate cancellation opportunities at function call boundaries.

**Inlining parameters** (from δ-node configuration or compiler defaults):

| Parameter | Symbol | Default | Description |
|-----------|--------|---------|-------------|
| Always-inline threshold | $T_{\text{always}}$ | 2 nodes | Functions this small are always inlined (smaller than a call) |
| Single-caller threshold | $T_{\text{one}}$ | $\infty$ | Max size when there is exactly one call site |
| Flexible threshold | $T_{\text{flex}}$ | 20 nodes | Max size for lightweight functions (no loops, no calls) at aggressive optimization |
| Max combined size | $T_{\text{combined}}$ | configurable | Post-inlining size limit: reject if $|N_{\text{target}}| + |N_\lambda|$ exceeds this |
| Allow loops | $F_{\text{loops}}$ | false | Whether to inline functions containing θ-nodes |
| Allow partial | $F_{\text{partial}}$ | false | Whether to split functions and inline only the fast path |

**1. Candidate selection:** For each apply-node $c$ invoking λ-node $\lambda$:

Evaluate inlining eligibility in priority order:

**(a) Always inline:** If $|N_\lambda| \leq T_{\text{always}}$, inline unconditionally. Functions this small (1–2 nodes) are cheaper inlined than called.

**(b) Single caller:** If $|\text{CLL}(\lambda)| = 1$ and $\lambda$ is not exported and $|N_\lambda| \leq T_{\text{one}}$, inline. The original λ-node can be removed after inlining, so code size does not increase.

**(c) Lightweight functions:** If $|N_\lambda| \leq T_{\text{flex}}$ and $\lambda$'s body contains no θ-nodes (unless $F_{\text{loops}}$ is set) and no apply-nodes (no calls), inline when optimizing aggressively for gate count or depth.

**(d) Quantum boundary benefit:** If boundary gate cancellation is possible (last gate of caller's qubit chain cancels with first gate of λ-body, or first gate of λ-body is the adjoint of the preceding gate on a through-qubit), add bonus $+k$ to the score even if the function exceeds $T_{\text{flex}}$.

**(e) Region annotation penalty:** If $\rho(\mathcal{R}_\lambda) \not\subseteq \rho(\mathcal{R}_\text{parent})$, reject or penalize.

**2. Post-inlining size check:** Before committing the inlining, verify:

$$|N_{\text{target}}| + |N_\lambda| \leq T_{\text{combined}}$$

where $|N_{\text{target}}|$ is the node count of the function being inlined *into*. If the combined size exceeds $T_{\text{combined}}$, reject the inlining even if other criteria pass. This prevents function bloat from cascading inlining, which is critical for quantum programs where large flat circuits slow down superlinear optimization passes ($O(n^3)$ ZX-calculus, equality saturation).

**3. Region annotation check:** If $\rho(\mathcal{R}_\lambda) = R_H$ and $\rho(\mathcal{R}_\text{parent}) = R_U$: reject inlining or weaken parent region to $R_H$.

**4. Inlining transformation:**

**(a)** Copy all nodes from $\lambda$'s region into $c$'s parent region.

**(b)** For each context variable $cv = (i, a)$ of $\lambda$: connect the copy of $a$'s consumers to $\text{origin}(i)$.

**(c)** For each function argument $k$ of $\lambda$: connect the copy of argument $a_k$'s consumers to the apply-node's $k$-th argument input origin.

**(d)** For each result $r_k$ of $\lambda$'s region: divert all users of $c$'s $k$-th output to the copy of $\text{origin}(r_k)$.

**(e)** Remove the apply-node $c$.

**5. Post-inlining optimization:** Run CNE (Algorithm IX) and DNE (Algorithm X) to exploit boundary cancellations.

**Quantum constraints:**

- Qubit count conservation: linear typing ensures no qubit duplication.
- Region annotation compatibility: inlining hybrid λ-nodes into unitary regions violates $R_U$ annotation constraints.
- Combined size limit is especially important for quantum: inlining a 100-gate subroutine called 10 times creates a 1000-gate flat region where $O(n^3)$ ZX-calculus and equality saturation become expensive. The size check prevents this while still allowing targeted inlining for boundary cancellation.

**Complexity:** $O(|N_\lambda| + |E_\lambda|)$ per inlining. Candidate selection: $O(|\text{apply-nodes}|)$.

**Example:**

```
Before inlining:
  λ_bell: gate bell(q₁, q₂) { H(q₁); CNOT(q₁, q₂) }
  Main: H(q₀); apply bell(q₀, q₁); Measure(q₀, q₁)

After inlining:
  Main: H(q₀); H(q₀); CNOT(q₀, q₁); Measure(q₀, q₁)

After boundary CNE:
  H(q₀)·H(q₀) → identity (cancelled!)
  CNOT(q₀, q₁); Measure(q₀, q₁)

Net: inlining enabled gate cancellation at function boundary.
```

---

### Algorithm XIX: λ-Node Reordering

*Source: Binaryen ReorderFunctions. Reorders λ-nodes in the ω-region for improved destruction output.*

Binaryen's ReorderFunctions puts more-called functions first to reduce variable-length encoding size. In the QRVSDG, λ-node ordering determines serialization order during destruction.

**1.** Compute call frequency: $\text{freq}(\lambda) = |\text{CLL}(\lambda)|$.

**2.** Sort λ-nodes by:
- Primary key: call frequency (descending)
- Secondary key: region annotation ($R_U$ before $R_H$ before $R_{EC}$)
- Tertiary key: qubit count $|A^q_\lambda|$ (ascending)

**3.** Assign function indices in sorted order for QIR output.

**Complexity:** $O(|\lambda| \log |\lambda|)$.

---

### Algorithm XX: θ-Node Loop Variable Optimization

*Source: Binaryen SimplifyLocals. Simplifies loop variables in θ-nodes.*

Binaryen's SimplifyLocals optimizes local variable usage. In the QRVSDG, the analog is loop variable optimization: eliminating redundant loop variables, detecting known-state classical demotion, and simplifying measurement-outcome loop variables.

For each θ-node $\theta$:

**1. Redundant loop variable merging:** For all pairs $lv_1, lv_2$ where $i_{lv_1} \cong i_{lv_2}$ and $r_{lv_1} \cong r_{lv_2}$:
- Divert all uses of $a_{lv_2}$ to $a_{lv_1}$ and $o_{lv_2}$ to $o_{lv_1}$.
- Remove $lv_2$. (Already covered by CNE on θ-nodes.)

**2. Known-state classical demotion:** For each quantum loop variable $lv$ where the last operation before result $r$ is a Reset or Prep (qubit always re-initialized to $|0\rangle$) and the origin of input $i$ is also a Prep:
- The qubit carries no information between iterations.
- Mark for optimization: may be allocated freshly each iteration rather than threaded as a loop variable.

**3. Measurement-outcome loop variable:** If a classical loop variable carries a measurement result computed fresh each iteration (not accumulated): convert to region-local computation.

**Complexity:** $O(|LV|^2)$ for pairwise comparison.

---

### Algorithm XXIV: Quantum Tail Call Optimization (φ → θ Conversion)

*Source: LLVM tailcallelim. Converts tail-recursive quantum φ-nodes to θ-nodes.*

LLVM's `tailcallelim` transforms self-recursive tail calls into loops. For quantum, this converts recursive algorithms (QFT, quantum walks, concatenated error correction) from φ-node representation to θ-node loops.

For each φ-node $\phi$ containing a single λ-node $\lambda$:

**1.** Identify tail-recursive calls: scan $\lambda$'s region for apply-nodes in tail position (output directly connects to λ-region result).

**2.** If tail call found with structure: body → base case check → recursive call with modified args.

**3.** Construct θ-node:
- **Loop variables**: one for each λ-argument (including quantum parameters).
- **θ-body**: the λ-body up to the recursive call.
- **Predicate**: negate the base case condition.
- **Next iteration values**: modified arguments from the recursive apply-node.
- **Loop output values**: base case return values.

**4.** Replace the φ-node with the θ-node.

**5.** Quantum linear typing verification: each quantum loop variable must thread correctly through the body.

**Quantum constraints:**

- Qubit conservation across recursion: each recursive call must pass the same number of qubits.
- Bounded recursion: quantum recursion is always bounded (finite qubit registers).

**Complexity:** $O(|N_\lambda| + |E_\lambda|)$.

**Example:**

```
Recursive QFT (φ-node):
  φ { λ qft(qubits, n):
    if n == 0: return qubits
    H(qubits[0])
    for k in 1..n-1: CPhase(π/2^k, qubits[0], qubits[k])
    return qft(qubits[1:], n-1)   ← tail call
  }

After tail call optimization (θ-node):
  θ { body: H(qubits[offset]); CPhase gates;
      r_n = n-1; r_offset = offset+1;
      predicate: n > 1 }
```

---

## Group 5: Structural Algorithms

### Algorithm XXVIII: Classical Node Splitting for Quantum Depth Reduction

*Source: RVSDG node splitting (dual of CSE). Duplicates classical computations to reduce quantum circuit depth.*

Node splitting duplicates a node when beneficial. For quantum programs, this applies only to *classical* computations (quantum nodes cannot be split due to no-cloning). Duplicating classical parameter computations can reduce critical path depth.

For each classical simple node $n$ in a region where $|\text{users}(o_n)| > 1$:

**1.** Analyze critical path: if $n$ is on the critical path and has multiple consumers on distinct qubits.

**2.** For each consumer $c$: if duplicating $n$ at $c$'s location would shorten the critical path:
- Create a copy $n'$ of $n$ with the same inputs.
- Redirect $c$'s input from $o_n$ to $o_{n'}$.

**3.** If $n$ has no remaining users: mark dead.

**Quantum constraint:** Only classical nodes may be split: $\text{type}(o_n) = \text{Qubit} \implies \text{no splitting}$.

**Complexity:** $O(N_c \cdot |\text{users}|_{\max})$.

**Example:**

```
Before splitting:
  θ_compute = expensive_angle(params)   — on critical path
  Rz(θ_compute, q₀)                    — uses angle
  Rz(θ_compute, q₁)                    — uses angle (fan-out)

After splitting:
  θ_compute₁ = expensive_angle(params)  — copy 1
  θ_compute₂ = expensive_angle(params)  — copy 2
  Rz(θ_compute₁, q₀)                   — parallel
  Rz(θ_compute₂, q₁)                   — parallel

Circuit depth reduced: both Rz gates execute independently.
```

---

### Algorithm XXIX: Quantum Region Flattening

*Source: RVSDG region restructuring. Removes unnecessary nesting of structural nodes.*

Region flattening removes unnecessary nesting. Nested conditionals can be flattened into multi-way branches, and nested loops can be simplified.

**1. Nested γ-node flattening:**

For each γ-node $\gamma_\text{outer}$ containing a directly nested γ-node $\gamma_\text{inner}$ (the only content of one or more regions):

**(a)** Verify the inner predicate is available at the outer level.

**(b)** Create a combined predicate: if $\gamma_\text{outer}$ has $m$ regions and $\gamma_\text{inner}$ has $n$ regions, the flattened γ-node has up to $m \times n$ regions.

**(c)** For each reachable combination $(i, j)$: create a flattened region containing the operations from $\gamma_\text{outer}$'s region $R_i$ with $\gamma_\text{inner}$ replaced by its region $R_j$.

**(d)** Construct the combined predicate as a classical encoding of both predicate values.

**(e)** Thread quantum entry/exit variables through the flattened structure.

**2. θ-γ simplification:**

For each θ-node $\theta$ whose body is a single γ-node $\gamma$ (conditional continue/exit):

**(a)** Absorb $\gamma$ into the θ-node: one γ-region becomes the continue path, the other the exit path.

**(b)** The γ-predicate becomes the θ-predicate.

**3. Unitary region extraction:**

For each hybrid region $R_H$ containing a maximal subgraph of connected UGate/Barrier nodes (no measurements or classical ops):

**(a)** Create a virtual unitary sub-region $R_U$.

**(b)** Apply unitary-specific optimizations to $R_U$.

**(c)** Re-integrate optimized nodes into $R_H$.

**Quantum constraints:**

- Region annotation must be correct: merging unitary with hybrid produces hybrid.
- Quantum state routing through flattened multi-way structure must preserve linear typing.

**Complexity:** Step 1: $O(m \cdot n)$. Step 2: $O(|N_\theta|)$. Step 3: $O(N_q)$.

**Example:**

```
Before flattening (teleportation correction):
  Measure(ψ) → m₁, Measure(a) → m₂
  γ-node(m₂):
    R₀: identity(b)
    R₁: γ-node(m₁):
           R₀: X(b)
           R₁: X(b); Z(b)

After flattening (4-way γ-node on encode(m₁, m₂)):
    R₀₀: identity(b)    — m₁=0, m₂=0
    R₀₁: X(b)           — m₁=0, m₂=1
    R₁₀: Z(b)           — m₁=1, m₂=0
    R₁₁: Y(b)           — m₁=1, m₂=1 (XZ → Y up to phase)
```

---

### Algorithm XXXVII: θ-Node Loop Unrolling

*Source: LLVM LoopUnrollPass. Selective loop unrolling with sizing heuristics for the RVSDG.*

LLVM's `LoopUnrollPass` uses a cost model driven by the target's instruction cost table, trip count analysis, and a threshold system that balances code size against optimization opportunity. The pass distinguishes full unrolling (known trip count, entire loop removed), partial unrolling (fixed factor, loop retained with larger body), and runtime unrolling (trip count unknown, peel + unrolled body + remainder).

The QRVSDG analog operates on θ-nodes. The RVSDG's structure simplifies several aspects: loop variables are explicit (no φ-function discovery needed), the iteration count analysis reduces to tracing the predicate result's dependency chain, and demand-dependence ensures no artificial sequentialization obscures the unrolled body.

**Unrolling parameters** (configurable per optimization level):

| Parameter | Symbol | Default | Description |
|-----------|--------|---------|-------------|
| Full unroll threshold | $T_{\text{full}}$ | 150 nodes | Max total unrolled size ($N \cdot |body|$) for complete θ-node removal |
| Partial unroll threshold | $T_{\text{partial}}$ | 300 nodes | Max body size after partial unrolling ($F \cdot |body|$) |
| Max unroll factor | $F_{\max}$ | 8 | Maximum partial unroll factor |
| Max combined size | $T_{\text{combined}}$ | configurable | Post-unrolling size of enclosing function must stay under this |
| Allow with calls | $F_{\text{calls}}$ | false | Whether to unroll loops containing apply-nodes |
| Allow nested | $F_{\text{nested}}$ | true | Whether to unroll nested θ-nodes (inner first) |

**Phase 1 — Trip Count Analysis:**

**1.** For each θ-node $\theta$, determine the trip count:

**(a)** Trace the predicate result $r_0$ backward through the body to identify the induction variable pattern: a loop variable $lv = (i, a, r, o)$ with $r = a + \text{step}$ (or similar linear recurrence) and the predicate comparing $r$ against a bound.

**(b)** If both the initial value ($\text{origin}(i)$), step, and bound are compile-time constants: compute exact trip count $N$.

**(c)** If the step/bound are loop-invariant but not constant: trip count is symbolic (cannot fully unroll, can partially unroll).

**(d)** Otherwise: trip count is unknown (no unrolling or peel-only).

**Phase 2 — Benefit Analysis:**

**2.** Estimate the benefit of unrolling:

**(a)** Count body operations that are loop-invariant but trapped inside the loop (these become redundant copies that CNE/DNE will eliminate post-unroll).

**(b)** Count operations whose inputs from the previous iteration become known constants after unrolling (enabling constant folding).

**(c)** Estimate the code-size cost: $F \cdot |body|$ nodes added, minus the θ-node overhead removed (for full unrolling).

**(d)** Check combined size: if $|N_{\text{enclosing}}| + F \cdot |body| > T_{\text{combined}}$, reject (same bloat-prevention heuristic as Algorithm XVIII's post-inlining size check).

**Phase 3 — Unrolling Decision:**

**3.** Select unroll factor:

**(a) Full unrolling:** $N$ known and $N \cdot |body| \leq T_{\text{full}}$ → $F = N$. Removes the θ-node entirely.

**(b) Partial unrolling:** $N$ known or symbolic, $|body| \leq T_{\text{partial}} / F_{\max}$ → $F = \min(F_{\max}, \lfloor T_{\text{partial}} / |body| \rfloor)$. For known $N$: prefer factors that divide $N$ evenly (avoids remainder). For symbolic $N$: emit remainder handling.

**(c) No unrolling:** body too large, or body contains constructs that complicate unrolling (nested θ-nodes when $F_{\text{nested}} = \text{false}$, apply-nodes when $F_{\text{calls}} = \text{false}$).

**Phase 4 — Unrolling Transformation:**

**4. Full unrolling** ($F = N$):

**(a)** Replicate the θ-body $N$ times into the parent region.

**(b)** Wire loop variables: iteration $i$'s results connect to iteration $i+1$'s arguments. First iteration takes θ-node inputs; last iteration produces θ-node outputs.

**(c)** Remove the θ-node.

**5. Partial unrolling** ($F < N$):

**(a)** Create θ-node $\theta'$ whose body contains $F$ sequential copies of the original body.

**(b)** Counter increment: multiply step by $F$. Predicate: compare against $\lfloor N/F \rfloor \cdot F \cdot \text{step}$ (adjusted bound).

**(c)** If $N \mod F \neq 0$: emit remainder iterations inline after $\theta'$ (or a small θ-node with the original body for $N \mod F$ iterations).

**Phase 5 — Post-Unrolling Optimization:**

**6.** Run CNE (Algorithm IX) and DNE (Algorithm X) to eliminate redundant computations exposed by unrolling (loop-invariant copies, dead induction variable arithmetic).

**7.** Run SCCP (Algorithm XXIII) and BSCP (Algorithm XXXIX) to propagate classical constants and basis states through unrolled iterations.

**Optimization level mapping:**

| Level | Behavior |
|---|---|
| O0 | No unrolling |
| O1 | Full unrolling only, $T_{\text{full}} = 50$ |
| O2 | Full + partial, $T_{\text{full}} = 150$, $F_{\max} = 4$ |
| O3 | Full + partial, $T_{\text{full}} = 300$, $F_{\max} = 8$, $F_{\text{calls}} = \text{true}$ |
| Os | Full unrolling only if $N \cdot |body| < |body| + \text{call overhead}$ (net size decrease) |

**Complexity:** Trip count analysis: $O(|body|)$ per θ-node. Transformation: $O(F \cdot |body|)$. Post-unrolling cleanup: $O(F \cdot |body| \cdot \alpha(N))$.

**Example:**

```
Before (factorial loop, N=5):
  θ-node:
    lv_n: (5, a_n, a_n - 1, n_out)
    lv_r: (1, a_r, a_r * a_n, r_out)
    predicate: a_n > 1

After full unrolling (5 iterations, body size 3 ≤ T_full):
  r = 1 * 5 * 4 * 3 * 2 * 1 → constant folded to 120
  θ-node removed, entire computation replaced with constant
```

---

## Group 6: Inter-Procedural Whole-Program Algorithms

### Algorithm XXX: λ-Node Congruence Elimination

*Source: Binaryen duplicate-function-elimination. Extends Algorithm IX (CNE) to ω-level λ-node deduplication.*

Binaryen's `duplicate-function-elimination` hashes all functions structurally, groups them by hash, then pairwise-compares within groups for exact body equivalence. Duplicates are merged and callers redirected. The pass iterates because merging may expose new duplicates (if A calls C₁ and B calls C₂, and C₁ ≡ C₂, then after merging, A ≡ B).

The QRVSDG analog detects congruent λ-node body regions at the ω-level and merges them, redirecting all apply-nodes.

**Input:** QRVSDG $G$.

**Phase 1 — λ-Node Structural Hashing:**

**1.** For each λ-node $\lambda$ in the ω-region, compute a structural hash $h(\lambda)$ over the body region:
- Hash node types, operator names, and parameter counts.
- Hash edge connectivity (source output index → target input index).
- For quantum edges: include edge type (Qubit vs classical) in the hash.
- Use α-equivalent hashing: argument and context variable positions are canonical (not names).

**Phase 2 — Candidate Grouping:**

**2.** Group λ-nodes by hash: $\text{groups} = \{ h \mapsto \{\lambda \mid h(\lambda) = h\} \}$.

**Phase 3 — Exact Congruence Verification:**

**3.** For each group with $|\text{group}| \geq 2$, pairwise compare λ-nodes:
- Verify identical argument count and types (including Qubit zone annotations).
- Verify identical result count and types.
- Verify identical context variable count and types.
- Verify that for each context variable position $k$, $\text{origin}(cv^{\lambda_1}_k) \cong \text{origin}(cv^{\lambda_2}_k)$ in the enclosing region.
- Verify identical region annotation: $\rho(\mathcal{R}_{\lambda_1}) = \rho(\mathcal{R}_{\lambda_2})$. For $R_{EC}$ regions, additionally verify matching QEC parameters (code distance, syndrome extraction structure).
- Verify body region structural isomorphism using recursive node-by-node comparison (extend CNE's congruence check from Algorithm IX to entire regions).

For quantum-specific congruence:
- Linear edge topology must be isomorphic: qubit threading patterns (Prep → UGate chain → Measure) match exactly.
- Eff token threading patterns must match.
- Entanglement-creating patterns (multi-qubit gate connectivity) must be isomorphic.

**Phase 4 — Merge and Redirect:**

**4.** For each congruent pair $(\lambda_1, \lambda_2)$, choose $\lambda_1$ as the survivor:
- For every apply-node $c$ whose QFunc input originates from $\lambda_2$'s output: redirect to $\lambda_1$'s output.
- Remove $\lambda_2$ from the ω-region.

**Phase 5 — Iterate:**

**5.** Repeat Phases 1–4 until no further merging occurs. Merging may expose new congruences when callee identities change.

**Quantum constraints:**
- Merging λ-nodes with Qubit-typed arguments is safe because linearity is a caller-side property (each call site's qubit edges satisfy $|\text{users}(o)| = 1$ independently).
- λ-nodes with Qubit-typed context variables are automatically excluded from merging: linearity prevents two λ-nodes from capturing the same qubit ($|\text{users}(o)| = 1$), so context variable origins can never be congruent for Qubit-typed captures.
- λ-nodes with different region annotations ($R_U$ vs $R_H$) are never congruent, even if their gate sequences are identical (the annotation constrains permissible operations and adjoint/controlled derivation).
- QFunc types with different derivation annotations (adjoint, controlled) are not congruent.
- λ-nodes within φ-nodes are excluded from whole-program congruence elimination unless all recursive references through the φ-node are also congruent.

**Complexity:** $O(|\lambda|^2 \cdot |body|)$ within groups per iteration; at most $O(|\lambda|)$ iterations.

**Example:**

```
Before:
  λ_bell1: gate bell1(q₁, q₂) { H(q₁); CNOT(q₁, q₂) }
  λ_bell2: gate bell2(a, b)   { H(a); CNOT(a, b) }      — α-equivalent to bell1
  apply bell1(q₀, q₁); apply bell2(q₂, q₃)

After:
  λ_bell1: gate bell1(q₁, q₂) { H(q₁); CNOT(q₁, q₂) }
  apply bell1(q₀, q₁); apply bell1(q₂, q₃)              — bell2 merged into bell1
```

---

### Algorithm XXXI: λ-Node Call-Context Specialization

*Source: Binaryen monomorphize. Creates specialized λ-node variants based on call-site context.*

Binaryen's `monomorphize` creates specialized function copies based on call-site context (constant arguments, known globals, dropped results). It uses empirical cost evaluation: the specialized copy is optimized and compared against the original; the specialization is accepted only if beneficial.

The QRVSDG analog creates specialized λ-node variants when an apply-node provides known constants, specific zone annotations, or structurally determined inputs.

**Input:** QRVSDG $G$, cost model $\mathcal{C}$.

**Phase 1 — Context Analysis:**

**1.** For each apply-node $c$ invoking λ-node $\lambda$, classify each input:
- **Constant**: origin is a literal simple node or δ-node → movable into body.
- **Known λ-ref**: origin is another λ-node's output → can resolve indirect calls in body.
- **Known zone**: origin qubit edge has zone annotation $Z \neq \text{Unspecified}$ → can specialize zone handling.
- **Dead output**: $c$'s classical-typed output $o_k$ has $|\text{users}(o_k)| = 0$ → result can be dropped. (Qubit-typed outputs always have $|\text{users}| = 1$ by the linear typing invariant and cannot be dead.)
- **Unknown**: origin depends on dynamic computation → remains as parameter.

**2.** Build context key: $\text{ctx}(c) = \langle \text{const}_1, \ldots, \text{const}_m, \text{zones}, \text{dead\_outputs} \rangle$.

**3.** Skip if context is trivial (all unknown, no dead outputs).

**Phase 2 — Specialization:**

**4.** For each non-trivial context, check memoization: if $(\lambda, \text{ctx})$ already specialized, reuse.

**5.** Create $\lambda' = \text{copy}(\lambda)$ with modifications:
- For each constant argument $k$: remove argument $a_k$ from $\lambda'$; insert a literal node producing the constant value in $\lambda'$'s body, replacing all uses of $a_k$.
- For each dead output $k$: mark result $r_k$ as dead (enables DNE to remove producing computation in body).
- For each known zone: refine argument type from $\text{Qubit}\langle\text{Unspecified}\rangle$ to $\text{Qubit}\langle Z \rangle$.
- Adjust $\lambda'$'s QFunc type signature accordingly.

**Phase 3 — Cost Evaluation:**

**6.** Run a lightweight optimization pass on $\lambda'$'s body (CNE, DNE, SCCP, BSCP at minimum).

**7.** Compare costs: $\text{benefit} = \mathcal{C}(\lambda) + \mathcal{C}(\text{context\_ops}) - \mathcal{C}(\lambda')$.
- For quantum: $\mathcal{C}$ weights CNOT gates ≈ 10× single-qubit gates, T gates per the T-count cost model.
- Accept if $\text{benefit} > 0$.

**Phase 4 — Application:**

**8.** Add $\lambda'$ to the ω-region. Redirect apply-node $c$ to invoke $\lambda'$ with reduced arguments.

**Quantum-specific opportunities:**
- **Known initial state**: If all callers of a preparation subroutine always pass $|0\rangle$, specialize to skip state-checking logic.
- **Gate fusion after constant propagation**: Inlining known rotation angles enables rotation merging ($R_z(\theta_1) \cdot R_z(\theta_2) \to R_z(\theta_1 + \theta_2)$).
- **QEC code distance specialization**: A generic error-corrected subroutine specialized for distance-3 vs distance-5 can use different syndrome extraction circuits.

**Quantum constraints:**
- Qubit arguments are linear: specializing a qubit argument's zone does not copy the qubit; it only refines the type annotation on the edge entering the specialized body.
- If $\lambda'$ has a more restrictive region annotation (e.g., $R_U$ body with known-constant classical params eliminates all hybrid operations), update $\rho(\mathcal{R}_{\lambda'})$ accordingly.

**Complexity:** $O(|\text{apply-nodes}| \cdot |body|)$ for context analysis; lightweight optimization adds a bounded factor.

**Example:**

```
Before:
  λ_ansatz(θ, q): Ry(θ, q)                         — generic rotation
  apply ansatz(π/4, q₀); apply ansatz(π/2, q₁)

After specialization:
  λ_ansatz_pi4(q): Ry(π/4, q)                       — constant folded
  λ_ansatz_pi2(q): Ry(π/2, q)                       — constant folded
  apply ansatz_pi4(q₀); apply ansatz_pi2(q₁)

After further optimization on λ_ansatz_pi4:
  Ry(π/4) may be decomposed to specific native gates at compile time.
```

---

### Algorithm XXXII: Indirect Apply Resolution

*Source: Binaryen directize. Resolves indirect apply-nodes to direct λ-node references.*

Binaryen's `directize` converts `call_indirect` (table-based) calls into direct `call` instructions when the table index is a compile-time constant. This enables downstream inlining and optimization.

The QRVSDG analog resolves apply-nodes whose QFunc input is statically determinable (e.g., the origin traces back to a known λ-node through constants and δ-nodes).

**Input:** QRVSDG $G$.

**1.** For each apply-node $c$ with QFunc-typed first input $f$:

**2.** Trace origin of $f$ backward through the graph:
- If $\text{origin}(f) = \lambda.\text{output}$ for some λ-node $\lambda$: already direct. Skip.
- If $\text{origin}(f) = \text{load from δ-node}$ containing a QFunc literal referencing $\lambda$: resolve. Replace $c$'s $f$ input edge to connect directly to $\lambda$'s output.
- If $\text{origin}(f) = \text{γ-node exit variable}$ where all branch results trace to specific λ-nodes $\lambda_0, \lambda_1, \ldots$: create a new γ-node with the same predicate as the source γ-node. For each input of $c$ (Qubit, classical, Eff), create an entry variable. In branch $j$, create a direct apply-node invoking $\lambda_j$ with the entry variable arguments. Exit variables map per-branch apply-node results to the new γ-node's outputs, preserving types (including Qubit linearity and Eff threading per branch).
- Otherwise: unresolvable; leave as indirect.

**3.** After resolution, refine $c$'s output types to match the resolved λ-node's result types (may be narrower than the generic QFunc signature).

**Quantum constraints:**
- Resolving an indirect apply exposes the full λ-node body for cross-boundary analysis, enabling:
  - Boundary gate cancellation (Algorithm XVIII post-inlining)
  - Argument dead elimination (Algorithm XXXIV)
  - Signature type refinement (Algorithm XXXV)
- QFunc edges used through φ-nodes (recursive groups) may resolve to one of several mutually recursive λ-nodes; resolution requires call-graph analysis consistent with φ-node semantics.
- Linear qubit arguments are preserved: resolution changes only the QFunc-typed edge (which is classical/copyable), not qubit edges.

**Complexity:** $O(|apply\text{-}nodes|)$; origin tracing is bounded by graph depth.

**Example:**

```
Before:
  δ_circuit_table: [λ_hadamard, λ_cnot, λ_toffoli]
  idx = classical_computation()      — resolves to constant 1
  f = load(circuit_table, idx)       — indirect QFunc
  apply f(q₀, q₁)                   — indirect apply

After directize:
  apply λ_cnot(q₀, q₁)              — direct apply; enables inlining
```

---

### Algorithm XXXIII: Repeated Subgraph Extraction (Outlining)

*Source: Binaryen outlining. Extracts repeated subgraph patterns into shared λ-nodes (inverse of inlining).*

Binaryen's `outlining` uses a suffix tree on a linearized instruction hash string to find repeated sequences across all functions, then extracts them into shared helper functions. This reduces code size significantly for repetitive programs.

The QRVSDG analog identifies isomorphic subgraph patterns appearing in multiple region bodies and extracts them to shared λ-nodes. This is especially valuable for quantum programs with repeated structure (ansatz layers, QEC cycles, Trotterization steps).

**Input:** QRVSDG $G$, minimum pattern size $S_{\min}$, minimum occurrence count $K_{\min}$.

**Phase 1 — Region Linearization and Hashing:**

**1.** For each region $\mathcal{R}$ in $G$ (leaf-to-root), compute a topological order $\sigma_\mathcal{R}$ and encode each node as a structural hash:
$$h(n) = \text{hash}(\text{op}(n), |\text{inputs}(n)|, |\text{outputs}(n)|, \text{types}(\text{inputs}), \text{types}(\text{outputs}))$$

**2.** Concatenate hashes per region with unique separator values at region boundaries, producing a global hash string $H$.

**3.** Record mapping: hash-string index $\to$ (region, node).

**Phase 2 — Repeated Subsequence Detection:**

**4.** Build a suffix tree (or suffix array) on $H$. Extract all repeated substrings of length $\geq S_{\min}$ occurring $\geq K_{\min}$ times.

**5.** Rank candidates by $\text{length} \times \text{count}$ (most code savings first).

**Phase 3 — Subgraph Validation and Extraction:**

**6.** For each candidate subsequence, validate that the corresponding node sequence forms a **convex subgraph** in the region (same requirement as Algorithm XVII, Subgraph Replacement from the quantum optimizations addendum):
- Every node on every path between two nodes in the set must also be in the set.
- The subgraph has a well-defined input/output boundary.

**7.** Validate quantum constraints:
- **Linearity preservation**: Every Qubit-typed edge entering the subgraph must also exit it (or terminate at a MeasureD/Dealloc within it). No qubit can enter without exiting.
- **Eff token threading**: Effect token edges are included as inputs/outputs if they cross the boundary.
- **Zone coherence**: All occurrences must operate in the same region annotation context ($R_U$, $R_H$, or $R_{EC}$).

**8.** Discard candidates violating any constraint.

**Phase 4 — λ-Node Creation:**

**9.** For the first occurrence of each valid candidate:
- Create a new λ-node $\lambda_{\text{outlined}}$ in the ω-region.
- Subgraph input edges become λ arguments (preserving types including Qubit zone annotations).
- Subgraph output edges become λ results.
- Subgraph internal nodes form the λ body region.
- Set region annotation: $\rho(\mathcal{R}_{\lambda}) = \rho(\mathcal{R}_{\text{source}})$.

**Phase 5 — Replacement:**

**10.** For each occurrence of the candidate (including the first), replace the subgraph in the source region with an apply-node invoking $\lambda_{\text{outlined}}$:
- Connect source region edges to apply-node inputs.
- Connect apply-node outputs to downstream consumers.
- Use Algorithm XVII (Subgraph Replacement) from the quantum optimizations addendum for the actual graph surgery.

**11.** Run Linear Type Verification (Algorithm XV from the quantum optimizations addendum) on modified regions.

**Quantum constraints:**
- Qubit edges must form complete pass-through chains in the extraction boundary: a qubit that enters must exit (or be consumed by measurement inside). This ensures $|A^q_\lambda| + |\text{internal Prep}| = |R^q_\lambda| + |\text{internal MeasureD}|$ for the subgraph boundary (qubits entering + qubits created = qubits exiting + qubits consumed).
- Overlapping occurrences on the same qubit wire are mutually exclusive (same as template matching in Algorithm XXII from quantum addendum).
- Occurrences across independent qubit wires can be extracted simultaneously.

**Complexity:** $O(N \log N)$ for suffix tree construction; $O(K \cdot S^2)$ for convexity validation of $K$ candidates of average size $S$; $O(N)$ for extraction and replacement.

**Example:**

```
Before (QAOA with p layers):
  θ-node (p iterations):
    body: Rzz(γ, q₀,q₁); Rzz(γ, q₁,q₂); Rx(β, q₀); Rx(β, q₁); Rx(β, q₂)

  Main circuit also contains:
    Rzz(γ', q₃,q₄); Rzz(γ', q₄,q₅); Rx(β', q₃); Rx(β', q₄); Rx(β', q₅)

After outlining (assuming parameter-agnostic extraction):
  λ_qaoa_layer(γ, β, q₀, q₁, q₂):
    Rzz(γ, q₀,q₁); Rzz(γ, q₁,q₂); Rx(β, q₀); Rx(β, q₁); Rx(β, q₂)

  θ-node: apply qaoa_layer(γ, β, q₀, q₁, q₂)
  Main:    apply qaoa_layer(γ', β', q₃, q₄, q₅)
```

---

### Algorithm XXXIV: λ-Node Argument Elimination

*Source: Binaryen DAE (Dead Argument Elimination). Removes unused or constant λ-node arguments.*

Binaryen's DAE performs whole-program analysis to remove function parameters that are never used, propagate constant arguments into function bodies, and handle forwarding chains via fixed-point analysis. The newer DAE2 builds a forwarding graph and computes the fixed point in one pass.

The QRVSDG analog eliminates unused λ-node arguments and context variables, propagates constant values into bodies, and handles forwarding chains where a λ argument is only passed through to another apply-node.

**Input:** QRVSDG $G$.

**Phase 1 — Usage Classification:**

**1.** For each λ-node $\lambda$, for each argument $a_k$ of $\lambda$'s body region:
- **Dead**: $|\text{users}(a_k)| = 0$ within the body region. Argument is unused.
- **Forwarded**: $|\text{users}(a_k)| = 1$ and the single user is an apply-node input for another λ-node. Record forwarding edge $\lambda.k \to \lambda'.j$.
- **Constant**: all apply-nodes calling $\lambda$ pass the same literal value for argument $k$. Record the constant.
- **Used**: argument has non-trivial consumers in the body.

**2.** Similarly classify context variables: $cv = (i, a)$ where $a$ is unused in the body → dead context variable. Skip Qubit-typed context variables: by linearity, their body arguments always have users (the captured qubit must be consumed), so they are never dead.

**Phase 2 — Forwarding Fixed-Point:**

**3.** Build forwarding graph $F = (V_F, E_F)$ where vertices are (λ-node, argument-index) pairs and edges represent forwarding.

**4.** Initialize: mark all "Used" arguments as live.

**5.** Propagate backward: if $\lambda'.j$ is live and $\lambda.k \to \lambda'.j$ is a forwarding edge, then $\lambda.k$ is live.

**6.** Iterate until fixed point. Arguments not marked live after convergence are eliminable (unused through all forwarding chains).

**Phase 3 — Elimination:**

**7.** For each eliminable argument $a_k$ of $\lambda$:
- **Dead argument**: Remove $a_k$ from $\lambda$'s signature. For each apply-node $c$ calling $\lambda$: remove the $k$-th input edge. If the removed input's origin is side-effect-free, the origin may become dead (handled by DNE).
- **Constant argument**: Replace all uses of $a_k$ in $\lambda$'s body with a literal node producing the constant value. Remove $a_k$ from the signature. Remove corresponding apply-node inputs.

**8.** For dead context variables: remove context variable pair $(i, a)$ from $\lambda$; remove the corresponding input edge.

**9.** Update $\lambda$'s QFunc type signature to reflect the reduced argument/result lists.

**Quantum constraints:**
- **Qubit arguments**: A Qubit-typed argument $a_k$ is "dead" only if:
  - (a) No user exists in the body, AND
  - (b) No corresponding Qubit-typed result $r_k$ traces back through the body to $a_k$.
  If a qubit enters a λ-node and exits via a result, the argument is live (even if no gate operates on it — it is a passthrough required by linearity).
- **Passthrough qubit detection**: If $a_k$ is Qubit-typed, $|\text{users}(a_k)| = 1$, and the single user is a region result $r_j$: the qubit is a passthrough. This is not dead, but may indicate the qubit argument is unnecessary at the algorithmic level (the function does not operate on it).
- **Eff tokens**: If an Eff-typed argument is unused (no side-effecting operations in body), it can be eliminated, enabling the caller to avoid sequentializing through this function.
- **Context variable qubit captures**: If a context variable captures a Qubit-typed value, removing it requires ensuring the qubit is properly consumed elsewhere (linearity must hold in the enclosing scope after elimination).

**Complexity:** $O(|\lambda\text{-nodes}| \cdot |\text{args}|)$ for classification; $O(|E_F|)$ for fixed-point; $O(|\text{apply-nodes}| \cdot |\text{eliminated args}|)$ for graph surgery.

**Example:**

```
Before:
  λ_oracle(flag, q₀, q₁, q₂):          — flag never used by any caller
    if flag: X(q₀)                       — dead branch (flag always false)
    CNOT(q₀, q₁); CNOT(q₁, q₂)

  All callers: apply oracle(false, ...)

After DAE:
  λ_oracle(q₀, q₁, q₂):                — flag removed, dead branch eliminated
    CNOT(q₀, q₁); CNOT(q₁, q₂)
```

---

### Algorithm XXXV: λ-Node Signature Type Refinement

*Source: Binaryen signature-refining. Narrows λ-node type signatures based on call-site analysis.*

Binaryen's `signature-refining` computes the LUB (least upper bound) of types actually passed by all callers for each parameter position and all types actually returned. If the LUB is more specific than the declared type, the signature is refined.

The QRVSDG analog refines λ-node QFunc types when all call sites use narrower types than declared, enabling downstream optimization based on the more specific types.

**Input:** QRVSDG $G$.

**Phase 1 — Type Group Collection:**

**1.** Group λ-nodes by their QFunc type signature: $\text{groups}[T] = \{ \lambda \mid \text{type}(\lambda) = T \}$.

**2.** For each group, verify refinement eligibility:
- Not exported via ω-region results (external callers may use the full type).
- Not a member of a φ-node recursive group. (φ-node members may be refined only via a type-level fixed-point computation where all mutual recursive references agree on the refined signature.)
- Not stored in a δ-node accessible to external code.

**Phase 2 — Argument Type LUB Computation:**

**3.** For each argument position $k$ in the type group:
$$T'_k = \bigsqcup_{\substack{c \in \text{ApplyNodes}(group)}} \text{type}(\text{origin}(c.\text{input}_k))$$

If $T'_k \sqsubset T_k$ (strictly more specific), argument $k$ is refinable.

**4.** For each result position $r$:
$$T'_r = \bigsqcup_{\substack{\lambda \in group}} \text{type}(\text{origin}(\lambda.\text{result}_r))$$

**Phase 3 — Refinement Application:**

**5.** Construct refined QFunc type: $\text{QFunc}(T'_1, \ldots, T'_n \to T'_{r_1}, \ldots, T'_{r_m})$.

**6.** Update all λ-nodes in the group:
- Narrow argument types in the body region.
- Narrow result types.

**7.** Update all apply-nodes referencing the group: narrow output edge types.

**8.** Recompute region annotations: for each λ-node whose body now contains only UGate and Barrier nodes (no Measure, Prep, Reset, MeasureD), promote $\rho(\mathcal{R}_\lambda)$ from $R_H$ to $R_U$.

**Quantum-specific refinements:

| Declared Type | Refined Type | Optimization Enabled |
|---|---|---|
| $\text{Qubit}\langle\text{Unspecified}\rangle$ | $\text{Qubit}\langle\text{Data}\rangle$ | Zone-specific scheduling; skip zone-dispatch logic |
| $\text{Qubit}\langle\text{Unspecified}\rangle$ | $\text{Qubit}\langle\text{Ancilla}\rangle$ | Ancilla reuse optimization; short-lifetime assumptions |
| $\text{MResult}$ | $\text{MResult}(\{0,1\})$ | Binary branching (2-region γ-nodes) instead of general |
| $\text{QFunc}(\ldots)$ | $\text{QFunc}^\dagger(\ldots)$ | Adjoint derivation enabled; R_U annotation inferable |
| Region $R_H$ | Region $R_U$ | If refinement proves no measurements → promote to unitary |

**Quantum constraints:**
- Qubit type refinement must be covariant for arguments (callers provide subtypes) but the linear consumption pattern means the qubit flows *through* the function — the output type must be consistently refined.
- Cannot refine types for λ-nodes in φ-node recursive groups unless all mutual references agree on the refined type (a fixed-point condition).
- Region annotation promotion ($R_H \to R_U$) requires verifying the body contains only unitary operations after refinement.

**Complexity:** $O(|\text{apply-nodes}| \cdot |\text{params}|)$ for LUB computation; $O(|\text{types}|)$ for rewriting.

**Example:**

```
Before:
  type T = QFunc(Qubit<Unspecified>, Float → Qubit<Unspecified>)
  λ_ry : T = { Ry(θ, q) }
  λ_rz : T = { Rz(θ, q) }

  All callers: apply ry(q@Data, π/4); apply rz(q@Data, π/2)

After refinement:
  type T' = QFunc(Qubit<Data>, Float → Qubit<Data>)
  λ_ry : T' = { Ry(θ, q@Data) }       — zone-specific scheduling now possible
  λ_rz : T' = { Rz(θ, q@Data) }
```

---

### Algorithm XXXVI: Whole-Program Content Flow Analysis

*Source: Binaryen GUFA (Grand Unified Flow Analysis). Whole-program analysis propagating value/type information across all λ-nodes.*

Binaryen's GUFA computes, for every expression location in the program, the set of possible runtime values (`PossibleContents`): constants, specific global references, type cones, or unknown. It uses this to replace known-constant locations, mark unreachable code, and refine types.

The QRVSDG analog propagates content information across every edge in the ω-node, leveraging the graph's demand-dependence to determine which edges carry known values, specific zone annotations, deterministic measurement outcomes, or resolvable function references.

**Input:** QRVSDG $G$.

**Phase 1 — Content Lattice Definition:**

**1.** Define a content lattice $\mathcal{L}$ for each edge type:

| Edge Type | Lattice (bottom → top) |
|---|---|
| Classical (Int, Float, Bool) | $\bot \to \text{Literal}(v) \to \text{Range}(lo, hi) \to \top$ |
| MResult$(S)$ | $\bot \to \text{Known}(v) \to \top$ |
| QFunc | $\bot \to \text{ExactFunc}(\lambda) \to \text{TypeCone}(T) \to \top$ |
| Qubit | $\bot \to \text{Qubit}\langle Z, E_{\text{class}} \rangle \to \text{Qubit}\langle Z \rangle \to \top$ |
| Eff | $\bot \to \top$ (not refined beyond existence) |

where $Z$ is a zone annotation and $E_{\text{class}}$ is an entanglement class identifier. The zone subtype lattice is: $\text{Data} \sqsubset \text{Unspecified}$, $\text{Ancilla} \sqsubset \text{Unspecified}$, $\text{Syndrome} \sqsubset \text{Unspecified}$, etc.; incomparable zones (e.g., $\text{Data}$ and $\text{Ancilla}$) join to $\text{Unspecified}$.

Join rules for MResult: $\text{Known}(v) \sqcup \text{Known}(v) = \text{Known}(v)$; $\text{Known}(v_1) \sqcup \text{Known}(v_2) = \top$ for $v_1 \neq v_2$. A measurement is $\text{Known}(v)$ only when analysis of the producing node proves the qubit is in an eigenstate of the measurement basis.

**Phase 2 — Initialization:**

**2.** For each node $n$, initialize output edge contents based on node type:
- Literal node → $\text{Literal}(v)$
- δ-node → content of the δ-node's initializer
- λ-node output → $\text{ExactFunc}(\lambda)$
- Prep node → $\text{Qubit}\langle\text{Utility}, \text{singleton}\rangle$ (fresh qubit, known zone, no entanglement)
- UGate node → derived from input contents:
  - Single-qubit gate: output zone = input zone; entanglement class unchanged.
  - Multi-qubit gate: each output qubit $q'_k$ inherits its zone from the corresponding input qubit $q_k$; entanglement class = union of all input classes.
- Measure node → MResult output initialized to $\top$; post-measurement Qubit inherits zone.

**Phase 3 — Propagation:**

**3.** Propagate contents through the graph using a worklist algorithm:

- **Simple nodes**: Transfer function applied; output = $f(\text{input contents})$.
- **γ-nodes**: Output content = $\bigsqcup_j \text{content}(\text{result}_j^{R_j})$ (join over all branch results). If predicate content is $\text{Known}(v)$, output = content of the selected branch only.
- **θ-nodes**: Fixed-point over loop body: initialize loop variable contents from inputs; iterate body until content annotations stabilize. Bounded by lattice height.
- **λ-nodes via apply-nodes**: Propagate caller input edge contents to λ argument edges; propagate λ result edge contents back to apply-node output edges. Handle multiple call sites by joining.
- **φ-nodes**: Fixed-point across mutually recursive λ-groups.

**Phase 4 — Optimization:**

**4.** Apply content-driven optimizations:

| Content | Optimization |
|---|---|
| Edge content = $\bot$ | Producing node is unreachable → DNE (Algorithm X) |
| Classical edge = $\text{Literal}(v)$ | Replace producing subgraph with literal node |
| QFunc edge = $\text{ExactFunc}(\lambda)$ | Convert indirect apply to direct apply (Algorithm XXXII) |
| MResult = $\text{Known}(v)$ | Replace downstream γ-node with selected branch |
| Qubit zone = known $Z$ | Annotate edge with zone for scheduling/layout |
| Entanglement class = known | Annotate for parallelism analysis |
| γ-node predicate = $\text{Known}(v)$ | Replace γ-node with selected branch body |

**5.** Run DNE (Algorithm X) and Vacuum (Algorithm XVII) to clean up.

**Quantum-specific analysis capabilities:**
- **Measurement determinism detection**: If a Prep($|0\rangle$) qubit has no intervening gates before measurement, the measurement always yields 0. Content: $\text{MResult} = \text{Known}(0)$.
- **Constant rotation propagation**: Known rotation angles propagated across function boundaries enable compile-time gate fusion by downstream passes (e.g., CNE Algorithm IX's extended identity congruence, Gate Fusion Algorithm XXV). GUFA does not directly detect gate identity sequences — that is the role of CNE and gate cancellation passes operating on the constant-propagated graph.
- **Zone inference**: If all paths to a qubit edge assign the same zone, annotate the edge — even if not explicitly annotated by the source language.
- **Entanglement class tracking**: Multi-qubit gates union the entanglement classes of their inputs; single-qubit gates preserve them; measurement may split classes.
- **Constant rotation propagation**: Known rotation angles propagated through the graph enable compile-time gate fusion across function boundaries.

**Quantum constraints:**
- Qubit content is NOT the quantum state vector (which is exponential) — it is an abstract annotation tracking zone, entanglement class, and known-state flags.
- The analysis is conservative: join produces $\top$ when branches disagree.
- Measurement nondeterminism: MResult content is $\top$ by default; only provably deterministic measurements yield $\text{Known}(v)$.

**Complexity:** $O(|edges| \cdot h \cdot I)$ where $h$ is lattice height and $I$ is number of iterations to convergence. In practice, the lattice is shallow (height ≤ 4) and convergence is fast.

**Example:**

```
Before:
  Prep |0⟩ → q₀
  No gates on q₀
  m = Measure(q₀)            — GUFA determines: m = Known(0)
  γ-node(m):
    R₀: expensive_circuit()   — always selected
    R₁: identity()            — never reached

After GUFA + cleanup:
  Prep |0⟩ → q₀
  m = Measure(q₀)
  expensive_circuit()         — γ-node eliminated; R₁ dead
```

---

## Group 7: Compound Type Algorithms

### Algorithm X-DFE: Dead Field Elimination

*Source: Novel. Field-sensitive mark phase extending Algorithm X (Quantum DNE) for compound types.*

Algorithm X (DNE) marks all inputs of a node alive when any output is alive. For compound types, this is conservative: a TupleConstruct output may be alive because one field is extracted, yet DNE marks all field inputs alive. Algorithm X-DFE introduces field-sensitive liveness to mark only the inputs whose fields are actually extracted downstream.

X-DFE extends the mark phase of Algorithm X with four rules. The sweep phase is unchanged.

**DFE-1 (Tuple/Struct field liveness):** When a TupleConstruct or StructConstruct output is alive and its sole user is a TupleExtract or StructExtract, mark only the inputs at positions corresponding to extracted fields that have live users. Remaining field inputs are dead candidates.

Formally: let compound node $c$ have output $o$ with $\text{type}(o) = \text{Tuple}(\tau_1, \ldots, \tau_k)$ or $\text{Struct}(\text{name}, \text{fields})$. For each user $u$ of $o$ that is a TupleExtract$(j)$ or StructExtract$(f)$: if $u$ is alive, mark input $i^j_c$ (or the input at field position $f$) alive. Inputs at positions with no live extraction user are dead candidates.

**DFE-2 (Array index liveness):** When an ArrayConstruct output is alive and accessed only through ArraySwap at known static indices, mark only the inputs at accessed indices as alive. Remaining positions are dead candidates.

Formally: let compound node $c$ have output $o$ with $\text{type}(o) = \text{Array}(\tau, n)$. Collect the set of accessed indices $J = \{j \mid \exists \text{ArraySwap}(j) \text{ consuming } o \text{ transitively}\}$. Mark input $i^j_c$ alive for $j \in J$. Inputs at positions $j \notin J$ are dead candidates.

**DFE-3 (Entanglement-aware partner marking):** If a quantum field $k$ is alive and the entanglement partition shows $q_k$ entangled with $q_m$ (where $q_m$ resides in field $m$ of the same compound), mark field $m$ as alive. Entangled qubits cannot be independently discarded.

Formally: let $\Pi$ be the entanglement partition at compound node $c$. If field $k$ is marked alive and $\exists m \neq k$ such that $q_k$ and $q_m$ belong to the same partition block $B \in \Pi$, mark field $m$ alive.

**DFE-4 (Structural node field-liveness resolver):** Field liveness must be resolved across structural node boundaries:

- For $\gamma$-nodes: an output field is dead if dead in ALL regions. Compute per-region field liveness sets and take the union: $\text{live\_fields}(\gamma, f) = \bigcup_j \text{live\_fields}(R_j, f)$.
- For $\theta$-nodes: a field is dead if never extracted post-loop AND only threaded unchanged in the loop body with no side-consuming use.

**Quantum dead fields:** Dead fields containing quantum data ($\text{Qubit}$) represent linearity violations, not optimization opportunities. X-DFE flags quantum dead fields for Algorithm XV (linearity verification) rather than silently eliminating them. The primary optimization benefit comes from eliminating dead classical fields in mixed compounds (e.g., removing unused metadata from a `Tuple(Qubit, Float, String)` when only the qubit is used).

**Complexity:** $O(N + E)$, same as baseline Algorithm VI/X. The field-liveness tracking adds only a constant factor per compound node (proportional to the number of fields, bounded by compound arity).

**Pass ordering:** X-DFE runs before SROA. X-DFE eliminates dead fields first, reducing the number of scalar edges SROA produces. After SROA decomposes remaining compounds to scalars, standard DNE (Algorithm X) cleans up any newly dead scalar values.

**Example:**

```
Before X-DFE:
  matrix = expensive_computation()
  eigenvalue = extract_eigenvalue(matrix)
  metadata = format_string("result")
  result = TupleConstruct(matrix, eigenvalue, metadata)
  ... γ-node ...
  val = TupleExtract(result, 1)         — only field 1 (eigenvalue) extracted
  use(val)

DNE analysis: result is alive (has user) → marks all 3 inputs alive
X-DFE analysis: only field 1 extracted → marks only eigenvalue input alive
  → matrix (field 0) and metadata (field 2) are dead candidates

After X-DFE + DNE sweep:
  eigenvalue = extract_eigenvalue(...)   — alive (field 1 used)
  result = TupleConstruct(⊥, eigenvalue, ⊥)
  val = TupleExtract(result, 1)
  use(val)
  (expensive_computation and format_string removed — dead)
```

---

### Algorithm SROA: Scalar Replacement of Aggregates

*Source: LLVM SROA. Decomposes compound values into individual scalar edges for optimization.*

SROA is the critical boundary between compound-level and scalar-level optimization in the pipeline. After compound-level optimizations (XI hoisting, XII loop-invariant, XV suffix factoring, X-DFE) have operated on intact compounds, SROA decomposes them to enable scalar-level optimizations (gate cancellation, gate fusion, basis state propagation).

**Input:** QRVSDG with compound types.

**Output:** QRVSDG with compound values decomposed to individual scalar values.

**Procedure:**

**1.** For each compound construction node (TupleConstruct, StructConstruct, ArrayConstruct):

**(a)** Replace the compound output with individual scalar outputs, one per field or element.

**(b)** Replace all downstream extraction nodes with direct edge connections. A TupleExtract$(j)$ becomes a direct connection to the $j$-th scalar output.

**(c)** For ArraySwap: replace with identity connections (the element is already a scalar after decomposition).

**(d)** Attach $\{\text{origin\_struct}, \text{origin\_field}\}$ provenance metadata to each scalar edge for reconstruction during destruction.

**2.** Handle structural node boundaries:

**(a)** $\gamma$-node: decompose compound entry/exit variables into individual scalars. A single compound entry variable becomes $k$ scalar entry variables (one per field). Each $\gamma$-region's compound argument becomes $k$ scalar arguments.

**(b)** $\theta$-node: decompose compound loop variables into individual scalars. A loop variable carrying a $k$-field compound becomes $k$ scalar loop variables.

**(c)** $\lambda$-node: decompose compound parameters into individual scalars. A compound-typed function parameter becomes $k$ scalar parameters. Call sites are updated to pass individual scalars.

**Expansion cost:** $O(n)$ per compound operation, where $n$ is the number of fields or elements. For 10-qubit arrays: approximately 3.7$\times$ IR expansion. For 100-qubit arrays: approximately 50$\times$ expansion. The expansion is linear in compound size but can produce significant IR growth for large arrays.

**Array size threshold:** At optimization Level $\leq 2$, arrays with $n > 50$ elements are deferred (not decomposed by SROA). This avoids excessive IR expansion for large QEC registers where scalar-level optimization provides diminishing returns. The threshold of $n > 50$ is a heuristic that should be calibrated on benchmark programs.

**Parameterization by optimization level:**

- Level 0: skip SROA. Algorithm VIII-CE handles compound lowering during destruction.
- Level 1: run SROA for all compounds.
- Level 2: run SROA with size threshold ($n \leq 50$).
- Level 3+: run SROA for all compounds, plus provenance-based reconstruction during destruction.

**SROA and VIII-CE complementarity:** If SROA runs, VIII-CE becomes unnecessary (compounds are already decomposed). If SROA is skipped (Level 0), VIII-CE is required for correctness during destruction. The two are complementary, not redundant.

**Post-SROA reconstruction** (Level 3+): During destruction, use provenance metadata ($\{\text{origin\_struct}, \text{origin\_field}\}$) to reconstruct compound operations for idiomatic target output (OpenQASM 3 `qubit[n]`, Q# arrays/structs, QIR aggregates). LLVM validates this approach: SROA always runs in LLVM's pipeline, and aggregates are reconstructed during code generation when targets support them.

**Complexity:** $O(N + E)$ for a single pass over the graph. Each compound node is visited once, and each extraction/swap node is replaced by a direct connection.

**Example:**

```
Before SROA:
  q0 = Prep |0⟩, q1 = Prep |0⟩, q2 = Prep |0⟩
  arr = ArrayConstruct(q0, q1, q2)       — Array(Qubit, 3)
  (arr', q1') = ArraySwap(arr, 1, ancilla)
  arr'' = ArrayApplyGate(arr', 0, H)
  (arr''', q2') = ArraySwap(arr'', 2, ancilla2)

After SROA:
  q0 = Prep |0⟩, q1 = Prep |0⟩, q2 = Prep |0⟩
  q1' = ancilla                           — ArraySwap(1) → direct swap
  q0' = H(q0)                             — ArrayApplyGate(0, H) → direct gate
  q2' = ancilla2                          — ArraySwap(2) → direct swap
  (provenance: q0' ← {arr, 0}, q1' ← {arr, 1}, q2' ← {arr, 2})

Scalar-level optimizations now apply directly to q0', q1', q2'.
```

---

### Algorithms Unchanged by Compound Types

The following algorithms require no modifications for compound type support:

- **Algorithm XVI (Local/Global CSE):** Operates on classical simple nodes and quantum gate chains. Compound construction/extraction nodes are simple nodes with unique operator identities; CSE correctly handles them through standard congruence (same operator, congruent inputs). Compound-typed values are classical-like for CSE purposes because the no-cloning constraint is enforced by Algorithm IX, not XVI.
- **Algorithm XVII (Vacuum):** Structural cleanup operates on γ-node/θ-node interface variables regardless of their types. Identity detection and passthrough pruning apply to compound-typed variables with no changes.
- **Algorithm XXVII (Node Pull-Through):** Gate motion into γ-nodes is type-agnostic; the operation checks operator congruence and input availability, not output types.
- **Algorithm XXVIII (Classical Node Splitting):** Applies only to classical simple nodes. Compound construction nodes are classical-like (their outputs may fan out if purely classical) and are handled by standard splitting logic.
- **Algorithm XXIX (Region Flattening):** Structural simplification that merges nested regions. Type-transparent.
- **Algorithm XXXVII (Loop Unrolling):** Unrolls θ-nodes by replicating the loop body. Loop variable types (including compound types) are propagated without modification.
- **Algorithm XXXVIII (Classical Constant Folding):** Operates exclusively on classical arithmetic and boolean expressions. Compound types do not introduce new foldable operations.
- **Algorithm XXXIX (BSCP):** Basis state tracking through Clifford gates operates on individual qubit chains. After SROA decomposes compounds to scalars, BSCP applies unchanged. Before SROA, compound-typed edges are opaque to BSCP (correctly conservative).

---

## Algorithm Summary Table

| # | Name | Source | Group | Extends |
|---|------|-------|-------|---------|
| XIV | Extended DCE | Binaryen/LLVM | Cleanup | X |
| XV | Code Folding (Suffix Factoring) | Binaryen | Code Motion | XI (dual) |
| XVI | Local/Global CSE | Binaryen | Value Opt. | IX |
| XVII | Vacuum | Binaryen | Cleanup | Novel |
| XVIII | Function Inlining | LLVM | Function-Level | Novel |
| XIX | λ-Node Reordering | Binaryen | Function-Level | Novel |
| XX | θ-Node Loop Variable Optimization | Binaryen | Function-Level | IX |
| XXI | Global Value Numbering | LLVM | Value Opt. | IX |
| XXII | Extended LICM | LLVM | Code Motion | XII |
| XXIII | SCCP | LLVM | Value Opt. | Novel |
| XXXIX | BSCP (Basis State and Constant Propagation) | Novel | Value Opt. | XXIII |
| XXIV | Tail Call Optimization | LLVM | Function-Level | Novel |
| XXV | Gate Fusion (Instruction Combining) | LLVM | Value Opt. | Novel |
| XXVI | Node Push-Through | RVSDG | Code Motion | XI |
| XXVII | Node Pull-Through | RVSDG | Code Motion | XI |
| XXVIII | Classical Node Splitting | RVSDG | Structural | Novel |
| XXIX | Region Flattening | RVSDG | Structural | Novel |
| XXXVII | θ-Node Loop Unrolling | LLVM | Structural | Novel |
| XXX | λ-Node Congruence Elimination | Binaryen | Inter-Procedural | IX |
| XXXI | λ-Node Call-Context Specialization | Binaryen | Inter-Procedural | XVIII |
| XXXII | Indirect Apply Resolution | Binaryen | Inter-Procedural | Novel |
| XXXIII | Repeated Subgraph Extraction | Binaryen | Inter-Procedural | Novel |
| XXXIV | λ-Node Argument Elimination | Binaryen | Inter-Procedural | Novel |
| XXXV | λ-Node Signature Type Refinement | Binaryen | Inter-Procedural | Novel |
| XXXVI | Whole-Program Content Flow Analysis | Binaryen | Inter-Procedural | XXIII, XXXIX |
| XXXVIII | Classical Constant Folding | LLVM/Binaryen | Value Opt. | Novel |
| X-DFE | Dead Field Elimination | Novel | Compound Type | X |
| SROA | Scalar Replacement of Aggregates | LLVM | Compound Type | Novel |

---

## Pass Ordering Recommendations

The twenty-eight algorithms in this addendum (XIV through XXXIX, plus X-DFE and SROA), combined with existing Algorithms IX through XII, form a comprehensive optimization pipeline. With the addition of compound types, the pipeline gains a compound-level optimization phase that must precede scalar decomposition. A full 9-phase compilation pipeline is being formalized in the quantum optimizations addendum (qrvsdg_addendum_quantum_optimizations.md); the ordering below covers the optimization-specific phases.

**Compound-specific ordering constraints:**

- X-DFE must run before SROA: eliminate dead fields before decomposing compounds to scalars.
- SROA must run before scalar-level quantum optimizations (gate cancellation XIX, gate fusion XXV, BSCP XXXIX, LICM XXII, GVN XXI): these algorithms operate on individual qubit/classical edges, not compound-typed edges.
- Compound-level optimizations (gate hoisting XI, loop-invariant XII, suffix factoring XV) run before SROA: they benefit from intact compound structure (e.g., hoisting an entire array operation rather than $n$ individual gates).

Recommended pass ordering:

```
 Phase A — Whole-Program Analysis:
 1. GUFA (XXXVI)                            — whole-program content flow
 2. Indirect Apply Resolution (XXXII)       — resolve indirect calls
 3. λ-Node Congruence Elimination (XXX)     — merge duplicate functions
 4. λ-Node Argument Elimination (XXXIV)     — remove dead/constant args
 5. λ-Node Signature Refinement (XXXV)      — narrow types

 Phase B — Compound-Level Optimization:
 6. CNE (IX) → DNE (X)                      — initial cleanup (compound-level)
 7. Gate Hoisting (XI) → Code Folding (XV)   — boundary optimization at compound level
 8. X-DFE                                   — dead field elimination
 9. SROA                                    — decompose compounds to scalars
    (At Level 0, skip steps 8–9; VIII-CE handles lowering during destruction)

 Phase C — Scalar-Level Intra-Procedural Optimization:
 10. CNE (IX) → DNE (X)                     — post-SROA cleanup
 11. Constant Folding (XXXVIII) → SCCP (XXIII) → BSCP (XXXIX) → DCE (XIV)
                                            — fold, propagate classical, propagate basis states, eliminate
 12. Gate Fusion (XXV)                       — peephole combining
 13. LICM (XXII)                             — loop optimization
 14. Loop Unrolling (XXXVII)                 — selective quantum-aware unrolling
 15. Gate Cancellation (XIX)                 — post-unrolling boundary cancellations

 Phase D — Inter-Procedural Optimization:
 16. Function Inlining (XVIII)               — inline small/single-call-site λ-nodes
 17. λ-Node Specialization (XXXI)            — specialize for known contexts
 18. CNE (IX) → DNE (X)                      — catch new opportunities

 Phase E — Cleanup:
 19. Repeated Subgraph Extraction (XXXIII)   — outline common patterns
 20. Vacuum (XVII)                           — structural cleanup
 21. GVN (XXI) → CSE (XVI)                   — final value optimization
 22. λ-Node Reordering (XIX-Opt)             — serialization ordering

 23. Repeat Phases A–E until fixed point
```

**Iteration group framework:** The phases above map to four iteration groups that can be scheduled independently:

- IterGroup D (Hoist + Suffix + CNE): operates at compound level within Phase B.
- IterGroups A, B, C: operate at scalar level within Phases C through E.

Repeat Phases A through E until no changes are observed across a full cycle. In practice, 2 to 3 iterations suffice for most programs. Each iteration group converges monotonically (node count decreases or remains constant).

Alternatively, equality saturation (Section 8.2 of QRVSDG paper) can replace the fixed pass ordering within Phases C through E with non-destructive exploration.

---

## References

[1] N. Reissmann, J. C. Meyer, H. Bahmann, and M. Själander, "RVSDG: An Intermediate Representation for Optimizing Compilers," *ACM Transactions on Embedded Computing Systems*, vol. 19, no. 6, pp. 1–28, 2020.

[2] QRVSDG: A Regionalized Value State Dependence Graph for Hybrid Quantum-Classical Compilation. (This work.)

[3] Binaryen WebAssembly optimizer. https://github.com/WebAssembly/binaryen.

[4] LLVM Language Reference Manual. https://llvm.org/docs/LangRef.html.

[5] LLVM Passes. https://llvm.org/docs/Passes.html.
