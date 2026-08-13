// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Source spans.
//!
//! A [`Span`] is a half-open byte range within a single source. It carries no
//! reference to the source it indexes, so pairing it with the right [`Source`]
//! is the caller's responsibility. [`crate::source::SourceMap`] does that
//! pairing for spans that come out of the parser.
//!
//! [`Source`]: crate::source::Source

pub use crate::vendor::span::{Span, WithSpan};
