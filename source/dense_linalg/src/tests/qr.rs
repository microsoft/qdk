// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the thin QR (`qr`).

use super::{SHAPES, adjoint, distance, identity, matrix, naive_product};
use crate::Qr;

/// A thin QR gives back its matrix, with orthonormal columns in `Q` and
/// `R` upper trapezoidal, on tall, wide, square and single-row shapes.
#[test]
fn decomposes_by_qr() {
    for (seed, &(rows, cols)) in (1..).zip(&SHAPES) {
        let a = matrix(rows, cols, seed);
        let Qr { q, r } = Qr::of(a.view());
        let k = rows.min(cols);
        assert_eq!((q.rows(), q.cols(), r.rows(), r.cols()), (rows, k, k, cols));
        assert!(distance(&naive_product(q.view(), r.view()), &a) < 1e-13);
        let gram = naive_product(adjoint(&q).view(), q.view());
        assert!(distance(&gram, &identity(k)) < 1e-13);
        for j in 0..cols {
            for i in j + 1..k {
                let zero = num_complex::Complex64::new(0.0, 0.0);
                assert_eq!(r.elements()[i + k * j], zero, "R[{i}, {j}]");
            }
        }
    }
}
