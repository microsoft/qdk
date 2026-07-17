// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Immutable source document projections for parsed OpenQASM syntax.

use crate::openqasm::span::Span;
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
    assert_send_sync::<SourceFile>();
    assert_send_sync::<SourceMap>();
    assert_send_sync::<SourceDocument>();
};
