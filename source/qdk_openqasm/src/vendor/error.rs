// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Vendored from `qsc_data_structures::error`.

#[cfg(test)]
mod tests;

use crate::parser::{SourceFileSnapshot, SourceSnapshot};
use crate::vendor::source::{Source, SourceMap};
use miette::{Diagnostic, MietteError, MietteSpanContents, SourceCode, SourceSpan, SpanContents};
use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
};

#[derive(Clone, Debug)]
pub struct WithSource<E> {
    sources: Vec<Source>,
    error: E,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSnapshotSourceCode {
    snapshot: SourceSnapshot,
}

impl SourceSnapshotSourceCode {
    #[must_use]
    pub fn new(snapshot: &SourceSnapshot) -> Self {
        Self {
            snapshot: snapshot.clone(),
        }
    }

    pub fn resolve_span(
        &self,
        span: &SourceSpan,
    ) -> Result<(&SourceFileSnapshot, SourceSpan), MietteError> {
        let offset = u32::try_from(span.offset()).map_err(|_| MietteError::OutOfBounds)?;
        let len = u32::try_from(span.len()).map_err(|_| MietteError::OutOfBounds)?;
        let end = offset.checked_add(len).ok_or(MietteError::OutOfBounds)?;
        let source = self
            .snapshot
            .files()
            .iter()
            .find(|source| {
                u32::try_from(source.text.len())
                    .ok()
                    .and_then(|text_len| source.offset.checked_add(text_len))
                    .is_some_and(|source_end| source.offset <= offset && end <= source_end)
            })
            .ok_or(MietteError::OutOfBounds)?;
        Ok((
            source,
            with_offset(span, |span_offset| {
                span_offset
                    - usize::try_from(source.offset)
                        .expect("u32 source offset should fit into usize")
            }),
        ))
    }
}

impl<E: Diagnostic + Send + Sync> WithSource<E> {
    pub fn error(&self) -> &E {
        &self.error
    }

    pub fn into_error(self) -> E {
        self.error
    }

    /// Construct a diagnostic with source information from a source map.
    /// Since errors may contain labeled spans from any source file in the
    /// compilation, the entire source map is needed to resolve offsets.
    pub fn from_map(sources: &SourceMap, error: E) -> Self {
        // Filter the source map to the relevant sources
        // to avoid cloning all of them.
        let mut filtered = Vec::<Source>::new();

        for offset in error
            .labels()
            .into_iter()
            .flatten()
            .filter_map(|label| u32::try_from(label.offset()).ok())
        {
            let Some(source) = sources.find_by_offset(offset) else {
                continue;
            };

            // Keep the vector sorted by source offsets
            match filtered.binary_search_by_key(&source.offset, |s| s.offset) {
                Ok(_) => {} // source already in vector
                Err(pos) => filtered.insert(pos, source.clone()),
            }
        }

        Self {
            sources: filtered,
            error,
        }
    }

    pub fn into_with_source<T>(self) -> WithSource<T>
    where
        T: From<E>,
    {
        WithSource {
            sources: self.sources,
            error: self.error.into(),
        }
    }

    /// Takes a span that uses `SourceMap` offsets, and returns
    /// a span that is relative to the `Source` that the span falls into,
    /// along with a reference to the `Source`.
    pub fn resolve_span(&self, span: &SourceSpan) -> Result<(&Source, SourceSpan), MietteError> {
        self.try_resolve_span(span).ok_or(MietteError::OutOfBounds)
    }

    /// Like [`resolve_span`](Self::resolve_span), but returns `None` when no
    /// source in the map contains the span's offset, instead of panicking.
    #[must_use]
    pub fn try_resolve_span(&self, span: &SourceSpan) -> Option<(&Source, SourceSpan)> {
        let offset = u32::try_from(span.offset()).ok()?;
        let source = self
            .sources
            .iter()
            .rev()
            .find(|source| offset >= source.offset)?;
        Some((
            source,
            with_offset(span, |offset| {
                offset
                    - usize::try_from(source.offset)
                        .expect("u32 source offset should fit into usize")
            }),
        ))
    }
}

impl<E: Diagnostic> Error for WithSource<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.error.source()
    }
}

impl<E: Diagnostic + Send + Sync> Diagnostic for WithSource<E> {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.error.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(self)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.error.labels()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        self.error.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.error.diagnostic_source()
    }
}

impl<E: Diagnostic + Display> Display for WithSource<E> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}

impl<E: Diagnostic + Sync + Send> SourceCode for WithSource<E> {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        let (source, source_relative_span) = self.resolve_span(span)?;

        let contents = source.contents.read_span(
            &source_relative_span,
            context_lines_before,
            context_lines_after,
        )?;

        Ok(Box::new(MietteSpanContents::new_named(
            source.name.to_string(),
            contents.data(),
            with_offset(contents.span(), |offset| {
                offset
                    + usize::try_from(source.offset)
                        .expect("u32 source offset should fit into usize")
            }),
            contents.line(),
            contents.column(),
            contents.line_count(),
        )))
    }
}

impl SourceCode for SourceSnapshotSourceCode {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        let (source, source_relative_span) = self.resolve_span(span)?;
        let contents = source.text.read_span(
            &source_relative_span,
            context_lines_before,
            context_lines_after,
        )?;

        Ok(Box::new(MietteSpanContents::new_named(
            source.path.to_string(),
            contents.data(),
            with_offset(contents.span(), |offset| {
                offset
                    + usize::try_from(source.offset)
                        .expect("u32 source offset should fit into usize")
            }),
            contents.line(),
            contents.column(),
            contents.line_count(),
        )))
    }
}

fn with_offset(span: &SourceSpan, f: impl FnOnce(usize) -> usize) -> SourceSpan {
    SourceSpan::new(f(span.offset()).into(), span.len())
}
