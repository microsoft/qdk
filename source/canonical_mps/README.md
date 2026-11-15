# canonical_mps: a canonical-form MPS engine, and its backend contract

The `canonical_mps` crate lets a QDK simulator written against
`tensornet`'s `MpsEngine` trait run its matrix product states (MPS) in
canonical form on any backend that implements a small set of tensor
operations. It has two parts:

- **[The engine](#the-engine):**
  [`CanonicalMpsEngine`](src/engine.rs) implements `tensornet::MpsEngine`
  over any `SiteAlgebra`: it routes, fuses, truncates and reads by one set
  of rules.
- **[The backend contract](#the-backend-contract):**
  [`SiteAlgebra`](src/algebra.rs), the tensor operations a backend
  implements, and a [truncation rule](#the-truncation-rule) that a backend
  which computes the singular values itself may use.

**The problem.** Some of an MPS engine's rules are the same on every
backend, the CPU or a GPU library: where the orthogonality centre moves,
which pair a gate splits, how gates on distant qubits are routed, how the
retained weight is kept, and the bounds every truncation stays within.
What differs is where the tensors live, which library factors them, and
which values a truncation keeps within those bounds. Written once per
backend, those rules drift apart.

**How it is solved.** The engine holds the rules, written once over the
contract, and a backend writes only the contract's operations. The
backends that run on a CPU and on a GPU library are planned, each in its
own crate; this crate tests the engine on a host
[test algebra](src/algebra/fake_algebra.rs).

**Why this approach.**

- **One implementation of the rules.** The centre's moves, the routing,
  the fusion of gates, the truncation's bookkeeping and the checks of
  every call belong to the engine alone, so every backend runs them alike,
  and a fix reaches every backend.
- **Small backends.** A backend writes five operations, on a site's
  storage or on two neighbouring sites; it checks no argument, since the
  engine checks every one before it calls, and reports only its own
  failures.
- **Open to more backends.** A new backend is a new type implementing the
  trait, in its own crate; nothing here changes.
- **Portable.** The crate depends on `tensornet`, `dense_linalg`,
  `num-complex` and `thiserror`, all pure Rust, so it builds on every
  [platform](#platforms) the QDK supports.

## Terms

An MPS engine's documentation uses terms of MPS simulation, and the
engine's own for the work it defers or reorders:

- **Site:** the tensor of one qubit, `T[a, s, b]`, with `s` the qubit's
  value and `a` and `b` the indices of its left and right **bonds**, which
  it shares with its neighbours. A bond's **extent** is its number of
  values; the **cut** `c` is the bond between sites `c` and `c + 1`.
- **Isometry:** a site that, read as a matrix towards one side, has
  orthonormal columns (or rows); it carries none of the state's norm.
- **Canonical form, orthogonality centre:** one site, the centre, holds
  the state's norm, and every other site is an isometry towards it, so the
  state's reads and truncations at the centre are local.
- **Gated pair:** the tensor `θ` of two neighbouring sites contracted over
  their bond, with a two-qubit gate applied.
- **Truncation, cutoff, capacity:** a split factors `θ` by an SVD and
  keeps only its largest singular values, which truncates the bond between
  the pair. The **cutoff** is the share of the squared singular values a
  split may drop; the **capacity** is the shape a caller prepares a state
  with, whose bond at each cut, its `χ`, is the largest a split at rest
  may keep there; the contract calls the largest bond one split may keep
  its **cap**.
- **Retained fraction, retained weight:** the share of the squared
  singular values a split keeps; the state's **retained weight** is the
  product of its splits' retained fractions, the weight its truncations
  kept.
- **Pending gate:** a gate the engine has accepted but not yet applied to
  the backend's tensors. A one-site gate waits on its site; the gates on
  the pair the engine last joined wait as one 4×4 gate.
- **Fusion:** the gates on one pair, one-site gates on either site
  included, multiply into the pair's pending gate, so that one split
  applies them all.
- **Route:** a gate on two sites that are not neighbours runs as SWAPs that
  bring them together, the gate, and SWAPs that take them back.

## The engine

[`CanonicalMpsEngine`](src/engine.rs) implements `tensornet`'s
[`MpsEngine`](../tensornet/src/engine.rs) over any
[`SiteAlgebra`](src/algebra.rs). What the trait requires is in
[`tensornet`'s README](../tensornet/README.md) and the trait's rustdoc;
this section says how this engine meets it, and why;
[`docs/engine.md`](docs/engine.md) gives the backend calls of each phase,
where the centre goes, and the numerics, and each file's header the
details.

Two kinds of method meet here: an _MPS simulation method_ is the algorithm
that runs a program's gates, measurements and shots on an MPS, and it calls
the _`MpsEngine` methods_ (`apply`, `project`, `probability_of_one`, …).
The engine calls the backend's _operations_ (`gate_split`, `qr_move`, …).

```mermaid
flowchart LR
  S["an MPS simulation method<br/>(gates, measurements, shots)"] -->|"MpsEngine methods"| E["<b>CanonicalMpsEngine</b>"]
  E -->|"SiteAlgebra operations"| B["a backend<br/>(the site tensors, where they live)"]
  E -->|"host products"| D["dense_linalg"]
```

### The engine's parts

[`src/engine.rs`](src/engine.rs) holds the engine, its state
(`CanonicalMpsState`, a chain of the backend's tensors) and the
`MpsEngine` implementation, which calls one part per kind of work:

- [`chain.rs`](src/engine/chain.rs): the sites, the centre, the pending
  gates, the retained weight and the [statistics](#settings-and-statistics);
- [`gates.rs`](src/engine/gates.rs) and
  [`routing.rs`](src/engine/routing.rs): gates on neighbours, fused, and
  on sites further apart, routed;
- [`split.rs`](src/engine/split.rs) and
  [`canonical_form.rs`](src/engine/canonical_form.rs): the pending pair's
  split, the one place the engine truncates, and the centre's moves;
- [`reads.rs`](src/engine/reads.rs) and
  [`expectation.rs`](src/engine/expectation.rs): the reads of one site, and
  an observable's expectation;
- [`policy.rs`](src/engine/policy.rs) and
  [`error.rs`](src/engine/error.rs): the default cutoff, with its reason,
  and `CanonicalMpsError`, why a call was refused or failed.

### Phases

The engine defers work it can share: a gate waits, pending, until an
operation needs the tensors it changes. So the engine's work on a state
falls into nine phases, which each `MpsEngine` method combines
([the table below](#how-each-mpsengine-method-is-implemented)):

- **check**: the method's `tensornet::checks` function, so the backend
  never sees a refused call.
- **pend**: a gate is multiplied into a pending gate, on the host; no
  backend call.
- **split**: the pending pair is applied and factored by one `gate_split`,
  which truncates; the centre is left on the side the next operation
  needs.
- **move**: the centre moves one site at a time, by one `qr_move` per site
  crossed, which applies the pending gate of the site it leaves.
- **route**: the SWAPs and the gate of a [route](#terms), each pending on
  the pair it acts on until the next one, on the neighbouring pair, splits
  it.
- **read**: the centre's site is copied to the host (`read`), its pending
  gate applied, and its values' weights are summed.
- **contract**: the expectation's products of host matrices, by
  `dense_linalg`.
- **upload**: a site computed on the host becomes a backend tensor
  (`upload`).
- **duplicate**: every site is copied, by the backend (`duplicate`).

### How each `MpsEngine` method is implemented

Each method first runs its check, when the contract has one; the table
names the phases that follow.

| `MpsEngine` method               | Phases                                                                                                         | How                                                                                                                                                                                                                                                   | Why                                                                                                                                                                                                    |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `prepare`                        | check; upload                                                                                                  | a site holding \|0⟩ per qubit, its bonds of extent 1; the centre on site 0, a retained weight of 1                                                                                                                                                    | a product state needs no split; the bonds grow only as gates entangle                                                                                                                                  |
| `apply`, one site                | check; pend                                                                                                    | the gate joins the pending pair's gate when its site is in the pair, else its site's pending gate                                                                                                                                                     | the next split or move applies it in a call it makes anyway                                                                                                                                            |
| `apply`, two neighbours          | check; split, of another pending pair; move; pend                                                              | the centre moves to the pair's site nearer it; the two sites' pending gates and the gate become the pair's pending gate, which later gates on the pair join                                                                                           | the gates on one pair share one split ([fusion](#terms)), and the centre moves no further than the pair                                                                                                |
| `apply`, distant sites           | check; split, of another pending pair; move; route                                                             | `swap-back-2x`: the site at `high` moves down to `low + 1` and back; the outbound SWAPs' splits apply the cutoff alone, the gate's and the return's the cutoff and the capacity; the last return's pair stays pending                                 | the chain is in the caller's order after every call; a route drops no weight beyond the cutoff's and the capacity's at a caller's cut ([`docs/routing.md`](docs/routing.md))                           |
| `probability_of_one`             | check; split, toward the site; move; read                                                                      | P(One) is the One value's weight over both values' weights                                                                                                                                                                                            | a value of weight exactly zero reads exactly 0; a gate's drift from norm 1, within the unitarity check's tolerance, divides out on the centre's site, and elsewhere shifts reads by at most that order |
| `project`                        | check; split, toward the site; move; read; upload                                                              | refused when the value's probability `p` is zero or NaN, what every read sees unchanged; else the other value's elements are zeroed, and the site divided by the norm of what it kept, computed scaled; returns `p`; the retained weight is unchanged | the state's norm is 1 to rounding, however few digits `p` holds                                                                                                                                        |
| `conditional_probability_of_one` | check; split and duplicate (a fork), with conditions; as `project` per condition; then as `probability_of_one` | the conditions are projected in order on one fork, each refused as `project` refuses; without conditions, the state is read directly                                                                                                                  | the caller's state is not projected; a rare condition whose complement rounds to 1 is not refused, as each value is read from its own weight                                                           |
| `sample`                         | check; as `conditional_probability_of_one` per new prefix                                                      | `tensornet::sampling`'s shared sampler, over this engine's conditional probabilities                                                                                                                                                                  | the draws are `tensornet`'s shared sampler's, as for every engine that uses it, and a value of probability zero is never drawn                                                                         |
| `expectation`                    | check; split, toward the centre; read; contract                                                                | the sites between the terms and the centre are read once each; the terms walk a trie of their factors, from the plain chain's environments; the sum is divided by the squared norm at the centre                                                      | a chemistry reading holds hundreds of Pauli strings that share their sites; one walk per term would read the sites many times over                                                                     |
| `fork`                           | split, toward the centre; duplicate                                                                            | the copy keeps the centre, the pending one-site gates, the retained weight and the statistics                                                                                                                                                         | a pending pair is split once, in the parent, not once per fork; a fork is the same state                                                                                                               |
| `shape`                          | split, toward the centre                                                                                       | the extents of the chain's bonds                                                                                                                                                                                                                      | a pending pair's truncation sets its bond                                                                                                                                                              |
| `retained_weight`                | split, toward the centre                                                                                       | the product of the splits' retained fractions                                                                                                                                                                                                         | a pending pair's truncation sets its share; the chain's norm stays about 1, so the weight is kept apart                                                                                                |

A split that would take the retained weight below the smallest normal
`f64` fails with `CanonicalMpsError::WeightUnderflow`, since callers divide
by the weight. After any failure the state is unspecified, as the
contract says, and the caller drops it.

### Settings and statistics

The cutoff and the capacity's `χ` trade time for accuracy:
[`CanonicalMpsEngine::with_cutoff`](src/engine.rs) says how, and
[`policy.rs`](src/engine/policy.rs) gives the default cutoff with how it
was chosen. A caller that tunes them reads a state's
[`SplitStatistics`](src/engine/chain.rs): its splits, SWAPs and moves of
the centre, and its largest bond at rest and during routes, apart.

## The backend contract

### The operations

[`SiteAlgebra`](src/algebra.rs) has one associated type for where a site's
elements live (`Tensor`) and one for its failures (`Error`). Its rustdoc
states each operation's guarantees, to rounding; the table says what each
is for.

| Operation    | What the backend computes                                                                                                                                                     | What the engine needs it for                                                                                |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `upload`     | a new tensor holding host values                                                                                                                                              | a new state's sites, and the sites it computes on the host (a projected site)                               |
| `read`       | a tensor's elements on the host, borrowed where they already live there                                                                                                       | the reads, which contract sites on the host (a probability)                                                 |
| `duplicate`  | a new tensor holding a copy of another's elements                                                                                                                             | a fork of a state, whose sites then change independently                                                    |
| `gate_split` | `θ` factored by one SVD, truncated as the call says, the kept values normalised and absorbed on the centre's side, the other side an isometry; the bond and retained fraction | a two-qubit gate on neighbouring qubits, with the centre on the pair                                        |
| `qr_move`    | the centre moved across a bond, exactly, by one QR of the site it leaves, a one-site gate applied to that site first                                                          | bringing the centre to a gate or a read without truncating, and applying a pending one-site gate on the way |

[`GateSplitCall`](src/algebra.rs) and [`QrMoveCall`](src/algebra.rs) define
each operation's inputs exactly, and
[`GateSplitOutcome`](src/algebra.rs) and [`QrMoveOutcome`](src/algebra.rs)
its outputs.

### The layout

Every tensor holds complex doubles, its first mode fastest, so a backend
factors a site read as a matrix with no copy; the
[module's documentation](src/algebra.rs) gives the layout in full.

## The reference algebra

[`CpuAlgebra`](src/algebra/cpu_algebra.rs), exported at the crate's root,
is the contract on the host CPU. Each operation is computed directly from
its contract, so a backend on another library can be compared with its
answers, call by call, and the engine's tests will run on it. It needs no
device, so it runs wherever the QDK runs.

`CpuAlgebra` holds each site in a `Vec<Complex64>`, in the contract's
layout, which is the column-major order of `dense_linalg`'s matrices, so
the operations read a site as a matrix without reordering it.

| Operation    | How                                                                                                                                                                                                           | Why                                                                                                                                                                                                     |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `upload`     | a copy of the host values into a new vector                                                                                                                                                                   | the tensors already live on the host                                                                                                                                                                    |
| `read`       | the tensor's first `len` elements, borrowed                                                                                                                                                                   | the engine's host reads will need no copy                                                                                                                                                               |
| `duplicate`  | a copy of the tensor's first `len` elements                                                                                                                                                                   | a fork's sites change apart from its parent's                                                                                                                                                           |
| `gate_split` | `θ` by one product and the gate's loops; one `Svd::of`; the values the [truncation rule](src/algebra/truncation.rs) keeps, normalised, multiplied into the centre's side, the other side the singular vectors | the SVD's factors are checked against `θ` before the truncation: when they do not give `θ` back, the split fails with `SvdError` rather than spread them; what it keeps is `θ`'s truncation, normalised |
| `qr_move`    | the one-site gate applied to the site the centre leaves; one thin `Qr::of` of it; the small factor multiplied into the other site                                                                             | a QR moves the centre exactly, and never fails                                                                                                                                                          |

The truncation rule drops the longest tail of singular values whose
squares sum to at most the cutoff's share, then keeps at most the cap;
its [documentation](src/algebra/truncation.rs) states it in full. Only
`gate_split` fails, when the SVD does
([`SvdError`](../dense_linalg/src/svd.rs)): `DidNotConverge` when its
rotations still turn after their last sweep, `FailedChecks` when `θ` holds
a value that is not finite or the factors fail the SVD's checks.

## Tests

The [unit tests](src/tests) check the truncation rule, the
[host test algebra](src/algebra/fake_algebra.rs) against the contract, and
the engine's rules on that algebra; where the engine reads values, they
compare them with closed-form values, or with `tensornet`'s exact reference
on circuits too large to check by hand. Each backend checks its
own implementation of the contract in its crate.

## Platforms

| Platform               | Builds                                                                                                                             | Tests run |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------- |
| Linux, macOS, Windows  | Yes                                                                                                                                | Yes       |
| WebAssembly (`wasm32`) | Yes, checked by a build and by Clippy for `wasm32-unknown-unknown`, but not in CI, since no WebAssembly package uses the crate yet | No        |
