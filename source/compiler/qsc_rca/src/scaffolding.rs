// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    ApplicationGeneratorSet, CallableComputeProperties, ComputePropertiesLookup,
    ItemComputeProperties, PackageComputeProperties, PackageStoreComputeProperties,
    common::GlobalSpecId,
};
use qsc_data_structures::index_map::IndexMap;
use qsc_fir::{
    fir::{
        self, BlockId, ExprId, LocalItemId, PackageId, StmtId, StoreBlockId, StoreExprId,
        StoreItemId, StoreStmtId,
    },
    ty::FunctorSetValue,
};
use rustc_hash::FxHashSet;

/// Scaffolding used to build the package store compute properties.
#[derive(Debug)]
pub struct InternalPackageStoreComputeProperties {
    // The compute properties for each package in the store, keyed by package ID.
    props: IndexMap<PackageId, InternalPackageComputeProperties>,
    // The alternative compute properties for each package if the callables are invoked
    // from within a parallel expression, keyed by package ID.
    parallel_props: IndexMap<PackageId, InternalPackageComputeProperties>,
    // The set of global callables that must be inlined (usually due to qubit allocation) and
    // cause any caller to also require inlining.
    must_inline_callables: FxHashSet<GlobalSpecId>,
    // The set of call expressions that must be inlined either because of the callable being invoked,
    // or because of the combination of callable and runtime features of the arguments to the call expr.
    must_inline_call_exprs: FxHashSet<StoreExprId>,
}

impl From<PackageStoreComputeProperties> for InternalPackageStoreComputeProperties {
    fn from(value: PackageStoreComputeProperties) -> Self {
        let mut scaffolding = IndexMap::<PackageId, InternalPackageComputeProperties>::default();
        for (package_id, package_compute_properties) in value.props {
            let mut items = IndexMap::<LocalItemId, InternalItemComputeProperties>::new();
            for (item_id, item_compute_properties) in package_compute_properties.items {
                let item_scaffolding = InternalItemComputeProperties::from(item_compute_properties);
                items.insert(item_id, item_scaffolding);
            }
            let package_compute_properties = InternalPackageComputeProperties {
                items,
                blocks: package_compute_properties.blocks,
                stmts: package_compute_properties.stmts,
                exprs: package_compute_properties.exprs,
                unresolved_callee_exprs: package_compute_properties
                    .unresolved_callee_exprs
                    .into_iter()
                    .collect(),
            };
            scaffolding.insert(package_id, package_compute_properties);
        }
        let mut parallel_scaffolding =
            IndexMap::<PackageId, InternalPackageComputeProperties>::default();
        for (package_id, package_compute_properties) in value.parallel_props {
            let mut items = IndexMap::<LocalItemId, InternalItemComputeProperties>::new();
            for (item_id, item_compute_properties) in package_compute_properties.items {
                let item_scaffolding = InternalItemComputeProperties::from(item_compute_properties);
                items.insert(item_id, item_scaffolding);
            }
            let package_compute_properties = InternalPackageComputeProperties {
                items,
                blocks: package_compute_properties.blocks,
                stmts: package_compute_properties.stmts,
                exprs: package_compute_properties.exprs,
                unresolved_callee_exprs: package_compute_properties
                    .unresolved_callee_exprs
                    .into_iter()
                    .collect(),
            };
            parallel_scaffolding.insert(package_id, package_compute_properties);
        }

        Self {
            props: scaffolding,
            parallel_props: parallel_scaffolding,
            must_inline_callables: value.must_inline_callables,
            must_inline_call_exprs: value.must_inline_call_exprs,
        }
    }
}

impl From<InternalPackageStoreComputeProperties> for PackageStoreComputeProperties {
    fn from(value: InternalPackageStoreComputeProperties) -> Self {
        let mut package_store_compute_properties =
            IndexMap::<PackageId, PackageComputeProperties>::default();
        for (package_id, package_scaffolding) in value.props {
            let mut items = IndexMap::<LocalItemId, ItemComputeProperties>::new();
            for (item_id, item_scaffolding) in package_scaffolding.items {
                let item_compute_properties = ItemComputeProperties::from(item_scaffolding);
                items.insert(item_id, item_compute_properties);
            }

            let package_compute_properties = PackageComputeProperties {
                items,
                blocks: package_scaffolding.blocks,
                stmts: package_scaffolding.stmts,
                exprs: package_scaffolding.exprs,
                unresolved_callee_exprs: package_scaffolding
                    .unresolved_callee_exprs
                    .into_iter()
                    .collect(),
            };
            package_store_compute_properties.insert(package_id, package_compute_properties);
        }
        let mut parallel_package_store_compute_properties =
            IndexMap::<PackageId, PackageComputeProperties>::default();
        for (package_id, package_scaffolding) in value.parallel_props {
            let mut items = IndexMap::<LocalItemId, ItemComputeProperties>::new();
            for (item_id, item_scaffolding) in package_scaffolding.items {
                let item_compute_properties = ItemComputeProperties::from(item_scaffolding);
                items.insert(item_id, item_compute_properties);
            }

            let package_compute_properties = PackageComputeProperties {
                items,
                blocks: package_scaffolding.blocks,
                stmts: package_scaffolding.stmts,
                exprs: package_scaffolding.exprs,
                unresolved_callee_exprs: package_scaffolding
                    .unresolved_callee_exprs
                    .into_iter()
                    .collect(),
            };
            parallel_package_store_compute_properties
                .insert(package_id, package_compute_properties);
        }
        Self {
            props: package_store_compute_properties,
            parallel_props: parallel_package_store_compute_properties,
            must_inline_callables: value.must_inline_callables,
            must_inline_call_exprs: value.must_inline_call_exprs,
        }
    }
}

impl ComputePropertiesLookup for InternalPackageStoreComputeProperties {
    fn find_block(&self, id: StoreBlockId, parallel: bool) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package, parallel).blocks.get(id.block)
    }

    fn find_expr(&self, id: StoreExprId, parallel: bool) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package, parallel).exprs.get(id.expr)
    }

    fn find_item(&self, _: StoreItemId, _: bool) -> Option<&ItemComputeProperties> {
        unimplemented!()
    }

    fn find_stmt(&self, id: StoreStmtId, parallel: bool) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package, parallel).stmts.get(id.stmt)
    }

    fn get_block(&self, id: StoreBlockId, parallel: bool) -> &ApplicationGeneratorSet {
        self.find_block(id, parallel)
            .expect("block compute properties should exist")
    }

    fn get_expr(&self, id: StoreExprId, parallel: bool) -> &ApplicationGeneratorSet {
        self.find_expr(id, parallel)
            .expect("expression compute properties should exist")
    }

    fn get_item(&self, _: StoreItemId, _: bool) -> &ItemComputeProperties {
        unimplemented!()
    }

    fn get_stmt(&self, id: StoreStmtId, parallel: bool) -> &ApplicationGeneratorSet {
        self.find_stmt(id, parallel)
            .expect("statement compute properties should exist")
    }
}

impl InternalPackageStoreComputeProperties {
    pub(crate) fn find_specialization(
        &self,
        id: GlobalSpecId,
        parallel: bool,
    ) -> Option<&ApplicationGeneratorSet> {
        self.get(id.callable.package, parallel)
            .items
            .get(id.callable.item)
            .and_then(|item_compute_properties| match item_compute_properties {
                InternalItemComputeProperties::NonCallable => None,
                InternalItemComputeProperties::Specializations(specializations) => {
                    Some(specializations)
                }
            })
            .and_then(|specializations| {
                specializations.get(SpecializationIndex::from(id.functor_set_value))
            })
    }

    pub(crate) fn get(&self, id: PackageId, parallel: bool) -> &InternalPackageComputeProperties {
        if parallel {
            self.parallel_props
                .get(id)
                .expect("package compute properties should be present in store")
        } else {
            self.props
                .get(id)
                .expect("package compute properties should be present in store")
        }
    }

    pub(crate) fn get_mut(
        &mut self,
        id: PackageId,
        parallel: bool,
    ) -> &mut InternalPackageComputeProperties {
        if parallel {
            self.parallel_props
                .get_mut(id)
                .expect("package compute properties should be present in store")
        } else {
            self.props
                .get_mut(id)
                .expect("package compute properties should be present in store")
        }
    }

    pub(crate) fn get_spec(&self, id: GlobalSpecId, parallel: bool) -> &ApplicationGeneratorSet {
        self.find_specialization(id, parallel)
            .expect("specialization should exist")
    }

    pub(crate) fn init(package_store: &fir::PackageStore) -> Self {
        let mut packages = IndexMap::<PackageId, InternalPackageComputeProperties>::default();
        let mut parallel_packages =
            IndexMap::<PackageId, InternalPackageComputeProperties>::default();
        for (package_id, _) in package_store {
            packages.insert(package_id, InternalPackageComputeProperties::default());
            parallel_packages.insert(package_id, InternalPackageComputeProperties::default());
        }
        Self {
            props: packages,
            parallel_props: parallel_packages,
            must_inline_callables: Default::default(),
            must_inline_call_exprs: Default::default(),
        }
    }

    pub(crate) fn insert_ty_item(
        &mut self,
        id: StoreItemId,
        value: InternalItemComputeProperties,
        parallel: bool,
    ) {
        self.get_mut(id.package, parallel)
            .items
            .insert(id.item, value);
    }

    pub(crate) fn insert_spec(
        &mut self,
        id: GlobalSpecId,
        value: ApplicationGeneratorSet,
        parallel: bool,
    ) {
        let items = &mut self.get_mut(id.callable.package, parallel).items;
        if let Some(item_compute_properties) = items.get_mut(id.callable.item) {
            if let InternalItemComputeProperties::Specializations(specializations) =
                item_compute_properties
            {
                // The item already exists but not the specialization.
                specializations.insert(SpecializationIndex::from(id.functor_set_value), value);
            } else {
                panic!("item should be a callable");
            }
        } else {
            // Insert both the specialization and the item.
            let mut specializations = IndexMap::new();
            specializations.insert(SpecializationIndex::from(id.functor_set_value), value);
            items.insert(
                id.callable.item,
                InternalItemComputeProperties::Specializations(specializations),
            );
        }
    }

    pub(crate) fn insert_stmt(
        &mut self,
        id: StoreStmtId,
        value: ApplicationGeneratorSet,
        parallel: bool,
    ) {
        self.get_mut(id.package, parallel)
            .stmts
            .insert(id.stmt, value);
    }

    pub(crate) fn insert_must_inline_callable(&mut self, id: GlobalSpecId) {
        self.must_inline_callables.insert(id);
    }

    pub(crate) fn is_must_inline_callable(&self, id: GlobalSpecId) -> bool {
        self.must_inline_callables.contains(&id)
    }

    pub(crate) fn insert_must_inline_call_expr(&mut self, id: StoreExprId) {
        self.must_inline_call_exprs.insert(id);
    }
}

/// Scaffolding used to build the compute properties of a package.
#[derive(Debug, Default)]
pub struct InternalPackageComputeProperties {
    /// The compute properties of the package items.
    pub items: IndexMap<LocalItemId, InternalItemComputeProperties>,
    /// The application generator sets of the package blocks.
    pub blocks: IndexMap<BlockId, ApplicationGeneratorSet>,
    /// The application generator sets of the package statements.
    pub stmts: IndexMap<StmtId, ApplicationGeneratorSet>,
    /// The application generator sets of the package expressions.
    pub exprs: IndexMap<ExprId, ApplicationGeneratorSet>,
    /// The expressions that were unresolved callees at analysis time.
    pub unresolved_callee_exprs: Vec<ExprId>,
}

/// Scaffolding used to build the compute properties of an item.
#[derive(Debug, Default)]
pub enum InternalItemComputeProperties {
    #[default]
    NonCallable,
    Specializations(SpecializationsComputeProperties),
}

impl From<ItemComputeProperties> for InternalItemComputeProperties {
    fn from(value: ItemComputeProperties) -> Self {
        match value {
            ItemComputeProperties::NonCallable => InternalItemComputeProperties::NonCallable,
            ItemComputeProperties::Callable(callable_compute_properties) => {
                InternalItemComputeProperties::Specializations(
                    SpecializationsComputeProperties::from(callable_compute_properties),
                )
            }
        }
    }
}

impl From<InternalItemComputeProperties> for ItemComputeProperties {
    fn from(value: InternalItemComputeProperties) -> Self {
        match value {
            InternalItemComputeProperties::NonCallable => ItemComputeProperties::NonCallable,
            InternalItemComputeProperties::Specializations(specializations) => {
                ItemComputeProperties::Callable(CallableComputeProperties::from(specializations))
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct SpecializationIndex(usize);

impl From<SpecializationIndex> for usize {
    fn from(value: SpecializationIndex) -> Self {
        value.0
    }
}

impl From<usize> for SpecializationIndex {
    fn from(value: usize) -> Self {
        SpecializationIndex(value)
    }
}

impl From<SpecializationIndex> for FunctorSetValue {
    fn from(value: SpecializationIndex) -> Self {
        match value {
            SpecializationIndex(0) => Self::Empty,
            SpecializationIndex(1) => Self::Adj,
            SpecializationIndex(2) => Self::Ctl,
            SpecializationIndex(3) => Self::CtlAdj,
            _ => panic!("invalid specialization index"),
        }
    }
}

impl From<FunctorSetValue> for SpecializationIndex {
    fn from(value: FunctorSetValue) -> Self {
        match value {
            FunctorSetValue::Empty => SpecializationIndex(0),
            FunctorSetValue::Adj => SpecializationIndex(1),
            FunctorSetValue::Ctl => SpecializationIndex(2),
            FunctorSetValue::CtlAdj => SpecializationIndex(3),
        }
    }
}

pub type SpecializationsComputeProperties = IndexMap<SpecializationIndex, ApplicationGeneratorSet>;

impl From<CallableComputeProperties> for SpecializationsComputeProperties {
    fn from(value: CallableComputeProperties) -> Self {
        let mut specializations = SpecializationsComputeProperties::default();
        specializations.insert(FunctorSetValue::Empty.into(), value.body);
        if let Some(adj_applications_table) = value.adj {
            specializations.insert(FunctorSetValue::Adj.into(), adj_applications_table);
        }
        if let Some(ctl_applications_table) = value.ctl {
            specializations.insert(FunctorSetValue::Ctl.into(), ctl_applications_table);
        }
        if let Some(ctl_adj_applications_table) = value.ctl_adj {
            specializations.insert(FunctorSetValue::CtlAdj.into(), ctl_adj_applications_table);
        }
        specializations
    }
}

impl From<SpecializationsComputeProperties> for CallableComputeProperties {
    fn from(value: SpecializationsComputeProperties) -> Self {
        let (mut body, mut adj, mut ctl, mut ctl_adj) = (
            Option::<ApplicationGeneratorSet>::default(),
            Option::<ApplicationGeneratorSet>::default(),
            Option::<ApplicationGeneratorSet>::default(),
            Option::<ApplicationGeneratorSet>::default(),
        );
        for (specialization_index, applications_table) in value {
            match specialization_index.into() {
                FunctorSetValue::Empty => body = Some(applications_table),
                FunctorSetValue::Adj => adj = Some(applications_table),
                FunctorSetValue::Ctl => ctl = Some(applications_table),
                FunctorSetValue::CtlAdj => ctl_adj = Some(applications_table),
            }
        }

        // If the body is not present, this is likely a callable where only
        // a non-body specialization is invoked in a parallel expression. We allow
        // the body to be effectively empty by using default, since only the non-body
        // specialization compute properties are relevant.
        CallableComputeProperties {
            body: body.unwrap_or_default(),
            adj,
            ctl,
            ctl_adj,
        }
    }
}
