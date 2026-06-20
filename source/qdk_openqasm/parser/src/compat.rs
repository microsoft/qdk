// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compatibility re-exports for the shared data-structure types used by the
//! parser. When the `internal` feature is enabled, these resolve to the
//! in-repo `qsc_data_structures` types so that the parser shares types with the
//! rest of the compiler. Otherwise, they resolve to the vendored copies so that
//! the crate can build standalone (e.g. for the Python distribution).

#[cfg(feature = "internal")]
#[allow(unused_imports)]
pub use qsc_data_structures::{
    error::WithSource,
    index_map::IndexMap,
    source::{Source, SourceMap},
    span::{Span, WithSpan},
};

#[cfg(not(feature = "internal"))]
#[allow(unused_imports)]
pub use crate::vendor::{
    error::WithSource,
    index_map::IndexMap,
    source::{Source, SourceMap},
    span::{Span, WithSpan},
};
