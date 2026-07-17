// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Source maps and strict source-local coordinate conversion.

#[path = "source/line_column.rs"]
mod line_column;

pub use crate::vendor::source::{
    Source, SourceContents, SourceMap, SourceName, longest_common_prefix,
};
pub use line_column::{
    Position, PositionEncoding, PositionError, Range, byte_offset, position_at, range_from_span,
    span_from_range,
};
