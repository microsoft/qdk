# `tensornet`

A backend-agnostic description of tensor networks, by shape alone.

## What this crate does

It describes one family of tensor network today: the matrix product state (MPS),
a chain of site tensors, given by the extents (axis lengths) of each site.
`MpsShape::new` checks that the extents form a chain, and the accessors answer
the questions callers ask of a chain's shape: its bond dimensions, its largest
bond, how many elements each site holds, and whether one chain fits inside
another.

`MpsShape` does **not** hold tensor elements, strides or device memory. Because
the description carries no data, the same shape can be handed to a GPU library,
to a CPU library, or to a reference implementation written purely for a test.

The crate also states what a backend that runs a chain does, as the trait
`MpsEngine`: prepare a chain within a capacity (an `MpsShape`), apply a unitary
to sites, read the probability of a one at a site (also given earlier values),
project, fork (a copy up to the engine's truncation), sample sites for many
shots from one seed, take the expectation
of a Pauli sum, and report the chain as realized and the weight its
truncations kept. It speaks tensors only: no
shots, RNG objects or instructions, so a caller that runs shots draws its own
values above it. How a chain is built and truncated stays with each engine;
`qdk_cutensornet` implements the trait over cuTensorNet.

`reference::ExactReference` is the trait's oracle: a dense state vector of at
most 20 sites whose shape is the exact Schmidt rank at each cut. It is not a
tensor network and not an engine to simulate with. It is the exact answer
every engine with enough bond must give, so engines check themselves against
it, and tests run on it where there is no GPU.

## Lifecycle at a glance

A method that runs an MPS on a backend uses `MpsShape` twice per state, once
before and once after the backend does its work:

```text
method                       tensornet                              backend
requested extents ──► MpsShape::new ─► target  ──► element_counts() ──► allocates site buffers
                                                                         applies gates, truncates
reported extents  ◄───────────────────────────────────────────────────── writes back the extents it used
      │
      └─────────► MpsShape::new ─► realized ──► realized.fits_within(&target)   the backend kept its bound
                                            └─► realized.max_bond()            the bond reached, for Cost
```

- **Before:** the target is the capacity the method asks for. Its element
  counts size the buffers the backend writes into.
- **After:** the realized chain is what the backend produced. Truncation can
  only shrink a bond, so a realized chain that does not fit its target is a
  backend defect, not a property of the program.

## Files

| File                 | Defines                                                                                    |
| -------------------- | ------------------------------------------------------------------------------------------ |
| `src/mps.rs`         | `MpsShape`: the chain's shape, its validation and its accessors                            |
| `src/error.rs`       | `MpsShapeError`: why a list of extents is not a chain, raised by `MpsShape::new`           |
| `src/engine.rs`      | `MpsEngine`: what a backend that runs a chain does; `SampleSeed`                           |
| `src/pauli.rs`       | `Pauli`, `PauliTerm`, `PauliSum`: the observables of `MpsEngine::expectation`              |
| `src/reference.rs`   | `ExactReference`: the dense oracle every engine is checked against                         |
| `src/conformance.rs` | `check`: the cases every engine must pass against `ExactReference` (feature `conformance`) |
| `src/lib.rs`         | the crate's public surface and its role                                                    |

The API reference is the rustdoc (`cargo doc -p tensornet --open`). The crate
depends only on `thiserror` and `num-complex`, so it builds for every target
the QDK supports, WebAssembly included.

An engine's crate tests itself with `conformance::check(&engine)`, which runs
the same operations on the engine and on `ExactReference` and returns every
case where they differ. It is behind the `conformance` feature, which such a
crate enables in its `[dev-dependencies]`, so no ordinary build compiles it.

## The shape of a chain

A chain of $n$ sites has a physical extent $p_k$ at each site and a bond
$\chi_k$ between sites $k$ and $k+1$. A site lists its extents in the order
`[bond_left, physical, bond_right]`, with the absent bond omitted at each end:

```text
 site 0         site 1                  site n-1
[p₀, χ₀] ── [χ₀, p₁, χ₁] ── … ── [χₙ₋₂, pₙ₋₁]
 rank 2        rank 3                  rank 2          (a lone site is [p₀], rank 1)
```

Site $k$ holds $\chi_{k-1}\,p_k\,\chi_k$ elements (an absent bond counts as 1),
which is what `element_counts` returns; `bond_dim(k)` is $\chi_k$, and
`max_bond` is $\max_k \chi_k$, or 1 for a lone site, which has no bond.

`MpsShape::new` rejects anything that is not a chain, each with its own
`MpsShapeError` variant: no sites, a rank that does not match the site's
position, an extent of zero, neighbours that disagree about the bond they share,
or a site whose element count does not fit a machine word. Rejecting the last
one up front is what lets every later element count be answered without
overflow.

Nothing here is specific to qubits. A physical extent is just a number, so a
chain of qutrits is describable without change; a caller that requires
two-level sites checks `physical_dim(site) == Some(2)` itself.

## Why a chain is a description of its own

A general tensor network can express a chain's topology (a line of nodes sharing
one index each) but not the invariants that make it a matrix product state: that
the sites form a line at all, that neighbours agree on the bond between them,
and that the two ends carry no outer bond. A general network has nowhere to put
those; `MpsShape` holds them. In the other direction a chain cannot express an
arbitrary topology, so if a general network is described here, it is a peer of
`MpsShape` rather than its supertype, and neither loses its invariants.

The _sweep_ that builds a chain is a strategy and stays with the method that
performs it. The _chain_ is a shape, and it is the shape callers reason about:
how large the bonds grew, and whether the result fits the capacity that was
asked for.

## Extents, but never strides

A site is a list of extents, and deliberately nothing else. Strides (the offsets
used to walk a tensor laid out in a flat buffer) are a property of a buffer, not
of a state. `MpsShape` carries no elements, so it has no buffer, and a layout
with nothing to lay out is meaningless.

This matters in practice. A backend that truncates writes a tensor smaller than
the allocation it was handed, and reports the strides it actually used; those
strides belong to that one readout, in the code that fetched it. The shape
belongs to the state. Keeping them apart is what lets one type describe both a
capacity a caller requests and a shape a backend produced: different values of
one type, related by `fits_within`, rather than two types that happen to hold
the same numbers.

## What is deliberately not here

- **General networks and their contraction.** No indices, networks,
  contraction queries or plans. No simulation method in the QDK contracts a
  general network yet; the types arrive with the first one that does.

- **A shared algorithm.** `MpsShape` is a description, and `MpsEngine` states
  what an engine answers, not how it builds or truncates a chain: backends
  differ too widely there for a shared implementation to be discovered rather
  than invented. The trait came with the first engine, shaped by the calls
  cuTensorNet makes, and is checked against `ExactReference`.

- **Truncation settings.** Libraries express approximation in incompatible
  ways (maximum bond dimension and discarded weight; an SVD algorithm with
  absolute and relative cutoffs plus a gauge option; none at all). A shared
  abstraction here would be invented rather than discovered, so each method
  takes its own options.

- **Vendor scalar types.** Extents are ordinary `usize` values, not
  cuTensorNet's `int32_t` and `int64_t`. The narrowing, and the overflow check
  that goes with it, belongs at the FFI boundary, where it is actually a
  constraint.
