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

/// Scaffolding used to build the package store compute properties.
#[derive(Debug)]
pub struct InternalPackageStoreComputeProperties(
    IndexMap<PackageId, InternalPackageComputeProperties>,
);

impl From<PackageStoreComputeProperties> for InternalPackageStoreComputeProperties {
    fn from(value: PackageStoreComputeProperties) -> Self {
        let mut scaffolding = IndexMap::<PackageId, InternalPackageComputeProperties>::default();
        for (package_id, package_compute_properties) in value.0 {
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

        Self(scaffolding)
    }
}

impl From<InternalPackageStoreComputeProperties> for PackageStoreComputeProperties {
    fn from(value: InternalPackageStoreComputeProperties) -> Self {
        let mut package_store_compute_properties =
            IndexMap::<PackageId, PackageComputeProperties>::default();
        for (package_id, package_scaffolding) in value.0 {
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
        Self(package_store_compute_properties)
    }
}

impl ComputePropertiesLookup for InternalPackageStoreComputeProperties {
    fn find_block(&self, id: StoreBlockId) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package).blocks.get(id.block)
    }

    fn find_expr(&self, id: StoreExprId) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package).exprs.get(id.expr)
    }

    fn find_item(&self, _: StoreItemId) -> Option<&ItemComputeProperties> {
        unimplemented!()
    }

    fn find_stmt(&self, id: StoreStmtId) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package).stmts.get(id.stmt)
    }

    fn get_block(&self, id: StoreBlockId) -> &ApplicationGeneratorSet {
        self.find_block(id)
            .expect("block compute properties should exist")
    }

    fn get_expr(&self, id: StoreExprId) -> &ApplicationGeneratorSet {
        self.find_expr(id)
            .expect("expression compute properties should exist")
    }

    fn get_item(&self, _: StoreItemId) -> &ItemComputeProperties {
        unimplemented!()
    }

    fn get_stmt(&self, id: StoreStmtId) -> &ApplicationGeneratorSet {
        self.find_stmt(id)
            .expect("statement compute properties should exist")
    }
}

impl InternalPackageStoreComputeProperties {
    pub fn find_specialization(&self, id: GlobalSpecId) -> Option<&ApplicationGeneratorSet> {
        self.get(id.callable.package)
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

    pub fn get(&self, id: PackageId) -> &InternalPackageComputeProperties {
        self.0
            .get(id)
            .expect("package compute properties should be present in store")
    }

    pub fn get_mut(&mut self, id: PackageId) -> &mut InternalPackageComputeProperties {
        self.0
            .get_mut(id)
            .expect("package compute properties should be present in store")
    }

    pub fn get_spec(&self, id: GlobalSpecId) -> &ApplicationGeneratorSet {
        self.find_specialization(id)
            .expect("specialization should exist")
    }

    pub fn init(package_store: &fir::PackageStore) -> Self {
        let mut packages = IndexMap::<PackageId, InternalPackageComputeProperties>::default();
        for (package_id, _) in package_store {
            packages.insert(package_id, InternalPackageComputeProperties::default());
        }
        Self(packages)
    }

    pub fn insert_block(&mut self, id: StoreBlockId, value: ApplicationGeneratorSet) {
        self.get_mut(id.package)
            .blocks
            .insert_if_absent(id.block, value);
    }

    pub fn insert_expr(&mut self, id: StoreExprId, value: ApplicationGeneratorSet) {
        self.get_mut(id.package)
            .exprs
            .insert_if_absent(id.expr, value);
    }

    pub fn insert_item(&mut self, id: StoreItemId, value: InternalItemComputeProperties) {
        self.get_mut(id.package).items.insert(id.item, value);
    }

    pub fn insert_spec(&mut self, id: GlobalSpecId, value: ApplicationGeneratorSet) {
        let items = &mut self.get_mut(id.callable.package).items;
        if let Some(item_compute_properties) = items.get_mut(id.callable.item) {
            if let InternalItemComputeProperties::Specializations(specializations) =
                item_compute_properties
            {
                // The item already exists but not the specialization.
                specializations
                    .insert_if_absent(SpecializationIndex::from(id.functor_set_value), value);
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

    pub fn insert_stmt(&mut self, id: StoreStmtId, value: ApplicationGeneratorSet) {
        self.get_mut(id.package)
            .stmts
            .insert_if_absent(id.stmt, value);
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

        CallableComputeProperties {
            body: body.expect("body should exist"),
            adj,
            ctl,
            ctl_adj,
        }
    }
}
