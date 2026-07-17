// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Immutable source document projections for parsed OpenQASM syntax.

use crate::qasm_ast::span::Span;
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
use qdk_openqasm::parser::{SourceSnapshot, SourceStatus};
use qdk_openqasm::source::{
    Position as NativePosition, PositionEncoding as NativePositionEncoding, Range as NativeRange,
    byte_offset as native_byte_offset, position_at as native_position_at,
    range_from_span as native_range_from_span, span_from_range as native_span_from_range,
};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) type ResolvedSource = (Arc<str>, Arc<str>, Arc<[Arc<str>]>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RewriteErrorCode {
    UnknownSource,
    IncludeEdit,
    InvalidPosition,
    MixedEncoding,
    ReversedRange,
    Overlap,
    AmbiguousInsertion,
    DocumentTooLarge,
}

impl RewriteErrorCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::UnknownSource => "unknown-source",
            Self::IncludeEdit => "include-edit",
            Self::InvalidPosition => "invalid-position",
            Self::MixedEncoding => "mixed-encoding",
            Self::ReversedRange => "reversed-range",
            Self::Overlap => "overlap",
            Self::AmbiguousInsertion => "ambiguous-insertion",
            Self::DocumentTooLarge => "document-too-large",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RewriteError {
    pub(crate) code: RewriteErrorCode,
    pub(crate) edit_index: Option<usize>,
    pub(crate) range: Option<SourceRange>,
}

impl RewriteError {
    fn for_edit(code: RewriteErrorCode, edit_index: usize, range: SourceRange) -> Self {
        Self {
            code,
            edit_index: Some(edit_index),
            range: Some(range),
        }
    }

    fn document_too_large() -> Self {
        Self {
            code: RewriteErrorCode::DocumentTooLarge,
            edit_index: None,
            range: None,
        }
    }
}

#[derive(Debug)]
struct NormalizedEdit<'a> {
    index: usize,
    start: usize,
    end: usize,
    replacement: &'a str,
    range: SourceRange,
}

#[pyclass(module = "qdk._native", eq, eq_int, frozen, hash, from_py_object)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PositionEncoding {
    #[pyo3(name = "UTF8")]
    Utf8,
    #[pyo3(name = "CODE_POINT")]
    CodePoint,
    #[pyo3(name = "UTF16")]
    Utf16,
}

impl PositionEncoding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf8",
            Self::CodePoint => "code-point",
            Self::Utf16 => "utf16",
        }
    }
}

impl From<PositionEncoding> for NativePositionEncoding {
    fn from(encoding: PositionEncoding) -> Self {
        match encoding {
            PositionEncoding::Utf8 => Self::Utf8,
            PositionEncoding::CodePoint => Self::CodePoint,
            PositionEncoding::Utf16 => Self::Utf16,
        }
    }
}

impl From<NativePositionEncoding> for PositionEncoding {
    fn from(encoding: NativePositionEncoding) -> Self {
        match encoding {
            NativePositionEncoding::Utf8 => Self::Utf8,
            NativePositionEncoding::CodePoint => Self::CodePoint,
            NativePositionEncoding::Utf16 => Self::Utf16,
        }
    }
}

#[pymethods]
impl PositionEncoding {
    #[getter]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn value(&self) -> &'static str {
        (*self).as_str()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> &'static str {
        (*self).as_str()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!(
            "PositionEncoding.{}",
            match self {
                Self::Utf8 => "UTF8",
                Self::CodePoint => "CODE_POINT",
                Self::Utf16 => "UTF16",
            }
        )
    }
}

#[pyclass(module = "qdk._native", frozen, eq, hash, skip_from_py_object)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Position {
    #[pyo3(get)]
    line: u32,
    #[pyo3(get)]
    column: u32,
    #[pyo3(get)]
    encoding: PositionEncoding,
}

impl From<Position> for NativePosition {
    fn from(position: Position) -> Self {
        Self {
            line: position.line,
            column: position.column,
            encoding: position.encoding.into(),
        }
    }
}

impl From<NativePosition> for Position {
    fn from(position: NativePosition) -> Self {
        Self {
            line: position.line,
            column: position.column,
            encoding: position.encoding.into(),
        }
    }
}

#[pymethods]
impl Position {
    #[new]
    #[pyo3(signature = (line, column, encoding=None))]
    fn new(line: u32, column: u32, encoding: Option<PositionEncoding>) -> Self {
        Self {
            line,
            column,
            encoding: encoding.unwrap_or(PositionEncoding::CodePoint),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Position(line={}, column={}, encoding={:?})",
            self.line,
            self.column,
            self.encoding.as_str()
        )
    }
}

#[pyclass(module = "qdk._native", frozen, eq, hash, skip_from_py_object)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceRange {
    #[pyo3(get)]
    source_id: u32,
    #[pyo3(get)]
    start: Position,
    #[pyo3(get)]
    end: Position,
    document_id: Option<u64>,
}

impl PartialEq for SourceRange {
    fn eq(&self, other: &Self) -> bool {
        self.source_id == other.source_id && self.start == other.start && self.end == other.end
    }
}

impl Eq for SourceRange {}

impl Hash for SourceRange {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source_id.hash(state);
        self.start.hash(state);
        self.end.hash(state);
    }
}

impl From<SourceRange> for NativeRange {
    fn from(source_range: SourceRange) -> Self {
        Self {
            start: source_range.start.into(),
            end: source_range.end.into(),
        }
    }
}

#[pymethods]
impl SourceRange {
    #[new]
    #[allow(clippy::needless_pass_by_value)]
    fn new(source_id: u32, start: PyRef<'_, Position>, end: PyRef<'_, Position>) -> Self {
        Self {
            source_id,
            start: *start,
            end: *end,
            document_id: None,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "SourceRange(source_id={}, start={:?}, end={:?})",
            self.source_id, self.start, self.end
        )
    }
}

#[pyclass(module = "qdk._native", frozen, eq, hash, from_py_object)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SourceEdit {
    #[pyo3(get)]
    range: SourceRange,
    #[pyo3(get)]
    replacement: String,
}

#[pymethods]
impl SourceEdit {
    #[new]
    #[allow(clippy::needless_pass_by_value)]
    fn new(range: PyRef<'_, SourceRange>, replacement: String) -> Self {
        Self {
            range: *range,
            replacement,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "SourceEdit(range={:?}, replacement={:?})",
            self.range, self.replacement
        )
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct SourceFileInner {
    id: u32,
    path: Arc<str>,
    text: Arc<str>,
    span: Span,
    status: SourceStatus,
    aliases: Arc<[Arc<str>]>,
}

#[derive(Debug)]
pub(crate) struct SourceDocumentInner {
    id: u64,
    files: Arc<[SourceFileInner]>,
}

impl PartialEq for SourceDocumentInner {
    fn eq(&self, other: &Self) -> bool {
        self.files == other.files
    }
}

impl Eq for SourceDocumentInner {}

impl Hash for SourceDocumentInner {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.files.hash(state);
    }
}

impl From<&SourceSnapshot> for SourceDocumentInner {
    fn from(snapshot: &SourceSnapshot) -> Self {
        let files = snapshot
            .files()
            .iter()
            .map(|file| {
                let text_len = u32::try_from(file.text.len())
                    .expect("source contents length should fit into u32");
                SourceFileInner {
                    id: file.id,
                    path: file.path.clone(),
                    text: file.text.clone(),
                    span: Span {
                        lo: file.offset,
                        hi: file
                            .offset
                            .checked_add(text_len)
                            .expect("source end should fit into u32"),
                    },
                    status: file.status,
                    aliases: file.aliases.clone(),
                }
            })
            .collect::<Vec<_>>();
        Self {
            id: NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed),
            files: files.into(),
        }
    }
}

impl SourceDocumentInner {
    fn entry(&self) -> &SourceFileInner {
        self.files
            .first()
            .expect("source document should have an entry")
    }

    fn apply_edits(&self, edits: &[SourceEdit]) -> Result<String, RewriteError> {
        let entry = self.entry();
        let mut normalized = edits
            .iter()
            .enumerate()
            .map(|(index, edit)| self.normalize_edit(entry, index, edit))
            .collect::<Result<Vec<_>, _>>()?;
        normalized.sort_unstable_by_key(|edit| (edit.start, edit.end, edit.index));
        validate_ordered_edits(&normalized)?;

        let output_len = checked_output_len(
            entry.text.len(),
            normalized
                .iter()
                .map(|edit| (edit.end - edit.start, edit.replacement.len())),
        )?;
        let mut output = String::with_capacity(output_len);
        let mut cursor = 0;
        for edit in normalized {
            output.push_str(&entry.text[cursor..edit.start]);
            output.push_str(edit.replacement);
            cursor = edit.end;
        }
        output.push_str(&entry.text[cursor..]);
        Ok(output)
    }

    fn normalize_edit<'a>(
        &self,
        entry: &SourceFileInner,
        index: usize,
        edit: &'a SourceEdit,
    ) -> Result<NormalizedEdit<'a>, RewriteError> {
        let range = edit.range;
        if range
            .document_id
            .is_some_and(|document_id| document_id != self.id)
        {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::InvalidPosition,
                index,
                range,
            ));
        }
        let Some(source) = self
            .files
            .iter()
            .find(|source| source.id == range.source_id)
        else {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::UnknownSource,
                index,
                range,
            ));
        };
        if source.id != entry.id {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::IncludeEdit,
                index,
                range,
            ));
        }
        if range.start.encoding != range.end.encoding {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::MixedEncoding,
                index,
                range,
            ));
        }
        if range.start.encoding != PositionEncoding::Utf8 {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::InvalidPosition,
                index,
                range,
            ));
        }

        let start = native_byte_offset(&entry.text, range.start.into())
            .map_err(|_| RewriteError::for_edit(RewriteErrorCode::InvalidPosition, index, range))?;
        let end = native_byte_offset(&entry.text, range.end.into())
            .map_err(|_| RewriteError::for_edit(RewriteErrorCode::InvalidPosition, index, range))?;
        if end < start {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::ReversedRange,
                index,
                range,
            ));
        }

        Ok(NormalizedEdit {
            index,
            start: start as usize,
            end: end as usize,
            replacement: &edit.replacement,
            range,
        })
    }
}

fn validate_ordered_edits(edits: &[NormalizedEdit<'_>]) -> Result<(), RewriteError> {
    for pair in edits.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if previous.start == previous.end
            && current.start == current.end
            && previous.start == current.start
        {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::AmbiguousInsertion,
                current.index,
                current.range,
            ));
        }
        if current.start < previous.end {
            return Err(RewriteError::for_edit(
                RewriteErrorCode::Overlap,
                current.index,
                current.range,
            ));
        }
    }
    Ok(())
}

fn checked_output_len(
    source_len: usize,
    edits: impl IntoIterator<Item = (usize, usize)>,
) -> Result<usize, RewriteError> {
    let mut removed_total = 0usize;
    let mut replacement_total = 0usize;
    for (removed_len, replacement_len) in edits {
        removed_total = removed_total
            .checked_add(removed_len)
            .ok_or_else(RewriteError::document_too_large)?;
        replacement_total = replacement_total
            .checked_add(replacement_len)
            .ok_or_else(RewriteError::document_too_large)?;
    }
    source_len
        .checked_sub(removed_total)
        .and_then(|length| length.checked_add(replacement_total))
        .filter(|length| u32::try_from(*length).is_ok())
        .ok_or_else(RewriteError::document_too_large)
}

#[pyclass(module = "qdk._native", frozen, eq, hash, skip_from_py_object)]
#[derive(Eq, Hash, PartialEq)]
pub(crate) struct SourceFile {
    document: Arc<SourceDocumentInner>,
    index: usize,
}

impl SourceFile {
    fn inner(&self) -> &SourceFileInner {
        &self.document.files[self.index]
    }

    fn new(document: Arc<SourceDocumentInner>, index: usize) -> Self {
        Self { document, index }
    }
}

#[pymethods]
impl SourceFile {
    #[getter]
    fn id(&self) -> u32 {
        self.inner().id
    }

    #[getter]
    fn path(&self) -> &str {
        &self.inner().path
    }

    #[getter]
    fn text(&self) -> &str {
        &self.inner().text
    }

    #[getter]
    fn span(&self) -> Span {
        self.inner().span
    }

    #[getter]
    fn is_entry(&self) -> bool {
        self.inner().status == SourceStatus::Entry
    }

    #[getter]
    fn is_resolved(&self) -> bool {
        self.inner().status != SourceStatus::Unresolved
    }

    #[getter]
    fn resolution_status(&self) -> &'static str {
        match self.inner().status {
            SourceStatus::Entry => "entry",
            SourceStatus::Resolved => "resolved",
            SourceStatus::Unresolved => "unresolved",
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "SourceFile(id={}, path={:?}, resolution_status={:?})",
            self.id(),
            self.path(),
            self.resolution_status()
        )
    }
}

#[pyclass(module = "qdk._native", frozen, eq, skip_from_py_object)]
#[derive(Eq, PartialEq)]
pub(crate) struct SourceMap {
    document: Arc<SourceDocumentInner>,
}

impl SourceMap {
    fn new(document: Arc<SourceDocumentInner>) -> Self {
        Self { document }
    }

    fn file(&self, py: Python<'_>, index: usize) -> PyResult<Py<SourceFile>> {
        Py::new(py, SourceFile::new(self.document.clone(), index))
    }

    fn source(&self, source_id: u32) -> PyResult<&SourceFileInner> {
        self.document
            .files
            .iter()
            .find(|file| file.id == source_id)
            .ok_or_else(|| PyValueError::new_err(format!("unknown source ID {source_id}")))
    }

    fn source_for_span(
        &self,
        span: Span,
    ) -> PyResult<(&SourceFileInner, qdk_openqasm::span::Span)> {
        if span.hi < span.lo {
            return Err(PyValueError::new_err("span end precedes span start"));
        }

        self.document
            .files
            .iter()
            .find_map(|file| {
                (file.span.lo <= span.lo && span.hi <= file.span.hi).then(|| {
                    (
                        file,
                        qdk_openqasm::span::Span {
                            lo: span.lo - file.span.lo,
                            hi: span.hi - file.span.lo,
                        },
                    )
                })
            })
            .ok_or_else(|| PyValueError::new_err("span is not contained in one source"))
    }
}

#[pymethods]
impl SourceMap {
    #[getter]
    fn entry(&self, py: Python<'_>) -> PyResult<Py<SourceFile>> {
        self.file(py, 0)
    }

    #[getter]
    fn files(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        let files = (0..self.document.files.len())
            .map(|index| self.file(py, index))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(PyTuple::new(py, files)?.unbind())
    }

    fn __len__(&self) -> usize {
        self.document.files.len()
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let files = (0..self.document.files.len())
            .map(|index| self.file(py, index))
            .collect::<PyResult<Vec<_>>>()?;
        let list = PyList::new(py, files)?;
        Ok(list.as_any().try_iter()?.into_any().unbind())
    }

    fn get(&self, py: Python<'_>, source_id: u32) -> PyResult<Py<SourceFile>> {
        let index = self
            .document
            .files
            .iter()
            .position(|file| file.id == source_id)
            .ok_or_else(|| PyKeyError::new_err(source_id))?;
        self.file(py, index)
    }

    fn find(&self, py: Python<'_>, path: &str) -> PyResult<Option<Py<SourceFile>>> {
        self.document
            .files
            .iter()
            .position(|file| file.path.as_ref() == path)
            .map(|index| self.file(py, index))
            .transpose()
    }

    fn find_all(&self, py: Python<'_>, path: &str) -> PyResult<Py<PyTuple>> {
        let files = self
            .document
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.path.as_ref() == path)
            .map(|(index, _)| self.file(py, index))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(PyTuple::new(py, files)?.unbind())
    }

    #[pyo3(signature = (source_id, byte_offset, *, encoding=None))]
    fn position_at(
        &self,
        source_id: u32,
        byte_offset: u32,
        encoding: Option<PositionEncoding>,
    ) -> PyResult<Position> {
        let source = self.source(source_id)?;
        native_position_at(
            &source.text,
            byte_offset,
            encoding.unwrap_or(PositionEncoding::CodePoint).into(),
        )
        .map(Position::from)
        .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    #[allow(clippy::needless_pass_by_value)]
    fn byte_offset(&self, source_id: u32, position: PyRef<'_, Position>) -> PyResult<u32> {
        let source = self.source(source_id)?;
        native_byte_offset(&source.text, (*position).into())
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    #[pyo3(signature = (span, *, encoding=None))]
    #[allow(clippy::needless_pass_by_value)]
    fn range_from_span(
        &self,
        span: PyRef<'_, Span>,
        encoding: Option<PositionEncoding>,
    ) -> PyResult<SourceRange> {
        let (source, local_span) = self.source_for_span(*span)?;
        let range = native_range_from_span(
            &source.text,
            local_span,
            encoding.unwrap_or(PositionEncoding::CodePoint).into(),
        )
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(SourceRange {
            source_id: source.id,
            start: range.start.into(),
            end: range.end.into(),
            document_id: Some(self.document.id),
        })
    }

    #[allow(clippy::needless_pass_by_value)]
    fn span_from_range(&self, source_range: PyRef<'_, SourceRange>) -> PyResult<Span> {
        if source_range
            .document_id
            .is_some_and(|document_id| document_id != self.document.id)
        {
            return Err(PyValueError::new_err(
                "source range belongs to a different document",
            ));
        }
        let source = self.source(source_range.source_id)?;
        let local_span = native_span_from_range(&source.text, (*source_range).into())
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Span {
            lo: source
                .span
                .lo
                .checked_add(local_span.lo)
                .ok_or_else(|| PyValueError::new_err("global span start overflows u32"))?,
            hi: source
                .span
                .lo
                .checked_add(local_span.hi)
                .ok_or_else(|| PyValueError::new_err("global span end overflows u32"))?,
        })
    }

    fn __repr__(&self) -> String {
        format!("SourceMap(files=[{} items])", self.document.files.len())
    }
}

#[pyclass(module = "qdk._native", frozen, eq, skip_from_py_object)]
#[derive(Eq, PartialEq)]
pub(crate) struct SourceDocument {
    inner: Arc<SourceDocumentInner>,
}

impl SourceDocument {
    pub(crate) fn from_snapshot(snapshot: &SourceSnapshot) -> Self {
        Self {
            inner: Arc::new(SourceDocumentInner::from(snapshot)),
        }
    }

    pub(crate) fn entry_source(&self) -> (&str, &str) {
        let entry = self.inner.entry();
        (&entry.text, &entry.path)
    }

    pub(crate) fn apply_edits(&self, edits: &[SourceEdit]) -> Result<String, RewriteError> {
        self.inner.apply_edits(edits)
    }

    pub(crate) fn resolved_sources(&self) -> Vec<ResolvedSource> {
        self.inner
            .files
            .iter()
            .filter(|source| source.status == SourceStatus::Resolved)
            .map(|source| {
                (
                    source.path.clone(),
                    source.text.clone(),
                    source.aliases.clone(),
                )
            })
            .collect()
    }
}

#[pymethods]
impl SourceDocument {
    #[getter]
    fn entry(&self, py: Python<'_>) -> PyResult<Py<SourceFile>> {
        Py::new(py, SourceFile::new(self.inner.clone(), 0))
    }

    #[getter]
    fn source_map(&self, py: Python<'_>) -> PyResult<Py<SourceMap>> {
        Py::new(py, SourceMap::new(self.inner.clone()))
    }

    fn __repr__(&self) -> String {
        format!("SourceDocument(files=[{} items])", self.inner.files.len())
    }
}

pub(crate) fn register_source_types(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PositionEncoding>()?;
    m.add_class::<Position>()?;
    m.add_class::<SourceRange>()?;
    m.add_class::<SourceEdit>()?;
    m.add_class::<SourceFile>()?;
    m.add_class::<SourceMap>()?;
    m.add_class::<SourceDocument>()?;
    Ok(())
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PositionEncoding>();
    assert_send_sync::<Position>();
    assert_send_sync::<SourceRange>();
    assert_send_sync::<SourceEdit>();
    assert_send_sync::<SourceFile>();
    assert_send_sync::<SourceMap>();
    assert_send_sync::<SourceDocument>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use qdk_openqasm::io::InMemorySourceResolver;

    fn document(source: &str) -> SourceDocument {
        let result =
            qdk_openqasm::parse_source(source, "main.qasm", None::<&mut InMemorySourceResolver>);
        SourceDocument::from_snapshot(&result.source_snapshot)
    }

    fn utf8_range(start: u32, end: u32) -> SourceRange {
        SourceRange {
            source_id: 0,
            start: Position {
                line: 0,
                column: start,
                encoding: PositionEncoding::Utf8,
            },
            end: Position {
                line: 0,
                column: end,
                encoding: PositionEncoding::Utf8,
            },
            document_id: None,
        }
    }

    fn edit(start: u32, end: u32, replacement: &str) -> SourceEdit {
        SourceEdit {
            range: utf8_range(start, end),
            replacement: replacement.to_string(),
        }
    }

    #[test]
    fn edits_apply_deterministically_and_allow_adjacency() {
        let document = document("abcdef");
        let edits = [edit(4, 6, "Z"), edit(0, 2, "X"), edit(2, 4, "Y")];

        let output = document.apply_edits(&edits).expect("edits should apply");

        assert_eq!(output, "XYZ");
    }

    #[test]
    fn edits_reject_overlap_before_application() {
        let document = document("abcdef");
        let edits = [edit(1, 4, "X"), edit(3, 5, "Y")];

        let error = document
            .apply_edits(&edits)
            .expect_err("overlapping edits should fail");

        assert_eq!(error.code, RewriteErrorCode::Overlap);
        assert_eq!(error.edit_index, Some(1));
    }

    #[test]
    fn edits_reject_duplicate_insertions() {
        let document = document("abcdef");
        let edits = [edit(2, 2, "X"), edit(2, 2, "Y")];

        let error = document
            .apply_edits(&edits)
            .expect_err("duplicate insertions should fail");

        assert_eq!(error.code, RewriteErrorCode::AmbiguousInsertion);
    }

    #[test]
    fn edits_reject_non_utf8_and_invalid_boundaries() {
        let document = document("aé");
        let mut non_utf8 = edit(0, 1, "X");
        non_utf8.range.end.encoding = PositionEncoding::CodePoint;

        let mixed_error = document
            .apply_edits(&[non_utf8])
            .expect_err("mixed encodings should fail");
        let boundary_error = document
            .apply_edits(&[edit(2, 3, "X")])
            .expect_err("invalid UTF-8 boundaries should fail");

        assert_eq!(mixed_error.code, RewriteErrorCode::MixedEncoding);
        assert_eq!(boundary_error.code, RewriteErrorCode::InvalidPosition);
    }

    #[test]
    fn checked_output_len_rejects_u32_overflow() {
        let error = checked_output_len(u32::MAX as usize, [(0, 1)])
            .expect_err("output larger than u32 should fail");

        assert_eq!(error.code, RewriteErrorCode::DocumentTooLarge);
        assert_eq!(error.edit_index, None);
    }

    #[test]
    fn edits_reject_ranges_from_another_snapshot() {
        let original = document("abcdef");
        let rewritten = document("abcdef");
        let mut stale_edit = edit(1, 2, "X");
        stale_edit.range.document_id = Some(original.inner.id);

        let error = rewritten
            .apply_edits(&[stale_edit])
            .expect_err("stale ranges should fail");

        assert_eq!(error.code, RewriteErrorCode::InvalidPosition);
    }
}
