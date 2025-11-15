// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write;
use std::{cell::RefCell, collections::BTreeMap, fmt::Display, iter::Peekable, ops::Deref, rc::Rc};

pub const PRELUDE: [[&str; 2]; 5] = [
    ["Std", "Canon"],
    ["Std", "Core"],
    ["Std", "Intrinsic"],
    ["Std", "Measurement"],
    ["Std", "Range"],
];

/// An ID that corresponds to a namespace in the global scope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default, PartialOrd, Ord)]
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

/// Denotes that a namespace from an external package has been overridden by a local package namespace.
/// This renders the contents of the foreign namespace unaccessible.
/// E.g. a user explicitly creating a namespace called `namespace Std.Diagnostics`, where `Std` is the
/// standard library.
#[derive(Debug)]
pub struct ClobberedNamespace;

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
#[derive(Clone)]
pub struct NamespaceTreeRoot {
    assigner: usize,
    tree: NamespaceTreeCell,
    memo: RefCell<FxHashMap<NamespaceId, MemoEntry>>,
}

impl std::fmt::Debug for NamespaceTreeRoot {
    // manual implementation to avoid infinite loops in printing
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "NamespaceTreeRoot\n{}",
            self.tree.borrow().debug_print(0, &mut FxHashSet::default())
        )
    }
}

impl std::fmt::Debug for NamespaceTreeNode {
    // manual implementation to avoid infinite loops in printing
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.debug_print(0, &mut FxHashSet::default()))
    }
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
            .find_namespace_by_id(*id, &[], &mut FxHashSet::default())
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

    /// Inserts an alias for an existing namespace into the tree, where the child namespace already exists.
    pub fn insert_with_id(
        &mut self,
        parent: Option<NamespaceId>,
        new_child: NamespaceId,
        alias: &str,
    ) -> Result<(), ClobberedNamespace> {
        let parent = parent.unwrap_or_else(|| self.root_id());
        let (_, parent_node) = self.find_namespace_by_id(&parent);
        let (_, existing_ns) = self.find_namespace_by_id(&new_child);
        if let Some(val) = parent_node.borrow().children.get(&Rc::from(alias))
            && val.borrow().id != existing_ns.borrow().id
        {
            return Err(ClobberedNamespace);
        }

        parent_node
            .borrow_mut()
            .children
            .insert(Rc::from(alias), existing_ns);

        Ok(())
    }

    /// Inserts (or finds) a new namespace as a child of an existing namespace.
    /// Primarily used for appending namespaces to a parent namespace which represents a module/external package..
    pub fn insert_or_find_namespace_from_root(
        &mut self,
        ns: impl Into<Vec<Rc<str>>>,
        root: NamespaceId,
    ) -> NamespaceId {
        let ns = ns.into();
        if ns.is_empty() {
            return root;
        }
        let (_root_name, root_contents) = self.find_namespace_by_id(&root);
        let id = root_contents
            .borrow_mut()
            .insert_or_find_namespace(ns.into_iter().peekable(), &mut self.assigner);

        id.expect("empty name checked for above")
    }

    pub fn insert_or_find_namespace_from_root_with_id(
        &mut self,
        mut ns: Vec<Rc<str>>,
        root: NamespaceId,
        base_id: NamespaceId,
    ) -> Result<(), ClobberedNamespace> {
        if ns.is_empty() {
            return Ok(());
        }
        let (_root_name, root_contents) = self.find_namespace_by_id(&root);
        // split `ns` into [0..len - 1] and [len - 1]
        let suffix = ns.split_off(ns.len() - 1)[0].clone();
        let prefix = ns;

        // if the prefix is empty, we are inserting into the root
        if prefix.is_empty() {
            self.insert_with_id(Some(root), base_id, &suffix)?;
        } else {
            let prefix_id = root_contents
                .borrow_mut()
                .insert_or_find_namespace(prefix.into_iter().peekable(), &mut self.assigner)
                .expect("empty name checked for above");

            self.insert_with_id(Some(prefix_id), base_id, &suffix)?;
        }
        Ok(())
    }

    /// Each item in this iterator is the same, single namespace. The reason there are multiple paths for it,
    /// each represented by a `Vec<Rc<str>>`, is because there may be multiple paths to the same
    /// namespace, through aliasing or re-exports.
    pub fn iter(&self) -> std::collections::btree_map::IntoValues<NamespaceId, Vec<Vec<Rc<str>>>> {
        let mut stack = vec![(vec![], self.tree.clone())];
        let mut result: Vec<(NamespaceId, Vec<Rc<str>>)> = vec![];
        while let Some((names, node)) = stack.pop() {
            result.push((node.borrow().id, names.clone()));
            for (name, child) in node.borrow().children() {
                let mut new_names = names.clone();
                new_names.push(name.clone());
                stack.push((new_names, child.clone()));
            }
            if node.borrow().children().is_empty() {
                result.push((node.borrow().id, names));
            }
        }

        // flatten the result into a list of paths

        // use a btree map here instead of a hash map for deterministic iteration --
        // while it shouldn't be consequential, any nondeterminism in a compiler makes
        // things more difficult to track down then they go wrong.
        let mut flattened_result = BTreeMap::default();
        for (id, names) in result {
            let entry = flattened_result.entry(id).or_insert_with(Vec::new);
            entry.push(names);
        }

        flattened_result.into_values()
    }
}

impl IntoIterator for &NamespaceTreeRoot {
    type Item = Vec<Vec<Rc<str>>>;
    type IntoIter = std::collections::btree_map::IntoValues<NamespaceId, Vec<Vec<Rc<str>>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Default for NamespaceTreeRoot {
    fn default() -> Self {
        Self {
            assigner: 0,
            tree: Rc::new(RefCell::new(NamespaceTreeNode {
                children: FxHashMap::default(),
                id: NamespaceId::new(0),
            })),
            memo: RefCell::new(FxHashMap::default()),
        }
    }
}

/// A node in the namespace tree. Each node has a unique ID and a map of children.
/// Supports interior mutability of children for inserting new nodes.
#[derive(Clone)]
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
        let mut rover: Option<NamespaceTreeCell> = None;
        for component in ns {
            if let Some(next_ns) = match rover {
                None => self.get(&Rc::from(component)),
                Some(buf) => buf.borrow().get(&Rc::from(component)),
            } {
                rover = Some(next_ns);
            } else {
                return None;
            }
        }
        Some(rover.map_or_else(|| self.id, |x| x.borrow().id))
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
        // `ids_visited` is used to avoid infinite loops in the case of cycles in the namespace tree.
        ids_visited: &mut FxHashSet<NamespaceId>,
    ) -> Option<(Vec<Rc<str>>, NamespaceTreeCell)> {
        if ids_visited.contains(&self.id) {
            return None;
        }
        ids_visited.insert(self.id);
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
            let Some((names, node)) = node.borrow().find_namespace_by_id(id, &names, ids_visited)
            else {
                continue;
            };
            return Some((names, node));
        }

        None
    }

    fn debug_print(
        &self,
        indentation_level: usize,
        visited_nodes: &mut FxHashSet<NamespaceId>,
    ) -> String {
        let indentation = "  ".repeat(indentation_level);

        if visited_nodes.contains(&self.id) {
            return format!("\n{indentation}Cycle Detected");
        }

        visited_nodes.insert(self.id);

        let mut result = String::new();

        if self.children.is_empty() {
            result.push_str("empty node");
        } else {
            write!(result, "\n{indentation}  children: [")
                .expect("writing to string should succeed");
            for (name, node) in &self.children {
                write!(
                    result,
                    "\n{}    {}(id {}) {{",
                    indentation,
                    name,
                    Into::<usize>::into(node.borrow().id)
                )
                .expect("writing to string should succeed");
                result.push_str(
                    node.borrow()
                        .debug_print(indentation_level + 2, visited_nodes)
                        .as_str(),
                );
                result.push(',');
            }
            write!(result, "\n{indentation}  ]").expect("writing to string should succeed");
            write!(result, "\n{indentation}").expect("writing to string should succeed");
        }
        result.push('}');
        result
    }
}
