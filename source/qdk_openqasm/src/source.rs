// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Source maps and strict source-local coordinate conversion.
//!
//! Conversion here is strict: a byte offset, position, or range that does not
//! name a real location in its source produces a [`PositionError`]. It is never
//! clamped. Clamping is the alternative convention, used by the compiler-side
//! coordinate types, where an out-of-range coordinate is silently moved to the
//! nearest valid one, so an offset past the end of a source becomes that
//! source's end rather than an error. Clamping keeps editor features from
//! failing on stale coordinates, but it also hides the mismatch, which is why
//! this crate reports the mismatch to its caller instead.

mod line_column;

pub use crate::vendor::source::{
    Source, SourceContents, SourceMap, SourceName, longest_common_prefix,
};
pub use line_column::{
    Position, PositionEncoding, PositionError, Range, byte_offset, position_at, range_from_span,
    span_from_range,
};
