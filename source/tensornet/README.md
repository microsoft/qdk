# `tensornet`

A backend-agnostic description of a tensor network.

## Why this crate exists

Three tensor network implementations now inform this repository: the
cuTensorNet MPS path in `qdk_cutensornet`, the tensor4all CPU prototype, and
the general contraction path being built on top of cuTensorNet's network API.
They disagree about almost everything downstream — who owns the tensor storage,
whether gates are applied imperatively or the whole network is declared up
front, how truncation is expressed — but they agree on what a tensor network
_is_.

That agreement is what lives here. Nothing in this crate knows how a network is
contracted, so the same description can be handed to a GPU library, to a CPU
contractor, or to a reference implementation written purely for a test. That
last one is the point: correctness work no longer needs a GPU.

## The model

Two families of description. A general network is built from four concepts;
alongside them, and not beneath them, `Mps` describes a chain.

| concept            | shape                     | role                                                  |
| ------------------ | ------------------------- | ----------------------------------------------------- |
| `Index`            | `{ id, dim }`             | an axis: an identity plus how many coordinates it has |
| `Indices`          | `List<Index>`             | an ordered axis list — a node's axes, or the result's |
| `TensorNetwork`    | `List<Indices>`           | the network: nodes joined by index identity           |
| `ContractionQuery` | `&TensorNetwork` + `keep` | what to compute from it                               |
| `Mps`              | `List<List<usize>>`       | a chain of site tensors, described by extents alone   |

The first three are the left side of einsum's notation; the query adds the
right side. `Mps` is a separate description, covered under "Chains are a second
description" below:

```text
  ( i j k ,  j l ,  i m )    ->    ( k l )
  └──────────────────────┘         └─────┘
        TensorNetwork                keep
  └─────┘  └───┘  └───┘
        Indices (one per node)
```

There is no `Join` operation. Joining is not something done _to_ nodes; it is
what index identity already means, so the network is simply the list of nodes.

### Surface

```rust
pub struct Index { /* id: u32, dim: usize */ }

impl Index {
    pub fn new(id: u32, dim: usize) -> Result<Self, NetworkError>;
    pub fn id(self) -> u32;
    pub fn dim(self) -> usize;
}

pub struct Indices { /* Vec<Index> */ }

impl Indices {
    pub fn new(indices: Vec<Index>) -> Result<Self, NetworkError>;
    pub fn as_slice(&self) -> &[Index];
    pub fn contains(&self, index: Index) -> bool;
    pub fn element_count(&self) -> Option<usize>;   // None when it exceeds usize
    pub fn strides(&self) -> Option<Vec<usize>>;    // column-major
    pub fn offset_of(&self, coords: &[usize]) -> Option<usize>;
}

pub struct TensorNetwork { /* nodes: Vec<Indices> */ }

impl TensorNetwork {
    pub fn new(nodes: Vec<Indices>) -> Result<Self, NetworkError>;
    pub fn nodes(&self) -> &[Indices];
    pub fn incidence(&self, index: Index) -> usize; // slots, multiplicity counted
}

pub struct ContractionQuery<'a> { /* network: &'a TensorNetwork, keep: Indices */ }

impl<'a> ContractionQuery<'a> {
    pub fn new(network: &'a TensorNetwork, keep: Indices) -> Result<Self, ContractionError>;
    pub fn network(&self) -> &TensorNetwork;
    pub fn keep(&self) -> &Indices;        // result axes, in order
    pub fn hyperedges(&self) -> Indices;   // slots + kept != 2
    pub fn marginalized(&self) -> Indices; // one slot and not kept
}

pub enum NetworkError {
    ZeroDimension { id: u32 },
    InconsistentDimension { id: u32, first: usize, second: usize },
}

pub enum ContractionError {
    UnknownKeptIndex { id: u32 },
    RepeatedKeptIndex { id: u32 },
    InconsistentDimension { id: u32, network: usize, kept: usize },
}

pub enum Operand {
    Input(usize),   // a position in TensorNetwork::nodes
    Result(usize),  // an earlier step of the same plan
}

pub struct ContractionStep { /* operands: Vec<Operand>, result_axes: Indices */ }

impl ContractionStep {
    pub fn new(operands: Vec<Operand>, result_axes: Indices) -> Self;
    pub fn operands(&self) -> &[Operand];
    pub fn result_axes(&self) -> &Indices;
    pub fn arity(&self) -> usize;
}

pub struct ContractionPlan { /* steps: Vec<ContractionStep> */ }

impl ContractionPlan {
    pub fn new(query: &ContractionQuery<'_>, steps: Vec<ContractionStep>) -> Result<Self, PlanError>;
    pub fn steps(&self) -> &[ContractionStep];
    pub fn is_pairwise(&self) -> bool; // every step consumes exactly two operands
}

pub enum PlanError {
    EmptyNetwork,
    MissingSteps { nodes: usize },
    UnknownInput { step: usize, index: usize },
    ForwardReference { step: usize, index: usize },
    AlreadyConsumed { step: usize, operand: Operand },
    EmptyStep { step: usize },
    RepeatedResultAxis { step: usize, id: u32 },
    WrongResultAxes { step: usize, expected: Vec<Index>, actual: Vec<Index> },
    WrongOutputAxes { expected: Vec<Index>, actual: Vec<Index> },
    UnconsumedOperands { operands: Vec<Operand> },
}
```

Every method above has a named consumer. Anything derivable that nothing yet
asks for — a network's distinct index list, its dangling axes, its shared ones
— is deliberately absent, and is a few lines over `nodes()` when something
needs it.

`element_count` and `strides` return `Option` because a result can legitimately
be too large to address: keeping 64 qubit axes is `2^64` elements. That is not
a failure but an answer, and it is precisely the case a planner has to slice.

`Indices` enforces one invariant — a given `id` carries one `dim` — and
`TensorNetwork` enforces the same across nodes. It deliberately does _not_
reject repeats: a node may legitimately repeat an index (`i i` is a trace).
`keep` may not, which is why `RepeatedKeptIndex` is a query-level error.

A network with no nodes is allowed. The empty product is the scalar one, so it
is well defined, and by the rule this crate follows throughout — reject what is
ambiguous, allow what is determined — there is nothing to reject. It is
unlikely to be what a caller meant, and a backend that cannot contract it is
the right place to say so.

### Plans schedule a query, without a numerical engine

A `ContractionPlan` is a portable, declarative schedule for one
`ContractionQuery`: an ordered list of steps, each naming the operands it
consumes (by input position or by an earlier step's result) and the axes its
result keeps, in order. It carries no coefficients, no native handles and no
optimizer identity — only enough to say which axes survive each step and in
what order, which is exactly what `ContractionPlan::new` checks against the
query it is built from.

Arity is not restricted to two; a step may consume any number of operands, and
`is_pairwise` reports whether a particular plan happens to use only binary
steps. That distinction matters because the currently supported execution
subset is unsliced, pairwise contraction plans with fixed coefficient
bindings — a capability of today's executor, not a limit of the model. A
single-node network needs no steps only when that node's axes already equal
the query's kept axes as a set; anything else, including a one-node diagonal,
needs a (possibly unary) step to say so.

A validated plan borrows nothing from the query or network it was checked
against, so the same plan can be validated again later against a fresh but
topologically identical query — the same guarantee a selected native
contraction path relies on when it outlives its optimizer and is imported into
a new owner without another search.

`Mps` is described separately, and shares nothing with the above but the crate
it lives in:

```rust
pub struct Mps { /* sites: List<List<usize>> */ }

impl Mps {
    fn new(sites: List<List<usize>>) -> Result<Mps, MpsError>;

    fn sites(&self) -> &[List<usize>];
    fn site_count(&self) -> usize;
    fn site(&self, site: usize) -> Option<&[usize]>;
    fn physical_dim(&self, site: usize) -> Option<usize>;
    fn bond_dim(&self, cut: usize) -> Option<usize>;
    fn max_bond(&self) -> usize;
    fn element_counts(&self) -> List<usize>;
    fn fits_within(&self, capacity: &Mps) -> bool;
}
```

A site's extents read `[bond_left, physical, bond_right]`, with the absent bond
omitted at each end: rank 2 at the ends, rank 3 inside, and rank 1 for a chain
of one site. `new` rejects anything that is not a chain — a wrong rank for a
position, a zero extent, neighbours that disagree about their bond, or a site
too large to count. Nothing here is specific to qubits; a physical extent is
just a number, and a caller that needs two-level sites checks for that itself.

The dependency runs one way. `TensorNetwork` never mentions
`ContractionQuery`, and the errors split along the same seam, so the compiler
enforces the separation rather than convention. The one thing a query must
still check about dimensions is that `keep` agrees with the network about them:
the network is valid on its own terms, but `keep` arrives from outside it.

### Semantics

Let the network hold nodes `T¹ … Tⁿ` with index sets `S¹ … Sⁿ`, let
`S = S¹ ∪ … ∪ Sⁿ`, and let `K` be `keep`. Then

```text
Result[coords on K]  =   Σ            ∏  Tⁱ[ assignment restricted to Sⁱ ]
                    assignments to     i
                        S \ K
```

Two consequences are worth stating outright, because they are the two things
that most often surprise people:

- **A repeated index is _identification_, not contraction.** Writing the same
  index in two nodes says "these are the same axis". It does not say "sum over
  it". So a network of `(i j)` and `(j l)` with `j` kept is a rank-3 object,
  not a matrix product. This is a deliberate departure from the physicists'
  reading, where joining two legs contracts them on the spot.
- **Contraction is identification _plus_ omission.** An index is summed over
  exactly when `keep` leaves it out. The network decides topology; the query
  decides summation.

The product is unconditional: every node is multiplied in, whether or not it
shares an index with any other. Two nodes sharing nothing yield an outer
product, and there is no way to make them contract — contraction needs a shared
index, by construction.

## Why a query refers to a network

One network admits many contractions, and `keep` is what selects between them:

| `keep`      | result                                          |
| ----------- | ----------------------------------------------- |
| `[i, k]`    | `Σⱼ A[i,j]·B[j,k]` — the matrix product         |
| `[i, j, k]` | rank 3, nothing summed — `j` is now a hyperedge |
| `[k, i]`    | the same sum, transposed                        |
| `[]`        | a scalar, `Σᵢⱼₖ`                                |

So a query **borrows** its network. Several queries can refer to one network
with no copying, nothing is consumed, and building a description of something
does not destroy it. `keep`, by contrast, belongs to exactly one query and is
taken by value.

The cost is that a `ContractionQuery` cannot outlive its network, so a helper
cannot build a network locally and return a query over it. Builders return the
`TensorNetwork`; the caller forms the query. That is the less committal
direction: an owning variant can be added later, whereas removing ownership
afterwards is the harder change.

Both references group the same way. cuTensorNet's `networkDescriptor` holds
appended tensors _and_ the output tensor, with `ContractionOptimize` a separate
step over it; cotengra's `ContractionTree(inputs, output, size_dict, ...)` is
the same pairing. The query is a planner's input, not its output.

### Contraction is terminal

A query yields a plan, not another network. Contraction does not nest.

The reason is that an inner contraction fixes _when_ a sum happens, and
choosing that order is exactly the planner's job — it is where the whole
difficulty of a large network lives. Nesting would let a caller silently
prescribe an order while believing they were only describing structure. Given a
flat network the planner is free to find the order itself, which is the
freedom worth protecting.

An expression language with contraction at inner nodes is coherent — it is what
a tensor network _program_ looks like, and an MPS sweep is one — but it is a
different type (a recursive tree, executed as a DAG of contractions) and it can
be added later without invalidating anything built on the flat form.

## Incidence, and what a hyperedge is

Count the occurrences of an index, **treating `keep` as one more operand**. An
index with exactly two occurrences is an ordinary edge. Anything else is a
hyperedge:

| expression               | index | occurrences              |                               |
| ------------------------ | ----- | ------------------------ | ----------------------------- |
| `i j, j k -> i k`        | `j`   | 2 inputs                 | ordinary edge                 |
| `i j, j k -> i j k`      | `j`   | 2 inputs + kept = 3      | hyperedge                     |
| `b i j, b j k -> b i k`  | `b`   | 2 inputs + kept = 3      | hyperedge (a batch index)     |
| `i j, j k, j l -> i k l` | `j`   | 3 inputs                 | hyperedge                     |
| `i i -> i`               | `i`   | 2 in one node + kept = 3 | hyperedge (a diagonal)        |
| `i j -> i`               | `j`   | 1 input                  | hyperedge (a marginalization) |

Occurrences are counted **with multiplicity**, which is why `i i` scores two
and the trace needs no special case.

cotengra classifies indices by exactly this rule
(`if len(nodes) == 2 and (not output)` is the ordinary-edge branch,
`hypergraph.py`), which is reassuring: it was arrived at independently.

Note that the rule catches both directions. Three or more occurrences is the
_copy_ case; **one** occurrence is the _marginalization_ case. Neither is an
ordinary edge, and each corresponds to one structural node under desugaring
(below).

Hyperedges are supported, and they are not exotic. A diagonal two-qubit gate —
`RZZ`, which dominates a 2D Ising Trotter circuit — is a hyperedge contraction
and nothing else. Stored dense it is a 4-leg, 16-element tensor with 12
structural zeros; stored as a hyperedge it is `d[p,q]` with 4 elements, and the
contraction `p q, p q -> p q` is an elementwise multiply. Identical result, a
quarter of the memory.

## Marginalization is derived, not declared

An index on exactly one node and not in `keep` is summed on its own. This is
einsum's rule, and it is the one place the notation can silently absorb a
mistake: a builder that fails to propagate a wire label produces
`i j, l k -> i k` rather than `i j, j k -> i k`, which runs, returns the right
_shape_, and gives the wrong numbers.

einx (Fervers et al., ICLR 2026) argues for marking reduced axes explicitly to
close exactly this hole. We considered requiring every incidence-1 index to be
declared, and rejected it: `keep` already determines the answer, so a
`marginalize` argument would be pure redundancy — derivable from the network
and `keep` — and a redundant declaration produced by the same builder that
produced the bug would simply agree with it.

So the fact stays derived, and is exposed as a query instead:

```rust
query.marginalized()   // incidence 1 and not kept
```

A circuit builder asserts this is empty, because a circuit network never
marginalizes. An adapter whose backend cannot marginalize rejects a non-empty
result. Same protection, paid for only where the knowledge is independent of
the thing being checked.

This catches accidental marginalization. It does not catch a wrong-but-paired
index — a label collision joining two nodes that should not touch stays a wrong
answer, and only differential testing against a reference contractor will find
it.

## Two layers

The model above is a **declarative surface**. It is deliberately high-level: a
caller writes `i i ->` to take a trace and never hears the words _cap_,
_spider_, or _Kronecker delta_.

Underneath, every hyperedge can be rewritten into ordinary edges by introducing
an explicit structural node, which is the standard diagrammatic construction:

| surface                    | desugars to                                                     |
| -------------------------- | --------------------------------------------------------------- |
| index on _k_ ≥ 3 operands  | a rank-_k_ `COPY` node, `δ(a₁ … a_k)`, each incidence renamed   |
| `i i` within one node      | the same, at rank 3 — or a rank-2 `δ` when the result is closed |
| index on 1 operand, summed | a rank-1 `ones` node                                            |

After desugaring, every index occurs exactly twice, which is the form every
backend accepts without question.

**Desugaring is an adapter's business, not the model's.** A backend that
handles hyperedges natively keeps them and keeps the memory win described
above; a backend that does not pays for the rewrite. Fixing the interface first
is what buys that freedom, and the choice is made once per backend rather than
once for everyone. It also belongs to the adapter for a second reason: a `ones`
or `δ` node is only correct if its _elements_ are right, and this crate carries
no elements — an adapter owns structure and data together and can synthesize
them.

## Where the data lives

**Not here.** `Indices` describes axes; it carries no elements. The references
split the same way, and it is not a close call: cotengra holds no arrays at all
(`tree.contract(arrays)`), and cuTensorNet separates `NetworkAppendTensor`,
which takes metadata, from `NetworkSetInputTensorMemory`, which takes a buffer.
Planning a contraction needs the shape of the problem and nothing else, so a
caller can ask "is this feasible?" without allocating anything.

For callers that do bind data, this crate defines the layout it assumes:
**column-major**, first axis varying fastest, with `strides()` and
`offset_of(coords)` provided so nobody has to re-derive it. Column-major is not
arbitrary — it is what cuTensorNet means by a `NULL` strides pointer and what
tensor4all's `ColMajorArray` uses.

Because `keep` is an `Indices` like any other, `query.keep().strides()` is
exactly the layout of the result buffer a caller must allocate — and a `None`
from it is the first sign that the result will not fit in memory at all. One
type serves both a node's axes and the result's.

## Chains are a second description

`Mps` sits beside `TensorNetwork`, not beneath it. A network can express a
chain's topology — a line of nodes sharing one index each — but not the
invariants that make it a matrix product state: that the sites form a line at
all, that neighbours agree on the bond between them, that the two ends carry no
outer bond. A network has nowhere to put those. In the other direction a chain
cannot express an arbitrary topology, so neither type contains the other, and
subtyping would cost one of them its invariants.

An earlier revision of this file excluded MPS vocabulary on the grounds that "a
matrix product state is one contraction strategy for a network, not a property
of the network". That conflates the object with the algorithm. The _sweep_ that
builds a chain is a strategy and stays with whoever performs it; the _chain_ is
a shape, and it is the shape callers reason about — how large the bonds grew,
whether the result fits the capacity that was asked for. Waiting for a second
backend was also already satisfied: the shape here was read off both
cuTensorNet's MPS path and tensor4all's CPU prototype.

The evidence that the type was missing rather than speculative is that it
already existed, spelled out five times in the cuTensorNet backend — as a
requested shape, as the metadata the library writes back, as a loose triple
passed into the dense contraction, and twice more on the public report. Every
one of them re-derived the same invariants.

### Extents, but never strides

A site is a list of extents, and deliberately nothing else. Strides — the
offsets used to walk a tensor laid out in a flat buffer — are a property of a
buffer, not of a state. `Mps` carries no elements, so it has no buffer, and a
layout with nothing to lay out is meaningless.

This matters in practice. A backend that truncates writes a tensor smaller than
the allocation it was handed, and reports the strides it actually used; those
strides belong to that one readout, on the one code path that fetched it. The
shape belongs to the state. Keeping them apart is what lets the same `Mps`
describe both a capacity a caller requests and a shape a backend produced —
different values of one type, related by `fits_within`, rather than two types
that happen to hold the same numbers.

Note the asymmetry with `Indices`, which _derives_ column-major strides for a
caller that wants them. That is a default offered to whoever binds data, not a
claim about a buffer that already exists.

## Vocabulary

The field has four names for each of two ideas. This crate picks one of each
and says so, to stop the translation tax being paid over and over:

| here    | cuTensorNet          | tensor4all | quimb             | cotengra        |
| ------- | -------------------- | ---------- | ----------------- | --------------- |
| `Index` | `mode` / `modeLabel` | `Index`    | `ind`             | `index`         |
| `dim`   | `extent`             | `dim`      | from `data.shape` | `size_dict[ix]` |

`Index` is chosen because it is the field's most common term and the one a
reader of ITensor, quimb, or tensor4all already holds. It does not collide with
`std::ops::Index`, which is not in the prelude — this was compile-checked, not
assumed.

Two traps worth flagging:

- **`extent` is a false friend.** In cuTensorNet it means an axis dimension. In
  cotengra it means the number of leaves under a contraction-tree node. It
  appears zero times in tensor4all and quimb. The word is avoided here.
- **`fuse` is a false friend.** In quimb, `Tensor.fuse` combines several axes
  into one fat axis — a reshape. It has nothing to do with joining tensors.

## Divergences from numpy's einsum

The model is einsum's, with three deliberate restrictions. All three were
checked against numpy rather than recalled:

- **No implicit mode.** `np.einsum("ij,jk", a, b)` silently contracts `j` and
  orders the survivors alphabetically. `keep` is always required here, because
  the alphabetical rule is exactly what hides the difference between a set of
  surviving indices and a sequence of them.
- **No repeated kept index.** numpy rejects `->ikk` too ("output subscript 'k'
  multiple times"). Duplication in `keep` would be a second way to say what a
  `COPY` node already says.
- **No ellipsis / broadcasting.** `...` has no meaning here.

Indices are identified by `id`, not by letter, so einsum's 52-label ceiling
does not apply. That is the same choice numpy offers through its sublist form,
`np.einsum(a, [0, 1], b, [1, 2], [0, 2])`.

## What is deliberately not here

- **An MPS trait family.** `Mps` is a struct and nothing more. Backends differ
  too widely in how they build and truncate a chain for a shared trait to be
  discovered rather than invented, and the original condition on that — a
  second implementation to generalize from — is about the _algorithm_, not the
  shape. See "Chains are a second description" for what did come in, and why.

- **Truncation.** The three implementations express approximation in three
  incompatible ways (maximum bond dimension and discarded weight; SVD algorithm
  with absolute and relative cutoffs plus a gauge option; nothing at all). A
  shared abstraction here would be invented rather than discovered. Backends
  will advertise what they support instead.

- **Vendor scalar types.** Indices and dimensions are described with ordinary
  Rust types, not cuTensorNet's `int32_t` and `int64_t`. The narrowing, and the
  overflow check that goes with it, belongs at the FFI boundary where it is
  actually a constraint.

- **Contraction itself.** No paths, no slicing, no cost model, no execution.
  A planner turns a `ContractionQuery` into a contraction plan, and that plan
  is where flop counts, largest intermediates, and slice counts live — the
  numbers that answer whether a network is feasible at all. It needs a memory
  budget and an objective to do that, so it is a separate concern with a
  separate configuration, and the model stops here.
