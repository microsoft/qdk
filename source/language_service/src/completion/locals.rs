// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Completion;
use crate::{compilation::Compilation, protocol::CompletionItemKind};
use qsc::{
    display::{CodeDisplay, Lookup},
    hir::ItemKind,
    resolve::Local,
};

/// Provides the locals that are visible at the cursor offset
pub(super) struct Locals<'a> {
    compilation: &'a Compilation,
    offset: u32,
}

impl Locals<'_> {
    pub fn new(offset: u32, compilation: &Compilation) -> Locals<'_> {
        Locals {
            compilation,
            offset,
        }
    }

    pub fn expr_names(&self) -> Vec<Completion> {
        Self::local_completions(self.compilation, self.offset, true, false)
    }

    pub fn type_names(&self) -> Vec<Completion> {
        Self::local_completions(self.compilation, self.offset, false, true)
    }

    fn local_completions(
        compilation: &Compilation,
        offset: u32,
        include_terms: bool,
        include_tys: bool,
    ) -> Vec<Completion> {
        compilation
            .user_unit()
            .ast
            .locals
            .get_all_at_offset(offset)
            .iter()
            .filter_map(|candidate| {
                Self::local_completion(candidate, compilation, include_terms, include_tys)
            })
            .collect::<Vec<_>>()
    }

    /// Convert a local into a completion item
    fn local_completion(
        candidate: &Local,
        compilation: &Compilation,
        include_terms: bool,
        include_tys: bool,
    ) -> Option<Completion> {
        let display = CodeDisplay { compilation };
        let (kind, detail) = match &candidate {
            Local::Item(item_id, _) => {
                let item = compilation.resolve_item_relative_to_user_package(item_id);
                let (detail, kind) = match &item.0.kind {
                    ItemKind::Callable(decl) => {
                        if !include_terms {
                            return None;
                        }
                        (
                            Some(display.hir_callable_decl(decl).to_string()),
                            CompletionItemKind::Function,
                        )
                    }
                    ItemKind::Namespace(_, _) => {
                        panic!("did not expect local namespace item")
                    }
                    ItemKind::Ty(_, udt) => {
                        if !include_terms && !include_tys {
                            return None;
                        }
                        (
                            Some(display.hir_udt(udt).to_string()),
                            CompletionItemKind::Interface,
                        )
                    }
                    // We don't want completions for items exported from the local scope
                    ItemKind::Export(_, _) => return None,
                };
                (kind, detail)
            }
            Local::Var(node_id, name) => {
                if !include_terms {
                    return None;
                }
                let detail = Some(display.name_ty_id(name, *node_id).to_string());
                (CompletionItemKind::Variable, detail)
            }
            Local::TyParam(..) => {
                if !include_tys {
                    return None;
                }
                (CompletionItemKind::TypeParameter, None)
            }
            Local::NamespaceImport(..) => return None,
        };

        Some(Completion::with_detail(
            candidate.name().expect("expected item name").to_string(),
            kind,
            detail,
        ))
    }
}
