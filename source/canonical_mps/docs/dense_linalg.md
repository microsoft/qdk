# The dense linear algebra: complex matrices with a checked SVD

The CPU algebra ([`CpuAlgebra`](../src/site_algebra/cpu_algebra.rs))
truncates a state by the singular values of small dense complex matrices,
many thousands of times per run, and keeps the singular vectors as an
orthonormal basis. The crate's private module
[`dense_linalg`](../src/dense_linalg.rs) gives it a singular value
decomposition (SVD) it can trust, with the product, thin QR decomposition
and adjoint that it and the engine's host computations also need, in three
parts:

- **[The checked SVD](#the-checked-svd):** `Svd::of` either answers
  factors checked against the matrix, or reports a failure (`SvdError`,
  public at the crate's root, as `CpuAlgebra`'s error).
- **[Matrices, product and QR](#matrices-product-and-qr):** `Matrix`,
  `MatrixRef` and `Qr`.
- **[The linear algebra library](#the-linear-algebra-library):** t4a-faer,
  behind the module-private trait `Kernels`.

The `conformance` feature makes the module public, so that another crate's
test fakes can compute as a backend library would.

**The problem.** The matrices met when simulating MPS are rank-deficient most of
the time, with repeated singular values, and general SVDs have returned factors
that do not give back the matrix on them, without reporting a failure
([`svd.rs`](../src/dense_linalg/svd.rs)'s header gives the cases). A simulator
that acts on such values truncates the wrong state, silently.

**The approach.** The SVD's rules are the module's own, written once: a trim
that reduces the matrix to full rank, leaving out only rounding; one-sided
Jacobi rotations, which converge on a matrix of full rank within a cap of
sweeps, or report the failure; and checks, to the bounds of LAPACK's SVD tests,
on every answer. A linear algebra library brings the steps that set the module's
speed (the products and the QR decompositions, and a fast route for large
full-rank matrices), behind one module-private trait, so that replacing the
library changes that trait's implementation and none of the SVD's rules. The SVD
checks every answer it gives, the fast route's included; the product and the
thin QR (scaled as the SVD's input is) are the library's, to rounding, and not
checked.

## The checked SVD

[`Svd::of`](../src/dense_linalg/svd.rs) answers
$a = u \operatorname{diag}(s) v^\dagger$ with $r$ values,
$1 \le r \le \min(\text{rows}, \text{cols})$, nonincreasing. Its guarantees and
its steps name the parts of its algorithm with these terms:

- **The trim:** a column-pivoted QR $a P = Q R$, whose rows of $R$ of
  smallest norm are dropped while their joint norm stays within rounding.
- **$W$:** the adjoint of the kept rows of $R P^\mathsf{T}$, of full rank;
  the SVD of $a$ follows from $W$'s.
- **The rotations:** one-sided Jacobi rotations of $W$'s columns, LAPACK
  `zgesvj`'s rules.
- **The fast route:** the library's bidiagonalization and QR algorithm.

It guarantees:

- **Checked factors:** $u \operatorname{diag}(s) v^\dagger$ gives back $a$, and
  $u$ and $v$ are orthonormal, to the bounds of LAPACK's SVD tests
  ([`svd/checks.rs`](../src/dense_linalg/svd/checks.rs)), else
  [`SvdError::FailedChecks`](../src/dense_linalg/svd.rs); when the rotations
  still rotate after their last sweep,
  [`SvdError::DidNotConverge`](../src/dense_linalg/svd.rs). A matrix with an
  entry that is not finite fails the checks, before any work.
- **Only rounding left out:** the values left out
  ($r < \min(\text{rows}, \text{cols})$) are those of the trim's dropped rows,
  whose joint norm is within $\max(\text{rows}, \text{cols}) \cdot \varepsilon$
  times the largest row's, $\varepsilon$ the machine epsilon; the zero matrix
  gives one value, 0.
- **Any scale:** down to subnormal entries, short of matrices whose
  singular values are too small or too large to store, which the checks
  refuse.

It runs these steps:

| Step                   | How                                                                                     | Why                                                                                                                                                                          |
| ---------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Zero matrix         | One value, 0, and the first unit vectors.                                               | It has no rank to keep and no largest entry to scale by.                                                                                                                     |
| 2. Wide matrix         | Factored through its adjoint.                                                           | The rotated columns are then the shorter side.                                                                                                                               |
| 3. Scaling             | By a power of two when the largest entry is far from 1.                                 | The squared norms neither overflow nor underflow; it rounds only entries it takes below the normal range.                                                                    |
| 4. Trim                | Drops the rows of $R$ of smallest norm, chosen by norm, not by pivot.                   | The rest sees full rank; the pivoting does not keep $\lvert R_{ii} \rvert$ nonincreasing.                                                                                    |
| 5. Route               | A large $W$ far from orthogonal: the fast route; any other: the rotations.              | The rotations are fast on nearly orthogonal $W$; the fast route on the rest. A reported failure or factors that are not finite fall back; finite wrong ones fail the checks. |
| 6. Left vectors        | The kept columns of $Q$, applied by its reflectors, times $W$'s right singular vectors. | Cheaper than forming $Q$: the reflectors act on the answer's $r$ columns alone, and those after the last kept row are skipped.                                               |
| Checks, on each answer | Reconstruction and orthonormality, to LAPACK's bounds.                                  | No route is trusted: a library's SVD has answered wrong factors without a failure.                                                                                           |

[`svd.rs`](../src/dense_linalg/svd.rs)'s header gives the details;
[`svd/checks.rs`](../src/dense_linalg/svd/checks.rs) and
[`svd/jacobi.rs`](../src/dense_linalg/svd/jacobi.rs) the checks and the
rotations.

## Matrices, product and QR

- [`Matrix`](../src/dense_linalg/matrix.rs) owns a dense complex matrix,
  column-major; [`MatrixRef`](../src/dense_linalg/matrix.rs) borrows one.
  `Matrix::product(a, b)` is $a b$, and `MatrixRef::adjoint` the conjugate
  transpose.
- [`Qr::of`](../src/dense_linalg/qr.rs) is the thin QR decomposition $a = q r$
  of a finite matrix, by Householder reflections, at any scale (scaled by a
  power of two as in the SVD's step 3). It cannot fail, and checks nothing: a
  matrix with an entry that is not finite gets unspecified factors.

## The linear algebra library

The module computes its products, QR decompositions and fast route with
t4a-faer, a fork of faer, through the module-private trait
[`Kernels`](../src/dense_linalg/kernels.rs);
[`kernels/t4a_faer.rs`](../src/dense_linalg/kernels/t4a_faer.rs) is its
implementation; its header says why the module uses t4a-faer, with how
much faster than nalgebra it is. t4a-faer is pinned to one version
([`Cargo.toml`](../Cargo.toml) says why), built without threads, and runs
its QR algorithm at every size, never its divide and conquer. Each
computation runs on the calling thread, in pure Rust, so the module builds
for WebAssembly too.

## Tests

The tests are in [`src/tests/dense_linalg/`](../src/tests/dense_linalg),
one file per module, through the module's public items where they can be:
random, rank-deficient, degenerate and graded matrices, every scale, each
route and its fallback, and the errors.
