// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod ast;
use crate::io::SourceResolver;
use ast::{Program, StmtKind};
use mut_visit::MutVisitor;
use qsc_data_structures::error::WithSource;
use qsc_data_structures::source::SourceMap;
use qsc_data_structures::span::Span;
use scan::ParserContext;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
pub(crate) mod tests;

pub mod completion;
mod error;
pub use error::Error;
pub use error::ErrorKind;
mod expr;
mod mut_visit;
mod prgm;
mod prim;
mod scan;
mod stmt;

struct Offsetter(pub(super) u32);

impl MutVisitor for Offsetter {
    fn visit_span(&mut self, span: &mut Span) {
        span.lo += self.0;
        span.hi += self.0;
    }
}

#[derive(Debug, Clone)]
pub struct QasmParseResult {
    pub source: QasmSource,
    pub source_map: SourceMap,
}

impl QasmParseResult {
    #[must_use]
    pub fn new(source: QasmSource) -> QasmParseResult {
        let source_map = create_source_map(&source);
        let mut source = source;
        update_offsets(&source_map, &mut source);
        QasmParseResult { source, source_map }
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.source.has_errors()
    }

    pub fn all_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        let mut self_errors = self.errors();
        let include_errors = self
            .source
            .includes()
            .iter()
            .flat_map(QasmSource::all_errors)
            .map(|e| self.map_error(e))
            .collect::<Vec<_>>();

        self_errors.extend(include_errors);
        self_errors
    }

    #[must_use]
    pub fn errors(&self) -> Vec<WithSource<crate::error::Error>> {
        self.source
            .errors()
            .iter()
            .map(|e| self.map_error(e.clone()))
            .collect::<Vec<_>>()
    }

    fn map_error(&self, error: Error) -> WithSource<crate::error::Error> {
        WithSource::from_map(
            &self.source_map,
            crate::error::Error(crate::error::ErrorKind::Parser(error)),
        )
    }
}

/// all spans and errors spans are relative to the start of the file
/// We need to update the spans based on the offset of the file in the source map.
/// We have to do this after a full parse as we don't know what files will be loaded
/// until we have parsed all the includes.
fn update_offsets(source_map: &SourceMap, source: &mut QasmSource) {
    let source_file = source_map.find_by_name(&source.path());
    let offset = source_file.map_or(0, |source| source.offset);
    // Update the errors' offset
    source
        .errors
        .iter_mut()
        .for_each(|e| *e = e.clone().with_offset(offset));
    // Update the program's spans with the offset
    let mut offsetter = Offsetter(offset);
    offsetter.visit_program(&mut source.program);

    // Recursively update the includes, their programs, and errors
    for include in source.includes_mut() {
        update_offsets(source_map, include);
    }
}

/// Parse a QASM file and return the parse result.
/// This function will resolve includes using the provided resolver.
/// If an include file cannot be resolved, an error will be returned.
/// If a file is included recursively, a stack overflow occurs.
pub fn parse_source<R: SourceResolver, S: Into<Arc<str>>, P: Into<Arc<str>>>(
    source: S,
    path: P,
    resolver: &mut R,
) -> QasmParseResult {
    let path = path.into();
    resolver
        .ctx()
        .check_include_errors(&path, Span::default())
        .expect("Failed to check include errors");
    let res = parse_qasm_source(source.into(), path.clone(), resolver);
    QasmParseResult::new(res)
}

/// Creates a Q# source map from a QASM parse output. The `QasmSource`
/// has all of the recursive includes resolved with their own source
/// and parse results.
fn create_source_map(source: &QasmSource) -> SourceMap {
    let mut files: Vec<(Arc<str>, Arc<str>)> = Vec::new();
    collect_source_files(source, &mut files);
    SourceMap::new(files, None)
}

/// Recursively collect all source files from the includes
fn collect_source_files(source: &QasmSource, files: &mut Vec<(Arc<str>, Arc<str>)>) {
    files.push((source.path(), source.source()));
    // Collect all source files from the includes, this
    // begins the recursive process of collecting all source files.
    for include in source.includes() {
        collect_source_files(include, files);
    }
}

/// Represents a QASM source file that has been parsed.
#[derive(Clone, Debug)]
pub struct QasmSource {
    /// The path to the source file. This is used for error reporting.
    /// This path is just a name, it does not have to exist on disk.
    path: Arc<str>,
    /// The source code of the file.
    source: Arc<str>,
    /// The parsed AST of the source file or any parse errors.
    program: Program,
    /// Any parse errors that occurred.
    errors: Vec<Error>,
    /// Any included files that were resolved.
    /// Note that this is a recursive structure.
    included: Vec<QasmSource>,
}

impl QasmSource {
    #[must_use]
    pub fn new(
        source: Arc<str>,
        path: Arc<str>,
        program: Program,
        errors: Vec<Error>,
        included: Vec<QasmSource>,
    ) -> QasmSource {
        QasmSource {
            path,
            source,
            program,
            errors,
            included,
        }
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        if !self.errors().is_empty() {
            return true;
        }
        self.includes().iter().any(QasmSource::has_errors)
    }

    #[must_use]
    pub fn all_errors(&self) -> Vec<crate::parser::Error> {
        let mut self_errors = self.errors();
        let include_errors = self.includes().iter().flat_map(QasmSource::all_errors);
        self_errors.extend(include_errors);
        self_errors
    }

    #[must_use]
    pub fn includes(&self) -> &Vec<QasmSource> {
        self.included.as_ref()
    }

    #[must_use]
    pub fn includes_mut(&mut self) -> &mut Vec<QasmSource> {
        self.included.as_mut()
    }

    #[must_use]
    pub fn program(&self) -> &Program {
        &self.program
    }

    #[must_use]
    pub fn path(&self) -> Arc<str> {
        self.path.clone()
    }

    #[must_use]
    pub fn errors(&self) -> Vec<crate::parser::Error> {
        self.errors.clone()
    }

    #[must_use]
    pub fn source(&self) -> Arc<str> {
        self.source.clone()
    }
}

fn strip_scheme(path: &str) -> (Option<Arc<str>>, Arc<str>) {
    if let Some(scheme_end) = path.find("://") {
        let scheme = &path[..scheme_end];
        let after_scheme = &path[scheme_end + 3..];

        (Some(Arc::from(scheme)), Arc::from(after_scheme))
    } else {
        (None, Arc::from(path))
    }
}

/// append a path to a base path, resolving any relative components
/// like `.` and `..` in the process.
/// When the base path is a URI, it will be resolved as well.
/// Uri schemes are stripped from the path, and the resulting path
/// is processed as a file path. The scheme is prepended back to the
/// resulting path if it was present in the base path.
fn resolve_path(base: &Path, path: &Path) -> miette::Result<PathBuf> {
    let (scheme, joined) = strip_scheme(&base.join(path).to_string_lossy());
    let joined = PathBuf::from(joined.as_ref());
    // Adapted from https://github.com/rust-lang/cargo/blob/a879a1ca12e3997d9fdd71b70f34f1f3c866e1da/crates/cargo-util/src/paths.rs#L84
    let mut components = joined.components().peekable();
    let mut normalized = if let Some(c @ Component::Prefix(..)) = components.peek().copied() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(c) => {
                normalized.push(c);
            }
        }
    }
    if let Some(scheme) = scheme {
        normalized = format!("{scheme}://{}", normalized.to_string_lossy()).into();
    }
    Ok(normalized)
}

/// Parse a QASM file and return the parse result using the provided resolver.
/// Returns `Err` if the resolver cannot resolve the file.
/// Returns `Ok` otherwise. Any parse errors will be included in the result.
///
/// This function is the start of a recursive process that will resolve all
/// includes in the QASM file. Any includes are parsed as if their contents
/// were defined where the include statement is.
fn parse_qasm_file<R>(
    path: &Arc<str>,
    resolver: &mut R,
    span: Span,
) -> miette::Result<QasmSource, crate::parser::Error>
where
    R: SourceResolver,
{
    let resolved_path = if let Some(current) = resolver.ctx().peek_current_file() {
        let current_path = Path::new(current.as_ref());
        let parent_dir = current_path.parent().unwrap_or(Path::new("."));
        let target_path = Path::new(path.as_ref());

        match resolve_path(parent_dir, target_path) {
            Ok(resolved_path) => Arc::from(resolved_path.display().to_string()),
            Err(_) => path.clone(),
        }
    } else {
        path.clone()
    };

    resolver
        .ctx()
        .check_include_errors(&resolved_path, span)
        .map_err(|e| io_to_parse_error(span, e))?;

    match resolver.resolve(&resolved_path, path) {
        Ok((path, source)) => {
            let parse_result = parse_qasm_source(source, path.clone(), resolver);

            // Once we finish parsing the source, we pop the file from the
            // resolver. This is needed to keep track of multiple includes
            // and cyclic includes.
            resolver.ctx().pop_current_file();

            Ok(parse_result)
        }
        Err(e) => Err(io_to_parse_error(span, e)),
    }
}

fn io_to_parse_error(span: Span, e: crate::io::Error) -> Error {
    let e = e.with_span(span);
    let error = crate::parser::error::ErrorKind::IO(e);
    crate::parser::Error(error, None)
}

fn parse_qasm_source<R>(source: Arc<str>, path: Arc<str>, resolver: &mut R) -> QasmSource
where
    R: SourceResolver,
{
    let (program, errors, includes) = parse_source_and_includes(source.as_ref(), resolver);
    QasmSource::new(source, path, program, errors, includes)
}

fn parse_source_and_includes<P: AsRef<str>, R>(
    source: P,
    resolver: &mut R,
) -> (Program, Vec<Error>, Vec<QasmSource>)
where
    R: SourceResolver,
{
    let (program, mut errors) = parse(source.as_ref());
    let (includes, inc_errors) = parse_includes(&program, resolver);
    errors.extend(inc_errors);
    (program, errors, includes)
}

fn parse_includes<R>(program: &Program, resolver: &mut R) -> (Vec<QasmSource>, Vec<Error>)
where
    R: SourceResolver,
{
    let mut includes = vec![];
    let mut errors = vec![];
    for stmt in &program.statements {
        if let StmtKind::Include(include) = stmt.kind.as_ref() {
            let file_path = &include.filename;
            // Skip the standard gates include file.
            // Handling of this file is done by the compiler.
            if matches!(
                file_path.to_lowercase().as_ref(),
                "stdgates.inc" | "qelib1.inc" | "qdk.inc"
            ) {
                continue;
            }
            let source = match parse_qasm_file(file_path, resolver, stmt.span) {
                Ok(source) => {
                    // If the include was successful, we add it to the list of includes.
                    source
                }
                Err(e) => {
                    let error = match e.0 {
                        error::ErrorKind::IO(e) => {
                            let error_kind = error::ErrorKind::IO(e.with_span(stmt.span));
                            crate::parser::Error(error_kind, None)
                        }
                        _ => e,
                    };
                    // we need to push the error so that the error span is correct
                    // for the include statement of the parent file.
                    errors.push(error.clone());
                    // If the include failed, we create a QasmSource with an empty program
                    // The source has no errors as the error will be associated with the
                    // include statement in the parent file.
                    QasmSource {
                        path: file_path.clone(),
                        source: Default::default(),
                        program: Program {
                            span: Span::default(),
                            statements: vec![].into_boxed_slice(),
                            version: None,
                        },
                        errors: vec![],
                        included: vec![],
                    }
                }
            };
            includes.push(source);
        }
    }

    (includes, errors)
}

pub(crate) type Result<T> = std::result::Result<T, crate::parser::error::Error>;

pub(crate) trait Parser<T>: FnMut(&mut ParserContext) -> Result<T> {}

impl<T, F: FnMut(&mut ParserContext) -> Result<T>> Parser<T> for F {}

#[must_use]
pub fn parse(input: &str) -> (Program, Vec<Error>) {
    let mut scanner = ParserContext::new(input);
    let program = prgm::parse(&mut scanner);
    (program, scanner.into_errors())
}
