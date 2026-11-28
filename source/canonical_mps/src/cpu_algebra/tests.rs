// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the CPU algebra promises beyond `SiteAlgebra`'s contract,
//! which the conformance kit checks it against
//! (`tests/site_algebra_conformance.rs`): a read borrows a site's elements
//! rather than copying them, and a read past a tensor's end says what the
//! caller got wrong. Its gate split's tests are beside it, in
//! `gate_split/tests.rs`, and the split's truncation rule, called
//! directly, in `gate_split/truncation/tests.rs`.

use crate::CpuAlgebra;
use crate::site_algebra::SiteAlgebra;
use num_complex::Complex64;
use std::borrow::Cow;

/// A tensor longer than the site it holds, as a backend that reuses
/// buffers may keep: a read returns the site's elements alone, borrowed
/// from the vector rather than copied.
#[test]
fn a_read_borrows_the_first_len_elements() {
    let tensor: Vec<Complex64> = (0..6).map(|k| c(f64::from(k), -1.0)).collect();
    let read = CpuAlgebra.read(&tensor, 4).expect("reads never fail");
    assert_eq!(*read, tensor[..4]);
    assert!(
        matches!(read, Cow::Borrowed(_)),
        "a host read copies nothing"
    );
}

/// A read of more elements than a site tensor holds is the caller's bug (a
/// bond misread). Reading or duplicating 12 elements of a tensor of 8
/// panics. The message names the site tensor and both lengths, so that the
/// caller sees what it got wrong, rather than Rust's text of a slice.
#[test]
fn a_read_past_the_end_names_the_tensor_and_both_lengths() {
    let rows: [(&str, fn()); 2] = [
        ("read", || {
            let tensor = vec![c(1.0, 0.0); 8];
            let _ = CpuAlgebra.read(&tensor, 12);
        }),
        ("duplicate", || {
            let tensor = vec![c(1.0, 0.0); 8];
            let _ = CpuAlgebra.duplicate(&tensor, 12);
        }),
    ];
    for (name, call) in rows {
        let message = panic_message(call);
        assert!(
            message.contains("site tensor"),
            "{name}: the tensor is named: {message}"
        );
        assert!(
            message.contains("12") && message.contains('8'),
            "{name}: both lengths are named: {message}"
        );
    }
}

/// The complex number `re + i·im`.
pub(super) fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// The message of the panic `f` makes; it fails when `f` returns.
pub(super) fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let payload = std::panic::catch_unwind(f).expect_err("a panic");
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => (*payload
            .downcast::<&str>()
            .expect("a panic message is a string"))
        .to_owned(),
    }
}
