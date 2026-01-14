// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod arrays;
mod assigns;
mod bindings;
mod binops;
mod callables;
mod calls;
mod cycles;
mod ifs;
mod intrinsics;
mod lambdas;
mod loops;
mod measurements;
mod overrides;
mod qubits;
mod strings;
mod structs;
mod types;
mod udts;
mod vars;

use crate::{Analyzer, ComputePropertiesLookup, PackageStoreComputeProperties};
use expect_test::Expect;
use qsc::incremental::Compiler;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_fir::fir::{ItemKind, LocalItemId, Package, PackageStore, StoreItemId};
use qsc_frontend::compile::PackageStore as HirPackageStore;
use qsc_lowerer::{Lowerer, map_hir_package_to_fir};
use qsc_passes::PackageType;

pub struct CompilationContext {
    pub compiler: Compiler,
    pub fir_store: PackageStore,
    pub compute_properties: PackageStoreComputeProperties,
    lowerer: Lowerer,
}

impl CompilationContext {
    #[must_use]
    pub fn new(capabilities: TargetCapabilityFlags) -> Self {
        let (std_id, store) = qsc::compile::package_store_with_stdlib(capabilities);
        let compiler = Compiler::new(
            SourceMap::default(),
            PackageType::Lib,
            capabilities,
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
        )
        .expect("should be able to create a new compiler");
        let fir_store = lower_hir_package_store(compiler.package_store());
        let analyzer = Analyzer::init(&fir_store);
        let compute_properties = analyzer.analyze_all();
        Self {
            compiler,
            fir_store,
            compute_properties,
            lowerer: Lowerer::new(),
        }
    }

    #[must_use]
    pub fn get_compute_properties(&self) -> &PackageStoreComputeProperties {
        &self.compute_properties
    }

    pub fn update(&mut self, source: &str) {
        let increment = self
            .compiler
            .compile_fragments_fail_fast("rca-test", source)
            .expect("code should compile");
        let package_id = map_hir_package_to_fir(self.compiler.package_id());
        let fir_package = self.fir_store.get_mut(package_id);
        self.lowerer
            .lower_and_update_package(fir_package, &increment.hir);
        self.compiler.update(increment);

        // Clear the compute properties of the package to update.
        let package_compute_properties = self.compute_properties.get_mut(package_id);
        package_compute_properties.clear();
        let analyzer = Analyzer::init_with_compute_properties(
            &self.fir_store,
            self.compute_properties.clone(),
        );
        self.compute_properties = analyzer.analyze_package(package_id);
    }
}

impl Default for CompilationContext {
    fn default() -> Self {
        Self::new(TargetCapabilityFlags::all())
    }
}

pub trait PackageStoreSearch {
    fn find_callable_id_by_name(&self, name: &str) -> Option<StoreItemId>;
}

impl PackageStoreSearch for PackageStore {
    fn find_callable_id_by_name(&self, name: &str) -> Option<StoreItemId> {
        for (package_id, package) in self {
            if let Some(item_id) = package.find_callable_id_by_name(name) {
                return Some((package_id, item_id).into());
            }
        }

        None
    }
}

pub trait PackageSearch {
    fn find_callable_id_by_name(&self, name: &str) -> Option<LocalItemId>;
}

impl PackageSearch for Package {
    fn find_callable_id_by_name(&self, name: &str) -> Option<LocalItemId> {
        for (item_id, item) in &self.items {
            if let ItemKind::Callable(callable_decl) = &item.kind
                && callable_decl.name.name.as_ref() == name
            {
                return Some(item_id);
            }
        }

        None
    }
}

pub fn check_callable_compute_properties(
    fir_package_store: &impl PackageStoreSearch,
    package_store_compute_properties: &PackageStoreComputeProperties,
    callable_name: &str,
    expect: &Expect,
) {
    let callable_id = fir_package_store
        .find_callable_id_by_name(callable_name)
        .expect("callable should exist");

    let callable_compute_properties = package_store_compute_properties.get_item(callable_id);
    expect.assert_eq(&callable_compute_properties.to_string());
}

pub fn check_last_statement_compute_properties(
    package_store_compute_properties: &PackageStoreComputeProperties,
    expect: &Expect,
) {
    let last_package_id = package_store_compute_properties
        .iter()
        .map(|(package_id, _)| package_id)
        .max()
        .expect("at least one package should exist");
    let package_compute_properties = package_store_compute_properties.get(last_package_id);
    let last_statement_id = package_compute_properties
        .stmts
        .iter()
        .map(|(stmt_id, _)| stmt_id)
        .max()
        .expect("at least one statement should exist");
    let stmt_compute_properties = package_compute_properties
        .stmts
        .get(last_statement_id)
        .expect("statement compute properties should exist");
    expect.assert_eq(&stmt_compute_properties.to_string());
}

fn lower_hir_package_store(hir_package_store: &HirPackageStore) -> PackageStore {
    let mut fir_store = PackageStore::new();
    for (id, unit) in hir_package_store {
        let mut lowerer = Lowerer::new();
        fir_store.insert(
            map_hir_package_to_fir(id),
            lowerer.lower_package(&unit.package, &fir_store),
        );
    }
    fir_store
}
