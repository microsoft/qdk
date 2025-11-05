// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests_base;

#[cfg(test)]
mod tests_adaptive;

#[cfg(test)]
mod tests_adaptive_plus_integers;

#[cfg(test)]
mod tests_adaptive_plus_integers_and_floats;

#[cfg(test)]
pub mod tests_common;

use qsc_data_structures::{span::Span, target::TargetCapabilityFlags};

use qsc_fir::{
    fir::{
        Attr, Block, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Global, Ident,
        Item, ItemKind, LocalItemId, LocalVarId, Package, PackageLookup, Pat, PatId, PatKind, Res,
        SpecDecl, SpecImpl, Stmt, StmtId, StmtKind,
    },
    ty::FunctorSetValue,
    visit::{Visitor, walk_callable_decl},
};

use qsc_lowerer::map_hir_package_to_fir;
use qsc_rca::{
    Analyzer, ComputeKind, ItemComputeProperties, PackageComputeProperties,
    PackageStoreComputeProperties, RuntimeFeatureFlags,
    errors::{Error, generate_errors_from_runtime_features, get_missing_runtime_features},
};
use rustc_hash::FxHashMap;

/// Lower a package store from `qsc_frontend` HIR store to a `qsc_fir` FIR store.
pub fn lower_store(
    package_store: &qsc_frontend::compile::PackageStore,
) -> qsc_fir::fir::PackageStore {
    let mut fir_store = qsc_fir::fir::PackageStore::new();
    for (id, unit) in package_store {
        let package = qsc_lowerer::Lowerer::new().lower_package(&unit.package, &fir_store);
        fir_store.insert(map_hir_package_to_fir(id), package);
    }
    fir_store
}

pub fn run_rca_pass(
    fir_store: &qsc_fir::fir::PackageStore,
    package_id: qsc_fir::fir::PackageId,
    capabilities: TargetCapabilityFlags,
) -> Result<PackageStoreComputeProperties, Vec<crate::Error>> {
    let analyzer = Analyzer::init(fir_store);
    let compute_properties = analyzer.analyze_all();
    let fir_package = fir_store.get(package_id);

    let package_compute_properties = compute_properties.get(package_id);
    let mut errors = check_supported_capabilities(
        fir_package,
        package_compute_properties,
        capabilities,
        fir_store,
    );

    if errors.is_empty() {
        Ok(compute_properties)
    } else {
        let errors = errors
            .drain(..)
            .map(crate::Error::CapabilitiesCk)
            .collect::<Vec<_>>();
        Err(errors)
    }
}

#[must_use]
pub fn check_supported_capabilities(
    package: &Package,
    compute_properties: &PackageComputeProperties,
    capabilities: TargetCapabilityFlags,
    store: &qsc_fir::fir::PackageStore,
) -> Vec<Error> {
    let checker = Checker {
        package,
        compute_properties,
        target_capabilities: capabilities,
        current_callable: None,
        missing_features_map: FxHashMap::<Span, RuntimeFeatureFlags>::default(),
        store,
    };

    checker.check_all()
}

struct Checker<'a> {
    package: &'a Package,
    compute_properties: &'a PackageComputeProperties,
    target_capabilities: TargetCapabilityFlags,
    current_callable: Option<LocalItemId>,
    missing_features_map: FxHashMap<Span, RuntimeFeatureFlags>,
    store: &'a qsc_fir::fir::PackageStore,
}

impl<'a> Visitor<'a> for Checker<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.package.get_block(id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.package.get_expr(id)
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.package.get_pat(id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.package.get_stmt(id)
    }

    fn visit_package(&mut self, package: &'a Package, _: &crate::fir::PackageStore) {
        package
            .items
            .iter()
            .for_each(|(_, item)| self.visit_item(item));
        package.entry.iter().for_each(|entry_expr_id| {
            self.check_entry_expr(*entry_expr_id);
        });
    }

    fn visit_callable_decl(&mut self, decl: &'a CallableDecl) {
        if decl.attrs.iter().all(|attr| *attr != Attr::Test) {
            walk_callable_decl(self, decl);
        }
    }

    fn visit_callable_impl(&mut self, callable_impl: &'a CallableImpl) {
        match callable_impl {
            CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {
                self.check_spec_decl(FunctorSetValue::Empty, None);
            }
            CallableImpl::Spec(spec_impl) => {
                self.check_spec_decl(FunctorSetValue::Empty, Some(&spec_impl.body));
                spec_impl.adj.iter().for_each(|spec_decl| {
                    self.check_spec_decl(FunctorSetValue::Adj, Some(spec_decl));
                });
                spec_impl.ctl.iter().for_each(|spec_decl| {
                    self.check_spec_decl(FunctorSetValue::Ctl, Some(spec_decl));
                });
                spec_impl.ctl_adj.iter().for_each(|spec_decl| {
                    self.check_spec_decl(FunctorSetValue::CtlAdj, Some(spec_decl));
                });
            }
        }
    }

    fn visit_expr(&mut self, expr_id: ExprId) {
        let expr = self.get_expr(expr_id);

        // We do not want to generate errors for auto-generated expressions.
        if self.is_expr_auto_generated(expr) {
            return;
        }

        match &expr.kind {
            ExprKind::Block(block_id) => self.visit_block(*block_id),
            ExprKind::If(condition_expr_id, body_block_id, otherwise_block_id) => self
                .check_expr_if(
                    expr_id,
                    *condition_expr_id,
                    *body_block_id,
                    *otherwise_block_id,
                ),
            ExprKind::While(condition_expr_id, body_block_id) => {
                self.check_expr_while(expr_id, *condition_expr_id, *body_block_id);
            }
            _ => self.check_expr(expr_id),
        }
    }

    fn visit_item(&mut self, item: &'a Item) {
        // We only care about callables.
        if let ItemKind::Callable(callable_decl) = &item.kind {
            self.set_current_callable(item.id);
            self.visit_callable_decl(callable_decl);
            let callable_id = self.clear_current_callable();
            assert!(item.id == callable_id);
        }
    }

    fn visit_spec_impl(&mut self, _: &'a SpecImpl) {
        panic!("visiting a specialization implementation directly should not happen");
    }

    fn visit_stmt(&mut self, stmt_id: StmtId) {
        let stmt = self.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Item(_) => {}
            StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
                self.visit_expr(*expr_id);
            }
        }
    }
}

impl<'a> Checker<'a> {
    pub fn check_all(mut self) -> Vec<Error> {
        self.visit_package(self.package, self.store);
        self.generate_errors()
    }

    fn check_entry_expr(&mut self, expr_id: ExprId) {
        let expr = self.get_expr(expr_id);
        if expr.span == Span::default() {
            // This is an auto-generated entry expression, so we only need to verify the output recording flags.
            self.check_output_recording(expr);
        } else {
            self.visit_expr(expr_id);
            if matches!(
                expr.kind,
                ExprKind::Block(_) | ExprKind::If(_, _, _) | ExprKind::While(_, _)
            ) {
                self.check_output_recording(expr);
            }
        }
    }

    fn check_expr(&mut self, expr_id: ExprId) {
        let compute_kind = self.compute_properties.get_expr(expr_id).inherent;
        let ComputeKind::Quantum(quantum_properties) = compute_kind else {
            return;
        };

        let missing_features = get_missing_runtime_features(
            quantum_properties.runtime_features,
            self.target_capabilities,
        );
        let expr = self.get_expr(expr_id);
        if !missing_features.is_empty() {
            self.missing_features_map
                .entry(expr.span)
                .and_modify(|f| *f |= missing_features)
                .or_insert(missing_features);
        }
    }

    fn check_expr_if(
        &mut self,
        expr_id: ExprId,
        condition_expr_id: ExprId,
        body_expr_id: ExprId,
        otherwise_expr_id: Option<ExprId>,
    ) {
        // Check each one of the sub-expressions individually to provide more granularity.
        let pre_spans_with_missing_features_count = self.missing_features_map.len();
        self.visit_expr(condition_expr_id);
        self.visit_expr(body_expr_id);
        if let Some(e) = otherwise_expr_id.as_ref() {
            self.visit_expr(*e);
        }
        let post_spans_with_missing_features_count = self.missing_features_map.len();

        // If no errors were added because of the individual expressions, check this expression as a whole.
        let errors_delta =
            post_spans_with_missing_features_count - pre_spans_with_missing_features_count;
        if errors_delta == 0 {
            self.check_expr(expr_id);
        }
    }

    fn check_expr_while(
        &mut self,
        expr_id: ExprId,
        condition_expr_id: ExprId,
        body_block_id: BlockId,
    ) {
        // Check each one of the sub-elements individually to provide more granularity.
        let pre_spans_with_missing_features_count = self.missing_features_map.len();
        self.visit_expr(condition_expr_id);
        self.visit_block(body_block_id);
        let post_spans_with_missing_features_count = self.missing_features_map.len();

        // If no errors were added because of the individual elements, check this expression as a whole.
        let errors_delta =
            post_spans_with_missing_features_count - pre_spans_with_missing_features_count;
        if errors_delta == 0 {
            self.check_expr(expr_id);
        }
    }

    fn check_spec_decl(
        &mut self,
        functor_set_value: FunctorSetValue,
        spec_decl: Option<&'a SpecDecl>,
    ) {
        let current_callable_id = self.get_current_callable();
        let ItemComputeProperties::Callable(callable_compute_properties) =
            self.compute_properties.get_item(current_callable_id)
        else {
            panic!("expected callable variant of item compute properties");
        };

        let spec_compute_properties = match functor_set_value {
            FunctorSetValue::Empty => &callable_compute_properties.body,
            FunctorSetValue::Adj => callable_compute_properties
                .adj
                .as_ref()
                .expect("adj specialization is none"),
            FunctorSetValue::Ctl => callable_compute_properties
                .ctl
                .as_ref()
                .expect("ctl specialization is none"),
            FunctorSetValue::CtlAdj => callable_compute_properties
                .ctl_adj
                .as_ref()
                .expect("ctl_adj specialization is none"),
        };

        if let ComputeKind::Quantum(quantum_properties) = spec_compute_properties.inherent {
            let missing_features = get_missing_runtime_features(
                quantum_properties.runtime_features,
                self.target_capabilities,
            );
            let missing_spec_level_runtime_features =
                get_spec_level_runtime_features(missing_features);

            // If there are any missing specialization-level runtime features, runtime features that affect the whole
            // specialization, just generate errors for the missing specialization-level runtime features and do not
            // generate statement-level or expression-level errors for these specializations.
            if !missing_spec_level_runtime_features.is_empty() {
                let current_callable = self
                    .package
                    .get_global(current_callable_id)
                    .expect("callable not present in package");
                let Global::Callable(callable_decl) = current_callable else {
                    panic!("");
                };

                self.missing_features_map
                    .entry(callable_decl.name.span)
                    .and_modify(|f| *f |= missing_spec_level_runtime_features)
                    .or_insert(missing_spec_level_runtime_features);
                return;
            }
        }

        // Visit the specialization block.
        if let Some(spec_decl) = spec_decl {
            self.visit_block(spec_decl.block);
        }
    }

    fn check_output_recording(&mut self, expr: &Expr) {
        let compute_kind = self.compute_properties.get_expr(expr.id).inherent;
        let ComputeKind::Quantum(quantum_properties) = compute_kind else {
            return;
        };

        let output_reporting_span = match &expr.kind {
            ExprKind::Call(callee_expr, _) if expr.span == Span::default() => {
                // Since this is auto-generated, use the callee expression span.
                self.get_expr(*callee_expr).span
            }
            _ => expr.span,
        };

        // Calculate the missing features but only consider the output recording flags.
        let missing_features = get_missing_runtime_features(
            quantum_properties.runtime_features,
            self.target_capabilities,
        ) & RuntimeFeatureFlags::output_recording_flags();
        if !missing_features.is_empty() {
            self.missing_features_map
                .entry(output_reporting_span)
                .and_modify(|f| *f |= missing_features)
                .or_insert(missing_features);
        }
    }

    fn clear_current_callable(&mut self) -> LocalItemId {
        self.current_callable
            .take()
            .expect("current callable is not set")
    }

    fn find_local_var_ident(&self, local_var_id: LocalVarId) -> Option<&Ident> {
        self.package
            .pats
            .iter()
            .map(|(_, pat)| pat)
            .find_map(|pat| {
                if let PatKind::Bind(ident) = &pat.kind {
                    if ident.id == local_var_id {
                        Some(ident)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
    }

    fn generate_errors(&mut self) -> Vec<Error> {
        let mut errors = Vec::new();
        let mut missing_features_map = self.missing_features_map.drain().collect::<Vec<_>>();
        missing_features_map.sort_unstable();
        for (span, missing_features) in missing_features_map {
            let mut span_errors = generate_errors_from_runtime_features(missing_features, span);
            errors.append(&mut span_errors);
        }
        errors
    }

    fn get_current_callable(&self) -> LocalItemId {
        self.current_callable.expect("current callable is not set")
    }

    fn is_expr_auto_generated(&self, expr: &Expr) -> bool {
        if expr.span.hi == 0 && expr.span.lo == 0 {
            return true;
        }

        if let ExprKind::Var(Res::Local(local_var_id), _) = expr.kind {
            let maybe_ident = self.find_local_var_ident(local_var_id);
            if let Some(ident) = maybe_ident {
                return ident.name.starts_with('@');
            }
        }

        false
    }

    fn set_current_callable(&mut self, id: LocalItemId) {
        assert!(self.current_callable.is_none());
        self.current_callable = Some(id);
    }
}

fn get_spec_level_runtime_features(runtime_features: RuntimeFeatureFlags) -> RuntimeFeatureFlags {
    const SPEC_LEVEL_RUNTIME_FEATURES: RuntimeFeatureFlags =
        RuntimeFeatureFlags::CyclicOperationSpec;
    runtime_features & SPEC_LEVEL_RUNTIME_FEATURES
}
