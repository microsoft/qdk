// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    hir::{
        self, Item, ItemId, ItemKind, ItemStatus, Package, PackageId, SpecBody, SpecGen, TermOrTy,
        Visibility,
    },
    ty::Scheme,
};
use qsc_data_structures::{
    index_map,
    namespaces::{NamespaceId, NamespaceTreeRoot},
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
    Namespace,
    Ty(Ty),
    Term(Term),
    Export(ItemId),
}

impl std::fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Kind::Namespace => write!(f, "Namespace"),
            Kind::Ty(ty) => write!(f, "Ty({})", ty.id),
            Kind::Term(term) => write!(f, "Term({})", term.id),
            Kind::Export(id) => write!(f, "Export{id}"),
        }
    }
}

pub struct Ty {
    pub id: ItemId,
}

pub struct Term {
    pub id: ItemId,
    pub scheme: Scheme,
    pub intrinsic: bool,
}

/// A lookup table used for looking up global core items for insertion in `qsc_passes`.
#[derive(Default)]
pub struct Table {
    tys: FxHashMap<NamespaceId, FxHashMap<Rc<str>, Ty>>,
    terms: FxHashMap<NamespaceId, FxHashMap<Rc<str>, Term>>,
    exports: FxHashMap<NamespaceId, FxHashMap<Rc<str>, ItemId>>,
    namespaces: NamespaceTreeRoot,
}

impl Table {
    #[must_use]
    pub fn resolve_ty(&self, namespace: NamespaceId, name: &str) -> Option<&Ty> {
        self.tys.get(&namespace).and_then(|terms| terms.get(name))
    }

    #[must_use]
    pub fn resolve_term(&self, namespace: NamespaceId, name: &str) -> Option<&Term> {
        self.terms.get(&namespace).and_then(|terms| terms.get(name))
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
        let mut terms: FxHashMap<_, FxHashMap<_, _>> = FxHashMap::default();
        let mut exports: FxHashMap<_, FxHashMap<_, _>> = FxHashMap::default();
        let mut namespaces = NamespaceTreeRoot::default();
        for global in iter {
            let namespace = namespaces.insert_or_find_namespace(global.namespace.into_iter());
            match global.kind {
                Kind::Ty(ty) => {
                    tys.entry(namespace).or_default().insert(global.name, ty);
                }
                Kind::Term(term) => {
                    terms
                        .entry(namespace)
                        .or_default()
                        .insert(global.name, term);
                }
                Kind::Namespace => {}
                Kind::Export(item) => {
                    exports
                        .entry(namespace)
                        .or_default()
                        .insert(global.name, item);
                }
            }
        }

        Self {
            tys,
            terms,
            namespaces,
            exports,
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
        let (id, visibility) = match &item.kind {
            ItemKind::Export(name, item_id) if item_id.package.is_some() => (
                ItemId {
                    package: item_id.package.or(self.id),
                    item: item_id.item,
                },
                hir::Visibility::Public,
            ),
            _ => (
                ItemId {
                    package: self.id,
                    item: item.id,
                },
                item.visibility,
            ),
        };
        //        todo!("The problem is that Length is coming out of this as a Term with visibility internal, when it should be either a public term or an export");
        let status = ItemStatus::from_attrs(item.attrs.as_ref());

        match (&item.kind, &parent) {
            (ItemKind::Callable(decl), Some(ItemKind::Namespace(namespace, _))) => Some(Global {
                namespace: namespace.into(),
                name: Rc::clone(&decl.name.name),
                visibility,
                status,
                kind: Kind::Term(Term {
                    id,
                    scheme: decl.scheme(),
                    intrinsic: decl.body.body == SpecBody::Gen(SpecGen::Intrinsic),
                }),
            }),
            (ItemKind::Ty(name, def), Some(ItemKind::Namespace(namespace, _))) => {
                self.next = Some(Global {
                    namespace: namespace.into(),
                    name: Rc::clone(&name.name),
                    visibility,
                    status,
                    kind: Kind::Term(Term {
                        id,
                        scheme: def.cons_scheme(id),
                        intrinsic: false,
                    }),
                });

                Some(Global {
                    namespace: namespace.into(),
                    name: Rc::clone(&name.name),
                    visibility,
                    status,
                    kind: Kind::Ty(Ty { id }),
                })
            }
            (ItemKind::Namespace(ident, _), None) => Some(Global {
                namespace: ident.into(),
                name: "".into(),
                visibility: Visibility::Public,
                status,
                kind: Kind::Namespace,
            }),
            (
                ItemKind::Export(name, item_id @ ItemId { package, item }),
                Some(ItemKind::Namespace(namespace, _)),
            ) => {
                if package.is_none() {
                    // if there is no package, then this was declared in this package
                    // and this is a noop -- it will be marked as public on export
                    return None;
                } else {
                    Some(Global {
                        namespace: namespace.into(),
                        name: name.name.clone(),
                        visibility: Visibility::Public,
                        status,
                        kind: Kind::Export(*item_id),
                    })
                }
            }
            _ => None,
            //             TODO(alex) verify below
            /*
            (ItemKind::Callable(_), None) => todo!(),
            (ItemKind::Namespace(_, _), Some(_)) => todo!(),
            (ItemKind::Ty(_, _), None) => todo!(),
            (ItemKind::Export(_, _), None) => todo!(),
            _ => todo!(),
            */
        }
    }
}

impl<'a> Iterator for PackageIter<'a> {
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
