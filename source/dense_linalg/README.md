# dense_linalg: dense complex matrices with a checked SVD

The QDK's matrix product state (MPS) simulators truncate a state by the
singular values of small dense complex matrices, many thousands of times per
run, and keep the singular vectors as an orthonormal basis. This crate gives
them a singular value decomposition (SVD) they can trust, with the product,
thin QR decomposition and adjoint they also need, in three parts:

- **[The checked SVD](#the-checked-svd):** [`Svd::of`](src/svd.rs) either
  answers factors checked against the matrix, or reports a failure
  ([`SvdError`](src/svd.rs)).
- **[Matrices, product and QR](#matrices-product-and-qr):**
  [`Matrix`, `MatrixRef`](src/matrix.rs) and [`Qr`](src/qr.rs).
- **[The linear algebra library](#the-linear-algebra-library):** t4a-faer,
  behind the crate-private trait [`Kernels`](src/kernels.rs).

**The problem.** The matrices met when simulating MPS are rank-deficient most
of the time, with repeated singular values, and general SVDs have returned
factors that do not give back the matrix on them, without reporting a
failure ([`svd.rs`](src/svd.rs)'s header gives the cases). A simulator that
acts on such values truncates the wrong state, silently.

**The approach.** The SVD's rules are the crate's own, written once: a trim
that reduces the matrix to full rank, leaving out only rounding; one-sided
Jacobi rotations, which converge on a matrix of full rank within a cap of
sweeps, or report the failure; and checks, to the bounds of LAPACK's SVD
tests, on every answer. A linear algebra library brings only the steps it
computes faster (the products and the QR decompositions, and a fast route
for large full-rank matrices), behind one crate-private trait, so that the
library is one file. The SVD checks every answer it gives, the fast route's
included; the product and the thin QR are the library's, to rounding, and
not checked.

## The checked SVD

[`Svd::of`](src/svd.rs) answers `a = u·diag(s)·v†` with `r` values,
`1 ≤ r ≤ min(rows, cols)`, nonincreasing. Its guarantees and its steps
name the parts of its algorithm with these terms:

- **The trim:** a column-pivoted QR `a·P = Q·R`, whose rows of `R` of
  smallest norm are dropped while their joint norm stays within rounding.
- **`W`:** the adjoint of the kept rows of `R·Pᵀ`, of full rank; the SVD of
  `a` follows from `W`'s.
- **The rotations:** one-sided Jacobi rotations of `W`'s columns, LAPACK
  `zgesvj`'s rules.
- **The fast route:** the library's bidiagonalization and QR algorithm.

It guarantees:

- **Checked factors:** `u·diag(s)·v†` gives back `a`, and `u` and `v` are
  orthonormal, to the bounds of LAPACK's SVD tests
  ([`svd/checks.rs`](src/svd/checks.rs)), else
  [`SvdError::FailedChecks`](src/svd.rs); when the rotations still
  rotate after their last sweep, [`SvdError::DidNotConverge`](src/svd.rs).
  A matrix with an entry that is not finite fails the checks, before any
  work.
- **Only rounding left out:** the values left out (`r < min(rows, cols)`)
  are those of the trim's dropped rows, whose joint norm is within
  `max(m, n) · ε` times the largest row's; the zero matrix gives one
  value, 0.
- **Any scale:** down to subnormal entries, short of matrices whose
  singular values are too small or too large to store, which the checks
  refuse.

It runs these steps:

| Step                   | How                                                                                     | Why                                                                                                                                                                          |
| ---------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Zero matrix         | One value, 0, and the first unit vectors.                                               | It has no rank to keep and no largest entry to scale by.                                                                                                                     |
| 2. Wide matrix         | Factored through its adjoint.                                                           | The rotated columns are then the shorter side.                                                                                                                               |
| 3. Scaling             | By a power of two when the largest entry is far from 1.                                 | The squared norms neither overflow nor underflow; it rounds only entries it takes below the normal range.                                                                    |
| 4. Trim                | Drops the rows of `R` of smallest norm, chosen by norm, not by pivot.                   | The rest sees full rank; the pivoting does not keep `\|R_ii\|` nonincreasing.                                                                                                |
| 5. Route               | A large `W` far from orthogonal: the fast route; any other: the rotations.              | The rotations are fast on nearly orthogonal `W`; the fast route on the rest. A reported failure or factors that are not finite fall back; finite wrong ones fail the checks. |
| 6. Left vectors        | The kept columns of `Q`, applied by its reflectors, times `W`'s right singular vectors. | Cheaper than forming `Q`: the reflectors act on the answer's `r` columns alone, and those after the last kept row are skipped.                                               |
| Checks, on each answer | Reconstruction and orthonormality, to LAPACK's bounds.                                  | No route is trusted: a library's SVD has answered wrong factors without a failure.                                                                                           |

[`svd.rs`](src/svd.rs)'s header gives the details;
[`svd/checks.rs`](src/svd/checks.rs) and [`svd/jacobi.rs`](src/svd/jacobi.rs)
the checks and the rotations.

The SVD of a matrix of rank 1, whose one singular value is 5 (the crate's
doctest):

```rust
use dense_linalg::{Matrix, Svd};
use num_complex::Complex64;

// (3, 4)ᵀ·(1, 0, 0): column-major, one column after the other.
let x = |re| Complex64::new(re, 0.0);
let zero = x(0.0);
let a = Matrix::new(2, 3, vec![x(3.0), x(4.0), zero, zero, zero, zero]);
let svd = Svd::of(a.view())?;
assert_eq!(svd.singular_values.len(), 1);
assert!((svd.singular_values[0] - 5.0).abs() < 1e-14);
```

## Matrices, product and QR

- [`Matrix`](src/matrix.rs) owns a dense complex matrix, column-major;
  [`MatrixRef`](src/matrix.rs) borrows one. `Matrix::product(a, b)` is
  `a·b`, and `MatrixRef::adjoint` the conjugate transpose.
- [`Qr::of`](src/qr.rs) is the thin QR decomposition `a = q·r` of a
  finite matrix, by Householder reflections, at any scale (scaled by a
  power of two as in the SVD's step 3). It cannot fail, and checks
  nothing: a matrix with an entry that is not finite gets unspecified
  factors.

## The linear algebra library

The crate computes its products, QR decompositions and fast route with
t4a-faer, a fork of faer, through the crate-private trait
[`Kernels`](src/kernels.rs); [`kernels/t4a_faer.rs`](src/kernels/t4a_faer.rs)
is its implementation. t4a-faer is pinned to one version
([`Cargo.toml`](Cargo.toml) says why), built without threads, and runs its
QR algorithm at every size, never its divide and conquer.

## Tests

The tests are in [`src/tests/`](src/tests), one file per module, through
the public API where they can be: random, rank-deficient, degenerate and
graded matrices, every scale, each route and its fallback, and the errors.

## Platforms

| Platform               | Builds | Tests run |
| ---------------------- | ------ | --------- |
| Linux, macOS, Windows  | Yes    | Yes       |
| WebAssembly (`wasm32`) | Yes    | No        |

Each computation runs on the calling thread, in pure Rust.
