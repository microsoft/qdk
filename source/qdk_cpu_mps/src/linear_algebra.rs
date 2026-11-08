// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`LinearAlgebra`]: the dense complex linear algebra a [`CpuMpsEngine`]
//! needs, and the column-major matrices it is stated on.
//!
//! The engine spends its time on small dense complex matrices: a site tensor
//! read as a matrix ([`chain`](crate::chain)), or two sites contracted into one
//! block. It needs three operations on them: a product, to apply gates and
//! contract sites; a thin QR decomposition, `A = Q·R` with `Q`'s columns
//! orthonormal, to move the orthogonality centre (`engine::canonical_form`);
//! and a thin singular value decomposition, to split a block and truncate it
//! (`engine::split`). Thin: the factors' inner size is at most the matrix's
//! smaller side.
//!
//! The trait states what the engine relies on from each, so that everything
//! else, such as reshaping site tensors, applying a gate's matrix to the
//! qubits' values and choosing how many singular values to keep, is the
//! engine's alone.
//!
//! The trait exists for the crate's tests: the engine is written once over it
//! ([`Engine`]), and the public [`CpuMpsEngine`] is that engine on
//! [`CpuLinearAlgebra`], the one implementation. The tests run the same engine
//! on algebras that count its QRs and products, to pin where it moves the
//! centre, and on one whose SVD fails, to pin how a failed split fails the
//! call, since no real matrix is known to make the SVD fail there. The module
//! is crate-private; only [`FactorizationError`] is public, re-exported at the
//! crate's root, as the payload of [`CpuMpsError::Factorization`].
//!
//! [`CpuMpsEngine`]: crate::CpuMpsEngine
//! [`Engine`]: crate::engine::Engine
//! [`CpuMpsError::Factorization`]: crate::CpuMpsError::Factorization

pub(crate) mod cpu;

pub(crate) use self::cpu::CpuLinearAlgebra;

use num_complex::Complex64;

/// The linear algebra of a [`CpuMpsEngine`](crate::CpuMpsEngine): products
/// and thin factorizations of dense complex matrices, column-major.
///
/// An implementation computes on the calling thread: the engine runs one
/// state per core, and the caller decides how many cores it uses.
pub(crate) trait LinearAlgebra {
    /// The product `a·b`. `a.cols()` equals `b.rows()`.
    fn multiply(&self, a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix;

    /// The thin QR decomposition `a = q·r`: with `k = min(rows, cols)`, `q`
    /// is `rows × k` with orthonormal columns and `r` is `k × cols`.
    fn qr(&self, a: MatrixRef<'_>) -> Result<Qr, FactorizationError>;

    /// The thin singular value decomposition `a = u·diag(s)·v†`, to
    /// rounding: `s` holds `r` values in nonincreasing order, with
    /// `1 ≤ r ≤ min(rows, cols)`, `u` is `rows × r` with orthonormal
    /// columns, and `v†` is `r × cols` with orthonormal rows. Values left
    /// out (`r < min(rows, cols)`) are at most rounding's: the engine keeps
    /// no zero value, so it needs no more than the nonzero ones, and at
    /// least one, a zero for the zero matrix.
    ///
    /// It must be accurate on rank-deficient matrices, with repeated and
    /// zero singular values: needed because a split of a state whose
    /// Schmidt rank is below the matrix's size meets them all the time, and
    /// truncation trusts the singular values.
    fn svd(&self, a: MatrixRef<'_>) -> Result<Svd, FactorizationError>;
}

/// A dense complex matrix borrowed in column-major order: element `(i, j)` at
/// `i + rows·j`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MatrixRef<'a> {
    rows: usize,
    cols: usize,
    elements: &'a [Complex64],
}

impl<'a> MatrixRef<'a> {
    /// `elements` as a `rows × cols` matrix.
    ///
    /// # Panics
    ///
    /// When `elements` does not hold `rows·cols` elements.
    #[must_use]
    pub(crate) fn new(rows: usize, cols: usize, elements: &'a [Complex64]) -> Self {
        assert_eq!(elements.len(), rows * cols, "a {rows} × {cols} matrix");
        Self {
            rows,
            cols,
            elements,
        }
    }

    /// The number of rows.
    #[must_use]
    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    /// The number of columns.
    #[must_use]
    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    /// The elements, column-major.
    #[must_use]
    pub(crate) fn elements(&self) -> &'a [Complex64] {
        self.elements
    }
}

/// A dense complex matrix owned in column-major order: element `(i, j)` at
/// `i + rows·j`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Matrix {
    rows: usize,
    cols: usize,
    elements: Vec<Complex64>,
}

impl Matrix {
    /// `elements` as a `rows × cols` matrix.
    ///
    /// # Panics
    ///
    /// When `elements` does not hold `rows·cols` elements.
    #[must_use]
    pub(crate) fn new(rows: usize, cols: usize, elements: Vec<Complex64>) -> Self {
        assert_eq!(elements.len(), rows * cols, "a {rows} × {cols} matrix");
        Self {
            rows,
            cols,
            elements,
        }
    }

    /// The `rows × cols` matrix whose element `(i, j)` is `element(i, j)`.
    #[must_use]
    pub(crate) fn from_fn(
        rows: usize,
        cols: usize,
        mut element: impl FnMut(usize, usize) -> Complex64,
    ) -> Self {
        let mut elements = Vec::with_capacity(rows * cols);
        for j in 0..cols {
            for i in 0..rows {
                elements.push(element(i, j));
            }
        }
        Self::new(rows, cols, elements)
    }

    /// The number of rows.
    #[must_use]
    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    /// The number of columns.
    #[must_use]
    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    /// The matrix, borrowed.
    #[must_use]
    pub(crate) fn view(&self) -> MatrixRef<'_> {
        MatrixRef::new(self.rows, self.cols, &self.elements)
    }

    /// The elements, column-major.
    #[must_use]
    pub(crate) fn into_elements(self) -> Vec<Complex64> {
        self.elements
    }
}

/// The conjugate transpose of `m`, for the engine and the SVD alike.
pub(crate) fn adjoint(m: MatrixRef<'_>) -> Matrix {
    let (rows, elements) = (m.rows(), m.elements());
    Matrix::from_fn(m.cols(), rows, |i, j| elements[j + rows * i].conj())
}

/// A thin QR decomposition, from [`LinearAlgebra::qr`].
#[derive(Clone, Debug)]
pub(crate) struct Qr {
    /// The factor with orthonormal columns.
    pub(crate) q: Matrix,
    /// The upper-triangular factor.
    pub(crate) r: Matrix,
}

/// A thin singular value decomposition, from [`LinearAlgebra::svd`].
#[derive(Clone, Debug)]
pub(crate) struct Svd {
    /// The left singular vectors, as columns.
    pub(crate) u: Matrix,
    /// The singular values, nonincreasing.
    pub(crate) singular_values: Vec<f64>,
    /// The adjoint of the right singular vectors: the vectors as rows.
    pub(crate) v_adjoint: Matrix,
}

/// An SVD the engine could not compute, which fails the call that needed
/// it ([`CpuMpsError::Factorization`]): one whose rotations did not
/// converge, or whose factors failed its accuracy checks. The SVD is the
/// engine's only factorization that can fail.
///
/// [`CpuMpsError::Factorization`]: crate::CpuMpsError::Factorization
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the SVD of a {rows} × {cols} matrix {reason}")]
pub struct FactorizationError {
    /// What went wrong: `"did not converge"` when the rotations still
    /// rotated after their last sweep, or `"failed its accuracy checks"`
    /// when the factors were not finite, did not give back the matrix, or
    /// had singular vectors that are not orthonormal.
    pub reason: &'static str,
    /// The matrix's rows.
    pub rows: usize,
    /// The matrix's columns.
    pub cols: usize,
}
