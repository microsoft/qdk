// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running code in a catch, and raising its panic again unless another
//! unwinds: keeping a panic of code the crate runs from meeting another,
//! which would abort the process, and from going on where it must not
//! (where each is absorbed or raised: [`teardown`](super::teardown)).
//!
//! Each value is dropped in a catch of its own, since a collection's drop
//! goes on dropping the others during the first one's unwind, where a
//! second panic aborts the process.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::thread;

/// A caught panic's payload.
pub(crate) type Panic = Box<dyn Any + Send>;

/// Runs `code`, absorbing its panic; returns whether `code` returned.
pub(crate) fn absorb(code: impl FnOnce()) -> bool {
    // Known issue, open (it needs two bugs at once; blocks neither this
    // commit nor a later one): a payload whose own drop panics
    // (`panic_any`) is dropped here, outside the catch, and its panic
    // escapes. So it is wherever this module drops a payload.
    panic::catch_unwind(AssertUnwindSafe(code)).is_ok()
}

/// Runs `code`, catching its panic, for the caller to raise once it did
/// what must follow ([`raise`]).
pub(crate) fn caught(code: impl FnOnce()) -> Option<Panic> {
    panic::catch_unwind(AssertUnwindSafe(code)).err()
}

/// Raises `panic` again, if any, unless the thread unwinds another panic,
/// which goes on: raising a second one would abort the process.
pub(crate) fn raise(panic: Option<Panic>) {
    if let Some(panic) = panic
        && !thread::panicking()
    {
        panic::resume_unwind(panic);
    }
}

/// Runs `end` on each of `values`, each in a catch of its own, and returns
/// the first panic; a later one's payload is dropped, as in [`absorb`].
pub(crate) fn end_each<T>(
    values: impl IntoIterator<Item = T>,
    mut end: impl FnMut(T),
) -> Option<Panic> {
    let mut first = None;
    for value in values {
        if let Err(panic) = panic::catch_unwind(AssertUnwindSafe(|| end(value))) {
            first.get_or_insert(panic);
        }
    }
    first
}

/// Runs `end` on each of `values`, absorbing each one's panic in a catch
/// of its own.
pub(crate) fn absorb_each<T>(values: impl IntoIterator<Item = T>, end: impl FnMut(T)) {
    drop(end_each(values, end));
}

/// Runs `end` on each of `values`, each in a catch of its own, then raises
/// again the first panic, once every value ended: `earlier`, when given (a
/// panic caught before the values were freed), else the first `end`'s; the
/// others are left to the panic hook, which reported them. While the
/// thread unwinds another panic, it raises none, which would abort the
/// process: that panic goes on.
pub(crate) fn end_each_raising_first<T>(
    earlier: Option<Panic>,
    values: impl IntoIterator<Item = T>,
    end: impl FnMut(T),
) {
    let later = end_each(values, end);
    raise(earlier.or(later));
}
