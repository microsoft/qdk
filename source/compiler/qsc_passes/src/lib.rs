// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod borrowck;
mod callable_limits;
mod capabilitiesck;
mod common;
mod conjugate_invert;
mod entry_point;
mod id_update;
mod index_assignment;
mod invert_block;
mod logic_sep;
mod loop_unification;
mod measurement;
mod replace_qubit_allocation;
mod reset;
mod spec_gen;
mod test_attribute;

use callable_limits::CallableLimits;
use capabilitiesck::{check_supported_capabilities, lower_store, run_rca_pass};
use entry_point::generate_entry_expr;
use index_assignment::ConvertToWSlash;
use loop_unification::LoopUni;
use miette::Diagnostic;
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_fir::fir;
use qsc_frontend::compile::CompileUnit;
use qsc_hir::{
    assigner::Assigner,
    global::{self, Table},
    hir::{Package, PackageId},
    mut_visit::MutVisitor,
    validate::Validator,
    visit::Visitor,
};
use qsc_lowerer::map_hir_package_to_fir;
use qsc_rca::{PackageComputeProperties, PackageStoreComputeProperties};
use replace_qubit_allocation::ReplaceQubitAllocation;
use thiserror::Error;

pub(crate) static CORE_NAMESPACE: &[&str] = &["Std", "Core"];
pub(crate) static QIR_RUNTIME_NAMESPACE: &[&str] = &["QIR", "Runtime"];

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
pub enum Error {
    BorrowCk(borrowck::Error),
    CallableLimits(callable_limits::Error),
    CapabilitiesCk(qsc_rca::errors::Error),
    ConjInvert(conjugate_invert::Error),
    EntryPoint(entry_point::Error),
    Measurement(measurement::Error),
    Reset(reset::Error),
    SpecGen(spec_gen::Error),
    TestAttribute(test_attribute::TestAttributeError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PackageType {
    Exe,
    Lib,
}

#[must_use]
pub fn lower_hir_to_fir(
    package_store: &qsc_frontend::compile::PackageStore,
    package_id: qsc_hir::hir::PackageId,
) -> (fir::PackageStore, fir::PackageId) {
    let fir_store = lower_store(package_store);
    let fir_package_id = map_hir_package_to_fir(package_id);
    (fir_store, fir_package_id)
}

pub struct PassContext {
    borrow_check: borrowck::Checker,
}

impl Default for PassContext {
    fn default() -> Self {
        Self::new()
    }
}

impl PassContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            borrow_check: borrowck::Checker::default(),
        }
    }

    /// Run the default set of passes required for evaluation.
    pub fn run_default_passes(
        &mut self,
        package: &mut Package,
        assigner: &mut Assigner,
        core: &Table,
        package_type: PackageType,
        package_id: PackageId,
    ) -> Vec<Error> {
        let mut call_limits = CallableLimits::default();
        call_limits.visit_package(package);
        let callable_errors = call_limits.errors;

        ConvertToWSlash { assigner }.visit_package(package);
        Validator::default().visit_package(package);

        self.borrow_check.visit_package(package);
        let borrow_errors = &mut self.borrow_check.errors;

        let spec_errors = spec_gen::generate_specs(core, package, assigner);
        Validator::default().visit_package(package);

        let conjugate_errors = conjugate_invert::invert_conjugate_exprs(core, package, assigner);
        Validator::default().visit_package(package);

        let measurement_decl_errors = measurement::validate_measurement_declarations(package);
        let reset_decl_errors = reset::validate_reset_declarations(package);

        let entry_point_errors = generate_entry_expr(package, assigner, package_type, package_id);
        Validator::default().visit_package(package);

        LoopUni { core, assigner }.visit_package(package);
        Validator::default().visit_package(package);

        ReplaceQubitAllocation::new(core, assigner).visit_package(package);
        Validator::default().visit_package(package);

        let test_attribute_errors = test_attribute::validate_test_attributes(package);
        Validator::default().visit_package(package);

        callable_errors
            .into_iter()
            .map(Error::CallableLimits)
            .chain(borrow_errors.drain(..).map(Error::BorrowCk))
            .chain(spec_errors.into_iter().map(Error::SpecGen))
            .chain(conjugate_errors.into_iter().map(Error::ConjInvert))
            .chain(entry_point_errors)
            .chain(measurement_decl_errors.into_iter().map(Error::Measurement))
            .chain(reset_decl_errors.into_iter().map(Error::Reset))
            .chain(test_attribute_errors.into_iter().map(Error::TestAttribute))
            .collect()
    }

    pub fn run_fir_passes_on_fir(
        fir_store: &qsc_fir::fir::PackageStore,
        package_id: qsc_fir::fir::PackageId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<PackageStoreComputeProperties, Vec<Error>> {
        run_rca_pass(fir_store, package_id, capabilities)
    }
}

/// Run the default set of passes required for evaluation.
pub fn run_default_passes(
    core: &Table,
    unit: &mut CompileUnit,
    package_type: PackageType,
    package_id: PackageId,
) -> Vec<Error> {
    PassContext::new().run_default_passes(
        &mut unit.package,
        &mut unit.assigner,
        core,
        package_type,
        package_id,
    )
}

pub fn run_core_passes(core: &mut CompileUnit) -> Vec<Error> {
    let mut borrow_check = borrowck::Checker::default();
    borrow_check.visit_package(&core.package);
    let borrow_errors = borrow_check.errors;

    let table = global::iter_package(PackageId::CORE, &core.package).collect();
    LoopUni {
        core: &table,
        assigner: &mut core.assigner,
    }
    .visit_package(&mut core.package);
    Validator::default().visit_package(&core.package);

    ReplaceQubitAllocation::new(&table, &mut core.assigner).visit_package(&mut core.package);
    Validator::default().visit_package(&core.package);

    borrow_errors.into_iter().map(Error::BorrowCk).collect()
}

pub fn run_fir_passes(
    package: &fir::Package,
    compute_properties: &PackageComputeProperties,
    capabilities: TargetCapabilityFlags,
    store: &fir::PackageStore,
) -> Vec<Error> {
    let capabilities_errors =
        check_supported_capabilities(package, compute_properties, capabilities, store);
    capabilities_errors
        .into_iter()
        .map(Error::CapabilitiesCk)
        .collect()
}
