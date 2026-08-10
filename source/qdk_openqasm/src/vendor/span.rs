// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::SourceSpan;
use std::{
    fmt::{self, Display, Formatter},
    ops::{Add, Index, Sub},
};

/// A region between two offsets in an array. Spans are the half-open interval `[lo, hi)`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Span {
    /// The smallest offset contained in the span.
    pub lo: u32,
    /// The next offset after the largest offset contained in the span.
    pub hi: u32,
}

/// A span qualified by the package that owns it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageSpan<P> {
    /// The package that owns the span.
    pub package: P,
    /// The span within the package.
    pub span: Span,
}

impl<P> PackageSpan<P> {
    /// Creates a package-qualified span.
    #[must_use]
    pub const fn new(package: P, span: Span) -> Self {
        Self { package, span }
    }
}

impl Span {
    /// Returns true if the position is within the span. Meaning it is in the
    /// right open interval `[self.lo, self.hi)`.
    #[must_use]
    pub fn contains(&self, offset: u32) -> bool {
        (self.lo..self.hi).contains(&offset)
    }

    /// Returns true if the position is in the closed interval `[self.lo, self.hi]`.
    #[must_use]
    pub fn touches(&self, offset: u32) -> bool {
        (self.lo..=self.hi).contains(&offset)
    }

    /// Intersect `other` with `self` and returns a new `Span` or `None`
    /// if the spans have no overlap.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let lo = self.lo.max(other.lo);
        let hi = self.hi.min(other.hi);

        if lo <= hi {
            Some(Self { lo, hi })
        } else {
            None
        }
    }
}

impl Add<u32> for Span {
    type Output = Self;

    fn add(self, rhs: u32) -> Self::Output {
        Self {
            lo: self.lo + rhs,
            hi: self.hi + rhs,
        }
    }
}

impl Sub<u32> for Span {
    type Output = Self;

    fn sub(self, rhs: u32) -> Self::Output {
        Self {
            lo: self.lo - rhs,
            hi: self.hi - rhs,
        }
    }
}

impl Display for Span {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "[{}-{}]", self.lo, self.hi)?;
        Ok(())
    }
}

impl Index<Span> for str {
    type Output = str;

    fn index(&self, index: Span) -> &Self::Output {
        &self[(index.lo as usize)..(index.hi as usize)]
    }
}

impl Index<&Span> for str {
    type Output = str;

    fn index(&self, index: &Span) -> &Self::Output {
        &self[(index.lo as usize)..(index.hi as usize)]
    }
}

impl Index<Span> for String {
    type Output = str;

    fn index(&self, index: Span) -> &Self::Output {
        &self[(index.lo as usize)..(index.hi as usize)]
    }
}

impl From<Span> for SourceSpan {
    fn from(value: Span) -> Self {
        Self::from((value.lo as usize)..(value.hi as usize))
    }
}

impl<P> From<(P, Span)> for PackageSpan<P> {
    fn from((package, span): (P, Span)) -> Self {
        Self::new(package, span)
    }
}

impl<P> From<PackageSpan<P>> for SourceSpan {
    fn from(value: PackageSpan<P>) -> Self {
        value.span.into()
    }
}

pub trait WithSpan {
    #[must_use]
    fn with_span(self, span: Span) -> Self;
}

impl<T> WithSpan for Box<T>
where
    T: WithSpan,
{
    fn with_span(mut self, span: Span) -> Self {
        *self = (*self).with_span(span);
        self
    }
}
