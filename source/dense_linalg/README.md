# dense_linalg: dense complex matrices with a checked SVD

The QDK's matrix product state (MPS) simulators truncate a state by the
singular values of small dense complex matrices, many thousands of times per
run, and keep the singular vectors as an orthonormal basis.
[Terms](docs/algorithms.md#terms) defines each word of linear algebra used
here. This crate gives the simulators a singular value decomposition (SVD)
they can trust, and its truncation, with the product, thin QR and LQ
decompositions, adjoint and norms they also need. Today `canonical_mps` uses
it, for its CPU backend's operations and its engine's computations on the
host. It has three parts:

- **[The checked SVD](#the-checked-svd):** [`Svd::of`](src/svd.rs) either
  answers factors checked against the matrix, or reports a failure
  ([`SvdError`](src/svd.rs)); `Svd::truncated` keeps its largest values.
- **[Matrices, products, decompositions and norms](#matrices-products-decompositions-and-norms):**
  [`Matrix`, `MatrixRef`](src/matrix.rs), [`Qr`](src/qr.rs),
  [`Lq`](src/lq.rs) and [the norms](src/norms.rs).
- **[The kernels](#the-kernels):** matrixmultiply's product and the crate's
  own QRs, behind the crate-private trait [`Kernels`](src/kernels.rs).

**The problem.** The matrices met in MPS simulations are rank-deficient
most of the time, with repeated singular values. General SVDs have returned
factors that do not give back the matrix on them, without reporting a
failure ([`svd.rs`](src/svd.rs)'s header gives the cases). A simulator that
acts on such factors truncates the wrong state, silently.

**The approach.** The SVD's rules are the crate's own, written once:

- a trim that reduces the matrix to full rank, leaving out only rounding;
- one-sided Jacobi rotations, which rotate the columns until orthogonal, or
  report a failure after a cap of sweeps;
- checks on every answer, to the bounds of LAPACK's SVD tests.

The steps that set the crate's speed (the products and the QR
decompositions) sit behind one crate-private trait. So replacing their
implementation changes none of the SVD's rules.

[`docs/algorithms.md`](docs/algorithms.md) explains, after those Terms, the
SVD's steps and the kernels.

## The checked SVD

[`Svd::of`](src/svd.rs) answers `a = u·diag(s)·v†` with `r` values,
`1 ≤ r ≤ min(rows, cols)`, nonincreasing. It guarantees:

- **Checked factors.** `u·diag(s)·v†` gives back `a`, and `u` and `v` are
  orthonormal, to the bounds of LAPACK's SVD tests
  ([`svd/checks.rs`](src/svd/checks.rs)). Otherwise it fails with an
  [`SvdError`](src/svd.rs). A matrix with an entry that is not finite fails
  before any work.
- **Only rounding left out.** The values left out
  (`r < min(rows, cols)`) are of rounding's size. The zero matrix gives one
  value, 0.
- **Any scale.** It answers down to subnormal entries, short of matrices
  whose singular values are too small or too large to store.

[`Svd::of`](src/svd.rs)'s doc states each guarantee exactly, and
[`svd.rs`](src/svd.rs)'s header gives the steps. [The checked SVD](docs/algorithms.md#the-checked-svd) explains them.

`Svd::truncated(k)` keeps the first `k` values, `u`'s first `k` columns and
`v†`'s first `k` rows, each element as it was: for instance, the
truncation an MPS simulator applies between two sites. It panics unless
`1 ≤ k ≤ r`.

The SVD of a matrix of rank 1, whose one singular value is 5 (the crate's
doctest):

```rust
use dense_linalg::{Matrix, Svd};
use num_complex::Complex64;

// (3, 4)ᵀ·(1, 0, 0): column-major, one column after the other.
let x = |re| Complex64::new(re, 0.0);
let zero = x(0.0);
let a = Matrix::new(2, 3, vec![x(3.0), x(4.0), zero, zero, zero, zero]);
let svd = Svd::of(a.view()).expect("a rank-1 matrix passes the checks");
assert_eq!(svd.singular_values.len(), 1);
assert!((svd.singular_values[0] - 5.0).abs() < 1e-14);
```

## Matrices, products, decompositions and norms

- [`Matrix`](src/matrix.rs) owns a dense complex matrix, column-major.
  [`MatrixRef`](src/matrix.rs) borrows one. `Matrix::product(a, b)` is
  `a·b`, and `MatrixRef::adjoint` the conjugate transpose.
  `Matrix::multiply_columns_by` and `multiply_rows_by` multiply each column
  or row by its own value, as `u·diag(s)` and `diag(s)·v†` do.
- [`Qr::of`](src/qr.rs) is the thin QR decomposition `a = q·r` of a finite
  matrix, by Householder reflections. It works at any scale short of an `r`
  too large to store (scaled by a power of two, as the
  [checked SVD](docs/algorithms.md#the-checked-svd)'s matrix is). It cannot
  fail and, like `Matrix::product`, does not check its answer. A matrix
  with an entry that is not finite gets unspecified factors.
- [`Lq::of`](src/lq.rs) is the thin LQ decomposition `a = l·q`, with `q`'s
  rows orthonormal: the QR of `a†`, read back. Like `Qr::of`, it works at
  any scale, cannot fail and does not check its answer.
- [`norm`, `squared_norm` and `norm_divided_by`](src/norms.rs) give a
  vector's norm, or a matrix's Frobenius norm, from its elements, summed
  from the first to the last. `norm_divided_by` stays in range at any
  scale.

## The kernels

The crate computes its products and QR decompositions through the
crate-private trait [`Kernels`](src/kernels.rs). Its implementation,
[`kernels/matrixmultiply_householder.rs`](src/kernels/matrixmultiply_householder.rs),
takes the product from matrixmultiply; its header says why. The QRs are
the crate's own Householder reflections.
[The kernels](docs/algorithms.md#the-kernels-matrixmultiplys-product-and-the-householder-qrs)
says what each keeps that the SVD relies on.

## Tests

Each module with behaviour of its own has its tests beside it, in its
folder's `tests.rs` (for instance [`src/svd/tests.rs`](src/svd/tests.rs)),
through the crate's public items where they can be.
The SVD's trim and the factoring of the rows it keeps are called
directly, on hand-made `R` matrices. So are the rotations' parts on
hand-made columns: the threshold, the norms a rotation keeps, the pairs
skipped, and the step at each pair. So are the pivoted QR's column norms,
on hand-made norms. The scaling, the rest of the kernels' implementation
and the rotations' sweeps are tested through the SVD, the QR, `Kernels`
and the rotations' entry point. They cover random,
rank-deficient, degenerate and graded matrices, every scale, the pivoted
QR's choice of pivots, the trim's choice of rows, the truncation, and the
errors.

## Platforms

| Platform               | Builds                | Tests run |
| ---------------------- | --------------------- | --------- |
| Linux, macOS, Windows  | Yes                   | Yes       |
| WebAssembly (`wasm32`) | Yes (not built in CI) | No        |

Each computation runs on the calling thread, in pure Rust.
