# The dense linear algebra: complex matrices with a checked SVD

The CPU algebra ([`CpuAlgebra`](../src/site_algebra/cpu_algebra.rs))
truncates a state by the singular values of small dense complex matrices,
many thousands of times per run, and keeps the singular vectors as an
orthonormal basis. The crate's module of dense complex matrices,
[`dense_linalg`](../src/dense_linalg.rs), gives it a singular value
decomposition (SVD) it can trust, with the product, thin QR decomposition
and adjoint that it and the engine's host computations also need, in three
parts:

- **[The checked SVD](#the-checked-svd):** `Svd::of` either answers
  factors checked against the matrix, or reports a failure (`SvdError`,
  public at the crate's root, as `CpuAlgebra`'s error).
- **[Matrices, product and QR](#matrices-product-and-qr):** `Matrix`,
  `MatrixRef` and `Qr`.
- **[The libraries and the pivoted QR](#the-libraries-and-the-pivoted-qr):**
  nalgebra, matrixmultiply and the module's own column-pivoted QR, behind
  the module-private trait `Kernels`.

The `conformance` feature makes the module public, so that another crate's
test fakes can compute as a backend library would.

**The problem.** General SVDs have returned factors that do not give back
the matrix, without reporting a failure, on the matrices MPS simulations
meet ([`svd.rs`](../src/dense_linalg/svd.rs)'s header gives why that
matters, and the cases).

**The approach.** The SVD's rules are the module's own, written once: a trim
that reduces the matrix to full rank, leaving out only rounding; one-sided
Jacobi rotations, which converge on a matrix of full rank within a cap of
sweeps, or report the failure; and checks, to the bounds of LAPACK's SVD tests,
on every answer. The steps that set the module's speed (the products and the QR
decompositions) sit behind one module-private trait, so that replacing their
implementation changes none of the SVD's rules. The SVD checks every answer it
gives; the product and the thin QR (scaled as the SVD's input is) are trusted
to rounding, not checked.

## The checked SVD

[`Svd::of`](../src/dense_linalg/svd.rs) answers
$a = u \operatorname{diag}(s) v^\dagger$ with $r$ values,
$1 \le r \le \min(\text{rows}, \text{cols})$, nonincreasing. Its
guarantees (checked factors, only rounding's singular values left out, any
scale) are stated in [`svd.rs`](../src/dense_linalg/svd.rs)'s header; its
steps name the parts of its algorithm with these terms:

- **The trim:** a column-pivoted QR $a P = Q R$, whose rows of $R$ of
  smallest norm are dropped while their joint norm stays within rounding.
- **$W$:** the adjoint of the kept rows of $R P^\mathsf{T}$, of full rank;
  the SVD of $a$ follows from $W$'s.
- **The rotations:** one-sided Jacobi rotations of $W$'s columns, LAPACK
  `zgesvj`'s rules.

It runs these steps:

| Step                   | How                                                                                     | Why                                                                                                                                                              |
| ---------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Zero matrix         | One value, 0, and the first unit vectors.                                               | It has no rank to keep and no largest entry to scale by.                                                                                                         |
| 2. Wide matrix         | Factored through its adjoint.                                                           | The rotated columns are then the shorter side.                                                                                                                   |
| 3. Scaling             | By a power of two when the largest entry is far from 1.                                 | The squared norms neither overflow nor underflow; it rounds only entries it takes below the normal range.                                                        |
| 4. Trim                | Drops the rows of $R$ of smallest norm, chosen by norm, not by pivot.                   | The rest sees full rank; the pivoting keeps $\lvert R_{ii} \rvert$ nonincreasing only to rounding.                                                               |
| 5. Rotations           | $W$'s columns rotated until orthogonal.                                                 | They converge on $W$ of full rank within a cap of sweeps; a faster SVD for $W$ far from orthogonal is planned ([`svd.rs`](../src/dense_linalg/svd.rs)'s header). |
| 6. Left vectors        | The kept columns of $Q$, applied by its reflectors, times $W$'s right singular vectors. | Cheaper than forming $Q$: the reflectors act on the answer's $r$ columns alone, and those after the last kept row are skipped.                                   |
| Checks, on each answer | Reconstruction and orthonormality, to LAPACK's bounds.                                  | No SVD is trusted: a library's has answered wrong factors without a failure, and the rotations' answer passes through the pivoted QR and the products.           |

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

## The libraries and the pivoted QR

The module computes its products and QR decompositions through the
module-private trait [`Kernels`](../src/dense_linalg/kernels.rs), whose
implementation,
[`kernels/nalgebra_matrixmultiply.rs`](../src/dense_linalg/kernels/nalgebra_matrixmultiply.rs),
uses libraries the QDK already depends on, and a QR of its own:

- the product: matrixmultiply's complex matrix product;
- the thin QR: nalgebra's;
- the column-pivoted QR and the application of its $Q$: the module's own
  Householder QR,
  [`householder_pivoted_qr.rs`](../src/dense_linalg/kernels/nalgebra_matrixmultiply/householder_pivoted_qr.rs),
  which keeps each pivot's $\lvert R_{kk} \rvert$ at least the norm of every
  later column's remaining rows, to within about $\sqrt{\varepsilon}/2$ of
  the pivot (its norms' downdate's allowance, explained in the file's
  header), as the trim needs.

Each computation runs on the calling thread, in pure Rust, so the module
builds for WebAssembly too.

## Tests

The tests are in [`src/tests/dense_linalg/`](../src/tests/dense_linalg),
one file per module, through the module's public items where they can be:
random, rank-deficient, degenerate and graded matrices, every scale, the
pivoted QR's choice of pivots, and the errors.
