# canonical_mps: a canonical-form MPS engine over a backend's tensor operations

The `canonical_mps` crate is for simulators that run matrix product states
(MPS) in canonical form on more than one backend, the CPU or a GPU library.
The engine's rules (where the orthogonality centre moves, how a two-site gate
is split and truncated, what weight a truncation keeps) are the same on every
backend; only where the tensors live and which library factors them differ.
So the engine is written once, over a small contract, and a backend writes
only the contract's tensor operations. The crate has two parts:

- **[The backend contract](#the-backend-contract):**
  [`SiteAlgebra`](src/algebra.rs), the tensor operations a backend
  implements, and the host [truncation rule](#the-truncation-rule) for
  backends that compute the singular values themselves.
- **[The engine](#the-engine-planned)**, which will implement `tensornet`'s
  `MpsEngine` over any `SiteAlgebra`, is planned: it is not in this crate yet.

**Why this approach.**

- **One implementation of the rules.** The centre's moves, the truncation's
  bookkeeping and the checks of every call belong to the engine alone, so
  every backend runs them alike, and a fix reaches every backend.
- **Small backends.** A backend writes seven operations, on two neighbouring
  sites or on small host matrices; it checks no argument, since the engine
  checks every one before it calls, and reports only its own failures.
- **Open to more backends.** A new backend is a new type implementing the
  trait, in its own crate; nothing here changes.
- **Portable.** The crate depends only on `num-complex` and
  [`dense_linalg`](../dense_linalg/README.md), so it builds for every
  target the QDK supports, WebAssembly included.

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
- **Retained fraction:** the share of the squared singular values a split
  keeps; the engine multiplies the state's retained weight by it.

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
| `multiply`   | the product of two small host matrices                                                                                                                                        | the reads' contractions on the host                                                                         |
| `svd`        | the thin SVD of a small host matrix, accurate on rank-deficient matrices                                                                                                      | factoring an operator on several qubits into one tensor per site; it trusts the values                      |

[`GateSplitCall`](src/algebra.rs) and [`QrMoveCall`](src/algebra.rs) define
each operation's inputs exactly, and
[`GateSplitOutcome`](src/algebra.rs) and [`QrMoveOutcome`](src/algebra.rs)
its outputs. The host matrices are `dense_linalg`'s
[`Matrix` and `MatrixRef`](../dense_linalg/src/matrix.rs), and a host
backend can answer `multiply` and `svd` with its product and its
[checked SVD](../dense_linalg/src/svd.rs).

### The layout

Every tensor holds complex doubles, its first mode fastest (Fortran order):
the element `(a, s, b)` of a site of extents `[left, 2, right]` is at
`a + left·(s + 2·b)`. A site read as a matrix is the column-major
`(2·left) × right` matrix of rows `a + left·s`, or the `left × (2·right)`
matrix of columns `s + 2·b`, over the same elements, so a backend factors a
site with no copy. Outputs are packed: a site of extents `[left, 2, right]`
holds exactly `left·2·right` elements.

### The truncation rule

`gate_split`'s contract bounds what a split may drop: at most `cutoff` of
the squared singular values, unless the cap cut the bond, and never the
largest. A backend whose library truncates keeps the library's rule within
those bounds. A backend that has the singular values on the host uses
[`Truncation`](src/algebra/truncation.rs), so that every host backend
truncates alike: it keeps the fewest values that meet the cutoff, within the
cap, and returns them normalised with the retained fraction, computed
without underflow however small the values are.

## The engine (planned)

The engine will implement `tensornet`'s `MpsEngine` over any `SiteAlgebra`:
it will hold a state's sites as the backend's tensors, check each call
against `tensornet`'s contract, move the centre with `qr_move`, apply
two-qubit gates with `gate_split`, and track the retained weight. It is not
in this crate yet.

## Tests

The [unit tests](src/tests) check the truncation rule, and a
[host test algebra](src/algebra/fake_algebra.rs), built on `dense_linalg`,
against the contract, since the engine's tests will run on it. Each backend
checks its own implementation of the contract in its crate.
