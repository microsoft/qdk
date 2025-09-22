// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The flattened intermediate representation for Q#. FIR is lowered from the HIR.
//! The blocks, exprs, pats, and stmts from HIR are replaced with IDs that index into
//! the corresponding lookups in the package. This allows for traversal without
//! leaking references to the FIR nodes.

#![warn(missing_docs)]

use crate::ty::{Arrow, FunctorSet, FunctorSetValue, GenericArg, Scheme, Ty, TypeParameter, Udt};
use indenter::{Indented, indented};
use num_bigint::BigInt;
use qsc_data_structures::{
    index_map::{IndexMap, Iter},
    span::Span,
};
use std::{
    cmp::Ordering,
    fmt::{self, Debug, Display, Formatter, Write},
    hash::{Hash, Hasher},
    ops,
    rc::Rc,
    result,
    str::FromStr,
};

fn set_indentation<'a, 'b>(
    indent: Indented<'a, Formatter<'b>>,
    level: usize,
) -> Indented<'a, Formatter<'b>> {
    match level {
        0 => indent.with_str(""),
        1 => indent.with_str("    "),
        2 => indent.with_str("        "),
        _ => unimplemented!("intentation level not supported"),
    }
}

/// A unique identifier for an FIR node.
#[derive(Clone, Copy, Debug)]
pub struct NodeId(u32);

impl NodeId {
    /// The ID of the first node.
    pub const FIRST: Self = Self(0);

    /// The successor of this ID.
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for NodeId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<NodeId> for usize {
    fn from(value: NodeId) -> Self {
        value.0 as usize
    }
}

impl From<usize> for NodeId {
    fn from(value: usize) -> Self {
        NodeId(
            value
                .try_into()
                .expect("Type Node ID does not fit into u32"),
        )
    }
}

impl From<NodeId> for u32 {
    fn from(value: NodeId) -> Self {
        value.0
    }
}

impl From<u32> for NodeId {
    fn from(value: u32) -> Self {
        NodeId(value)
    }
}

impl PartialEq for NodeId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for NodeId {}

impl PartialOrd for NodeId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NodeId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl Hash for NodeId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

macro_rules! fir_id {
    ($id:ident) => {
        /// A unique identifier for an FIR node.
        #[derive(Debug, Clone, Copy)]
        pub struct $id(pub u32);

        impl $id {
            /// The successor of this ID.
            #[must_use]
            pub fn successor(self) -> Self {
                Self(self.0 + 1)
            }
        }

        impl Default for $id {
            fn default() -> Self {
                Self(0)
            }
        }

        impl From<NodeId> for $id {
            fn from(val: NodeId) -> Self {
                $id(val.into())
            }
        }

        impl From<$id> for NodeId {
            fn from(val: $id) -> Self {
                NodeId(val.into())
            }
        }

        impl From<u32> for $id {
            fn from(val: u32) -> Self {
                $id(val)
            }
        }

        impl From<$id> for u32 {
            fn from(id: $id) -> Self {
                id.0
            }
        }

        impl From<$id> for usize {
            fn from(value: $id) -> Self {
                value.0 as usize
            }
        }

        impl From<usize> for $id {
            fn from(value: usize) -> Self {
                $id(value.try_into().expect(&format!(
                    "Value, {}, does not fit into {}",
                    value,
                    stringify!($id)
                )))
            }
        }

        impl PartialEq for $id {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl Eq for $id {}

        impl PartialOrd for $id {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $id {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.0.cmp(&other.0)
            }
        }

        impl std::hash::Hash for $id {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.0.hash(state);
            }
        }

        impl Display for $id {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                Display::fmt(&self.0, f)
            }
        }
    };
}

fir_id!(BlockId);
fir_id!(ExprId);
fir_id!(PatId);
fir_id!(StmtId);
fir_id!(LocalVarId);

/// A unique identifier for a package within a package store.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageId(usize);

impl PackageId {
    /// The package ID of the core library.
    pub const CORE: Self = Self(0);

    /// The successor of this ID.
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for PackageId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<PackageId> for usize {
    fn from(value: PackageId) -> Self {
        value.0
    }
}

impl From<usize> for PackageId {
    fn from(value: usize) -> Self {
        PackageId(value)
    }
}

/// A unique identifier for an item within a package.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalItemId(usize);

impl LocalItemId {
    /// The successor of this ID.
    #[must_use]
    pub fn successor(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for LocalItemId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<usize> for LocalItemId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<LocalItemId> for usize {
    fn from(value: LocalItemId) -> Self {
        value.0
    }
}

/// A unique identifier for an item within a package store.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemId {
    /// The package ID or `None` for the local package.
    pub package: Option<PackageId>,
    /// The item ID.
    pub item: LocalItemId,
}

impl Display for ItemId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.package {
            None => write!(f, "Item {}", self.item),
            Some(package) => write!(f, "Item {} (Package {package})", self.item),
        }
    }
}

/// A resolution. This connects a usage of a name with the declaration of that name by uniquely
/// identifying the node that declared it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum Res {
    /// An invalid resolution.
    Err,
    /// A global item.
    Item(ItemId),
    /// A local variable.
    Local(LocalVarId),
}

impl Display for Res {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Res::Err => f.write_str("Err"),
            Res::Item(item) => Display::fmt(item, f),
            Res::Local(node) => write!(f, "Local {node}"),
        }
    }
}

/// A global item.
pub enum Global<'a> {
    /// A global callable.
    Callable(&'a CallableDecl),
    /// A global user-defined type.
    Udt,
}

/// A unique identifier for an item within a package store.
#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq)]
pub struct StoreItemId {
    /// The package ID.
    pub package: PackageId,
    /// The item ID.
    pub item: LocalItemId,
}

impl StoreItemId {
    /// The item ID for the Complex type in the core library.
    #[must_use]
    pub fn complex() -> Self {
        Self {
            package: PackageId::CORE,
            item: LocalItemId(3),
        }
    }
}

impl Display for StoreItemId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "<item {} in package {}>", self.item, self.package)
    }
}

impl From<(PackageId, LocalItemId)> for StoreItemId {
    fn from(tuple: (PackageId, LocalItemId)) -> Self {
        Self {
            package: tuple.0,
            item: tuple.1,
        }
    }
}

/// A unique identifier for a block within a package store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoreBlockId {
    /// The package ID.
    pub package: PackageId,
    /// The item ID.
    pub block: BlockId,
}

impl Display for StoreBlockId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "<block {} in package {}>", self.block, self.package)
    }
}

impl From<(PackageId, BlockId)> for StoreBlockId {
    fn from(tuple: (PackageId, BlockId)) -> Self {
        Self {
            package: tuple.0,
            block: tuple.1,
        }
    }
}

/// A unique identifier for an expression within a package store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoreExprId {
    /// The package ID.
    pub package: PackageId,
    /// The expression ID.
    pub expr: ExprId,
}

impl Display for StoreExprId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "<expression {} in package {}>", self.expr, self.package)
    }
}

impl From<(PackageId, ExprId)> for StoreExprId {
    fn from(tuple: (PackageId, ExprId)) -> Self {
        Self {
            package: tuple.0,
            expr: tuple.1,
        }
    }
}

/// A unique identifier for a pattern within a package store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StorePatId {
    /// The package ID.
    pub package: PackageId,
    /// The pat ID.
    pub pat: PatId,
}

impl Display for StorePatId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "<pattern {} in package {}>", self.pat, self.package)
    }
}

impl From<(PackageId, PatId)> for StorePatId {
    fn from(tuple: (PackageId, PatId)) -> Self {
        Self {
            package: tuple.0,
            pat: tuple.1,
        }
    }
}

/// A unique identifier for a statement within a package store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoreStmtId {
    /// The package ID.
    pub package: PackageId,
    /// The statement ID.
    pub stmt: StmtId,
}

impl Display for StoreStmtId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "<pattern {} in package {}>", self.stmt, self.package)
    }
}

impl From<(PackageId, StmtId)> for StoreStmtId {
    fn from(tuple: (PackageId, StmtId)) -> Self {
        Self {
            package: tuple.0,
            stmt: tuple.1,
        }
    }
}

/// A trait to find elements in a package store.
pub trait PackageStoreLookup {
    /// Gets a block.
    fn get_block(&self, id: StoreBlockId) -> &Block;
    /// Gets an expression.
    fn get_expr(&self, id: StoreExprId) -> &Expr;
    /// Gets a global.
    fn get_global(&self, id: StoreItemId) -> Option<Global>;
    /// Gets a pat.
    fn get_pat(&self, id: StorePatId) -> &Pat;
    /// Gets a statement.
    fn get_stmt(&self, id: StoreStmtId) -> &Stmt;
    /// Gets an item.
    fn get_item(&self, id: StoreItemId) -> &Item;
}

/// A FIR package store.
#[derive(Debug, Default)]
pub struct PackageStore(IndexMap<PackageId, Package>);

impl PackageStoreLookup for PackageStore {
    fn get_block(&self, id: StoreBlockId) -> &Block {
        self.get(id.package).get_block(id.block)
    }

    fn get_expr(&self, id: StoreExprId) -> &Expr {
        self.get(id.package).get_expr(id.expr)
    }

    fn get_global(&self, id: StoreItemId) -> Option<Global> {
        self.get(id.package).get_global(id.item)
    }

    fn get_pat(&self, id: StorePatId) -> &Pat {
        self.get(id.package).get_pat(id.pat)
    }

    fn get_stmt(&self, id: StoreStmtId) -> &Stmt {
        self.get(id.package).get_stmt(id.stmt)
    }

    fn get_item(&self, id: StoreItemId) -> &Item {
        self.get(id.package).get_item(id.item)
    }
}

impl PackageStore {
    /// Gets a package from the store.
    #[must_use]
    pub fn get(&self, id: PackageId) -> &Package {
        self.0.get(id).expect("store should have package")
    }

    /// Gets a mutable package from the store.
    #[must_use]
    pub fn get_mut(&mut self, id: PackageId) -> &mut Package {
        self.0.get_mut(id).expect("store should have package")
    }

    /// Inserts a package to the store.
    pub fn insert(&mut self, id: PackageId, package: Package) {
        self.0.insert(id, package);
    }

    /// Gets a package store iterator.
    #[must_use]
    pub fn iter(&self) -> Iter<PackageId, Package> {
        self.0.iter()
    }

    /// Creates a package store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> IntoIterator for &'a PackageStore {
    type IntoIter = qsc_data_structures::index_map::Iter<'a, PackageId, Package>;
    type Item = (PackageId, &'a Package);

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A trait to find elements in a package.
pub trait PackageLookup {
    /// Gets a block.
    fn get_block(&self, id: BlockId) -> &Block;
    /// Gets an expression.
    fn get_expr(&self, id: ExprId) -> &Expr;
    /// Gets a global.
    fn get_global(&self, id: LocalItemId) -> Option<Global>;
    /// Gets an item.
    fn get_item(&self, id: LocalItemId) -> &Item;
    /// Gets a pat.
    fn get_pat(&self, id: PatId) -> &Pat;
    /// Gets a statement.
    fn get_stmt(&self, id: StmtId) -> &Stmt;
}

/// The root node of the FIR.
/// ### Notes
/// We maintain a dense map of ids within the package.
/// `BlockId`, `ExprId`, `PatId`, `StmtId`, and `NodeId`s are all assigned
/// from a type specific counter in the assigner.
///
/// `BlockId`, `ExprId`, `PatId`, `StmtId` ids don't leak and are only used
/// within the containing node. Node ids are used to identify nodes within
/// the package and require mapping from the HIR node id to the new FIR node id.
/// `PackageId`s and `LocalItemId`s are 1:1 from the HIR and are not remapped.
#[derive(Debug)]
pub struct Package {
    /// The items in the package.
    pub items: IndexMap<LocalItemId, Item>,
    /// The entry expression for an executable package.
    pub entry: Option<ExprId>,
    /// The control flow graph for the entry expression in the package.
    pub entry_exec_graph: ExecGraph,
    /// The blocks in the package.
    pub blocks: IndexMap<BlockId, Block>,
    /// The expressions in the package.
    pub exprs: IndexMap<ExprId, Expr>,
    /// The patterns in the package.
    pub pats: IndexMap<PatId, Pat>,
    /// The statements in the package.
    pub stmts: IndexMap<StmtId, Stmt>,
}

impl Display for Package {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "Package:")?;
        indent = set_indentation(indent, 1);
        if let Some(e) = &self.entry {
            write!(indent, "\nEntry Expression: {e}")?;
        }

        write!(indent, "\nItems:")?;
        indent = set_indentation(indent, 2);
        for item in self.items.values() {
            write!(indent, "\n{item}")?;
        }

        indent = set_indentation(indent, 1);
        write!(indent, "\nBlocks:")?;
        indent = set_indentation(indent, 2);
        for block in self.blocks.values() {
            write!(indent, "\n{block}")?;
        }

        indent = set_indentation(indent, 1);
        write!(indent, "\nStmts:")?;
        indent = set_indentation(indent, 2);
        for stmt in self.stmts.values() {
            write!(indent, "\n{stmt}")?;
        }

        indent = set_indentation(indent, 1);
        write!(indent, "\nExprs:")?;
        indent = set_indentation(indent, 2);
        for expr in self.exprs.values() {
            write!(indent, "\n{expr}")?;
        }

        indent = set_indentation(indent, 1);
        write!(indent, "\nPats:")?;
        indent = set_indentation(indent, 2);
        for pat in self.pats.values() {
            write!(indent, "\n{pat}")?;
        }
        Ok(())
    }
}

impl PackageLookup for Package {
    fn get_block(&self, id: BlockId) -> &Block {
        self.blocks.get(id).expect("Block not found")
    }

    fn get_expr(&self, id: ExprId) -> &Expr {
        self.exprs.get(id).expect("Expression not found")
    }

    fn get_global(&self, id: LocalItemId) -> Option<Global> {
        match &self.items.get(id)?.kind {
            ItemKind::Callable(callable) => Some(Global::Callable(callable)),
            ItemKind::Namespace(..) => None,
            ItemKind::Ty(..) => Some(Global::Udt),
            ItemKind::Export(_name, _id) => None,
        }
    }

    fn get_item(&self, id: LocalItemId) -> &Item {
        self.items.get(id).expect("Item not found")
    }

    fn get_pat(&self, id: PatId) -> &Pat {
        self.pats.get(id).expect("Pattern not found")
    }

    fn get_stmt(&self, id: StmtId) -> &Stmt {
        self.stmts.get(id).expect("Statement not found")
    }
}

/// An item.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The ID.
    pub id: LocalItemId,
    /// The span.
    pub span: Span,
    /// The parent item.
    pub parent: Option<LocalItemId>,
    /// The documentation.
    pub doc: Rc<str>,
    /// The attributes.
    pub attrs: Vec<Attr>,
    /// The visibility.
    pub visibility: Visibility,
    /// The item kind.
    pub kind: ItemKind,
}

impl Display for Item {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(
            indent,
            "Item {} {} ({:?}):",
            self.id, self.span, self.visibility
        )?;

        indent = set_indentation(indent, 1);
        if let Some(parent) = self.parent {
            write!(indent, "\nParent: {parent}")?;
        }

        if !self.doc.is_empty() {
            write!(indent, "\nDoc:")?;
            indent = set_indentation(indent, 2);
            write!(indent, "\n{}", self.doc)?;
            indent = set_indentation(indent, 1);
        }

        for attr in &self.attrs {
            write!(indent, "\n{attr:?}")?;
        }

        write!(indent, "\n{}", self.kind)?;
        Ok(())
    }
}

/// An item kind.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    /// A `function` or `operation` declaration.
    Callable(CallableDecl),
    /// A `namespace` declaration.
    Namespace(Ident, Vec<LocalItemId>),
    /// A `newtype` declaration.
    Ty(Ident, Udt),
    /// An export referring to another item
    Export(Ident, Res),
}

impl Display for ItemKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            ItemKind::Callable(decl) => write!(f, "{decl}"),
            ItemKind::Namespace(name, items) => {
                write!(f, "Namespace ({name}):")?;
                let mut items = items.iter();
                if let Some(item) = items.next() {
                    write!(f, " Item {item}")?;
                    for item in items {
                        write!(f, ", Item {item}")?;
                    }
                    Ok(())
                } else {
                    write!(f, " <empty>")
                }
            }
            ItemKind::Ty(name, udt) => write!(f, "Type ({name}): {udt}"),
            ItemKind::Export(name, item) => write!(f, "Export ({name}): {item}"),
        }
    }
}

/// A callable declaration header.
#[derive(Clone, Debug, PartialEq)]
pub struct CallableDecl {
    /// The node ID.
    pub id: NodeId,
    /// The span.
    pub span: Span,
    /// The callable kind.
    pub kind: CallableKind,
    /// The name of the callable.
    pub name: Ident,
    /// The generic parameters to the callable.
    pub generics: Vec<TypeParameter>,
    /// The input to the callable.
    pub input: PatId,
    /// The return type of the callable.
    pub output: Ty,
    /// The functors supported by the callable.
    pub functors: FunctorSetValue,
    /// The callable implementation.
    pub implementation: CallableImpl,
    /// The attributes of the callable, (e.g.: Measurement or Reset).
    pub attrs: Vec<Attr>,
}

impl CallableDecl {
    /// The type scheme of the callable.
    #[must_use]
    pub fn scheme<'a>(&self, f: impl Fn(PatId) -> &'a Pat) -> Scheme {
        Scheme::new(
            self.generics.clone(),
            Box::new(Arrow {
                kind: self.kind,
                input: Box::new(f(self.input).ty.clone()),
                output: Box::new(self.output.clone()),
                functors: FunctorSet::Value(self.functors),
            }),
        )
    }
}

impl Display for CallableDecl {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(
            indent,
            "Callable {} {} ({}):",
            self.id, self.span, self.kind
        )?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nname: {}", self.name)?;
        if !self.generics.is_empty() {
            write!(indent, "\ngenerics:")?;
            indent = set_indentation(indent, 2);
            for (ix, param) in self.generics.iter().enumerate() {
                write!(indent, "\n{ix}: {param}")?;
            }
            indent = set_indentation(indent, 1);
        }
        write!(indent, "\ninput: {}", self.input)?;
        write!(indent, "\noutput: {}", self.output)?;
        write!(indent, "\nfunctors: {}", self.functors)?;
        write!(indent, "\nimplementation: {}", self.implementation)?;
        Ok(())
    }
}

/// A callable implementations.
#[derive(Clone, Debug, PartialEq)]
pub enum CallableImpl {
    /// An intrinsic callable implementation.
    Intrinsic,
    /// A specialized callable implementation.
    Spec(SpecImpl),
    /// An intrinsic with a simulation override.
    SimulatableIntrinsic(SpecDecl),
}

impl Display for CallableImpl {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        match self {
            CallableImpl::Intrinsic => {
                write!(indent, "Instrinsic")?;
            }
            CallableImpl::Spec(spec_impl) => {
                write!(indent, "Spec:")?;
                indent = set_indentation(indent, 1);
                write!(indent, "\n{spec_impl}")?;
            }
            CallableImpl::SimulatableIntrinsic(spec_decl) => {
                write!(indent, "SimulatableIntrinsic:")?;
                indent = set_indentation(indent, 1);
                write!(indent, "\n{spec_decl}")?;
            }
        }

        Ok(())
    }
}

/// A specialized implementation.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecImpl {
    /// The body implementation.
    pub body: SpecDecl,
    /// The adjoint specialization.
    pub adj: Option<SpecDecl>,
    /// The controlled specialization.
    pub ctl: Option<SpecDecl>,
    /// The controlled adjoint specialization.
    pub ctl_adj: Option<SpecDecl>,
}

impl Display for SpecImpl {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "SpecImpl:")?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nbody: {}", self.body)?;
        match &self.adj {
            Some(spec) => write!(indent, "\nadj: {spec}")?,
            None => write!(indent, "\nadj: <none>")?,
        }
        match &self.ctl {
            Some(spec) => write!(indent, "\nctl: {spec}")?,
            None => write!(indent, "\nctl: <none>")?,
        }
        match &self.ctl_adj {
            Some(spec) => write!(indent, "\nctl-adj: {spec}")?,
            None => write!(indent, "\nctl-adj: <none>")?,
        }
        Ok(())
    }
}

/// A specialization declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecDecl {
    /// The node ID.
    pub id: NodeId,
    /// The span.
    pub span: Span,
    /// The block that implements the specialization.
    pub block: BlockId,
    /// The input of the specialization.
    pub input: Option<PatId>,
    /// The flattened control flow graph for the execution of the specialization.
    pub exec_graph: ExecGraph,
}

impl Display for SpecDecl {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SpecDecl {} {}: {:?} {}",
            self.id, self.span, self.input, self.block
        )
    }
}

/// An execution graph represented by a reference counted vector of nodes.
pub type ExecGraph = Rc<[ExecGraphNode]>;

#[derive(Copy, Clone, Debug, PartialEq)]
/// A node within the control flow graph.
pub enum ExecGraphNode {
    /// A binding of a value to a variable.
    Bind(PatId),
    /// An expression to execute.
    Expr(ExprId),
    /// An unconditional jump with to given location.
    Jump(u32),
    /// A conditional jump with to given location, where the jump is only taken if the condition is
    /// true.
    JumpIf(u32),
    /// A conditional jump with to given location, where the jump is only taken if the condition is
    /// false.
    JumpIfNot(u32),
    /// An indication that the current accumulated result value should be stored into the value stack.
    Store,
    /// A no-op Unit node that tells execution to insert a unit value into the current accumulated result.
    Unit,
    /// The end of the control flow graph.
    Ret,
    /// The end of the control flow graph plus a pop of the current debug frame. Used instead of `Ret`
    /// when debugging.
    RetFrame,
    /// A statement to track for debugging.
    Stmt(StmtId),
    /// A push of a new scope, used when tracking variables for debugging.
    PushScope,
    /// A pop of the current scope, used when tracking variables for debugging.
    PopScope,
    /// The end of a block, used in debugging to have a step point after all statements in a block have been executed,
    /// but before the block is exited.
    BlockEnd(BlockId),
}

/// A sequenced block of statements.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The node ID.
    pub id: BlockId,
    /// The span.
    pub span: Span,
    /// The block type.
    pub ty: Ty,
    /// The statements in the block.
    pub stmts: Vec<StmtId>,
}

impl Display for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.stmts.is_empty() {
            write!(f, "Block {} {}: <empty>", self.id, self.span)?;
        } else {
            let mut indent = set_indentation(indented(f), 0);
            write!(
                indent,
                "Block {} {} [Type {}]:",
                self.id, self.span, self.ty
            )?;
            indent = set_indentation(indent, 1);
            for s in &self.stmts {
                write!(indent, "\n{s}")?;
            }
        }
        Ok(())
    }
}

/// A statement.
#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    /// The stmt ID.
    pub id: StmtId,
    /// The span.
    pub span: Span,
    /// The statement kind.
    pub kind: StmtKind,
    /// The locations within the containing control flow graph for the current statement.
    pub exec_graph_range: ops::Range<usize>,
}

impl Display for Stmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Stmt {} {}: {}", self.id, self.span, self.kind)
    }
}

/// A statement kind.
#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    /// An expression without a trailing semicolon.
    Expr(ExprId),
    /// An item.
    Item(LocalItemId),
    /// A let or mutable binding: `let a = b;` or `mutable x = b;`.
    Local(Mutability, PatId, ExprId),
    /// An expression with a trailing semicolon.
    Semi(ExprId),
}

impl Display for StmtKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        match self {
            StmtKind::Expr(e) => write!(indent, "Expr: {e}")?,
            StmtKind::Item(item) => write!(indent, "Item: {item}")?,
            StmtKind::Local(m, lhs, rhs) => {
                write!(indent, "Local ({m:?}):")?;
                indent = set_indentation(indent, 1);
                write!(indent, "\n{lhs}")?;
                write!(indent, "\n{rhs}")?;
            }
            StmtKind::Semi(e) => write!(indent, "Semi: {e}")?,
        }
        Ok(())
    }
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// The expr ID.
    pub id: ExprId,
    /// The span.
    pub span: Span,
    /// The expression type.
    pub ty: Ty,
    /// The expression kind.
    pub kind: ExprKind,
    /// The locations within the containing control flow graph for the current expression.
    pub exec_graph_range: ops::Range<usize>,
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Expr {} {} [Type {}]: {}",
            self.id, self.span, self.ty, self.kind
        )
    }
}

/// An expression kind.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// An array: `[a, b, c]`.
    Array(Vec<ExprId>),
    /// An array of literal values, ie: `[1, 2, 3]`.
    ArrayLit(Vec<ExprId>),
    /// An array constructed by repeating a value: `[a, size = b]`.
    ArrayRepeat(ExprId, ExprId),
    /// An assignment: `set a = b`.
    Assign(ExprId, ExprId),
    /// An assignment with a compound operator. For example: `set a += b`.
    AssignOp(BinOp, ExprId, ExprId),
    /// An assignment with a compound field update operator: `set a w/= B <- c`.
    AssignField(ExprId, Field, ExprId),
    /// An assignment with a compound index update operator: `set a w/= b <- c`.
    AssignIndex(ExprId, ExprId, ExprId),
    /// A binary operator.
    BinOp(BinOp, ExprId, ExprId),
    /// A block: `{ ... }`.
    Block(BlockId),
    /// A call: `a(b)`.
    Call(ExprId, ExprId),
    /// A closure that fixes the vector of local variables as arguments to the callable item.
    Closure(Vec<LocalVarId>, LocalItemId),
    /// A failure: `fail "message"`.
    Fail(ExprId),
    /// A field accessor: `a::F` or `a.F`.
    Field(ExprId, Field),
    /// An unspecified expression, _, which may indicate partial application or discards
    Hole,
    /// An if expression with an optional else block: `if a { ... } else { ... }`.
    ///
    /// Note that, as a special case, `elif ...` is effectively parsed as `else if ...`, without a
    /// block wrapping the `if`. This distinguishes `elif ...` from `else { if ... }`, which does
    /// have a block.
    If(ExprId, ExprId, Option<ExprId>),
    /// An index accessor: `a[b]`.
    Index(ExprId, ExprId),
    /// A literal.
    Lit(Lit),
    /// A range: `start..step..end`, `start..end`, `start...`, `...end`, or `...`.
    Range(Option<ExprId>, Option<ExprId>, Option<ExprId>),
    /// A return: `return a`.
    Return(ExprId),
    /// A struct constructor.
    Struct(Res, Option<ExprId>, Vec<FieldAssign>),
    /// A string.
    String(Vec<StringComponent>),
    /// Update array index: `a w/ b <- c`.
    UpdateIndex(ExprId, ExprId, ExprId),
    /// A tuple: `(a, b, c)`.
    Tuple(Vec<ExprId>),
    /// A unary operator.
    UnOp(UnOp, ExprId),
    /// A record field update: `a w/ B <- c`.
    UpdateField(ExprId, Field, ExprId),
    /// A variable and its generic arguments.
    Var(Res, Vec<GenericArg>),
    /// A while loop: `while a { ... }`.
    While(ExprId, BlockId),
}

impl Display for ExprKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        match self {
            ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) => display_array(indent, exprs)?,
            ExprKind::ArrayRepeat(val, size) => display_array_repeat(indent, *val, *size)?,
            ExprKind::Assign(lhs, rhs) => display_assign(indent, *lhs, *rhs)?,
            ExprKind::AssignOp(op, lhs, rhs) => display_assign_op(indent, *op, *lhs, *rhs)?,
            ExprKind::AssignField(record, field, replace) => {
                display_assign_field(indent, *record, field, *replace)?;
            }
            ExprKind::AssignIndex(container, item, replace) => {
                display_assign_index(indent, *container, *item, *replace)?;
            }
            ExprKind::BinOp(op, lhs, rhs) => display_bin_op(indent, *op, *lhs, *rhs)?,
            ExprKind::Block(block) => write!(indent, "Expr Block: {block}")?,
            ExprKind::Call(callable, arg) => display_call(indent, *callable, *arg)?,
            ExprKind::Closure(args, callable) => display_closure(indent, args, *callable)?,
            ExprKind::Fail(e) => write!(indent, "Fail: {e}")?,
            ExprKind::Field(expr, field) => display_field(indent, *expr, field)?,
            ExprKind::Hole => write!(indent, "Hole")?,
            ExprKind::If(cond, body, els) => display_if(indent, *cond, *body, *els)?,
            ExprKind::Index(array, index) => display_index(indent, *array, *index)?,
            ExprKind::Lit(lit) => write!(indent, "Lit: {lit}")?,
            ExprKind::Range(start, step, end) => display_range(indent, *start, *step, *end)?,
            ExprKind::Return(e) => write!(indent, "Return: {e}")?,
            ExprKind::Struct(name, copy, fields) => display_struct(indent, name, *copy, fields)?,
            ExprKind::String(components) => display_string(indent, components)?,
            ExprKind::UpdateIndex(expr1, expr2, expr3) => {
                display_update_index(indent, *expr1, *expr2, *expr3)?;
            }
            ExprKind::Tuple(exprs) => display_tuple(indent, exprs)?,
            ExprKind::UnOp(op, expr) => display_un_op(indent, *op, *expr)?,
            ExprKind::UpdateField(record, field, replace) => {
                display_update_field(indent, *record, field, *replace)?;
            }
            ExprKind::Var(res, args) => display_var(indent, *res, args)?,
            ExprKind::While(cond, block) => display_while(indent, *cond, *block)?,
        }
        Ok(())
    }
}

fn display_array(mut indent: Indented<Formatter>, exprs: &Vec<ExprId>) -> fmt::Result {
    write!(indent, "Array:")?;
    indent = set_indentation(indent, 1);
    for e in exprs {
        write!(indent, "\n{e}")?;
    }
    Ok(())
}

fn display_array_repeat(mut indent: Indented<Formatter>, val: ExprId, size: ExprId) -> fmt::Result {
    write!(indent, "ArrayRepeat:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{val}")?;
    write!(indent, "\n{size}")?;
    Ok(())
}

fn display_assign(mut indent: Indented<Formatter>, lhs: ExprId, rhs: ExprId) -> fmt::Result {
    write!(indent, "Assign:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{lhs}")?;
    write!(indent, "\n{rhs}")?;
    Ok(())
}

fn display_assign_op(
    mut indent: Indented<Formatter>,
    op: BinOp,
    lhs: ExprId,
    rhs: ExprId,
) -> fmt::Result {
    write!(indent, "AssignOp ({op:?}):")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{lhs}")?;
    write!(indent, "\n{rhs}")?;
    Ok(())
}

fn display_assign_field(
    mut indent: Indented<Formatter>,
    record: ExprId,
    field: &Field,
    replace: ExprId,
) -> fmt::Result {
    write!(indent, "AssignField:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{record}")?;
    write!(indent, "\n{field}")?;
    write!(indent, "\n{replace}")?;
    Ok(())
}

fn display_assign_index(
    mut indent: Indented<Formatter>,
    array: ExprId,
    index: ExprId,
    replace: ExprId,
) -> fmt::Result {
    write!(indent, "AssignIndex:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{array}")?;
    write!(indent, "\n{index}")?;
    write!(indent, "\n{replace}")?;
    Ok(())
}

fn display_bin_op(
    mut indent: Indented<Formatter>,
    op: BinOp,
    lhs: ExprId,
    rhs: ExprId,
) -> fmt::Result {
    write!(indent, "BinOp ({op:?}):")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{lhs}")?;
    write!(indent, "\n{rhs}")?;
    Ok(())
}

fn display_call(mut indent: Indented<Formatter>, callable: ExprId, arg: ExprId) -> fmt::Result {
    write!(indent, "Call:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{callable}")?;
    write!(indent, "\n{arg}")?;
    Ok(())
}

fn display_closure(
    mut f: Indented<Formatter>,
    args: &[LocalVarId],
    callable: LocalItemId,
) -> fmt::Result {
    f.write_str("Closure([")?;
    let mut args = args.iter();
    if let Some(arg) = args.next() {
        write!(f, "{arg}")?;
    }
    for arg in args {
        write!(f, ", {arg}")?;
    }
    write!(f, "], {callable})")
}

fn display_field(mut indent: Indented<Formatter>, expr: ExprId, field: &Field) -> fmt::Result {
    write!(indent, "Field:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{expr}")?;
    write!(indent, "\n{field:?}")?;
    Ok(())
}

fn display_if(
    mut indent: Indented<Formatter>,
    cond: ExprId,
    body: ExprId,
    els: Option<ExprId>,
) -> fmt::Result {
    write!(indent, "If:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{cond}")?;
    write!(indent, "\n{body}")?;
    if let Some(e) = els {
        write!(indent, "\n{e}")?;
    }
    Ok(())
}

fn display_index(mut indent: Indented<Formatter>, array: ExprId, index: ExprId) -> fmt::Result {
    write!(indent, "Index:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{array}")?;
    write!(indent, "\n{index}")?;
    Ok(())
}

fn display_range(
    mut indent: Indented<Formatter>,
    start: Option<ExprId>,
    step: Option<ExprId>,
    end: Option<ExprId>,
) -> fmt::Result {
    write!(indent, "Range:")?;
    indent = set_indentation(indent, 1);
    match start {
        Some(e) => write!(indent, "\n{e}")?,
        None => write!(indent, "\n<no start>")?,
    }
    match step {
        Some(e) => write!(indent, "\n{e}")?,
        None => write!(indent, "\n<no step>")?,
    }
    match end {
        Some(e) => write!(indent, "\n{e}")?,
        None => write!(indent, "\n<no end>")?,
    }
    Ok(())
}

fn display_struct(
    mut indent: Indented<Formatter>,
    name: &Res,
    copy: Option<ExprId>,
    fields: &Vec<FieldAssign>,
) -> fmt::Result {
    write!(indent, "Struct ({name}):")?;
    if copy.is_none() && fields.is_empty() {
        write!(indent, " <empty>")?;
        return Ok(());
    }
    indent = set_indentation(indent, 1);
    if let Some(copy) = copy {
        write!(indent, "\nCopy: {copy}")?;
    }
    for field in fields {
        write!(indent, "\n{field}")?;
    }
    Ok(())
}

fn display_string(mut indent: Indented<Formatter>, components: &[StringComponent]) -> fmt::Result {
    write!(indent, "String:")?;
    indent = set_indentation(indent, 1);
    for component in components {
        match component {
            StringComponent::Expr(expr) => write!(indent, "\nExpr: {expr}")?,
            StringComponent::Lit(str) => write!(indent, "\nLit: {str:?}")?,
        }
    }

    Ok(())
}

fn display_update_index(
    mut indent: Indented<Formatter>,
    expr1: ExprId,
    expr2: ExprId,
    expr3: ExprId,
) -> fmt::Result {
    write!(indent, "UpdateIndex:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{expr1}")?;
    write!(indent, "\n{expr2}")?;
    write!(indent, "\n{expr3}")?;
    Ok(())
}

fn display_tuple(mut indent: Indented<Formatter>, exprs: &Vec<ExprId>) -> fmt::Result {
    if exprs.is_empty() {
        write!(indent, "Unit")?;
    } else {
        write!(indent, "Tuple:")?;
        indent = set_indentation(indent, 1);
        for e in exprs {
            write!(indent, "\n{e}")?;
        }
    }
    Ok(())
}

fn display_un_op(mut indent: Indented<Formatter>, op: UnOp, expr: ExprId) -> fmt::Result {
    write!(indent, "UnOp ({op}):")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{expr}")?;
    Ok(())
}

fn display_update_field(
    mut indent: Indented<Formatter>,
    record: ExprId,
    field: &Field,
    replace: ExprId,
) -> fmt::Result {
    write!(indent, "UpdateField:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{record}")?;
    write!(indent, "\n{field}")?;
    write!(indent, "\n{replace}")?;
    Ok(())
}

fn display_var(mut f: Indented<Formatter>, res: Res, args: &[GenericArg]) -> fmt::Result {
    if args.is_empty() {
        write!(f, "Var: {res}")
    } else {
        write!(f, "Var:")?;
        f = set_indentation(f, 1);
        write!(f, "\nres: {res}")?;
        write!(f, "\ngenerics:")?;
        f = set_indentation(f, 2);
        for arg in args {
            write!(f, "\n{arg}")?;
        }
        Ok(())
    }
}

fn display_while(mut indent: Indented<Formatter>, cond: ExprId, block: BlockId) -> fmt::Result {
    write!(indent, "While:")?;
    indent = set_indentation(indent, 1);
    write!(indent, "\n{cond}")?;
    write!(indent, "\n{block}")?;
    Ok(())
}

/// A field assignment in a struct constructor expression.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldAssign {
    /// The node ID.
    pub id: NodeId,
    /// The span.
    pub span: Span,
    /// The field to assign.
    pub field: Field,
    /// The value to assign to the field.
    pub value: ExprId,
}

impl Display for FieldAssign {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FieldsAssign {} {}: ({}) {}",
            self.id, self.span, self.field, self.value
        )
    }
}

/// A string component.
#[derive(Clone, Debug, PartialEq)]
pub enum StringComponent {
    /// An expression.
    Expr(ExprId),
    /// A string literal.
    Lit(Rc<str>),
}

/// A pattern.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pat {
    /// The node ID.
    pub id: PatId,
    /// The span.
    pub span: Span,
    /// The pattern type.
    pub ty: Ty,
    /// The pattern kind.
    pub kind: PatKind,
}

impl Display for Pat {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Pat {} {} [Type {}]: {}",
            self.id, self.span, self.ty, self.kind
        )
    }
}

/// A pattern kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatKind {
    /// A binding.
    Bind(Ident),
    /// A discarded binding, `_`.
    Discard,
    /// A tuple: `(a, b, c)`.
    Tuple(Vec<PatId>),
}

impl Display for PatKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        match self {
            PatKind::Bind(id) => {
                write!(indent, "Bind: {id}")?;
            }
            PatKind::Discard => write!(indent, "Discard")?,
            PatKind::Tuple(ps) => {
                if ps.is_empty() {
                    write!(indent, "Unit")?;
                } else {
                    write!(indent, "Tuple:")?;
                    indent = set_indentation(indent, 1);
                    for p in ps {
                        write!(indent, "\n{p}")?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// An identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Ident {
    /// The node ID.
    pub id: LocalVarId,
    /// The span.
    pub span: Span,
    /// The identifier name.
    pub name: Rc<str>,
}

impl Display for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Ident {} {} \"{}\"", self.id, self.span, self.name)
    }
}

/// An attribute.
#[derive(Clone, Debug, PartialEq)]
pub enum Attr {
    /// Indicates that a callable is an entry point to a program.
    EntryPoint,
    /// Indicates that a callable is a measurement.
    Measurement,
    /// Indicates that a callable is a reset.
    Reset,
    /// Indicates that a callable is used for unit testing.
    Test,
}

/// A field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Field {
    /// A field path.
    Path(FieldPath),
    /// A primitive field for a built-in type.
    Prim(PrimField),
    /// An invalid field.
    Err,
}

impl Display for Field {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Field::Path(path) => write!(f, "Path({:?})", path.indices),
            Field::Prim(prim) => write!(f, "Prim({prim:?}"),
            Field::Err => f.write_str("Err"),
        }
    }
}

/// A path to a field in a tuple or user-defined type.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct FieldPath {
    /// The tuple item indices to follow in order from top to bottom.
    pub indices: Vec<usize>,
}

/// A primitive field for a built-in type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PrimField {
    /// The start of a range.
    Start,
    /// The step of a range.
    Step,
    /// The end of a range.
    End,
}

impl FromStr for PrimField {
    type Err = ();

    fn from_str(s: &str) -> result::Result<Self, <Self as FromStr>::Err> {
        match s {
            "Start" => Ok(Self::Start),
            "Step" => Ok(Self::Step),
            "End" => Ok(Self::End),
            _ => Err(()),
        }
    }
}

/// The visibility of a declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Visibility {
    /// Visible everywhere.
    Public,
    /// Visible within a package.
    Internal,
}

/// A callable kind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum CallableKind {
    /// A function.
    Function,
    /// An operation.
    Operation,
}

impl Display for CallableKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            CallableKind::Function => f.write_str("function"),
            CallableKind::Operation => f.write_str("operation"),
        }
    }
}

/// The mutability of a binding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Mutability {
    /// An immutable binding.
    Immutable,
    /// A mutable binding.
    Mutable,
}

/// The source of an allocated qubit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QubitSource {
    /// A qubit initialized to the zero state.
    Fresh,
    /// A qubit borrowed from another part of the program that may be in any state, and is expected
    /// to be returned to that state before being released.
    Dirty,
}

/// A literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    /// A big integer literal.
    BigInt(BigInt),
    /// A boolean literal.
    Bool(bool),
    /// A floating-point literal.
    Double(f64),
    /// An integer literal.
    Int(i64),
    /// A Pauli operator literal.
    Pauli(Pauli),
    /// A measurement result literal.
    Result(Result),
}

impl Display for Lit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Lit::BigInt(val) => write!(f, "BigInt({val})")?,
            Lit::Bool(val) => write!(f, "Bool({val})")?,
            Lit::Double(val) => write!(f, "Double({val})")?,
            Lit::Int(val) => write!(f, "Int({val})")?,
            Lit::Pauli(val) => write!(f, "Pauli({val:?})")?,
            Lit::Result(val) => write!(f, "Result({val:?})")?,
        }
        Ok(())
    }
}

/// A measurement result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Result {
    /// The zero eigenvalue.
    Zero,
    /// The one eigenvalue.
    One,
}

/// A Pauli operator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Pauli {
    /// The Pauli I operator.
    I,
    /// The Pauli X operator.
    X,
    /// The Pauli Y operator.
    Y,
    /// The Pauli Z operator.
    Z,
}

/// A functor that may be applied to an operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Functor {
    /// The adjoint functor.
    Adj,
    /// The controlled functor.
    Ctl,
}

impl Display for Functor {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Functor::Adj => f.write_str("Adj"),
            Functor::Ctl => f.write_str("Ctl"),
        }
    }
}

/// A unary operator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnOp {
    /// A functor application.
    Functor(Functor),
    /// Negation: `-`.
    Neg,
    /// Bitwise NOT: `~~~`.
    NotB,
    /// Logical NOT: `not`.
    NotL,
    /// A leading `+`.
    Pos,
    /// Unwrap a user-defined type: `!`.
    Unwrap,
}

impl Display for UnOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            UnOp::Functor(func) => write!(f, "Functor {func:?}")?,
            _ => fmt::Debug::fmt(self, f)?,
        }
        Ok(())
    }
}

/// A binary operator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BinOp {
    /// Addition: `+`.
    Add,
    /// Bitwise AND: `&&&`.
    AndB,
    /// Logical AND: `and`.
    AndL,
    /// Division: `/`.
    Div,
    /// Equality: `==`.
    Eq,
    /// Exponentiation: `^`.
    Exp,
    /// Greater than: `>`.
    Gt,
    /// Greater than or equal: `>=`.
    Gte,
    /// Less than: `<`.
    Lt,
    /// Less than or equal: `<=`.
    Lte,
    /// Modulus: `%`.
    Mod,
    /// Multiplication: `*`.
    Mul,
    /// Inequality: `!=`.
    Neq,
    /// Bitwise OR: `|||`.
    OrB,
    /// Logical OR: `or`.
    OrL,
    /// Shift left: `<<<`.
    Shl,
    /// Shift right: `>>>`.
    Shr,
    /// Subtraction: `-`.
    Sub,
    /// Bitwise XOR: `^^^`.
    XorB,
}
