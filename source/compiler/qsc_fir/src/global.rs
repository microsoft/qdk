// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    fir::{Item, ItemId, ItemKind, Package, PackageId, Visibility},
    ty::Scheme,
};
use qsc_data_structures::index_map;
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub struct Global {
    pub namespace: Rc<str>,
    pub name: Rc<str>,
    pub visibility: Visibility,
    pub kind: Kind,
}

pub enum Kind {
    Namespace,
    Ty(Ty),
    Term(Term),
}

pub struct Ty {
    pub id: ItemId,
}

pub struct Term {
    pub id: ItemId,
    pub scheme: Scheme,
}

#[derive(Default)]
pub struct Table {
    tys: FxHashMap<Rc<str>, FxHashMap<Rc<str>, Ty>>,
    terms: FxHashMap<Rc<str>, FxHashMap<Rc<str>, Term>>,
}

impl Table {
    #[must_use]
    pub fn resolve_ty(&self, namespace: &str, name: &str) -> Option<&Ty> {
        self.tys.get(namespace).and_then(|ty| ty.get(name))
    }

    #[must_use]
    pub fn resolve_term(&self, namespace: &str, name: &str) -> Option<&Term> {
        self.terms.get(namespace).and_then(|terms| terms.get(name))
    }
}

impl FromIterator<Global> for Table {
    fn from_iter<T: IntoIterator<Item = Global>>(iter: T) -> Self {
        let mut tys: FxHashMap<_, FxHashMap<_, _>> = FxHashMap::default();
        let mut terms: FxHashMap<_, FxHashMap<_, _>> = FxHashMap::default();
        for global in iter {
            match global.kind {
                Kind::Ty(ty) => {
                    tys.entry(global.namespace)
                        .or_default()
                        .insert(global.name, ty);
                }
                Kind::Term(term) => {
                    terms
                        .entry(global.namespace)
                        .or_default()
                        .insert(global.name, term);
                }
                Kind::Namespace => {}
            }
        }

        Self { tys, terms }
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
        let id = ItemId {
            package: self.id,
            item: item.id,
        };

        match (&item.kind, &parent) {
            (ItemKind::Callable(decl), Some(ItemKind::Namespace(namespace, _))) => Some(Global {
                namespace: Rc::clone(&namespace.name),
                name: Rc::clone(&decl.name.name),
                visibility: item.visibility,
                kind: Kind::Term(Term {
                    id,
                    scheme: decl
                        .scheme(|key| self.package.pats.get(key).expect("couldn't find pat")),
                }),
            }),
            (ItemKind::Ty(name, def), Some(ItemKind::Namespace(namespace, _))) => {
                self.next = Some(Global {
                    namespace: Rc::clone(&namespace.name),
                    name: Rc::clone(&name.name),
                    visibility: item.visibility,
                    kind: Kind::Term(Term {
                        id,
                        scheme: def.cons_scheme(id),
                    }),
                });

                Some(Global {
                    namespace: Rc::clone(&namespace.name),
                    name: Rc::clone(&name.name),
                    visibility: item.visibility,
                    kind: Kind::Ty(Ty { id }),
                })
            }
            (ItemKind::Namespace(ident, _), None) => Some(Global {
                namespace: "".into(),
                name: Rc::clone(&ident.name),
                visibility: Visibility::Public,
                kind: Kind::Namespace,
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
pub fn iter_package(id: Option<PackageId>, package: &'_ Package) -> PackageIter<'_> {
    PackageIter {
        id,
        package,
        items: package.items.values(),
        next: None,
    }
}
