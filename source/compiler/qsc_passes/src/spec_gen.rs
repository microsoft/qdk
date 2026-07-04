// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub(crate) mod adj_gen;
mod ctl_gen;

#[cfg(test)]
mod tests;

use crate::{id_update::NodeIdRefresher, invert_block::adj_invert_block};

use self::{adj_gen::AdjDistrib, ctl_gen::CtlDistrib};
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    global::Table,
    hir::{
        Attr, Block, CallableDecl, CallableKind, Functor, Ident, Item, NodeId, Package, Pat,
        PatKind, Res, SpecBody, SpecDecl, SpecGen,
    },
    mut_visit::{MutVisitor, walk_item},
    ty::{Prim, SizeKind, Ty},
};
use std::option::Option;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[diagnostic(transparent)]
    #[error(transparent)]
    CtlGen(ctl_gen::Error),

    #[diagnostic(transparent)]
    #[error(transparent)]
    AdjGen(adj_gen::Error),

    #[error("invalid specialization generator")]
    #[diagnostic(code("Qdk.Qsc.SpecGen.InvalidAdjGen"))]
    #[diagnostic(help(
        "valid specialization generators for adjoint are `auto`, `invert`, and `self`"
    ))]
    InvalidAdjGen(#[label] Span),

    #[error("invalid specialization generator")]
    #[diagnostic(code("Qdk.Qsc.SpecGen.InvalidBodyGen"))]
    #[diagnostic(help("body specialization only supports `intrinsic`"))]
    InvalidBodyGen(#[label] Span),

    #[error("invalid specialization generator")]
    #[diagnostic(code("Qdk.Qsc.SpecGen.InvalidCtlGen"))]
    #[diagnostic(help(
        "valid specialization generators for controlled are `auto` and `distribute`"
    ))]
    InvalidCtlGen(#[label] Span),

    #[error("invalid specialization generator")]
    #[diagnostic(code("Qdk.Qsc.SpecGen.InvalidCtlAdjGen"))]
    #[diagnostic(help(
        "valid specialization generators for controlled adjoint are `auto`, `distribute`, `invert`, and `self`"
    ))]
    InvalidCtlAdjGen(#[label] Span),

    #[error("specialization generation missing required body implementation")]
    #[diagnostic(code("Qdk.Qsc.SpecGen.MissingBody"))]
    MissingBody(#[label] Span),

    #[error(
        "specialization generation is not supported for callables with the attribute `SimulatableIntrinsic`"
    )]
    #[diagnostic(code("Qdk.Qsc.SpecGen.SimulatableIntrinsic"))]
    #[diagnostic(help(
        "try removing the specializations for this callable and providing them via a separate wrapper operation"
    ))]
    SimulatableIntrinsic(#[label] Span),
}

/// Generates specializations for the given compile unit, updating it in-place.
pub(super) fn generate_specs(
    core: &Table,
    package: &mut Package,
    assigner: &mut Assigner,
) -> Vec<Error> {
    generate_placeholders(package, assigner);
    generate_spec_impls(core, package, assigner)
}

fn generate_placeholders(package: &mut Package, assigner: &mut Assigner) {
    SpecPlacePass { assigner }.visit_package(package);
}

struct SpecPlacePass<'a> {
    assigner: &'a mut Assigner,
}

impl MutVisitor for SpecPlacePass<'_> {
    fn visit_callable_decl(&mut self, decl: &mut CallableDecl) {
        // Only applies to operations.
        if decl.kind == CallableKind::Function {
            return;
        }

        let is_adj = decl.functors.contains(&Functor::Adj);
        let is_ctl = decl.functors.contains(&Functor::Ctl);
        if !is_adj && !is_ctl {
            return;
        }

        if is_adj && decl.adj.is_none() {
            decl.adj = Some(SpecDecl {
                id: self.assigner.next_node(),
                span: decl.span,
                body: SpecBody::Gen(SpecGen::Invert),
            });
        }

        if is_ctl && decl.ctl.is_none() {
            decl.ctl = Some(SpecDecl {
                id: self.assigner.next_node(),
                span: decl.span,
                body: SpecBody::Gen(SpecGen::Distribute),
            });
        }

        let has_explicit_adj =
            matches!(&decl.adj, Some(s) if matches!(&s.body, SpecBody::Impl(..)));
        let has_explicit_ctl =
            matches!(&decl.ctl, Some(s) if matches!(&s.body, SpecBody::Impl(..)));
        let has_explicit_ctl_adj =
            matches!(&decl.ctl_adj, Some(s) if !matches!(&s.body, SpecBody::Gen(SpecGen::Auto)));

        if is_adj && is_ctl && !has_explicit_ctl_adj {
            let spec_gen = if is_self_adjoint(decl) {
                SpecGen::Slf
            } else if has_explicit_ctl && !has_explicit_adj {
                SpecGen::Invert
            } else {
                SpecGen::Distribute
            };
            decl.ctl_adj = Some(SpecDecl {
                id: self.assigner.next_node(),
                span: decl.span,
                body: SpecBody::Gen(spec_gen),
            });
        }
    }
}

fn is_self_adjoint(decl: &CallableDecl) -> bool {
    matches!(&decl.adj, Some(s) if matches!(&s.body, SpecBody::Gen(SpecGen::Slf)))
}

fn generate_spec_impls(core: &Table, package: &mut Package, assigner: &mut Assigner) -> Vec<Error> {
    let mut pass = SpecImplPass {
        core,
        assigner,
        errors: Vec::new(),
        is_codegen_intrinsic: false,
    };
    pass.visit_package(package);
    pass.errors
}

struct SpecImplPass<'a> {
    core: &'a Table,
    assigner: &'a mut Assigner,
    errors: Vec<Error>,
    is_codegen_intrinsic: bool,
}

impl SpecImplPass<'_> {
    fn ctl_distrib(&mut self, spec_decl: &mut SpecDecl, block: &Block) {
        let ctls_id = self.assigner.next_node();

        // Clone the reference block and use the pass to update the calls inside.
        let mut ctl_block = block.clone();
        let mut distrib = CtlDistrib {
            ctls: Res::Local(ctls_id),
            errors: Vec::new(),
        };
        distrib.visit_block(&mut ctl_block);
        self.errors
            .extend(distrib.errors.into_iter().map(Error::CtlGen));

        // Update the specialization body to reflect the generated block.
        spec_decl.body = SpecBody::Impl(
            Some(Pat {
                id: NodeId::default(),
                span: spec_decl.span,
                ty: Ty::Array(Box::new(Ty::Prim(Prim::Qubit)), SizeKind::Unknown),
                kind: PatKind::Bind(Ident {
                    id: ctls_id,
                    span: spec_decl.span,
                    name: "ctls".into(),
                }),
            }),
            ctl_block,
        );
    }

    fn adj_invert(&mut self, spec_decl: &mut SpecDecl, block: &Block, ctls_pat: Option<Pat>) {
        // Clone the reference block and use the pass to update the calls inside.
        let mut adj_block = block.clone();
        if let Err(invert_errors) = adj_invert_block(self.core, self.assigner, &mut adj_block) {
            self.errors.extend(
                invert_errors
                    .into_iter()
                    .map(adj_gen::Error::LogicSep)
                    .map(Error::AdjGen),
            );
            return;
        }
        let mut distrib = AdjDistrib { errors: Vec::new() };
        distrib.visit_block(&mut adj_block);
        self.errors
            .extend(distrib.errors.into_iter().map(Error::AdjGen));

        // Update the specialization body to reflect the generated block.
        spec_decl.body = SpecBody::Impl(ctls_pat, adj_block);
    }
}

impl MutVisitor for SpecImplPass<'_> {
    fn visit_item(&mut self, item: &mut Item) {
        self.is_codegen_intrinsic = item.attrs.contains(&Attr::SimulatableIntrinsic);
        walk_item(self, item);
        self.is_codegen_intrinsic = false;
    }

    fn visit_callable_decl(&mut self, decl: &mut CallableDecl) {
        let body = &decl.body;

        match body.body {
            SpecBody::Impl(..) | SpecBody::Gen(SpecGen::Intrinsic) => {}
            SpecBody::Gen(_) => {
                self.errors.push(Error::InvalidBodyGen(body.span));
                return;
            }
        }

        // Only applies to operations.
        if decl.kind == CallableKind::Function {
            return;
        }

        let adj = &mut decl.adj;
        let ctl = &mut decl.ctl;
        let ctl_adj = &mut decl.ctl_adj;
        let has_specializations = adj.is_some() || ctl.is_some() || ctl_adj.is_some();

        let SpecBody::Impl(_, body_block) = &body.body else {
            if body.body == SpecBody::Gen(SpecGen::Intrinsic) && has_specializations {
                self.errors.push(Error::MissingBody(body.span));
            }
            return;
        };
        if self.is_codegen_intrinsic && has_specializations {
            self.errors.push(Error::SimulatableIntrinsic(decl.span));
            return;
        }

        if let Some(ctl) = ctl.as_mut() {
            match ctl.body {
                SpecBody::Gen(SpecGen::Auto | SpecGen::Distribute) => {
                    self.ctl_distrib(ctl, body_block);
                    NodeIdRefresher::new(self.assigner).visit_spec_decl(ctl);
                }
                SpecBody::Impl(..) => {}
                SpecBody::Gen(_) => self.errors.push(Error::InvalidCtlGen(ctl.span)),
            }
        }

        if let Some(adj) = adj.as_mut() {
            match adj.body {
                SpecBody::Gen(SpecGen::Slf) => {
                    adj.body = body.body.clone();
                    NodeIdRefresher::new(self.assigner).visit_spec_decl(adj);
                }
                SpecBody::Gen(SpecGen::Invert | SpecGen::Auto) => {
                    self.adj_invert(adj, body_block, None);
                    NodeIdRefresher::new(self.assigner).visit_spec_decl(adj);
                }
                SpecBody::Impl(..) => {}
                SpecBody::Gen(_) => self.errors.push(Error::InvalidAdjGen(adj.span)),
            }
        }

        if let (Some(ctl_adj), Some(adj), Some(ctl)) = (ctl_adj.as_mut(), &adj, &ctl) {
            match &ctl_adj.body {
                SpecBody::Gen(SpecGen::Auto | SpecGen::Distribute) => {
                    if let SpecBody::Impl(_, adj_block) = &adj.body {
                        self.ctl_distrib(ctl_adj, adj_block);
                        NodeIdRefresher::new(self.assigner).visit_spec_decl(ctl_adj);
                    }
                }
                SpecBody::Gen(SpecGen::Slf) => {
                    ctl_adj.body = ctl.body.clone();
                    NodeIdRefresher::new(self.assigner).visit_spec_decl(ctl_adj);
                }
                SpecBody::Gen(SpecGen::Invert) => {
                    if let SpecBody::Impl(pat, ctl_block) = &ctl.body {
                        self.adj_invert(ctl_adj, ctl_block, pat.clone());
                        NodeIdRefresher::new(self.assigner).visit_spec_decl(ctl_adj);
                    }
                }
                SpecBody::Impl(..) => {}
                SpecBody::Gen(_) => self.errors.push(Error::InvalidCtlAdjGen(ctl_adj.span)),
            }
        }
    }
}
