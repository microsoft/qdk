// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod error;
pub use error::Error;
pub use error::ErrorKind;
use qsc_data_structures::span::Span;

use std::sync::Arc;

use rustc_hash::FxHashMap;

/// A trait for resolving include file paths to their contents.
/// This is used by the parser to resolve `include` directives.
/// Implementations of this trait can be provided to the parser
/// to customize how include files are resolved.
pub trait SourceResolver {
    fn ctx(&mut self) -> &mut SourceResolverContext;

    fn resolve(
        &mut self,
        path: &Arc<str>,
        original_path: &Arc<str>,
    ) -> miette::Result<(Arc<str>, Arc<str>), Error>;
}

pub struct IncludeGraphNode {
    parent: Option<Arc<str>>,
    children: Vec<Arc<str>>,
}

#[derive(Default)]
pub struct SourceResolverContext {
    /// A graph representation of the include chain.
    include_graph: FxHashMap<Arc<str>, IncludeGraphNode>,
    /// Path being resolved.
    current_file: Option<Arc<str>>,
}

impl SourceResolverContext {
    pub fn check_include_errors(
        &mut self,
        path: &Arc<str>,
        span: Span,
    ) -> miette::Result<(), Error> {
        // If the new path makes a cycle in the include graph, we return
        // an error showing the cycle to the user.
        if let Some(cycle) = self.cycle_made_by_including_path(path) {
            return Err(Error(ErrorKind::CyclicInclude(span, cycle)));
        }

        // If the new path doesn't make a cycle but it was already
        // included before, we return a `MultipleInclude`
        // error saying "<FILE> was already included in <FILE>".
        if let Some(parent_file) = self.path_was_already_included(path) {
            return Err(Error(ErrorKind::MultipleInclude(
                span,
                path.to_string(),
                parent_file.to_string(),
            )));
        }

        self.add_path_to_include_graph(path);

        Ok(())
    }

    /// Changes `current_path` to its parent in the `include_graph`.
    pub fn pop_current_file(&mut self) {
        let parent = self
            .current_file
            .as_ref()
            .and_then(|file| self.include_graph.get(file).map(|node| node.parent.clone()))
            .flatten();
        self.current_file = parent;
    }

    pub fn peek_current_file(&mut self) -> Option<Arc<str>> {
        self.current_file.clone()
    }

    /// If including the path makes a cycle, returns a vector of the paths
    /// that make the cycle. Else, returns None.
    ///
    /// To check if adding `path` to the include graph creates a cycle we just
    /// need to verify if path is an ancestor of the current file.
    fn cycle_made_by_including_path(&self, path: &Arc<str>) -> Option<Cycle> {
        let mut current_file = self.current_file.clone()?;
        let mut paths = Vec::new();

        loop {
            let file = current_file.clone();
            paths.push(file.clone());
            current_file = self.get_parent(&current_file)?;
            if file == *path {
                paths.reverse();
                paths.push(path.clone());
                return Some(Cycle { paths });
            }
        }
    }

    /// Returns the file that included `path`.
    /// Returns `None` if `path` is the "main" file.
    fn get_parent(&self, path: &Arc<str>) -> Option<Arc<str>> {
        self.include_graph
            .get(path)
            .and_then(|node| node.parent.clone())
    }

    /// If the path was already included, returns the path of the file that
    /// included it. Else, returns None.
    fn path_was_already_included(&self, path: &Arc<str>) -> Option<Arc<str>> {
        // SAFETY: The call to expect should be unreachable, since the parent
        //         will only be None for the "main" file. But including the
        //         main file will trigger a cyclic include error before this
        //         function is called.
        self.include_graph
            .get(path)
            .map(|node| node.parent.clone())?
    }

    /// Adds `path` as a child of `current_path`, and then changes
    /// the `current_path` to `path`.
    fn add_path_to_include_graph(&mut self, path: &Arc<str>) {
        // 1. Add path to the current file children.
        if let Some(file) = self.current_file.as_ref()
            && let Some(node) = self.include_graph.get_mut(file)
        {
            node.children.push(path.clone());
        }

        // 2. Add path to the include graph.
        self.include_graph.insert(
            path.clone(),
            IncludeGraphNode {
                parent: self.current_file.clone(),
                children: Vec::new(),
            },
        );

        // 3. Update the current file.
        self.current_file = Some(path.clone());
    }
}

/// We use this struct to print a nice error message when we find a cycle.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Cycle {
    pub paths: Vec<Arc<str>>,
}

impl std::fmt::Display for Cycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parents = self.paths[0..(self.paths.len() - 1)].iter();
        let children = self.paths[1..].iter();

        for (parent, child) in parents.zip(children) {
            write!(f, "\n  {parent} includes {child}")?;
        }

        Ok(())
    }
}

/// A source resolver that resolves include files from an in-memory map.
/// This is useful for testing or environments in which file system access
/// is not available.
///
/// This requires users to build up a map of include file paths to their
/// contents prior to parsing.
pub struct InMemorySourceResolver {
    sources: FxHashMap<Arc<str>, Arc<str>>,
    ctx: SourceResolverContext,
}

impl FromIterator<(Arc<str>, Arc<str>)> for InMemorySourceResolver {
    fn from_iter<T: IntoIterator<Item = (Arc<str>, Arc<str>)>>(iter: T) -> Self {
        let mut map = FxHashMap::default();
        for (path, source) in iter {
            map.insert(path, source);
        }

        InMemorySourceResolver {
            sources: map,
            ctx: Default::default(),
        }
    }
}

impl SourceResolver for InMemorySourceResolver {
    fn ctx(&mut self) -> &mut SourceResolverContext {
        &mut self.ctx
    }

    fn resolve(
        &mut self,
        path: &Arc<str>,
        original_path: &Arc<str>,
    ) -> miette::Result<(Arc<str>, Arc<str>), Error> {
        match self.sources.get(path) {
            Some(source) => Ok((path.clone(), source.clone())),
            None => Err(Error(ErrorKind::NotFound(
                Span::default(),
                format!("Could not resolve include file: {original_path}"),
            ))),
        }
    }
}
