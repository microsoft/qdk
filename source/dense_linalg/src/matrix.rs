// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Matrix`] and [`MatrixRef`]: a dense complex matrix, owned or
//! borrowed, in column-major order, with its product, its adjoint, and its
//! columns or rows multiplied by values.

use crate::kernels::{DefaultKernels, Kernels};
use num_complex::Complex64;

/// A dense complex matrix owned in column-major order. Element `(i, j)` is
/// at `i + rows·j`.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    elements: Vec<Complex64>,
}

impl Matrix {
    /// `elements` as a `rows × cols` matrix.
    ///
    /// # Panics
    ///
    /// When `rows·cols` overflows `usize`, or `elements` does not hold
    /// `rows·cols` elements.
    #[must_use]
    pub fn new(rows: usize, cols: usize, elements: Vec<Complex64>) -> Self {
        assert_eq!(
            elements.len(),
            element_count(rows, cols),
            "a {rows} × {cols} matrix"
        );
        Self {
            rows,
            cols,
            elements,
        }
    }

    /// The `rows × cols` matrix whose element `(i, j)` is `element(i, j)`,
    /// called column by column.
    ///
    /// # Panics
    ///
    /// When `rows·cols` overflows `usize`, before any call to `element`.
    #[must_use]
    pub fn from_fn(
        rows: usize,
        cols: usize,
        mut element: impl FnMut(usize, usize) -> Complex64,
    ) -> Self {
        let mut elements = Vec::with_capacity(element_count(rows, cols));
        // Without rows, the columns, up to `usize::MAX` of them, hold
        // nothing to visit.
        if rows > 0 {
            for j in 0..cols {
                for i in 0..rows {
                    elements.push(element(i, j));
                }
            }
        }
        Self::new(rows, cols, elements)
    }

    /// The product `a·b`, to rounding.
    ///
    /// # Panics
    ///
    /// When `a.cols()` differs from `b.rows()`. Also when
    /// `a.rows()·b.cols()` overflows `usize`, as [`Matrix::new`] would for
    /// the product's shape. Empty operands can have such dimensions:
    /// `2⁶³ × 0` times `0 × 2`.
    #[must_use]
    pub fn product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Self {
        assert_eq!(
            a.cols(),
            b.rows(),
            "the product of a {} × {} and a {} × {} matrix",
            a.rows(),
            a.cols(),
            b.rows(),
            b.cols()
        );
        // Before the kernels count the product's elements, so that the
        // panic is the crate's, naming the shape.
        element_count(a.rows(), b.cols());
        DefaultKernels::product(a, b)
    }

    /// The number of rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The number of columns.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// The matrix, borrowed.
    #[must_use]
    pub fn view(&self) -> MatrixRef<'_> {
        MatrixRef::new(self.rows, self.cols, &self.elements)
    }

    /// The elements, column-major.
    #[must_use]
    pub fn elements(&self) -> &[Complex64] {
        &self.elements
    }

    /// The elements, column-major, moved out of the matrix.
    #[must_use]
    pub fn into_elements(self) -> Vec<Complex64> {
        self.elements
    }

    /// Multiplies column `j` by `values[j]`, in place: a matrix `m`
    /// becomes `m·diag(values)`. For instance, `u·diag(s)` of an
    /// [`Svd`](crate::Svd).
    ///
    /// # Panics
    ///
    /// When `values` does not hold one value per column.
    pub fn multiply_columns_by(&mut self, values: &[f64]) {
        assert_eq!(
            values.len(),
            self.cols,
            "multiplying the columns of a {} × {} matrix by {} values",
            self.rows,
            self.cols,
            values.len()
        );
        if self.elements.is_empty() {
            return;
        }
        for (column, &value) in self.elements.chunks_exact_mut(self.rows).zip(values) {
            for element in column {
                *element *= value;
            }
        }
    }

    /// Multiplies row `i` by `values[i]`, in place: a matrix `m` becomes
    /// `diag(values)·m`. For instance, `diag(s)·v†` of an
    /// [`Svd`](crate::Svd).
    ///
    /// # Panics
    ///
    /// When `values` does not hold one value per row.
    pub fn multiply_rows_by(&mut self, values: &[f64]) {
        assert_eq!(
            values.len(),
            self.rows,
            "multiplying the rows of a {} × {} matrix by {} values",
            self.rows,
            self.cols,
            values.len()
        );
        if self.elements.is_empty() {
            return;
        }
        for column in self.elements.chunks_exact_mut(self.rows) {
            for (element, &value) in column.iter_mut().zip(values) {
                *element *= value;
            }
        }
    }
}

/// A dense complex matrix borrowed in column-major order. Element `(i, j)`
/// is at `i + rows·j`.
#[derive(Clone, Copy, Debug)]
pub struct MatrixRef<'a> {
    rows: usize,
    cols: usize,
    elements: &'a [Complex64],
}

impl<'a> MatrixRef<'a> {
    /// `elements` as a `rows × cols` matrix.
    ///
    /// # Panics
    ///
    /// When `rows·cols` overflows `usize`, or `elements` does not hold
    /// `rows·cols` elements.
    #[must_use]
    pub fn new(rows: usize, cols: usize, elements: &'a [Complex64]) -> Self {
        assert_eq!(
            elements.len(),
            element_count(rows, cols),
            "a {rows} × {cols} matrix"
        );
        Self {
            rows,
            cols,
            elements,
        }
    }

    /// The number of rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The number of columns.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// The elements, column-major.
    #[must_use]
    pub fn elements(&self) -> &'a [Complex64] {
        self.elements
    }

    /// The adjoint (conjugate transpose), `cols × rows`.
    #[must_use]
    pub fn adjoint(&self) -> Matrix {
        let (rows, elements) = (self.rows, self.elements);
        Matrix::from_fn(self.cols, rows, |i, j| elements[j + rows * i].conj())
    }
}

/// `rows·cols`, the number of elements of a `rows × cols` matrix.
///
/// # Panics
///
/// When it overflows `usize`. Unchecked, in a build without overflow
/// checks, it would wrap to a count that a shorter `elements` could match.
fn element_count(rows: usize, cols: usize) -> usize {
    let Some(count) = rows.checked_mul(cols) else {
        panic!("a {rows} × {cols} matrix has more elements than `usize` counts");
    };
    count
}

#[cfg(test)]
mod tests;
