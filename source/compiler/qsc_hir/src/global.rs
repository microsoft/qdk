// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    hir::{Item, ItemId, ItemKind, ItemStatus, Package, Res, SpecBody, SpecGen, Visibility},
    ty::Scheme,
};
use qsc_data_structures::{
    index_map,
    namespaces::{NamespaceId, NamespaceTreeRoot},
    span::PackageId,
};
use rustc_hash::FxHashMap;
use std::rc::Rc;

#[derive(Debug)]
pub struct Global {
    pub namespace: Vec<Rc<str>>,
    pub name: Rc<str>,
    pub visibility: Visibility,
    pub status: ItemStatus,
    pub kind: Kind,
}

pub enum Kind {
    Namespace(ItemId),
    Ty(Ty),
    Callable(Callable),
    Export(ItemId),
}

impl std::fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Kind::Namespace(id) => write!(f, "Namespace({id})"),
            Kind::Ty(ty) => write!(f, "Ty({})", ty.id),
            Kind::Callable(term) => write!(f, "Callable({})", term.id),
            Kind::Export(id) => write!(f, "Export({id:?})"),
        }
    }
}

pub struct Ty {
    pub id: ItemId,
}

pub struct Callable {
    pub id: ItemId,
    pub scheme: Scheme,
    pub intrinsic: bool,
}

/// A lookup table used for looking up global core items for insertion in `qsc_passes`.
#[derive(Default)]
pub struct Table {
    tys: FxHashMap<NamespaceId, FxHashMap<Rc<str>, Ty>>,
    callables: FxHashMap<NamespaceId, FxHashMap<Rc<str>, Callable>>,
    namespaces: NamespaceTreeRoot,
}

impl Table {
    #[must_use]
    pub fn resolve_ty(&self, namespace: NamespaceId, name: &str) -> Option<&Ty> {
        self.tys.get(&namespace).and_then(|terms| terms.get(name))
    }

    #[must_use]
    pub fn resolve_callable(&self, namespace: NamespaceId, name: &str) -> Option<&Callable> {
        self.callables
            .get(&namespace)
            .and_then(|terms| terms.get(name))
    }

    pub fn find_namespace<'a>(
        &self,
        query: impl IntoIterator<Item = &'a str>,
    ) -> Option<NamespaceId> {
        // find a namespace if it exists and return its id
        self.namespaces.get_namespace_id(query)
    }
}

impl FromIterator<Global> for Table {
    fn from_iter<T: IntoIterator<Item = Global>>(iter: T) -> Self {
        let mut tys: FxHashMap<_, FxHashMap<_, _>> = FxHashMap::default();
        let mut callables: FxHashMap<_, FxHashMap<_, _>> = FxHashMap::default();
        let mut namespaces = NamespaceTreeRoot::default();
        for global in iter {
            let namespace = namespaces.insert_or_find_namespace(global.namespace.into_iter());
            match global.kind {
                Kind::Ty(ty) => {
                    tys.entry(namespace).or_default().insert(global.name, ty);
                }
                Kind::Callable(term) => {
                    callables
                        .entry(namespace)
                        .or_default()
                        .insert(global.name, term);
                }
                Kind::Namespace(_) | Kind::Export(_) => {}
            }
        }

        Self {
            tys,
            callables,
            namespaces,
        }
    }
}

pub struct PackageIter<'a> {
    id: Option<PackageId>,
    package: &'a Package,
    items: index_map::Values<'a, Item>,
    next: Option<Global>,
}

impl PackageIter<'_> {
    fn global_item(&mut self, item: &Item) -> Option<Global> {
        let parent = item.parent.map(|parent| {
            &self
                .package
                .items
                .get(parent)
                .expect("parent should exist")
                .kind
        });

        let item_id = ItemId {
            package: self.id,
            item: item.id,
        };
        let status = ItemStatus::from_attrs(item.attrs.as_ref());
        let visibility = item.visibility;

        match (&item.kind, &parent) {
            (ItemKind::Callable(decl), Some(ItemKind::Namespace(namespace, _))) => Some(Global {
                namespace: namespace.into(),
                name: Rc::clone(&decl.name.name),
                visibility,
                status,
                kind: Kind::Callable(Callable {
                    id: item_id,
                    scheme: decl.scheme(),
                    intrinsic: decl.body.body == SpecBody::Gen(SpecGen::Intrinsic),
                }),
            }),
            (ItemKind::Callable(decl), None) => Some(Global {
                namespace: Vec::new(),
                name: Rc::clone(&decl.name.name),
                visibility,
                status,
                kind: Kind::Callable(Callable {
                    id: item_id,
                    scheme: decl.scheme(),
                    intrinsic: decl.body.body == SpecBody::Gen(SpecGen::Intrinsic),
                }),
            }),
            (ItemKind::Ty(name, _def), Some(ItemKind::Namespace(namespace, _))) => Some(Global {
                namespace: namespace.into(),
                name: Rc::clone(&name.name),
                visibility,
                status,
                kind: Kind::Ty(Ty { id: item_id }),
            }),
            (ItemKind::Ty(name, _def), None) => Some(Global {
                namespace: Vec::new(),
                name: Rc::clone(&name.name),
                visibility,
                status,
                kind: Kind::Ty(Ty { id: item_id }),
            }),
            (ItemKind::Namespace(full_name, _), None) => {
                let (name, parent) = full_name
                    .0
                    .split_last()
                    .expect("namespace name should not be empty");
                // Parent namespace can be empty
                Some(Global {
                    namespace: parent.iter().map(|i| Rc::clone(&i.name)).collect(),
                    name: Rc::clone(&name.name),
                    visibility: Visibility::Public,
                    status,
                    kind: Kind::Namespace(item_id),
                })
            }
            (
                ItemKind::Export(name, Res::Item(item_id)),
                Some(ItemKind::Namespace(namespace, _)),
            ) => Some(Global {
                namespace: namespace.into(),
                name: Rc::clone(&name.name),
                visibility,
                status,
                // Export items can refer to different packages, so be sure
                // to use the provided package id
                kind: Kind::Export(ItemId {
                    package: item_id.package.or(self.id),
                    item: item_id.item,
                }),
            }),
            _ => None,
        }
    }
}

impl Iterator for PackageIter<'_> {
    type Item = Global;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(global) = self.next.take() {
            Some(global)
        } else {
            loop {
                let item = self.items.next()?;
                if let Some(global) = self.global_item(item) {
                    break Some(global);
                }
            }
        }
    }
}

#[must_use]
pub fn iter_package(id: Option<PackageId>, package: &Package) -> PackageIter {
    PackageIter {
        id,
        package,
        items: package.items.values(),
        next: None,
    }
}
