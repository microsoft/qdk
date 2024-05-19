// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use std::{cell::RefCell, fmt::Display, iter::Peekable, ops::Deref, rc::Rc};

pub const PRELUDE: [[&str; 3]; 4] = [
    ["Microsoft", "Quantum", "Canon"],
    ["Microsoft", "Quantum", "Core"],
    ["Microsoft", "Quantum", "Intrinsic"],
    ["Microsoft", "Quantum", "Measurement"],
];

/// An ID that corresponds to a namespace in the global scope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default)]
pub struct NamespaceId(usize);
impl NamespaceId {
    /// Create a new namespace ID.
    #[must_use]
    pub fn new(value: usize) -> Self {
        Self(value)
    }
}

impl From<usize> for NamespaceId {
    fn from(value: usize) -> Self {
        Self::new(value)
    }
}

impl From<NamespaceId> for usize {
    fn from(value: NamespaceId) -> Self {
        value.0
    }
}

impl From<&NamespaceId> for usize {
    fn from(value: &NamespaceId) -> Self {
        value.0
    }
}

impl Deref for NamespaceId {
    type Target = usize;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for NamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Namespace {}", self.0)
    }
}

/// A reference counted cell that supports interior mutability for namespace tree nodes.
/// Interior mutability is required to update the tree when inserting new data structures.
type NamespaceTreeCell = Rc<RefCell<NamespaceTreeNode>>;

/// An entry in the memoization table for namespace ID lookups.
type MemoEntry = (Vec<Rc<str>>, NamespaceTreeCell);

/// The root of the data structure that represents the namespaces in a program.
/// The tree is a trie (prefix tree) where each node is a namespace and the children are the sub-namespaces.
/// For example, the namespace `Microsoft.Quantum.Canon` would be represented as a traversal from the root node:
/// ```
/// root
/// └ Microsoft
///   └ Quantum
///     └ Canon
/// ```
/// This data structure is optimized for looking up namespace IDs by a given name. Looking up a namespace name by ID is
/// less efficient, as it performs a breadth-first search. Because of this inefficiency, the results of this lookup are memoized.
/// [`NamespaceTreeNode`]s are all stored in [`NamespaceTreeCell`]s, which are reference counted and support interior mutability for namespace
/// insertions and clone-free lookups.
#[derive(Debug, Clone)]
pub struct NamespaceTreeRoot {
    assigner: usize,
    tree: NamespaceTreeCell,
    memo: RefCell<FxHashMap<NamespaceId, MemoEntry>>,
}

impl NamespaceTreeRoot {
    /// Create a new namespace tree root. The assigner is used to assign new namespace IDs.
    #[must_use]
    pub fn new_from_parts(assigner: usize, tree: NamespaceTreeNode) -> Self {
        Self {
            assigner,
            tree: Rc::new(RefCell::new(tree)),
            memo: RefCell::new(FxHashMap::default()),
        }
    }

    /// Get the namespace tree field. This is the root of the namespace tree.
    #[must_use]
    pub fn tree(&self) -> NamespaceTreeCell {
        self.tree.clone()
    }

    /// Insert a namespace into the tree. If the namespace already exists, return its ID.
    /// Panics if the `ns` iterator is empty.
    #[must_use]
    pub fn insert_or_find_namespace(
        &mut self,
        ns: impl IntoIterator<Item = Rc<str>>,
    ) -> NamespaceId {
        self.tree
            .borrow_mut()
            .insert_or_find_namespace(ns.into_iter().peekable(), &mut self.assigner)
            .expect("namespace creation should not fail")
    }

    /// Get the ID of a namespace given its name.
    pub fn get_namespace_id<'a>(
        &self,
        ns: impl IntoIterator<Item = &'a str>,
    ) -> Option<NamespaceId> {
        self.tree.borrow().get_namespace_id(ns)
    }

    /// Given a [`NamespaceId`], find the namespace in the tree. Note that this function is not
    /// particularly efficient, as it performs a breadth-first search. The results of this search
    /// are memoized to avoid repeated lookups, reducing the impact of the BFS.
    #[must_use]
    pub fn find_namespace_by_id(&self, id: &NamespaceId) -> (Vec<Rc<str>>, NamespaceTreeCell) {
        if let Some(res) = self.memo.borrow().get(id) {
            return res.clone();
        }
        let (names, node) = self
            .tree
            .borrow()
            .find_namespace_by_id(*id, &[])
            .unwrap_or_else(|| (vec![], self.tree.clone()));

        self.memo
            .borrow_mut()
            .insert(*id, (names.clone(), node.clone()));
        (names, node.clone())
    }

    #[must_use]
    pub fn root_id(&self) -> NamespaceId {
        self.tree.borrow().id
    }
}

impl Default for NamespaceTreeRoot {
    fn default() -> Self {
        let mut tree = Self {
            assigner: 0,
            tree: Rc::new(RefCell::new(NamespaceTreeNode {
                children: FxHashMap::default(),
                id: NamespaceId::new(0),
            })),
            memo: RefCell::new(FxHashMap::default()),
        };
        // insert the prelude namespaces using the `NamespaceTreeRoot` API
        for ns in &PRELUDE {
            let iter = ns.iter().map(|s| Rc::from(*s)).peekable();
            let _ = tree.insert_or_find_namespace(iter);
        }
        tree
    }
}

/// A node in the namespace tree. Each node has a unique ID and a map of children.
/// Supports interior mutability of children for inserting new nodes.
#[derive(Debug, Clone)]
pub struct NamespaceTreeNode {
    pub children: FxHashMap<Rc<str>, NamespaceTreeCell>,
    pub id: NamespaceId,
}
impl NamespaceTreeNode {
    /// Create a new namespace tree node with the given ID and children. The `id` should come from the `NamespaceTreeRoot` assigner.
    #[must_use]
    fn new(id: NamespaceId, children: FxHashMap<Rc<str>, NamespaceTreeCell>) -> Self {
        Self { children, id }
    }

    /// Get a reference to the children of the namespace tree node.
    #[must_use]
    pub fn children(&self) -> &FxHashMap<Rc<str>, NamespaceTreeCell> {
        &self.children
    }

    /// See [`FxHashMap::get`] for more information.
    fn get(&self, component: &Rc<str>) -> Option<NamespaceTreeCell> {
        self.children.get(component).cloned()
    }

    /// Get the ID of this namespace tree node.
    #[must_use]
    pub fn id(&self) -> NamespaceId {
        self.id
    }

    /// Check if this namespace tree node contains a given namespace as a child.
    #[must_use]
    pub fn contains<'a>(&self, ns: impl IntoIterator<Item = &'a str>) -> bool {
        self.get_namespace_id(ns).is_some()
    }

    /// Finds the ID of a namespace given its string name. This function is generally more efficient
    /// than [`NamespaceTreeNode::find_namespace_by_id`], as it utilizes the prefix tree structure to
    /// find the ID in `O(n)` time, where `n` is the number of components in the namespace name.
    pub fn get_namespace_id<'a>(
        &self,
        ns: impl IntoIterator<Item = &'a str>,
    ) -> Option<NamespaceId> {
        let mut buf: Option<NamespaceTreeCell> = None;
        for component in ns {
            if let Some(next_ns) = match buf {
                None => self.get(&Rc::from(component)),
                Some(buf) => buf.borrow().get(&Rc::from(component)),
            } {
                buf = Some(next_ns);
            } else {
                return None;
            }
        }
        Some(buf.map_or_else(|| self.id, |x| x.borrow().id))
    }

    /// Inserts a new namespace into the tree, if it does not yet exist.
    /// Returns the ID of the namespace.
    /// Returns `None` if an empty iterator is passed in.
    pub fn insert_or_find_namespace<I>(
        &mut self,
        mut iter: Peekable<I>,
        assigner: &mut usize,
    ) -> Option<NamespaceId>
    where
        I: Iterator<Item = Rc<str>>,
    {
        let next_item = iter.next()?;
        let next_node = self.children.get_mut(&next_item);
        match (next_node, iter.peek()) {
            (Some(next_node), Some(_)) => {
                return next_node
                    .borrow_mut()
                    .insert_or_find_namespace(iter, assigner);
            }
            (Some(next_node), None) => {
                return Some(next_node.borrow().id);
            }
            _ => {}
        }
        *assigner += 1;
        let mut new_node =
            NamespaceTreeNode::new(NamespaceId::new(*assigner), FxHashMap::default());
        if iter.peek().is_none() {
            let new_node_id = new_node.id;
            self.children
                .insert(next_item, Rc::new(RefCell::new(new_node)));
            Some(new_node_id)
        } else {
            let id = new_node.insert_or_find_namespace(iter, assigner);
            self.children
                .insert(next_item, Rc::new(RefCell::new(new_node)));
            id
        }
    }

    fn find_namespace_by_id(
        &self,
        id: NamespaceId,
        names_buf: &[Rc<str>],
    ) -> Option<(Vec<Rc<str>>, NamespaceTreeCell)> {
        // first, check if any children are the one we are looking for
        for (name, node) in &self.children {
            if node.borrow().id == id {
                let mut names = names_buf.to_vec();
                names.push(name.clone());
                return Some((names, node.clone()));
            }
        }

        // if it wasn't found, recurse into children
        for (name, node) in &self.children {
            let mut names = names_buf.to_vec();
            names.push(name.clone());
            let Some((names, node)) = node.borrow().find_namespace_by_id(id, &names) else {
                continue;
            };
            return Some((names, node));
        }

        None
    }
}
