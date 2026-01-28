// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod imports;
#[cfg(test)]
mod tests;

use crate::compile::preprocess::TrackedName;
pub use crate::resolve::imports::iter_valid_items;
use miette::Diagnostic;
use qsc_ast::ast::{ImportOrExportDecl, Package};
use qsc_ast::{
    ast::{
        self, CallableBody, CallableDecl, ClassConstraints, Ident, Idents, ImportKind, NodeId,
        PathKind, SpecBody, SpecGen, TopLevelNode, TypeParameter,
    },
    visit::{self as ast_visit, Visitor as AstVisitor, walk_attr},
};
use qsc_data_structures::{
    index_map::IndexMap,
    namespaces::{NamespaceId, NamespaceTreeRoot, PRELUDE},
    span::Span,
};
use qsc_hir::{
    assigner::Assigner,
    global,
    hir::{self, ItemId, ItemStatus, LocalItemId, PackageId},
    ty::{ParamId, Prim},
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Ordering;
use std::{collections::hash_map::Entry, rc::Rc, str::FromStr, vec};
use thiserror::Error;

/// The `Names` map contains an entry for every name in the source that was successfully
/// bound or resolved. A name can be in a declaration (a binding) or a usage (a resolved name).
/// Name nodes are `Path`s or `Ident`s. Unresolved names don't exist in the map.
///
/// ## Declarations:
///
/// In a declaration, the name `Ident` gets mapped to the declared item's ID. e.g.
///
/// Callables:
///
///     function <NAME>  ...
///
/// Locals:
///
///     let <NAME> = ...
///
/// In namespaces, the last `Ident` is used as the name node:
///
///     namespace <full.dotted.name> {
///                           <NAME>
///
/// An aliased import/export:
///
///     export <path> as <NAME>
///
/// A non-aliased import/export. In this case, note how the *name* of the export
/// overlaps with the *path* of the export, which will be separately mapped as a usage:
///
///     export <dotted.path>
///                   <NAME>
///
///     export <path>
///            <NAME>
///
///
/// ## Usages
///
/// A usage is a name that got resolved to a previously declared name. For a usage, the name
/// node gets mapped to the resolved item's ID. Usages are typically represented by `Path`s;
/// the exception is a field access in a local (see below). Examples:
///
/// Expressions:
///
///     <PATH>()
///
/// Types:
///
///     let x: <PATH> = ...
///
/// Import/exports:
///
///     export <PATH> as <name>
///
/// Field access. If a `Path` node gets resolved as a field access, then the first `Ident`
/// is mapped to the local declaration:
///
///     <LOCAL>.<field>
///
pub(super) type Names = IndexMap<NodeId, Res>;

// If the path is a field accessor, returns the mapped node id of the first ident's declaration and the vec of part's idents.
// Otherwise, returns None.
// Field accessor paths have their leading segment mapped as a local variable, whereas namespace paths have their path id mapped.
#[must_use]
pub fn path_as_field_accessor<'a>(
    names: &Names,
    path: &'a impl Idents,
) -> Option<(NodeId, Vec<&'a ast::Ident>)> {
    let parts: Vec<&Ident> = path.iter().collect();
    let first = parts.first().expect("path should have at least one part");
    if let Some(&Res::Local(node_id)) = names.get(first.id) {
        return Some((node_id, parts));
    }
    // If any of the above conditions are not met, return None.
    None
}

/// A resolution. This connects a usage of a name with the declaration of that name by uniquely
/// identifying the node that declared it.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Res {
    /// A global or local item.
    Item(ItemId, ItemStatus),
    /// A local variable.
    Local(NodeId),
    /// A type/functor parameter in the generics section of the parent callable decl.
    Param {
        id: ParamId,
        bounds: ClassConstraints,
    },
    /// A primitive type.
    PrimTy(Prim),
    /// The unit type.
    UnitTy,
    /// An item that can be used in an import/export: callable, UDT or namespace.
    Importable(Importable),
}

impl Res {
    #[must_use]
    pub fn item_id(&self) -> Option<ItemId> {
        match self {
            Res::Item(id, ..)
            | Res::Importable(Importable::Callable(id, _) | Importable::Ty(id, _), ..) => Some(*id),
            Res::Importable(Importable::Namespace(_, id)) => *id,
            _ => None,
        }
    }
}

/// Kind of importable item - callable, UDT or namespace.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Copy)]
pub enum Importable {
    Callable(ItemId, ItemStatus),
    Ty(ItemId, ItemStatus),
    Namespace(NamespaceId, Option<ItemId>),
}

#[derive(Clone, Debug, Diagnostic, Error, PartialEq)]
pub(super) enum Error {
    #[error("`{name}` could refer to the item in `{first_open}` or `{second_open}`")]
    #[diagnostic(code("Qsc.Resolve.Ambiguous"))]
    Ambiguous {
        name: String,
        first_open: String,
        second_open: String,
        #[label("ambiguous name")]
        name_span: Span,
        #[label("found in this namespace")]
        first_open_span: Span,
        #[label("and also in this namespace")]
        second_open_span: Span,
    },

    #[error("`{name}` could refer to the item in `{candidate_a}` or an item in `{candidate_b}`")]
    #[diagnostic(help("both namespaces are implicitly opened by the prelude"))]
    #[diagnostic(code("Qsc.Resolve.AmbiguousPrelude"))]
    AmbiguousPrelude {
        name: String,
        candidate_a: String,
        candidate_b: String,
        #[label("ambiguous name")]
        span: Span,
    },

    #[error("duplicate declaration of `{0}` in namespace `{1}`")]
    #[diagnostic(code("Qsc.Resolve.Duplicate"))]
    Duplicate(String, String, #[label] Span),

    #[error("duplicate name `{0}` in pattern")]
    #[diagnostic(help("a name cannot shadow another name in the same pattern"))]
    #[diagnostic(code("Qsc.Resolve.DuplicateBinding"))]
    DuplicateBinding(String, #[label] Span),

    #[error("duplicate intrinsic `{0}`")]
    #[diagnostic(help(
        "each callable declared as `body intrinsic` or `@SimulatableIntrinsic` must have a globally unique name"
    ))]
    #[diagnostic(code("Qsc.Resolve.DuplicateIntrinsic"))]
    DuplicateIntrinsic(String, #[label] Span),

    #[error("duplicate export of `{name}`")]
    DuplicateExport {
        name: String,
        #[label]
        span: Span,
        #[label("item was previously exported here")]
        existing_span: Span,
    },

    #[error("`{0}` not found")]
    #[diagnostic(code("Qsc.Resolve.NotFound"))]
    NotFound(String, #[label] Span),

    #[error("`{0}` not found")]
    #[diagnostic(help(
        "found a matching item `{1}` that is not available for the current compilation configuration"
    ))]
    #[diagnostic(code("Qsc.Resolve.NotFound"))]
    NotAvailable(String, String, #[label] Span),

    #[error("`Qubit` not found")]
    #[diagnostic(help(
        "to allocate qubits, use syntax like `use q = Qubit();` or `use qs = Qubit[N];` or `use (q1, q2) = (Qubit(), Qubit());`"
    ))]
    #[diagnostic(code("Qsc.Resolve.NotFoundQubit"))]
    NotFoundQubit(#[label] Span),

    #[error("use of unimplemented item `{0}`")]
    #[diagnostic(help("this item is not implemented and cannot be used"))]
    #[diagnostic(code("Qsc.Resolve.Unimplemented"))]
    Unimplemented(String, #[label] Span),

    #[error("export statements are not allowed in a local scope")]
    #[diagnostic(code("Qsc.Resolve.ExportFromLocalScope"))]
    ExportFromLocalScope(#[label] Span),

    #[error("import resolution exceeded maximum iterations ({0})")]
    #[diagnostic(code("Qsc.Resolve.ImportResolutionLimitExceeded"))]
    ImportResolutionLimitExceeded(usize),
}

#[derive(Debug, Clone)]
pub struct Scope {
    /// The span that the scope applies to. For callables and namespaces, this includes
    /// the entire callable / namespace declaration. For blocks, this includes the braces.
    span: Span,
    kind: ScopeKind,
    /// Open statements. The key is the alias.
    opens: FxHashMap<Option<Rc<str>>, Vec<Open>>,
    /// Local newtype declarations.
    tys: FxHashMap<Rc<str>, Res>,
    /// Local callable and newtype declarations.
    terms: FxHashMap<Rc<str>, Res>,
    /// Importables, such as callables, types, and namespaces, and their aliases declared
    /// via imports or exports.
    importables: FxHashMap<Rc<str>, Res>,
    /// Local variables, including callable parameters, for loop bindings, etc.
    /// The u32 is the `valid_at` offset - the lowest offset at which the variable name is available.
    /// It's used to determine which variables are visible at a specific offset in the scope.
    ///
    /// Bug: Because we keep track of only one `valid_at` offset per name,
    /// when a variable is later shadowed in the same scope,
    /// it is missed in the list. <a href=https://github.com/microsoft/qsharp/issues/897 />
    vars: FxHashMap<Rc<str>, (u32, NodeId)>,
    /// Type parameters.
    ty_vars: FxHashMap<Rc<str>, (ParamId, ClassConstraints)>,
}

impl Scope {
    fn new(kind: ScopeKind, span: Span) -> Self {
        Self {
            span,
            kind,
            opens: FxHashMap::default(),
            tys: FxHashMap::default(),
            terms: FxHashMap::default(),
            importables: FxHashMap::default(),
            vars: FxHashMap::default(),
            ty_vars: FxHashMap::default(),
        }
    }

    fn item(&self, kind: NameKind, name: &str) -> Option<Res> {
        let items = match kind {
            NameKind::Term => &self.terms,
            NameKind::Ty => &self.tys,
            NameKind::Importable => &self.importables,
        };
        items.get(name).cloned()
    }

    /// A `ScopeKind, Span` pair uniquely identifies a scope.
    fn key(&self) -> (&ScopeKind, Span) {
        (&self.kind, self.span)
    }
}

type ScopeId = usize;

#[derive(Debug, Clone, Default)]
pub struct Locals {
    // order is ascending by span (outermost -> innermost)
    scopes: Vec<Scope>,
}

impl Locals {
    fn get_scopes<'a>(
        &'a self,
        scope_chain: &'a [ScopeId],
    ) -> impl Iterator<Item = &'a Scope> + 'a {
        // reverse to go from innermost -> outermost
        scope_chain.iter().rev().map(|id| {
            self.scopes
                .get(*id)
                .unwrap_or_else(|| panic!("scope with id {id:?} should exist"))
        })
    }

    fn push_scope(&mut self, kind: ScopeKind, span: Span) -> ScopeId {
        // First, check if this scope has already been created in a prior pass.
        for (id, existing_scope) in self.scopes.iter_mut().enumerate() {
            if existing_scope.key() == (&kind, span) {
                // If the scope already exists, return that.
                return id;
            }
        }

        // Add it to the list of known scopes.
        let id = self.scopes.len();
        self.scopes.insert(id, Scope::new(kind, span));
        id
    }

    fn get_scope_mut(&mut self, id: ScopeId) -> &mut Scope {
        self.scopes
            .get_mut(id)
            .unwrap_or_else(|| panic!("scope with id {id:?} should exist"))
    }

    #[must_use]
    pub fn get_all_at_offset(&self, offset: u32) -> Vec<Local> {
        let mut vars = true;
        let mut all_locals = Vec::new();
        self.for_each_scope_at_offset(offset, |scope| {
            // inner to outer
            all_locals.extend(get_scope_locals(scope, offset, vars));

            if scope.kind == ScopeKind::Callable {
                // Since local callables are not closures, hide local variables in parent scopes.
                vars = false;
            }
        });

        // deduping by name will effectively make locals in a child scope
        // shadow the locals in its parent scopes
        all_locals.dedup_by(|a, b| a.name().is_some() && a.name() == b.name());

        all_locals
    }

    fn for_each_scope_at_offset<F>(&self, offset: u32, mut f: F)
    where
        F: FnMut(&Scope),
    {
        // reverse to go from innermost -> outermost
        self.scopes.iter().rev().for_each(|scope| {
            // the block span includes the delimiters (e.g. the braces)
            if scope.span.lo < offset && scope.span.hi > offset {
                f(scope);
            }
        });
    }
}

#[derive(Debug)]
pub enum Local {
    /// A local callable or UDT.
    Item(ItemId, Rc<str>),
    /// A type parameter.
    TyParam(ParamId, Rc<str>),
    /// A local variable or parameter.
    Var(NodeId, Rc<str>),
    /// A namespace import (`import Foo as A` or `open Foo`)
    NamespaceImport(NamespaceId, Option<Rc<str>>),
}

impl Local {
    #[must_use]
    pub fn name(&self) -> Option<&Rc<str>> {
        match self {
            Local::Item(_, name) | Local::TyParam(_, name) | Local::Var(_, name) => Some(name),
            Local::NamespaceImport(_, alias) => alias.as_ref(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct GlobalScope {
    /// Global names that are valid in a type context (UDTs, builtin types...)
    tys: IndexMap<NamespaceId, FxHashMap<Rc<str>, Res>>,
    /// Global names that are valid in an expression context (callables, UDTs...)
    terms: IndexMap<NamespaceId, FxHashMap<Rc<str>, Res>>,
    /// Global names that are valid in an import/export context (callables, UDTs, namespaces, and any aliases)
    importables: IndexMap<NamespaceId, FxHashMap<Rc<str>, Res>>,
    /// Known namespaces, used to key into the above maps. Exported namespaces are also tracked here.
    namespaces: NamespaceTreeRoot,
    /// Known intrinsics, used to check for duplicates
    intrinsics: FxHashSet<Rc<str>>,
    /// Known self-exports, used to check for duplicates
    self_exported_item_ids: FxHashMap<ItemId, Span>,
}

impl GlobalScope {
    fn get(&self, kind: NameKind, namespace: NamespaceId, name: &str) -> Option<&Res> {
        self.table(kind)
            .get(namespace)
            .and_then(|items| items.get(name))
    }

    pub fn table(&self, kind: NameKind) -> &IndexMap<NamespaceId, FxHashMap<Rc<str>, Res>> {
        match kind {
            NameKind::Term => &self.terms,
            NameKind::Ty => &self.tys,
            NameKind::Importable => &self.importables,
        }
    }

    /// Creates a namespace in the namespace mapping. Note that namespaces are tracked separately from their
    /// item contents. This returns a [`NamespaceId`] which you can use to add more tys and terms to the scope.
    fn insert_or_find_namespace(
        &mut self,
        name: impl IntoIterator<Item = Rc<str>>,
        root: Option<NamespaceId>,
    ) -> NamespaceId {
        let name = name.into_iter().collect::<Vec<_>>();
        let root = root.unwrap_or_else(|| self.namespaces.root_id());
        if name.is_empty() {
            return root;
        }
        self.namespaces
            .insert_or_find_namespace_from_root(name, root)
    }

    /// Finds a namespace by its path.
    pub fn find_namespace<'a>(
        &self,
        name: impl IntoIterator<Item = &'a str>,
        root: Option<NamespaceId>,
    ) -> Option<NamespaceId> {
        match root {
            None => self.namespaces.get_namespace_id(name),
            Some(root_id) => self
                .namespaces
                .find_namespace_by_id(&root_id)
                .1
                .borrow()
                .get_namespace_id(name),
        }
    }

    /// Adds another path to an existing namespace, used for namespace exports.
    fn insert_alias_for_namespace(
        &mut self,
        existing_namespace: NamespaceId,
        alias_name: &Rc<str>,
        alias_parent: NamespaceId,
    ) {
        // Any name collisions should have been detected before
        // calling this function.
        self.namespaces
            .insert_or_find_namespace_from_root_with_id(
                vec![alias_name.clone()],
                alias_parent,
                existing_namespace,
            )
            .expect("alias should not clobber an existing namespace");
    }

    /// The id of the root ("") namespace.
    fn root_namespace(&self) -> NamespaceId {
        self.namespaces.root_id()
    }

    /// Returns the full namespace path for a given namespace ID.
    pub fn format_namespace_name(&self, namespace_id: NamespaceId) -> String {
        self.namespaces
            .find_namespace_by_id(&namespace_id)
            .0
            .join(".")
    }

    pub fn namespace_children(&self, namespace_id: NamespaceId) -> Vec<Rc<str>> {
        self.namespaces
            .find_namespace_by_id(&namespace_id)
            .1
            .borrow()
            .children
            .keys()
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum ScopeKind {
    Namespace(NamespaceId),
    Callable,
    Block,
}

#[derive(Clone, Copy, Debug)]
pub enum NameKind {
    /// A name that is valid as an expression context: a callable or UDT
    Term,
    /// A name that is valid in a type context: builtin or UDT
    Ty,
    /// A name that is valid in an import/export: a callable, a UDT,
    /// a namespace, or another import
    Importable,
}

#[derive(Debug, Clone)]
struct Open {
    namespace: NamespaceId,
    span: Span,
}

impl Eq for Open {}

impl PartialEq<Self> for Open {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace
    }
}

impl PartialOrd<Self> for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        let a: usize = self.namespace.into();
        let b: usize = other.namespace.into();
        a.cmp(&b)
    }
}

pub(super) struct Resolver {
    /// The ID of the package being resolved
    package_id: PackageId,
    /// Successfully bound or resolved names (see doc for `Names` for full details)
    names: Names,
    /// Global scope
    globals: GlobalScope,
    /// All the local scopes in the program
    locals: Locals,
    /// Errors encountered during resolution
    errors: Vec<Error>,
    /// List of names that were dropped by the preprocessor
    dropped_names: Vec<TrackedName>,
    /// Visitor state: if currently within a callable scope, the parameters of the callable.
    curr_params: Option<FxHashSet<Rc<str>>>,
    /// Visitor state: Current chain of scopes
    curr_scope_chain: Vec<ScopeId>,
}

impl Resolver {
    pub(crate) fn resolve(&mut self, assigner: &mut Assigner, package: &Package) {
        // We start with all global declarations already bound.
        // Callable and UDT names have been added to the global scope in the
        // previous binding step, but imports have not.

        // Now, resolve all the global imports and exports.
        // Import/export names will be added to the global scope as they are
        // successfully resolved.
        imports::resolve_all_namespace_imports_and_exports(self, package);
        imports::resolve_top_level_imports(self, package);

        // Now that all global imports are resolved,
        // proceed to resolve the rest of the package.
        self.with(assigner).visit_package(package);
    }

    pub(super) fn new(
        package_id: PackageId,
        globals: GlobalTable,
        dropped_names: Vec<TrackedName>,
    ) -> Self {
        Self {
            package_id,
            names: globals.names,
            dropped_names,
            curr_params: None,
            globals: globals.scope,
            locals: Locals::default(),
            curr_scope_chain: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(super) fn with_persistent_local_scope(
        package_id: PackageId,
        globals: GlobalTable,
        dropped_names: Vec<TrackedName>,
    ) -> Self {
        let mut locals = Locals::default();
        let scope_id = locals.push_scope(
            ScopeKind::Block,
            Span {
                lo: 0,
                hi: u32::MAX,
            },
        );
        Self {
            package_id,
            names: globals.names,
            dropped_names,
            curr_params: None,
            globals: globals.scope,
            locals,
            curr_scope_chain: vec![scope_id],
            errors: Vec::new(),
        }
    }

    pub(super) fn names(&self) -> &Names {
        &self.names
    }

    pub(super) fn locals(&self) -> &Locals {
        &self.locals
    }

    pub(super) fn globals(&self) -> &GlobalScope {
        &self.globals
    }

    pub(super) fn drain_errors(&mut self) -> vec::Drain<'_, Error> {
        self.errors.drain(..)
    }

    fn with<'a>(&'a mut self, assigner: &'a mut Assigner) -> With<'a> {
        With {
            resolver: self,
            assigner,
        }
    }

    pub(super) fn into_result(self) -> (Names, GlobalScope, Locals, Vec<Error>) {
        (self.names, self.globals, self.locals, self.errors)
    }

    pub(super) fn extend_dropped_names(&mut self, dropped_names: Vec<TrackedName>) {
        self.dropped_names.extend(dropped_names);
    }

    pub(super) fn bind_fragments(&mut self, ast: &ast::Package, assigner: &mut Assigner) {
        for node in &mut ast.nodes.iter() {
            match node {
                TopLevelNode::Namespace(namespace) => {
                    bind_global_items(
                        self.package_id,
                        &mut self.names,
                        &mut self.globals,
                        namespace,
                        assigner,
                        &mut self.errors,
                    );
                }
                TopLevelNode::Stmt(stmt) => {
                    if let ast::StmtKind::Item(item) = stmt.kind.as_ref() {
                        self.bind_local_item(assigner, item);
                    }
                }
            }
        }
    }

    fn check_item_status(&mut self, res: &Res, name: String, span: Span) {
        if let Res::Item(_, ItemStatus::Unimplemented) = res {
            self.errors.push(Error::Unimplemented(name, span));
        }
    }

    fn resolve_ident(&mut self, kind: NameKind, name: &Ident) {
        match resolve(
            kind,
            &self.globals,
            self.locals.get_scopes(&self.curr_scope_chain),
            name,
            None,
        ) {
            Ok(res) => {
                self.check_item_status(&res, name.name.to_string(), name.span);
                self.names.insert(name.id, res);
            }
            Err(err) => self.errors.push(err),
        }
    }

    fn resolve_path_kind(&mut self, kind: NameKind, path: &ast::PathKind) -> Result<(), Error> {
        match path {
            PathKind::Ok(path) => self.resolve_path(kind, path).map(|_| ()),
            PathKind::Err(incomplete_path) => {
                // First we check if the the path can be resolved as a field accessor.
                // We do this by checking if the first part of the path is a local variable.
                if let (NameKind::Term, Some(incomplet_path)) = (kind, incomplete_path) {
                    let first = incomplet_path
                        .segments
                        .first()
                        .expect("path `segments` should have at least one element");
                    match resolve(
                        NameKind::Term,
                        &self.globals,
                        self.locals.get_scopes(&self.curr_scope_chain),
                        first,
                        None,
                    ) {
                        Ok(res) if matches!(res, Res::Local(_)) => {
                            // The path is a field accessor.
                            self.names.insert(first.id, res.clone());
                            return Ok(());
                        }
                        Err(err) if !matches!(err, Error::NotFound(_, _)) => return Err(err), // Local was found but has issues.
                        _ => return Ok(()), // The path is assumed to not be a field accessor, so move on.
                    }
                }
                Ok(())
            }
        }
    }

    fn resolve_path(&mut self, kind: NameKind, path: &ast::Path) -> Result<Res, Error> {
        let name = &path.name;
        let segments = &path.segments;

        // First we check if the the path can be resolved as a field accessor.
        // We do this by checking if the first part of the path is a local variable.
        if let (NameKind::Term, Some(segments)) = (kind, segments) {
            let first = segments
                .first()
                .expect("path `segments` should have at least one element");
            match resolve(
                kind,
                &self.globals,
                self.locals.get_scopes(&self.curr_scope_chain),
                first,
                None,
            ) {
                Ok(res) if matches!(res, Res::Local(_)) => {
                    // The path is a field accessor.
                    self.names.insert(first.id, res.clone());
                    return Ok(res);
                }
                Err(err) if !matches!(err, Error::NotFound(_, _)) => return Err(err), // Local was found but has issues.
                _ => {} // The path is assumed to not be a field accessor, so move on to process it as a namespace path.
            }
        }

        // If the path is not a field accessor, we resolve it as a namespace path.
        // This is done by passing in the last part of the path as the name to resolve,
        // with the rest of the parts as the namespace segments.
        match resolve(
            kind,
            &self.globals,
            self.locals.get_scopes(&self.curr_scope_chain),
            name,
            segments.as_deref(),
        ) {
            Ok(res) => {
                self.check_item_status(&res, path.name.name.to_string(), path.span);
                self.names.insert(path.id, res.clone());
                Ok(res)
            }
            Err(err) => {
                if let Error::NotFound(name, span) = err {
                    if let Some(dropped_name) =
                        self.dropped_names.iter().find(|n| n.name.as_ref() == name)
                    {
                        Err(Error::NotAvailable(
                            name,
                            format!("{}.{}", dropped_name.namespace, dropped_name.name),
                            span,
                        ))
                    } else if name == "Qubit" {
                        Err(Error::NotFoundQubit(span))
                    } else {
                        Err(Error::NotFound(name, span))
                    }
                } else {
                    Err(err)
                }
            }
        }
    }

    /// # Arguments
    ///
    /// * `pat` - The pattern to bind.
    /// * `valid_at` - The offset at which the name becomes defined. This is used to determine
    ///   whether a name is in scope at a given offset.
    ///   e.g. For a local variable, this would be immediately after the declaration statement.
    ///   For input parameters to a callable, this would be the start of the body block.
    fn bind_pat(&mut self, pat: &ast::Pat, valid_at: u32) {
        let mut bindings = FxHashSet::default();
        self.bind_pat_recursive(pat, valid_at, &mut bindings);
    }

    fn bind_pat_recursive(
        &mut self,
        pat: &ast::Pat,
        valid_at: u32,
        bindings: &mut FxHashSet<Rc<str>>,
    ) {
        match &*pat.kind {
            ast::PatKind::Bind(name, _) => {
                if !bindings.insert(Rc::clone(&name.name)) {
                    self.errors
                        .push(Error::DuplicateBinding(name.name.to_string(), name.span));
                }
                self.names.insert(name.id, Res::Local(name.id));
                self.current_scope_mut()
                    .vars
                    .insert(Rc::clone(&name.name), (valid_at, name.id));
            }
            ast::PatKind::Discard(_) | ast::PatKind::Elided | ast::PatKind::Err => {}
            ast::PatKind::Paren(pat) => self.bind_pat_recursive(pat, valid_at, bindings),
            ast::PatKind::Tuple(pats) => pats
                .iter()
                .for_each(|p| self.bind_pat_recursive(p, valid_at, bindings)),
        }
    }

    /// Adds this namespace to the current scope's opens. When in a namespace scope,
    /// the path is resolved relative to the current namespace.
    fn resolve_and_add_open(&mut self, path: &impl Idents, alias: Option<&Ident>) {
        let base = {
            let current_scope = self.current_scope_mut();
            if let ScopeKind::Namespace(ns_id) = current_scope.kind {
                Some(ns_id)
            } else {
                None
            }
        };

        // Attempt to resolve the namespace name relative to the given base namespace,
        // then relative to the root.
        let ns_id = base
            .and_then(|base| self.globals.find_namespace(path.str_iter(), Some(base)))
            .or_else(|| self.globals.find_namespace(path.str_iter(), None));
        let Some(id) = ns_id else {
            self.errors.push(Error::NotFound(
                path.full_name().to_string(),
                path.full_span(),
            ));
            return;
        };

        let alias = alias.as_ref().map(|a| Rc::clone(&a.name));

        self.bind_open(path, id, alias);
    }

    fn bind_open(&mut self, path: &impl Idents, id: NamespaceId, alias: Option<Rc<str>>) {
        let current_opens = self.current_scope_mut().opens.entry(alias).or_default();

        let open = Open {
            namespace: id,
            span: path.full_span(),
        };
        if !current_opens.contains(&open) {
            current_opens.push(open);
        }
    }

    pub(super) fn bind_local_item(&mut self, assigner: &mut Assigner, item: &ast::Item) {
        match &*item.kind {
            ast::ItemKind::Open(..) => {
                // opens are handled in the import pass
            }
            ast::ItemKind::Callable(decl) => {
                let id = intrapackage(self.package_id, assigner.next_item());
                let status = ItemStatus::from_attrs(&ast_attrs_as_hir_attrs(&item.attrs));
                self.names.insert(decl.name.id, Res::Item(id, status));
                let scope = self.current_scope_mut();
                scope
                    .terms
                    .insert(Rc::clone(&decl.name.name), Res::Item(id, status));
                scope.importables.insert(
                    Rc::clone(&decl.name.name),
                    Res::Importable(Importable::Callable(id, status)),
                );
            }
            ast::ItemKind::Ty(name, _) => {
                let id = intrapackage(self.package_id, assigner.next_item());
                let status = ItemStatus::from_attrs(&ast_attrs_as_hir_attrs(&item.attrs));
                self.names.insert(name.id, Res::Item(id, status));
                let scope = self.current_scope_mut();
                scope
                    .tys
                    .insert(Rc::clone(&name.name), Res::Item(id, status));
                scope
                    .terms
                    .insert(Rc::clone(&name.name), Res::Item(id, status));
                scope.importables.insert(
                    Rc::clone(&name.name),
                    Res::Importable(Importable::Ty(id, status)),
                );
            }
            ast::ItemKind::Struct(decl) => {
                let id = intrapackage(self.package_id, assigner.next_item());
                let status = ItemStatus::from_attrs(&ast_attrs_as_hir_attrs(&item.attrs));
                self.names.insert(decl.name.id, Res::Item(id, status));
                let scope = self.current_scope_mut();
                scope
                    .tys
                    .insert(Rc::clone(&decl.name.name), Res::Item(id, status));
                scope
                    .terms
                    .insert(Rc::clone(&decl.name.name), Res::Item(id, status));
                scope.importables.insert(
                    Rc::clone(&decl.name.name),
                    Res::Importable(Importable::Ty(id, status)),
                );
            }
            ast::ItemKind::ImportOrExport(decl) => {
                assign_item_ids(
                    decl,
                    || intrapackage(self.package_id, assigner.next_item()),
                    &mut self.names,
                );
            }
            ast::ItemKind::Err => (),
        }
    }

    /// For a given callable declaration, bind the names of the type parameters
    /// into the current scope. Tracks the constraints defined on the type parameters
    /// as well, for later use in type checking.
    fn bind_type_parameters(&mut self, decl: &CallableDecl) {
        decl.generics
            .iter()
            .enumerate()
            .for_each(|(ix, type_parameter)| {
                self.current_scope_mut().ty_vars.insert(
                    Rc::clone(&type_parameter.ty.name),
                    (ix.into(), type_parameter.constraints.clone()),
                );
                self.names.insert(
                    type_parameter.ty.id,
                    Res::Param {
                        id: ix.into(),
                        bounds: type_parameter.constraints.clone(),
                    },
                );
            });
    }

    fn push_scope(&mut self, span: Span, kind: ScopeKind) {
        let scope_id = self.locals.push_scope(kind, span);
        self.curr_scope_chain.push(scope_id);
    }

    fn pop_scope(&mut self) {
        self.curr_scope_chain
            .pop()
            .expect("pushed scope should be the last element on the stack");
    }

    /// Returns the innermost scope in the current scope chain.
    fn current_scope_mut(&mut self) -> &mut Scope {
        let scope_id = *self
            .curr_scope_chain
            .last()
            .expect("there should be at least one scope at location");

        self.locals.get_scope_mut(scope_id)
    }
}

/// Constructed from a [Resolver] and an [Assigner], this structure implements `Visitor`
/// on the package AST where it resolves identifiers, and assigns IDs to them.
pub(super) struct With<'a> {
    resolver: &'a mut Resolver,
    assigner: &'a mut Assigner,
}

impl With<'_> {
    /// Given a function, apply that function to `Self` within a scope. In other words, this
    /// function automatically pushes a scope before `f` and pops it after.
    fn with_scope(&mut self, span: Span, kind: ScopeKind, f: impl FnOnce(&mut Self)) {
        self.resolver.push_scope(span, kind);
        f(self);
        self.resolver.pop_scope();
    }

    /// Apply `f` to self while a pattern's constituent identifiers are in scope. Removes those
    /// identifiers from the scope after `f`.
    fn with_pat(&mut self, span: Span, kind: ScopeKind, pat: &ast::Pat, f: impl FnOnce(&mut Self)) {
        self.visit_pat(pat);
        self.with_scope(span, kind, |visitor| {
            // The bindings are valid from the beginning of the scope
            visitor.resolver.bind_pat(pat, span.lo);
            f(visitor);
        });
    }

    fn with_spec_pat(
        &mut self,
        span: Span,
        kind: ScopeKind,
        pat: &ast::Pat,
        f: impl FnOnce(&mut Self),
    ) {
        let mut bindings = self
            .resolver
            .curr_params
            .as_ref()
            .map_or_else(FxHashSet::default, std::clone::Clone::clone);
        self.with_scope(span, kind, |visitor| {
            visitor
                .resolver
                .bind_pat_recursive(pat, span.lo, &mut bindings);
            f(visitor);
        });
    }
}

impl AstVisitor<'_> for With<'_> {
    fn visit_namespace(&mut self, namespace: &ast::Namespace) {
        let ns = self
            .resolver
            .globals
            .find_namespace(namespace.name.str_iter(), None)
            .expect("namespace should exist by this point");

        let kind = ScopeKind::Namespace(ns);
        self.with_scope(namespace.span, kind, |visitor| {
            for item in &namespace.items {
                ast_visit::walk_item(visitor, item);
            }
        });
    }
    fn visit_attr(&mut self, attr: &ast::Attr) {
        // The Config and EntryPoint attributes' arguments do not go through name resolution.
        if !matches!(
            hir::Attr::from_str(attr.name.name.as_ref()),
            Ok(hir::Attr::Config | hir::Attr::EntryPoint)
        ) {
            walk_attr(self, attr);
        }
    }

    fn visit_callable_decl(&mut self, decl: &CallableDecl) {
        fn collect_param_names(pat: &ast::Pat, names: &mut FxHashSet<Rc<str>>) {
            match &*pat.kind {
                ast::PatKind::Bind(name, _) => {
                    names.insert(Rc::clone(&name.name));
                }
                ast::PatKind::Discard(_) | ast::PatKind::Elided | ast::PatKind::Err => {}
                ast::PatKind::Paren(pat) => collect_param_names(pat, names),
                ast::PatKind::Tuple(pats) => {
                    pats.iter().for_each(|p| collect_param_names(p, names));
                }
            }
        }
        let mut param_names = FxHashSet::default();
        collect_param_names(&decl.input, &mut param_names);
        let prev_param_names = self.resolver.curr_params.replace(param_names);
        self.with_scope(decl.span, ScopeKind::Callable, |visitor| {
            visitor.resolver.bind_type_parameters(decl);
            // The parameter bindings are valid after the end of the input pattern.
            // (More accurately, in the callable body, but we don't have a start offset for that).
            visitor.resolver.bind_pat(&decl.input, decl.input.span.hi);
            ast_visit::walk_callable_decl(visitor, decl);
        });
        self.resolver.curr_params = prev_param_names;
    }

    fn visit_spec_decl(&mut self, decl: &ast::SpecDecl) {
        if let SpecBody::Impl(input, block) = &decl.body {
            self.with_spec_pat(block.span, ScopeKind::Block, input, |visitor| {
                visitor.visit_block(block);
            });
        } else {
            ast_visit::walk_spec_decl(self, decl);
        }
    }

    fn visit_ty(&mut self, ty: &ast::Ty) {
        match &*ty.kind {
            ast::TyKind::Path(PathKind::Ok(path)) => {
                if let Err(e) = self.resolver.resolve_path(NameKind::Ty, path) {
                    self.resolver.errors.push(e);
                }
            }
            ast::TyKind::Param(TypeParameter { ty, .. }) => {
                self.resolver.resolve_ident(NameKind::Ty, ty);
            }
            _ => ast_visit::walk_ty(self, ty),
        }
    }

    fn visit_block(&mut self, block: &ast::Block) {
        self.with_scope(block.span, ScopeKind::Block, |visitor| {
            // First, bind all declarations besides imports
            for stmt in &block.stmts {
                if let ast::StmtKind::Item(item) = &*stmt.kind {
                    visitor.resolver.bind_local_item(visitor.assigner, item);
                }
            }

            // Now resolve and bind imports as well
            imports::resolve_block_imports(visitor.resolver, block);

            ast_visit::walk_block(visitor, block);
        });
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match &*stmt.kind {
            ast::StmtKind::Item(item) => self.visit_item(item),
            ast::StmtKind::Local(_, pat, _) => {
                ast_visit::walk_stmt(self, stmt);
                // The binding is valid after end of the statement.
                self.resolver.bind_pat(pat, stmt.span.hi);
            }
            ast::StmtKind::Qubit(_, pat, init, block) => {
                ast_visit::walk_qubit_init(self, init);
                if let Some(block) = block {
                    self.with_pat(block.span, ScopeKind::Block, pat, |visitor| {
                        visitor.visit_block(block);
                    });
                } else {
                    // The binding is valid after end of the statement.
                    self.resolver.bind_pat(pat, stmt.span.hi);
                }
            }
            ast::StmtKind::Empty
            | ast::StmtKind::Expr(_)
            | ast::StmtKind::Semi(_)
            | ast::StmtKind::Err => {
                ast_visit::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match &*expr.kind {
            ast::ExprKind::For(pat, iter, block) => {
                self.visit_expr(iter);
                self.with_pat(block.span, ScopeKind::Block, pat, |visitor| {
                    visitor.visit_block(block);
                });
            }
            ast::ExprKind::Lambda(_, input, output) => {
                self.with_pat(output.span, ScopeKind::Block, input, |visitor| {
                    visitor.visit_expr(output);
                });
            }
            ast::ExprKind::Path(path) => {
                if let Err(e) = self.resolver.resolve_path_kind(NameKind::Term, path) {
                    self.resolver.errors.push(e);
                }
            }
            ast::ExprKind::TernOp(ast::TernOp::Update, container, index, replace)
            | ast::ExprKind::AssignUpdate(container, index, replace) => {
                self.visit_expr(container);
                if !is_field_update(
                    &self.resolver.globals,
                    self.resolver
                        .locals
                        .get_scopes(&self.resolver.curr_scope_chain),
                    index,
                ) {
                    self.visit_expr(index);
                }
                self.visit_expr(replace);
            }
            ast::ExprKind::Struct(PathKind::Ok(path), copy, fields) => {
                if let Err(e) = self.resolver.resolve_path(NameKind::Ty, path) {
                    self.resolver.errors.push(e);
                }
                if let Some(c) = copy {
                    self.visit_expr(c);
                }
                fields.iter().for_each(|f| self.visit_field_assign(f));
            }
            _ => ast_visit::walk_expr(self, expr),
        }
    }
}

pub(super) struct GlobalTable {
    names: Names,
    scope: GlobalScope,
}

impl GlobalTable {
    pub(super) fn new() -> Self {
        let builtins: [(Rc<str>, Res); 10] = [
            ("BigInt".into(), Res::PrimTy(Prim::BigInt)),
            ("Bool".into(), Res::PrimTy(Prim::Bool)),
            ("Double".into(), Res::PrimTy(Prim::Double)),
            ("Int".into(), Res::PrimTy(Prim::Int)),
            ("Pauli".into(), Res::PrimTy(Prim::Pauli)),
            ("Qubit".into(), Res::PrimTy(Prim::Qubit)),
            ("Range".into(), Res::PrimTy(Prim::Range)),
            ("Result".into(), Res::PrimTy(Prim::Result)),
            ("String".into(), Res::PrimTy(Prim::String)),
            ("Unit".into(), Res::UnitTy),
        ];
        let mut core: FxHashMap<Rc<str>, Res> = FxHashMap::default();
        for (name, res) in builtins {
            core.insert(name, res);
        }

        let mut scope = GlobalScope::default();
        let ns = scope.insert_or_find_namespace(vec![Rc::from("Std"), Rc::from("Core")], None);

        let mut tys = IndexMap::default();
        tys.insert(ns, core);

        Self {
            names: IndexMap::new(),
            scope: GlobalScope {
                tys,
                terms: IndexMap::default(),
                importables: IndexMap::default(),
                namespaces: scope.namespaces,
                intrinsics: FxHashSet::default(),
                self_exported_item_ids: FxHashMap::default(),
            },
        }
    }

    pub(super) fn add_local_package(
        &mut self,
        assigner: &mut Assigner,
        package: &ast::Package,
        package_id: PackageId,
    ) -> Vec<Error> {
        let mut errors = Vec::new();
        for node in &package.nodes {
            match node {
                TopLevelNode::Namespace(namespace) => {
                    bind_global_items(
                        package_id,
                        &mut self.names,
                        &mut self.scope,
                        namespace,
                        assigner,
                        &mut errors,
                    );
                }
                TopLevelNode::Stmt(_) => {
                    unimplemented!("did not expect top-level statements in the ast")
                }
            }
        }

        errors
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn add_external_package(
        &mut self,
        id: PackageId,
        package: &hir::Package,
        store: &crate::compile::PackageStore,
        alias: Option<&str>,
    ) {
        // if there is a package-level alias defined, use that for the root namespace.
        let package_root = alias.map(|alias| {
            self.scope
                .insert_or_find_namespace(vec![Rc::from(alias)], None)
        });

        for global in global::iter_package(id, package).filter(|global| {
            global.visibility == hir::Visibility::Public
                || matches!(&global.kind, global::Kind::Callable(t) if t.intrinsic)
        }) {
            // If the namespace is `Main` and we have an alias, we treat it as the root of the package, so there's no
            // namespace prefix between the dependency alias and the defined items.
            let global_namespace = if global.namespace.len() == 1
                && &*global.namespace[0] == "Main"
                && alias.is_some()
            {
                vec![]
            } else {
                global.namespace.clone()
            };

            let namespace = self
                .scope
                .insert_or_find_namespace(global_namespace.clone(), package_root);

            match (global.kind, global.visibility) {
                (global::Kind::Ty(ty), hir::Visibility::Public) => {
                    self.scope
                        .terms
                        .get_mut_or_default(namespace)
                        .insert(global.name.clone(), Res::Item(ty.id, global.status));
                    self.scope
                        .tys
                        .get_mut_or_default(namespace)
                        .insert(global.name.clone(), Res::Item(ty.id, global.status));
                    self.scope.importables.get_mut_or_default(namespace).insert(
                        global.name,
                        Res::Importable(Importable::Ty(ty.id, global.status)),
                    );
                }
                (global::Kind::Callable(term), visibility) => {
                    if visibility == hir::Visibility::Public {
                        self.scope
                            .terms
                            .get_mut_or_default(namespace)
                            .insert(global.name.clone(), Res::Item(term.id, global.status));

                        self.scope.importables.get_mut_or_default(namespace).insert(
                            global.name.clone(),
                            Res::Importable(Importable::Callable(term.id, global.status)),
                        );
                    }
                    if term.intrinsic {
                        self.scope.intrinsics.insert(global.name);
                    }
                }
                (global::Kind::Namespace(item_id), hir::Visibility::Public) => {
                    if package_root == Some(namespace) && global.name.as_ref() == "Main" {
                        // If the namespace is `Main` and we have an alias, we treat it as the root of the package,
                        // so there's no namespace prefix between the dependency alias and the defined items.
                        continue;
                    }
                    let this_namespace = self
                        .scope
                        .insert_or_find_namespace(vec![global.name.clone()], Some(namespace));

                    self.scope.importables.get_mut_or_default(namespace).insert(
                        global.name,
                        Res::Importable(Importable::Namespace(this_namespace, Some(item_id))),
                    );
                }
                (global::Kind::Export(item_id), _) => {
                    let Some(item) = find_item(store, item_id) else {
                        continue;
                    };
                    match item.kind {
                        hir::ItemKind::Callable(..) => {
                            self.scope
                                .terms
                                .get_mut_or_default(namespace)
                                .insert(global.name.clone(), Res::Item(item_id, global.status));
                            self.scope.importables.get_mut_or_default(namespace).insert(
                                global.name,
                                Res::Importable(Importable::Callable(item_id, global.status)),
                            );
                        }
                        hir::ItemKind::Namespace(original_name, _) => {
                            let orig_id = self.scope.insert_or_find_namespace(
                                original_name.0.iter().map(|i| i.name.clone()),
                                package_root,
                            );

                            self.scope
                                .insert_alias_for_namespace(orig_id, &global.name, namespace);

                            self.scope.importables.get_mut_or_default(namespace).insert(
                                global.name,
                                Res::Importable(Importable::Namespace(orig_id, Some(item_id))),
                            );
                        }
                        hir::ItemKind::Ty(..) => {
                            self.scope
                                .tys
                                .get_mut_or_default(namespace)
                                .insert(global.name.clone(), Res::Item(item_id, global.status));
                            self.scope
                                .terms
                                .get_mut_or_default(namespace)
                                .insert(global.name.clone(), Res::Item(item_id, global.status));
                            self.scope.importables.get_mut_or_default(namespace).insert(
                                global.name,
                                Res::Importable(Importable::Ty(item_id, global.status)),
                            );
                        }
                        hir::ItemKind::Export(_, _) => {
                            unreachable!("find_item will never return an Export")
                        }
                    }
                }
                (_, hir::Visibility::Internal) => {}
            }
        }
    }
}

fn find_item(store: &crate::compile::PackageStore, item: ItemId) -> Option<hir::Item> {
    let package = store.get(item.package)?;
    let item = package.package.items.get(item.item)?;
    match &item.kind {
        hir::ItemKind::Callable(_) | hir::ItemKind::Namespace(_, _) | hir::ItemKind::Ty(_, _) => {
            Some(item.clone())
        }
        hir::ItemKind::Export(_alias, hir::Res::Item(item)) => find_item(store, *item),
        hir::ItemKind::Export(_, _) => None,
    }
}

/// Given some namespace `namespace`, add all the globals declared within it to the global scope.
fn bind_global_items(
    package_id: PackageId,
    names: &mut IndexMap<NodeId, Res>,
    scope: &mut GlobalScope,
    namespace: &ast::Namespace,
    assigner: &mut Assigner,
    errors: &mut Vec<Error>,
) {
    let namespace_id = bind_namespace(package_id, namespace, assigner, names, scope);

    for item in &namespace.items {
        match bind_global_item(
            names,
            scope,
            namespace_id,
            || intrapackage(package_id, assigner.next_item()),
            item,
        ) {
            Ok(()) => {}
            Err(mut e) => errors.append(&mut e),
        }
    }
}

fn bind_namespace(
    package_id: PackageId,
    namespace: &ast::Namespace,
    assigner: &mut Assigner,
    names: &mut IndexMap<NodeId, Res>,
    scope: &mut GlobalScope,
) -> NamespaceId {
    let mut parent_id = scope.root_namespace();
    let mut namespace_id = scope.root_namespace();
    for part in namespace.name.rc_str_iter() {
        // Bind every part of a dotted namespace declaration as
        // an importable, so import/export items can reference it.
        parent_id = namespace_id;
        namespace_id = scope.insert_or_find_namespace(vec![part.clone()], Some(parent_id));

        if let Entry::Vacant(vacant_entry) = scope
            .importables
            .get_mut_or_default(parent_id)
            .entry(part.clone())
        {
            vacant_entry.insert(Res::Importable(Importable::Namespace(namespace_id, None)));
        }
    }

    let name = namespace
        .name
        .last()
        .expect("namespace name should contain at least one ident");

    let item_id = intrapackage(package_id, assigner.next_item());
    let res = Res::Item(item_id, ItemStatus::Available);
    scope.importables.get_mut_or_default(parent_id).insert(
        name.name.clone(),
        Res::Importable(Importable::Namespace(namespace_id, Some(item_id))),
    );
    names.insert(name.id, res);

    namespace_id
}

/// Tries to extract a field name from an expression in cases where it is syntactically ambiguous
/// whether the expression is a field name or a variable name. This applies to the index operand in
/// a ternary update operator.
pub(super) fn extract_field_name<'a>(names: &Names, expr: &'a ast::Expr) -> Option<&'a Rc<str>> {
    // Follow the same reasoning as `is_field_update`.
    match &*expr.kind {
        ast::ExprKind::Path(PathKind::Ok(path))
            if path.segments.is_none() && !matches!(names.get(path.id), Some(Res::Local(_))) =>
        {
            Some(&path.name.name)
        }
        _ => None,
    }
}

fn is_field_update<'a>(
    globals: &GlobalScope,
    scopes: impl Iterator<Item = &'a Scope>,
    index: &ast::Expr,
) -> bool {
    // Disambiguate the update operator by looking at the index expression. If it's an
    // unqualified path that doesn't resolve to a local, assume that it's meant to be a field name.
    match &*index.kind {
        ast::ExprKind::Path(PathKind::Ok(path)) if path.segments.is_none() => !matches!(
            {
                let name = &path.name;
                let namespace = &path.segments;
                resolve(NameKind::Term, globals, scopes, name, namespace.as_deref())
            },
            Ok(Res::Local(_))
        ),
        _ => false,
    }
}

fn ast_attrs_as_hir_attrs(attrs: &[Box<ast::Attr>]) -> Vec<hir::Attr> {
    attrs
        .iter()
        .filter_map(|attr| hir::Attr::from_str(attr.name.name.as_ref()).ok())
        .collect()
}

fn bind_global_item(
    names: &mut Names,
    scope: &mut GlobalScope,
    namespace: NamespaceId,
    next_id: impl FnMut() -> ItemId,
    item: &ast::Item,
) -> Result<(), Vec<Error>> {
    match &*item.kind {
        ast::ItemKind::Callable(decl) => {
            bind_callable(decl, namespace, next_id, item, names, scope)
        }
        ast::ItemKind::Ty(name, _) => bind_ty(name, namespace, next_id, item, names, scope),
        ast::ItemKind::Struct(decl) => bind_ty(&decl.name, namespace, next_id, item, names, scope),
        ast::ItemKind::ImportOrExport(decl) => {
            assign_item_ids(decl, next_id, names);
            Ok(())
        }
        ast::ItemKind::Err | ast::ItemKind::Open(..) => Ok(()),
    }
}

fn assign_item_ids(
    decl: &ImportOrExportDecl,
    mut next_id: impl FnMut() -> ItemId,
    names: &mut IndexMap<NodeId, Res>,
) {
    for item in iter_valid_items(decl) {
        if let ImportKind::Wildcard = item.kind {
            // don't bind any names for wildcard imports
            continue;
        }
        // Assign a new item id to this import, but don't bind the name yet
        names.insert(item.name().id, Res::Item(next_id(), ItemStatus::Available));
    }
}

fn bind_callable(
    decl: &CallableDecl,
    namespace: NamespaceId,
    next_id: impl FnOnce() -> ItemId,
    item: &ast::Item,
    names: &mut IndexMap<NodeId, Res>,
    scope: &mut GlobalScope,
) -> Result<(), Vec<Error>> {
    let item_id = next_id();
    let attrs = ast_attrs_as_hir_attrs(item.attrs.as_ref());
    let status = ItemStatus::from_attrs(&attrs);
    let res = Res::Item(item_id, status);
    let name = &decl.name;
    names.insert(name.id, res.clone());
    let mut errors = vec![];

    match (
        scope
            .terms
            .get_mut_or_default(namespace)
            .entry(Rc::clone(&name.name)),
        scope
            .importables
            .get_mut_or_default(namespace)
            .entry(Rc::clone(&name.name)),
    ) {
        (Entry::Occupied(_), _) | (_, Entry::Occupied(_)) => errors.push(Error::Duplicate(
            name.name.to_string(),
            scope.format_namespace_name(namespace),
            name.span,
        )),
        (Entry::Vacant(term_entry), Entry::Vacant(importable_entry)) => {
            term_entry.insert(res);
            importable_entry.insert(Res::Importable(Importable::Callable(item_id, status)));
        }
    }

    if decl_is_intrinsic(decl, &attrs) && !scope.intrinsics.insert(Rc::clone(&decl.name.name)) {
        errors.push(Error::DuplicateIntrinsic(
            decl.name.name.to_string(),
            decl.name.span,
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn bind_ty(
    name: &Ident,
    namespace: NamespaceId,
    next_id: impl FnOnce() -> ItemId,
    item: &ast::Item,
    names: &mut IndexMap<NodeId, Res>,
    scope: &mut GlobalScope,
) -> Result<(), Vec<Error>> {
    let item_id = next_id();
    let status = ItemStatus::from_attrs(&ast_attrs_as_hir_attrs(item.attrs.as_ref()));
    let res = Res::Item(item_id, status);
    names.insert(name.id, res.clone());
    match (
        scope
            .terms
            .get_mut_or_default(namespace)
            .entry(Rc::clone(&name.name)),
        scope
            .tys
            .get_mut_or_default(namespace)
            .entry(Rc::clone(&name.name)),
        scope
            .importables
            .get_mut_or_default(namespace)
            .entry(Rc::clone(&name.name)),
    ) {
        (Entry::Occupied(_), _, _) | (_, Entry::Occupied(_), _) | (_, _, Entry::Occupied(_)) => {
            Err(vec![Error::Duplicate(
                name.name.to_string(),
                scope.format_namespace_name(namespace),
                name.span,
            )])
        }
        (Entry::Vacant(term_entry), Entry::Vacant(ty_entry), Entry::Vacant(importable_entry)) => {
            term_entry.insert(res.clone());
            ty_entry.insert(res);
            importable_entry.insert(Res::Importable(Importable::Ty(item_id, status)));
            Ok(())
        }
    }
}

fn decl_is_intrinsic(decl: &CallableDecl, attrs: &[hir::Attr]) -> bool {
    if attrs
        .iter()
        .any(|attr| matches!(attr, hir::Attr::SimulatableIntrinsic))
    {
        return true;
    }
    if let CallableBody::Specs(specs) = decl.body.as_ref() {
        specs
            .iter()
            .any(|spec| matches!(spec.body, SpecBody::Gen(SpecGen::Intrinsic)))
    } else {
        false
    }
}

/// Resolves a given symbol and namespace name, according to the Q# shadowing rules.
/// Shadowing rules are as follows:
/// - Local variables shadow everything. They are the first priority.
/// - Next, we check open statements for a non-prelude open.
/// - Then, we check the prelude.
/// - Lastly, we check the global namespace.
///
/// In the example `Foo.Bar.Baz()` -- the `provided_namespace_name` would be
///`Foo.Bar` and the `provided_symbol_name` would be `Baz`.
///
/// In the example `Foo()` -- the `provided_namespace_name` would be `None` and the
/// `provided_symbol_name` would be `Foo`.
/// returns the resolution if successful, or an error if not.
fn resolve<'a>(
    kind: NameKind,
    globals: &GlobalScope,
    scopes: impl Iterator<Item = &'a Scope>,
    provided_symbol_name: &Ident,
    provided_namespace_name: Option<&[Ident]>,
) -> Result<Res, Error> {
    if let Some(value) = check_all_scopes(
        kind,
        globals,
        provided_symbol_name,
        provided_namespace_name,
        scopes,
    ) {
        return value;
    }

    // check the prelude
    if provided_namespace_name.is_none() {
        let prelude_candidates = find_symbol_in_namespaces(
            kind,
            globals,
            provided_namespace_name,
            provided_symbol_name,
            prelude_namespaces(globals).into_iter(),
            // prelude is opened by default
            &(std::iter::once((None, prelude_namespaces(globals))).collect()),
        );

        if prelude_candidates.len() > 1 {
            // If there are multiple candidates, sort them by namespace and return an error.
            let candidates: Vec<_> = prelude_candidates.into_iter().collect();
            let mut candidates = candidates
                .into_iter()
                .map(|(_candidate, ns_name)| ns_name)
                .collect::<Vec<_>>();
            candidates.sort();

            let mut candidates = candidates.into_iter();
            let candidate_a = candidates
                .next()
                .expect("infallible as per length check above");
            let candidate_b = candidates
                .next()
                .expect("infallible as per length check above");
            return Err(Error::AmbiguousPrelude {
                span: provided_symbol_name.span,
                name: provided_symbol_name.name.to_string(),
                candidate_a,
                candidate_b,
            });
        }
        // if there is a candidate, return it
        if let Some((res, _)) = single(prelude_candidates) {
            return Ok(res);
        }
    }

    // Lastly, check unopened globals. This is anything declared in the root namespace, which is
    // therefore globally available to the package
    let global_candidates = find_symbol_in_namespaces(
        kind,
        globals,
        provided_namespace_name,
        provided_symbol_name,
        std::iter::once((globals.root_namespace(), ())),
        // there are no aliases in globals
        &FxHashMap::default(),
    );

    // we don't have to throw an error if there are extra candidates here, as we are only looking at the root,
    // and that's only one namespace. individual namespaces cannot have duplicate declarations.
    if let Some(res) = single(global_candidates.into_keys()) {
        return Ok(res);
    }

    Err(match provided_namespace_name {
        Some(ns) => {
            let full_name = (ns, provided_symbol_name);
            Error::NotFound(full_name.full_name().to_string(), full_name.full_span())
        }
        None => Error::NotFound(
            provided_symbol_name.name.to_string(),
            provided_symbol_name.span,
        ),
    })
}
/// Checks all given scopes, in the correct order, for a resolution.
/// Calls `check_scoped_resolutions` on each scope, and tracks if we should allow local variables in closures in parent scopes
/// using the `vars` parameter.
fn check_all_scopes<'a>(
    kind: NameKind,
    globals: &GlobalScope,
    provided_symbol_name: &Ident,
    provided_namespace_name: Option<&[Ident]>,
    scopes: impl Iterator<Item = &'a Scope>,
) -> Option<Result<Res, Error>> {
    let mut vars = true;

    for scope in scopes {
        if let Some(value) = check_scoped_resolutions(
            kind,
            globals,
            provided_symbol_name,
            provided_namespace_name,
            &mut vars,
            scope,
        ) {
            return Some(value);
        }
    }
    None
}

/// This function checks scopes for a given symbol and namespace name.
/// In a given [Scope], check:
/// 1. if any locally declared symbols match `provided_symbol_name`
/// 2. if any aliases in this scope match the provided namespace, and if they contain `provided_symbol_name`
/// 3. if any opens in this scope contain the `provided_symbol_name`
///
/// It follows the Q# shadowing rules:
/// - Local variables shadow everything. They are the first priority.
/// - Next, we check open statements for an explicit open.
/// - Then, we check the prelude.
/// - Lastly, we check the global namespace.
///
/// # Parameters
///
/// * `kind` - The [`NameKind`] of the name
/// * `globals` - The global scope to resolve the name against.
/// * `provided_symbol_name` - The symbol name to resolve.
/// * `provided_namespace_name` - The namespace of the symbol, if any.
/// * `vars` - A mutable reference to a boolean indicating whether to allow local variables in closures in parent scopes.
/// * `scope` - The scope to check for resolutions.
fn check_scoped_resolutions(
    kind: NameKind,
    globals: &GlobalScope,
    provided_symbol_name: &Ident,
    provided_namespace_name: Option<&[Ident]>,
    vars: &mut bool,
    scope: &Scope,
) -> Option<Result<Res, Error>> {
    if provided_namespace_name.is_none()
        && let Some(res) =
            resolve_scope_locals(kind, globals, scope, *vars, &provided_symbol_name.name)
    {
        // Local declarations shadow everything.
        return Some(Ok(res));
    }

    let aliases = scope
        .opens
        .iter()
        .map(|(alias, opens)| {
            (
                alias.clone(),
                opens.iter().map(|x| (x.namespace, x)).collect(),
            )
        })
        .collect::<FxHashMap<_, _>>();

    let explicit_open_candidates = find_symbol_in_namespaces(
        kind,
        globals,
        provided_namespace_name,
        provided_symbol_name,
        scope
            .opens
            .iter()
            .flat_map(|(_, open)| open)
            .map(|open @ Open { namespace, .. }| (*namespace, open)),
        &aliases,
    );

    match explicit_open_candidates.len() {
        1 => {
            return Some(Ok(single(explicit_open_candidates.into_keys())
                .expect("we asserted on the length, so this is infallible")));
        }
        len if len > 1 => {
            return Some(Err(ambiguous_symbol_error(
                globals,
                provided_symbol_name,
                explicit_open_candidates
                    .into_iter()
                    .map(|(a, b)| (a, b.clone()))
                    .collect(),
            )));
        }
        _ => (),
    }
    if scope.kind == ScopeKind::Callable {
        // Since local callables are not closures, hide local variables in parent scopes.
        *vars = false;
    }
    None
}

/// This function returns type `Error::Ambiguous` and contains
/// the name of the ambiguous symbol and the namespaces that contain the conflicting entities.
/// # Arguments
///
/// * `globals` - The global scope to resolve the name against.
/// * `provided_symbol_name` - The symbol name that is ambiguous.
/// * `candidates` - A map of possible resolutions for the symbol, each associated with the `Open`
///   statement that brought it into scope. Note that only the first two opens in
///   the candidates are actually used in the error message.
fn ambiguous_symbol_error(
    globals: &GlobalScope,
    provided_symbol_name: &Ident,
    candidates: FxHashMap<Res, Open>,
) -> Error {
    let mut opens: Vec<_> = candidates.into_values().collect();
    opens.sort_unstable_by_key(|open| open.span);
    let first_open_ns = globals.format_namespace_name(opens[0].namespace);
    let second_open_ns = globals.format_namespace_name(opens[1].namespace);
    Error::Ambiguous {
        name: provided_symbol_name.name.to_string(),
        first_open: first_open_ns,
        second_open: second_open_ns,
        name_span: provided_symbol_name.span,
        first_open_span: opens[0].span,
        second_open_span: opens[1].span,
    }
}

fn find_symbol_in_namespaces<T, O>(
    kind: NameKind,
    globals: &GlobalScope,
    provided_namespace_name: Option<&[Ident]>,
    provided_symbol_name: &Ident,
    namespaces_to_search: T,
    aliases: &FxHashMap<Option<Rc<str>>, Vec<(NamespaceId, O)>>,
) -> FxHashMap<Res, O>
where
    T: Iterator<Item = (NamespaceId, O)>,
    O: Clone + std::fmt::Debug,
{
    let opens = match provided_namespace_name {
        None => aliases.get(&None),
        Some(namespace_name) => aliases.get(&namespace_name.iter().next().map(|x| x.name.clone())),
    };

    let mut candidates = FxHashMap::default();
    if let Some(opens) = opens {
        for open in opens {
            find_symbol_in_namespace(
                kind,
                globals,
                provided_namespace_name.as_ref().map(|x| &x[1..]),
                provided_symbol_name,
                &mut candidates,
                open.0,
                open.1.clone(),
            );
        }
        // check aliases to see if the provided namespace is actually an alias
        if provided_namespace_name.is_none() {
            candidates.extend(&mut opens.iter().filter_map(|(ns_id, open)| {
                globals
                    .get(kind, *ns_id, &provided_symbol_name.name)
                    .map(|res| (res.clone(), open.clone()))
            }));
        }
    }

    for (candidate_namespace_id, open) in namespaces_to_search {
        find_symbol_in_namespace(
            kind,
            globals,
            provided_namespace_name,
            provided_symbol_name,
            &mut candidates,
            candidate_namespace_id,
            open,
        );
    }

    if candidates.len() > 1 {
        // If there are multiple candidates, remove unimplemented items. This allows resolution to
        // succeed in cases where both an older, unimplemented API and newer, implemented API with the
        // same name are both in scope without forcing the user to fully qualify the name.
        candidates.retain(|res, _| !matches!(res, &Res::Item(_, ItemStatus::Unimplemented)));
    }
    candidates
}

fn find_symbol_in_namespace<O>(
    kind: NameKind,
    globals: &GlobalScope,
    provided_namespace_name: Option<&[Ident]>,
    provided_symbol_name: &Ident,
    candidates: &mut FxHashMap<Res, O>,
    candidate_namespace_id: NamespaceId,
    open: O,
) where
    O: Clone + std::fmt::Debug,
{
    let namespace = match provided_namespace_name {
        Some(provided_namespace_name) => {
            // Attempt to find a namespace within the candidate_namespace that matches the provided_namespace_name
            let namespace = globals.find_namespace(
                provided_namespace_name.str_iter(),
                Some(candidate_namespace_id),
            );
            match namespace {
                Some(ns) => ns,
                None => {
                    // if a namespace was provided, but not found, then this is not the correct namespace.
                    // for example, if the query is `Foo.Bar.Baz`, we know there must exist a `Foo.Bar` somewhere.
                    // If we didn't find it above, then even if we find `Baz` here, it is not the correct location.
                    return;
                }
            }
        }
        None => globals.root_namespace(),
    };

    let res = globals.get(kind, namespace, &provided_symbol_name.name);

    // If a symbol was found, insert it into the candidates map
    if let Some(res) = res {
        candidates.insert(res.clone(), open);
    }
}

/// Fetch the name and namespace ID of all prelude namespaces.
pub fn prelude_namespaces(globals: &GlobalScope) -> Vec<(NamespaceId, String)> {
    let mut prelude = Vec::with_capacity(PRELUDE.len());

    // add prelude to the list of candidate namespaces last, as they are the final fallback for a symbol
    for prelude_namespace in PRELUDE {
        // when evaluating the prelude namespaces themselves, they won't have been created yet. So we only
        // include the ids that have been created.
        if let Some(id) = globals.find_namespace(prelude_namespace, None) {
            prelude.push((id, prelude_namespace.join(".")));
        }
    }
    prelude
}
/// Implements shadowing rules within a single scope.
/// A local variable always wins out against an item with the same name, even if they're declared in
/// the same scope. It is implemented in a way that resembles Rust:
/// ```rust
/// let foo = || 1;
/// fn foo() -> i32 { 2 }
/// dbg!(foo()); // 1, not 2
/// ```
fn resolve_scope_locals(
    kind: NameKind,
    globals: &GlobalScope,
    scope: &Scope,
    vars: bool,
    name: &str,
) -> Option<Res> {
    if vars {
        match kind {
            NameKind::Term => {
                if let Some(&(_, id)) = scope.vars.get(name) {
                    return Some(Res::Local(id));
                }
            }
            NameKind::Ty => {
                if let Some((id, bounds)) = scope.ty_vars.get(name) {
                    return Some(Res::Param {
                        id: *id,
                        bounds: bounds.clone(),
                    });
                }
            }
            NameKind::Importable => {
                // local variables are not importable
            }
        }
    }

    if let Some(res) = scope.item(kind, name) {
        return Some(res);
    }

    if let ScopeKind::Namespace(namespace) = &scope.kind
        && let Some(res) = globals.get(kind, *namespace, name)
    {
        return Some(res.clone());
    }

    None
}

fn get_scope_locals(scope: &Scope, offset: u32, vars: bool) -> Vec<Local> {
    let mut names = Vec::new();

    // variables
    if vars {
        names.extend(scope.vars.iter().filter_map(|(name, (valid_at, id))| {
            // Bug: Because we keep track of only one `valid_at` offset per name,
            // when a variable is later shadowed in the same scope,
            // it is missed in the list. https://github.com/microsoft/qsharp/issues/897
            if offset >= *valid_at {
                Some(Local::Var(*id, name.clone()))
            } else {
                None
            }
        }));

        names.extend(
            scope
                .ty_vars
                .iter()
                .map(|(name, (id, _constraints))| Local::TyParam(*id, name.clone())),
        );
    }

    // items
    // gather from the terms table, since it happens to contain all the
    // declared and imported callables and newtypes
    names.extend(scope.terms.iter().filter_map(|(name, res)| {
        if let Res::Item(id, _) = res {
            Some(Local::Item(*id, name.clone()))
        } else {
            None
        }
    }));

    // opens and namespace imports
    names.extend(scope.opens.iter().flat_map(|(alias, opens)| {
        opens
            .iter()
            .map(|open| Local::NamespaceImport(open.namespace, alias.clone()))
    }));

    names
}

/// Creates an [`ItemId`] for an item that is local to this package (internal to it).
fn intrapackage(package: PackageId, item: LocalItemId) -> ItemId {
    ItemId { package, item }
}

fn single<T>(xs: impl IntoIterator<Item = T>) -> Option<T> {
    let mut xs = xs.into_iter();
    let x = xs.next();
    match xs.next() {
        None => x,
        Some(_) => None,
    }
}
