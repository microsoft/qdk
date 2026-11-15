# canonical_mps: a canonical-form MPS engine over a backend's tensor operations

The `canonical_mps` crate is for simulators that run matrix product states
(MPS) in canonical form on more than one backend, the CPU or a GPU library.
Some of the engine's rules are the same on every backend: where the
orthogonality centre moves, which pair a gate splits, how the retained
weight is kept, and the bounds every truncation stays within. What differs
is where the tensors live, which library factors them, and which values a
truncation keeps within those bounds. So the engine is to be written once,
over a small contract, and a backend writes only the contract's tensor
operations. The crate has two parts:

- **[The backend contract](#the-backend-contract):**
  [`SiteAlgebra`](src/algebra.rs), the tensor operations a backend
  implements, and a [truncation rule](#the-truncation-rule) that a backend
  which computes the singular values itself may use.
- **[The engine](#the-engine-planned)**, which will implement `tensornet`'s
  `MpsEngine` over any `SiteAlgebra`, is planned: it is not in this crate yet.

**Why this approach.**

- **One implementation of the rules.** The centre's moves, the truncation's
  bookkeeping and the checks of every call will belong to the engine alone,
  so every backend will run them alike, and a fix will reach every backend.
- **Small backends.** A backend writes five operations, on a site's
  storage or on two neighbouring sites; it checks no argument, since its
  caller checks every one before it calls, and reports only its own
  failures.
- **Open to more backends.** A new backend is a new type implementing the
  trait, in its own crate; nothing here changes.
- **Portable.** The crate depends only on `num-complex`, so it builds on
  every [platform](#platforms) the QDK supports.

## The backend contract

### Terms

The contract's documentation uses these terms of MPS simulation:

- **Site:** the tensor of one qubit, `T[a, s, b]`, with `s` the qubit's
  value and `a` and `b` the indices of its left and right **bonds**, which
  it shares with its neighbours. A bond's **extent** is its number of
  values.
- **Isometry:** a site that, read as a matrix towards one side, has
  orthonormal columns (or rows); it carries none of the state's norm.
- **Canonical form, orthogonality centre:** one site, the centre, holds
  the state's norm, and every other site is an isometry towards it, so the
  state's reads and truncations at the centre are local.
- **Gated pair:** the tensor `θ` of two neighbouring sites contracted over
  their bond, with a two-qubit gate applied.
- **Truncation, cutoff, cap:** a split factors `θ` by an SVD and keeps
  only its largest singular values, which truncates the bond between the
  pair. The **cutoff** is the share of the squared singular values a
  split may drop; the **cap** is the largest bond it may keep.
- **Retained fraction:** the share of the squared singular values a split
  keeps; the engine will multiply the state's retained weight by it.

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

### The truncation rule

A backend that computes the singular values itself may truncate them with
[`Truncation`](src/algebra/truncation.rs), whose documentation states the
rule, within the bounds `gate_split`'s contract sets for every backend.

## The engine (planned)

The engine will implement `tensornet`'s `MpsEngine` over any `SiteAlgebra`:
it will hold a state's sites as the backend's tensors, check each call
against `tensornet`'s contract, move the centre with `qr_move`, apply
two-qubit gates with `gate_split`, and track the retained weight. Its
reads and its factoring of operators, on small host matrices, will call
[`dense_linalg`](../dense_linalg/README.md) directly, the same on every
backend. It is not in this crate yet.

## Tests

The [unit tests](src/tests) check the truncation rule, and a
[host test algebra](src/algebra/fake_algebra.rs), built on `dense_linalg`,
against the contract, since the engine's tests will run on it. Each backend
checks its own implementation of the contract in its crate.

## Platforms

| Platform               | Builds                                                                                                                             | Tests run |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------- |
| Linux, macOS, Windows  | Yes                                                                                                                                | Yes       |
| WebAssembly (`wasm32`) | Yes, checked by a build and by Clippy for `wasm32-unknown-unknown`, but not in CI, since no WebAssembly package uses the crate yet | No        |
