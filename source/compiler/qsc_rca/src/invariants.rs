// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Debug-only invariant checks for RCA results.
//!
//! This module provides a post-walk (`assert_arity_consistency`) that verifies
//! every `ApplicationGeneratorSet` recorded in a `PackageStoreComputeProperties`
//! has `dynamic_param_applications.len()` matching the arity (i.e., the number
//! of flattened input parameters) of its owning callable specialization, or
//! `0` for top-level statements and entry expressions.
//!
//! The module is gated on `#[cfg(debug_assertions)]` so release builds compile
//! it out entirely.

use crate::{ApplicationGeneratorSet, PackageStoreComputeProperties};
use qsc_fir::{
    fir::{
        Block, BlockId, CallableImpl, Expr, ExprId, ItemKind, Package, PackageId, PackageStore,
        Pat, PatId, SpecDecl, Stmt, StmtId,
    },
    visit::{self, Visitor},
};
use rustc_hash::FxHashMap;

/// Walks `store` and `props` and asserts that every recorded
/// `ApplicationGeneratorSet.dynamic_param_applications` vector has the arity
/// of its owning specialization (or `0` for top-level statements and entry
/// expressions).
///
/// Every package in the store is checked. Entries whose ownership cannot be
/// resolved from the FIR walk are silently skipped (see [`check_entry`]).
pub(crate) fn assert_arity_consistency(
    store: &PackageStore,
    props: &PackageStoreComputeProperties,
) {
    for (package_id, package) in store {
        let ownership = collect_ownership(package_id, package);
        let package_props = props.get(package_id);

        for (block_id, generator) in package_props.blocks.iter() {
            check_entry(
                package_id,
                ElementKey::Block(block_id),
                generator,
                &ownership,
            );
        }
        for (stmt_id, generator) in package_props.stmts.iter() {
            check_entry(package_id, ElementKey::Stmt(stmt_id), generator, &ownership);
        }
        for (expr_id, generator) in package_props.exprs.iter() {
            check_entry(package_id, ElementKey::Expr(expr_id), generator, &ownership);
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
enum ElementKey {
    Block(BlockId),
    Stmt(StmtId),
    Expr(ExprId),
}

fn check_entry(
    package_id: PackageId,
    key: ElementKey,
    generator: &ApplicationGeneratorSet,
    ownership: &FxHashMap<ElementKey, usize>,
) {
    let Some(&expected) = ownership.get(&key) else {
        // Unknown ownership is silently tolerated: this indicates either a
        // synthesized element not contributing to RCA, or a gap that a
        // future invariant refinement should cover.
        return;
    };
    let actual = generator.dynamic_param_applications.len();
    debug_assert!(
        actual == expected,
        "RCA invariant: package {package_id:?} {key:?} application generator has {actual} \
         param applications but owning specialization has arity {expected}",
    );
}

fn collect_ownership(package_id: PackageId, package: &Package) -> FxHashMap<ElementKey, usize> {
    let mut collector = OwnershipCollector {
        package,
        map: FxHashMap::default(),
        current_arity: 0,
    };

    // Walk each callable item so spec-owned IDs are recorded with the
    // callable's input-pat arity. Top-level statements and the entry
    // expression are recorded after item walks with arity 0.
    for (_, item) in &package.items {
        if let ItemKind::Callable(callable) = &item.kind {
            let arity = package.derive_callable_input_params(callable).len();
            collector.current_arity = arity;
            match &callable.implementation {
                CallableImpl::Spec(spec_impl) => {
                    collector.visit_spec_decl(&spec_impl.body);
                    if let Some(spec) = spec_impl.adj.as_ref() {
                        collector.visit_spec_decl(spec);
                    }
                    if let Some(spec) = spec_impl.ctl.as_ref() {
                        collector.visit_spec_decl(spec);
                    }
                    if let Some(spec) = spec_impl.ctl_adj.as_ref() {
                        collector.visit_spec_decl(spec);
                    }
                }
                // `SimulatableIntrinsic` bodies are not analyzed by the core
                // analyzer; their stmts receive an arity-matched default
                // generator set via `core::set_all_stmts_in_block_to_default`.
                // Record ownership at the callable's arity so the invariant
                // sees a consistent view.
                CallableImpl::SimulatableIntrinsic(spec_decl) => {
                    collector.visit_spec_decl(spec_decl);
                }
                // `Intrinsic` callables have no body to walk.
                CallableImpl::Intrinsic => {}
            }
        }
    }

    // Top-level stmts + entry expr live outside any spec and have arity 0.
    collector.current_arity = 0;
    for (stmt_id, _) in &package.stmts {
        collector.map.entry(ElementKey::Stmt(stmt_id)).or_insert(0);
    }
    if let Some(entry_expr) = package.entry {
        collector
            .map
            .entry(ElementKey::Expr(entry_expr))
            .or_insert(0);
        // Walk the entry expression tree so nested exprs/blocks/stmts are
        // captured too.
        collector.visit_expr(entry_expr);
    }

    let _ = package_id; // Kept for signature symmetry / future diagnostics.
    collector.map
}

struct OwnershipCollector<'a> {
    package: &'a Package,
    map: FxHashMap<ElementKey, usize>,
    current_arity: usize,
}

impl<'a> Visitor<'a> for OwnershipCollector<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.package.blocks.get(id).expect("block should exist")
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.package.exprs.get(id).expect("expr should exist")
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.package.pats.get(id).expect("pat should exist")
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.package.stmts.get(id).expect("stmt should exist")
    }

    fn visit_block(&mut self, id: BlockId) {
        // First-wins insertion prevents a later arity-0 entry-expression
        // walk from clobbering a spec-body arity recorded by the earlier
        // item walk. The sharing case is dormant today but this hardening
        // removes a latent aliasing hazard at zero cost.
        self.map
            .entry(ElementKey::Block(id))
            .or_insert(self.current_arity);
        visit::walk_block(self, id);
    }

    fn visit_stmt(&mut self, id: StmtId) {
        self.map
            .entry(ElementKey::Stmt(id))
            .or_insert(self.current_arity);
        visit::walk_stmt(self, id);
    }

    fn visit_expr(&mut self, id: ExprId) {
        self.map
            .entry(ElementKey::Expr(id))
            .or_insert(self.current_arity);
        visit::walk_expr(self, id);
    }

    fn visit_spec_decl(&mut self, decl: &'a SpecDecl) {
        // Skip pat to avoid recording pattern IDs (we only track blocks/stmts/exprs).
        self.visit_block(decl.block);
    }
}
