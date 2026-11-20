// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One readout's detail beside its bit: [`ReadoutDetail`], a confidence, a
//! probability or a level, of the kind its instruction declares.

use crate::ReadoutDetailKind;

/// One readout's detail, beside its hard
/// [`ReadoutValue`](crate::ReadoutValue) in the same slot: soft
/// information a part below gives and a part above reads, of the
/// [`ReadoutDetailKind`] its instruction declares.
///
/// Two numbers are equal when their bits are, so that readouts compare as
/// equal to themselves, a NaN included.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ReadoutDetail {
    /// A floating-point number, such as a confidence or a probability.
    Number(f64),
    /// An integer, such as a leakage level.
    Integer(i64),
}

impl ReadoutDetail {
    /// Its kind.
    #[must_use]
    pub const fn kind(&self) -> ReadoutDetailKind {
        match self {
            ReadoutDetail::Number(_) => ReadoutDetailKind::Number,
            ReadoutDetail::Integer(_) => ReadoutDetailKind::Integer,
        }
    }
}

impl PartialEq for ReadoutDetail {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ReadoutDetail::Number(a), ReadoutDetail::Number(b)) => a.to_bits() == b.to_bits(),
            (ReadoutDetail::Integer(a), ReadoutDetail::Integer(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ReadoutDetail {}
