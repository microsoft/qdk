// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Runtime Capabilities Analysis (RCA) is the process of determining the capabilities a quantum kernel needs to be able
//! to run a particular program. This implementation also identifies program elements that can be pre-computed before
//! execution on a quantum kernel and does not consider these elements when determining the capabilities. Additionally,
//! this implementation also provides details on why the program requires each capability.

#[cfg(test)]
mod tests;

mod analyzer;
mod applications;
mod common;
mod core;
mod cycle_detection;
mod cyclic_callables;
pub mod errors;
mod overrider;
mod scaffolding;

use crate::common::set_indentation;
use bitflags::bitflags;
use indenter::indented;
use qsc_data_structures::{
    index_map::{IndexMap, Iter},
    target::TargetCapabilityFlags,
};
use qsc_fir::{
    fir::{
        BlockId, ExprId, LocalItemId, PackageId, StmtId, StoreBlockId, StoreExprId, StoreItemId,
        StoreStmtId,
    },
    ty::Ty,
};
use rustc_hash::FxHashSet;

use std::{
    cmp::Ord,
    fmt::{self, Debug, Display, Formatter, Write},
};

pub use crate::analyzer::Analyzer;

/// A trait to look for the compute properties of elements in a package store.
pub trait ComputePropertiesLookup {
    /// Searches for the application generator set of a block with the specified ID.
    fn find_block(&self, id: StoreBlockId) -> Option<&ApplicationGeneratorSet>;
    /// Searches for the application generator set of an expression with the specified ID.
    fn find_expr(&self, id: StoreExprId) -> Option<&ApplicationGeneratorSet>;
    /// Searches for the compute properties of an item with the specified ID.
    fn find_item(&self, id: StoreItemId) -> Option<&ItemComputeProperties>;
    /// Searches for the application generator set of a statement with the specified ID.
    fn find_stmt(&self, id: StoreStmtId) -> Option<&ApplicationGeneratorSet>;
    /// Gets the application generator set of a block.
    fn get_block(&self, id: StoreBlockId) -> &ApplicationGeneratorSet;
    /// Gets the application generator set of an expression.
    fn get_expr(&self, id: StoreExprId) -> &ApplicationGeneratorSet;
    /// Gets the compute properties of an item.
    fn get_item(&self, id: StoreItemId) -> &ItemComputeProperties;
    /// Gets the application generator set of a statement.
    fn get_stmt(&self, id: StoreStmtId) -> &ApplicationGeneratorSet;
}

/// The compute properties of a package store.
#[derive(Clone, Debug, Default)]
pub struct PackageStoreComputeProperties(IndexMap<PackageId, PackageComputeProperties>);

impl ComputePropertiesLookup for PackageStoreComputeProperties {
    fn find_block(&self, id: StoreBlockId) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package).blocks.get(id.block)
    }

    fn find_expr(&self, id: StoreExprId) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package).exprs.get(id.expr)
    }

    fn find_item(&self, id: StoreItemId) -> Option<&ItemComputeProperties> {
        self.get(id.package).items.get(id.item)
    }

    fn find_stmt(&self, id: StoreStmtId) -> Option<&ApplicationGeneratorSet> {
        self.get(id.package).stmts.get(id.stmt)
    }

    fn get_block(&self, id: StoreBlockId) -> &ApplicationGeneratorSet {
        self.find_block(id)
            .expect("block compute properties not found")
    }

    fn get_expr(&self, id: StoreExprId) -> &ApplicationGeneratorSet {
        self.find_expr(id)
            .expect("expression compute properties not found")
    }

    fn get_item(&self, id: StoreItemId) -> &ItemComputeProperties {
        self.find_item(id)
            .expect("item compute properties not found")
    }

    fn get_stmt(&self, id: StoreStmtId) -> &ApplicationGeneratorSet {
        self.find_stmt(id)
            .expect("statement compute properties not found")
    }
}

impl<'a> IntoIterator for &'a PackageStoreComputeProperties {
    type IntoIter = qsc_data_structures::index_map::Iter<'a, PackageId, PackageComputeProperties>;
    type Item = (PackageId, &'a PackageComputeProperties);

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl PackageStoreComputeProperties {
    #[must_use]
    pub fn get(&self, id: PackageId) -> &PackageComputeProperties {
        self.0.get(id).expect("package should exist")
    }

    #[must_use]
    pub fn get_mut(&mut self, id: PackageId) -> &mut PackageComputeProperties {
        self.0.get_mut(id).expect("package should exist")
    }

    pub fn insert_block(&mut self, id: StoreBlockId, value: ApplicationGeneratorSet) {
        self.get_mut(id.package).blocks.insert(id.block, value);
    }

    pub fn insert_expr(&mut self, id: StoreExprId, value: ApplicationGeneratorSet) {
        self.get_mut(id.package).exprs.insert(id.expr, value);
    }

    pub fn insert_item(&mut self, id: StoreItemId, value: ItemComputeProperties) {
        self.get_mut(id.package).items.insert(id.item, value);
    }

    pub fn insert_stmt(&mut self, id: StoreStmtId, value: ApplicationGeneratorSet) {
        self.get_mut(id.package).stmts.insert(id.stmt, value);
    }

    #[must_use]
    pub fn iter(&self) -> Iter<'_, PackageId, PackageComputeProperties> {
        self.0.iter()
    }

    #[must_use]
    pub fn is_unresolved_callee_expr(&self, id: StoreExprId) -> bool {
        self.get(id.package)
            .unresolved_callee_exprs
            .contains(&id.expr)
    }
}

/// The compute properties of a package.
#[derive(Clone, Debug)]
pub struct PackageComputeProperties {
    /// The compute properties of the package items.
    pub items: IndexMap<LocalItemId, ItemComputeProperties>,
    /// The application generator sets of the package blocks.
    pub blocks: IndexMap<BlockId, ApplicationGeneratorSet>,
    /// The application generator sets of the package statements.
    pub stmts: IndexMap<StmtId, ApplicationGeneratorSet>,
    /// The application generator sets of the package expressions.
    pub exprs: IndexMap<ExprId, ApplicationGeneratorSet>,
    /// The expressions that were unresolved callees at analysis time.
    pub unresolved_callee_exprs: FxHashSet<ExprId>,
}

impl Default for PackageComputeProperties {
    fn default() -> Self {
        Self {
            items: IndexMap::new(),
            blocks: IndexMap::new(),
            stmts: IndexMap::new(),
            exprs: IndexMap::new(),
            unresolved_callee_exprs: FxHashSet::default(),
        }
    }
}

impl Display for PackageComputeProperties {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "Package:")?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nItems:")?;
        indent = set_indentation(indent, 2);
        for (item_id, item) in self.items.iter() {
            write!(indent, "\nItem {item_id}: {item}")?;
        }
        indent = set_indentation(indent, 1);
        write!(indent, "\nBlocks:")?;
        indent = set_indentation(indent, 2);
        for (block_id, block) in self.blocks.iter() {
            write!(indent, "\nBlock {block_id}: {block}")?;
        }
        indent = set_indentation(indent, 1);
        write!(indent, "\nStmts:")?;
        indent = set_indentation(indent, 2);
        for (stmt_id, stmt) in self.stmts.iter() {
            write!(indent, "\nStmt {stmt_id}: {stmt}")?;
        }
        indent = set_indentation(indent, 1);
        write!(indent, "\nExprs:")?;
        indent = set_indentation(indent, 2);
        for (expr_id, expr) in self.exprs.iter() {
            write!(indent, "\nExpr {expr_id}: {expr}")?;
        }
        Ok(())
    }
}

impl PackageComputeProperties {
    pub fn clear(&mut self) {
        self.items.clear();
        self.blocks.clear();
        self.stmts.clear();
        self.exprs.clear();
    }

    #[must_use]
    pub fn get_block(&self, id: BlockId) -> &ApplicationGeneratorSet {
        self.blocks
            .get(id)
            .expect("block compute properties not found")
    }

    #[must_use]
    pub fn get_expr(&self, id: ExprId) -> &ApplicationGeneratorSet {
        self.exprs
            .get(id)
            .expect("expression compute properties not found")
    }

    #[must_use]
    pub fn get_item(&self, id: LocalItemId) -> &ItemComputeProperties {
        self.items
            .get(id)
            .expect("item compute properties not found")
    }

    #[must_use]
    pub fn get_stmt(&self, id: StmtId) -> &ApplicationGeneratorSet {
        self.stmts
            .get(id)
            .expect("statement compute properties not found")
    }
}

/// The compute properties of an item.
#[derive(Clone, Debug)]
pub enum ItemComputeProperties {
    /// The compute properties of a callable.
    Callable(CallableComputeProperties),
    /// The compute properties of a non-callable (for completeness only).
    NonCallable,
}

impl Display for ItemComputeProperties {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            ItemComputeProperties::Callable(callable_compute_properties) => {
                write!(f, "Callable: {callable_compute_properties}")
            }
            ItemComputeProperties::NonCallable => write!(f, "NonCallable"),
        }
    }
}

/// The compute properties of a callable.
#[derive(Clone, Debug)]
pub struct CallableComputeProperties {
    /// The application generator set for the callable's body.
    pub body: ApplicationGeneratorSet,
    /// The application generator set for the callable's adjoint specialization.
    pub adj: Option<ApplicationGeneratorSet>,
    /// The application generator set for the callable's controlled specialization.
    pub ctl: Option<ApplicationGeneratorSet>,
    /// The application generator set for the callable's controlled adjoint specialization.
    pub ctl_adj: Option<ApplicationGeneratorSet>,
}

impl Display for CallableComputeProperties {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "CallableComputeProperties:",)?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nbody: {}", self.body)?;
        match &self.adj {
            Some(spec) => write!(indent, "\nadj: {spec}")?,
            None => write!(indent, "\nadj: <none>")?,
        }
        match &self.ctl {
            Some(spec) => write!(indent, "\nctl: {spec}")?,
            None => write!(indent, "\nctl: <none>")?,
        }
        match &self.ctl_adj {
            Some(spec) => write!(indent, "\nctl-adj: {spec}")?,
            None => write!(indent, "\nctl-adj: <none>")?,
        }
        Ok(())
    }
}

/// A set of compute properties associated to a callable or one of its elements, from which the properties of any
/// particular call application can be derived.
#[derive(Clone, Debug)]
pub struct ApplicationGeneratorSet {
    /// The inherent compute kind of a program element, which is determined by binding all the parameters it depends on
    /// to static values.
    pub inherent: ComputeKind,
    /// Each element in the vector represents the compute kind(s) of a call application when the parameter associated to
    /// the vector index is bound to a dynamic value.
    pub(crate) dynamic_param_applications: Vec<ParamApplication>,
}

impl Default for ApplicationGeneratorSet {
    fn default() -> Self {
        Self {
            inherent: ComputeKind::Classical,
            dynamic_param_applications: Vec::new(),
        }
    }
}

impl Display for ApplicationGeneratorSet {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "ApplicationsGeneratorSet:",)?;
        indent = set_indentation(indent, 1);
        write!(indent, "\ninherent: {}", self.inherent)?;
        write!(indent, "\ndynamic_param_applications:")?;
        if self.dynamic_param_applications.is_empty() {
            write!(indent, " <empty>")?;
        } else {
            indent = set_indentation(indent, 2);
            for (param_index, param_application) in
                self.dynamic_param_applications.iter().enumerate()
            {
                write!(indent, "\n[{param_index}]: {param_application}")?;
            }
        }
        Ok(())
    }
}

impl ApplicationGeneratorSet {
    #[must_use]
    pub fn generate_application_compute_kind(&self, args_value_kinds: &[ValueKind]) -> ComputeKind {
        assert!(self.dynamic_param_applications.len() == args_value_kinds.len());
        let mut compute_kind = self.inherent;
        for (arg_value_kind, param_application) in args_value_kinds
            .iter()
            .zip(self.dynamic_param_applications.iter())
        {
            // Since the generator set can have parameters with generic types as its basis, the value kind of the
            // arguments used to derive a particular application might not match the variant of the generator set.
            // Therefore, we need to fix the mismatch to know what particular compute kinds to aggregate.
            let mapped_value_kind = match param_application {
                ParamApplication::Array(_) => {
                    let mut mapped_value_kind =
                        ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static);
                    arg_value_kind.project_onto_variant(&mut mapped_value_kind);
                    mapped_value_kind
                }
                ParamApplication::Element(_) => {
                    let mut mapped_value_kind = ValueKind::Element(RuntimeKind::Static);
                    arg_value_kind.project_onto_variant(&mut mapped_value_kind);
                    mapped_value_kind
                }
            };

            // Now that we have fixed any possible mismatch between the value kind variants of the generator set
            // parameters and the actual arguments used to derive the application, we can decide what to aggregate.
            if let ValueKind::Element(RuntimeKind::Dynamic) = mapped_value_kind {
                let ParamApplication::Element(param_compute_kind) = param_application else {
                    panic!("parameter application was expected to be an element variant");
                };

                compute_kind = compute_kind.aggregate(*param_compute_kind);
            } else if let ValueKind::Array(content_runtime_value, size_runtime_value) =
                mapped_value_kind
            {
                let ParamApplication::Array(array_param_application) = param_application else {
                    panic!("parameter application was expected to be an array variant");
                };

                let param_compute_kind = match (content_runtime_value, size_runtime_value) {
                    // When both the content and the size are static, we can treat it as aggregating a classical element.
                    (RuntimeKind::Static, RuntimeKind::Static) => ComputeKind::Classical,
                    (RuntimeKind::Dynamic, RuntimeKind::Static) => {
                        array_param_application.dynamic_content_static_size
                    }
                    (RuntimeKind::Static, RuntimeKind::Dynamic) => {
                        array_param_application.static_content_dynamic_size
                    }
                    (RuntimeKind::Dynamic, RuntimeKind::Dynamic) => {
                        array_param_application.dynamic_content_dynamic_size
                    }
                };

                compute_kind = compute_kind.aggregate(param_compute_kind);
            }
        }
        compute_kind
    }
}

#[derive(Clone, Debug)]
pub enum ParamApplication {
    Element(ComputeKind),
    Array(ArrayParamApplication),
}

impl Display for ParamApplication {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            Self::Element(compute_kind) => write!(f, "[Parameter Type Element] {compute_kind}")?,
            Self::Array(array_param_application) => {
                write!(f, "[Parameter Type Array] {array_param_application}")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ArrayParamApplication {
    pub static_content_dynamic_size: ComputeKind,
    pub dynamic_content_static_size: ComputeKind,
    pub dynamic_content_dynamic_size: ComputeKind,
}

impl Display for ArrayParamApplication {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "ArrayParamApplication:",)?;
        indent = set_indentation(indent, 1);
        write!(
            indent,
            "\nstatic_content_dynamic_size: {}",
            self.static_content_dynamic_size
        )?;
        write!(
            indent,
            "\ndynamic_content_static_size: {}",
            self.dynamic_content_static_size
        )?;
        write!(
            indent,
            "\ndynamic_content_dynamic_size: {}",
            self.dynamic_content_dynamic_size
        )?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ComputeKind {
    Classical,
    Quantum(QuantumProperties),
}

impl Display for ComputeKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            ComputeKind::Quantum(quantum_properties) => write!(f, "Quantum: {quantum_properties}")?,
            ComputeKind::Classical => write!(f, "Classical")?,
        }
        Ok(())
    }
}

impl ComputeKind {
    pub(crate) fn map_to_type(compute_kind: Self, ty: &Ty) -> Self {
        match compute_kind {
            ComputeKind::Classical => ComputeKind::Classical,
            ComputeKind::Quantum(quantum_properties) => {
                let runtime_features = quantum_properties.runtime_features;
                let mut value_kind = ValueKind::new_static_from_type(ty);
                quantum_properties
                    .value_kind
                    .project_onto_variant(&mut value_kind);
                ComputeKind::Quantum(QuantumProperties {
                    runtime_features,
                    value_kind,
                })
            }
        }
    }

    pub(crate) fn new_with_runtime_features(
        runtime_features: RuntimeFeatureFlags,
        value_kind: ValueKind,
    ) -> Self {
        Self::Quantum(QuantumProperties {
            runtime_features,
            value_kind,
        })
    }

    pub(crate) fn aggregate(self, value: Self) -> Self {
        let ComputeKind::Quantum(value_quantum_properties) = value else {
            // A classical compute kind has nothing to aggregate so just return self with no changes.
            return self;
        };

        // Determine the aggregated runtime features.
        let runtime_features = match self {
            Self::Classical => value_quantum_properties.runtime_features,
            Self::Quantum(ref self_quantum_properties) => {
                self_quantum_properties.runtime_features | value_quantum_properties.runtime_features
            }
        };

        // Determine the aggregated value kind.
        let value_kind = match self {
            Self::Classical => value_quantum_properties.value_kind,
            Self::Quantum(self_quantum_properties) => self_quantum_properties
                .value_kind
                .aggregate(value_quantum_properties.value_kind),
        };

        // Return the aggregated compute kind.
        ComputeKind::Quantum(QuantumProperties {
            runtime_features,
            value_kind,
        })
    }

    pub(crate) fn aggregate_runtime_features(
        self,
        value: ComputeKind,
        default_value_kind: ValueKind,
    ) -> Self {
        let Self::Quantum(value_quantum_properties) = value else {
            // A classical compute kind has nothing to aggregate so just return the self with no changes.
            return self;
        };

        // Determine the aggregated runtime features.
        let runtime_features = match self {
            Self::Classical => value_quantum_properties.runtime_features,
            Self::Quantum(ref self_quantum_properties) => {
                self_quantum_properties.runtime_features | value_quantum_properties.runtime_features
            }
        };

        // Use the value kind equivalent from self.
        let value_kind = match self {
            // If self was classical, the aggregated value kind is all static.
            Self::Classical => default_value_kind,
            Self::Quantum(self_quantum_properties) => self_quantum_properties.value_kind,
        };

        // Return the aggregated compute kind.
        ComputeKind::Quantum(QuantumProperties {
            runtime_features,
            value_kind,
        })
    }

    pub(crate) fn aggregate_value_kind(&mut self, value: ValueKind) {
        let Self::Quantum(quantum_properties) = self else {
            panic!("a value kind can only be aggregated to a compute kind of the quantum variant");
        };

        quantum_properties.value_kind = quantum_properties.value_kind.aggregate(value);
    }

    pub(crate) fn is_dynamic(self) -> bool {
        match self {
            Self::Classical => false,
            Self::Quantum(quantum_properties) => quantum_properties.value_kind.is_dynamic(),
        }
    }

    pub(crate) fn value_kind(self) -> Option<ValueKind> {
        match self {
            Self::Classical => None,
            Self::Quantum(quantum_properties) => Some(quantum_properties.value_kind),
        }
    }

    pub(crate) fn value_kind_or_default(self, default: ValueKind) -> ValueKind {
        match self {
            Self::Classical => default,
            Self::Quantum(quantum_properties) => quantum_properties.value_kind,
        }
    }
}

/// The quantum properties of a program element.
#[derive(Clone, Copy, Debug)]
pub struct QuantumProperties {
    /// The runtime features used by the program element.
    pub runtime_features: RuntimeFeatureFlags,
    /// The kind of value of the program element.
    pub value_kind: ValueKind,
}

impl Display for QuantumProperties {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "QuantumProperties:",)?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nruntime_features: {:?}", self.runtime_features)?;
        write!(indent, "\nvalue_kind: {}", self.value_kind)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ValueKind {
    /// The first runtime kind corresponds to the content of the array while the second corresponds to the size.
    Array(RuntimeKind, RuntimeKind),
    /// Runtime kind corresponding to a single element.
    Element(RuntimeKind),
}

impl Display for ValueKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            Self::Array(content_runtime_value, size_runtime_value) => write!(
                f,
                "Array(Content: {content_runtime_value}, Size: {size_runtime_value})"
            )?,
            Self::Element(runtime_value) => write!(f, "Element({runtime_value})")?,
        }
        Ok(())
    }
}

impl ValueKind {
    pub(crate) fn new_dynamic_from_type(ty: &Ty) -> Self {
        if *ty == Ty::UNIT {
            // The associated value kind for a unit type is always static.
            Self::Element(RuntimeKind::Static)
        } else {
            match ty {
                // For a dynamic array, the content is dynamic and the size is static.
                // We assume this because the source of the array produces something with dynamic length,
                // that source should have already added the runtime feature flag for dynamic arrays.
                Ty::Array(_) => ValueKind::Array(RuntimeKind::Dynamic, RuntimeKind::Static),
                // For every other dynamic type, we use the element variant with a dynamic runtime value.
                _ => ValueKind::Element(RuntimeKind::Dynamic),
            }
        }
    }

    pub(crate) fn new_static_from_type(ty: &Ty) -> Self {
        match ty {
            // For a static array, both contents and size are static.
            Ty::Array(_) => ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static),
            // For every other static type, we use the element variant with a static runtime value.
            _ => ValueKind::Element(RuntimeKind::Static),
        }
    }

    pub(crate) fn aggregate(self, value: ValueKind) -> Self {
        match self {
            Self::Array(self_content_runtime_value, self_size_runtime_value) => {
                let Self::Array(other_content_runtime_value, other_size_runtime_value) = value
                else {
                    panic!("only value kinds of the same variant can be aggregated");
                };

                Self::Array(
                    self_content_runtime_value.aggregate(other_content_runtime_value),
                    self_size_runtime_value.aggregate(other_size_runtime_value),
                )
            }
            Self::Element(self_runtime_value) => {
                let Self::Element(other_runtime_value) = value else {
                    panic!("only value kinds of the same variant can be aggregated");
                };
                Self::Element(self_runtime_value.aggregate(other_runtime_value))
            }
        }
    }

    #[must_use]
    pub fn is_dynamic(self) -> bool {
        match self {
            Self::Array(content_runtime_kind, size_runtime_kind) => {
                matches!(content_runtime_kind, RuntimeKind::Dynamic)
                    || matches!(size_runtime_kind, RuntimeKind::Dynamic)
            }
            Self::Element(runtime_kind) => matches!(runtime_kind, RuntimeKind::Dynamic),
        }
    }

    pub(crate) fn project_onto_variant(self, variant: &mut ValueKind) {
        match variant {
            ValueKind::Array(content_runtime_kind, size_runtime_kind) => match self {
                // We should resolve to an array value kind variant.
                ValueKind::Array(self_content_runtime_kind, self_size_runtime_kind) => {
                    *content_runtime_kind = self_content_runtime_kind;
                    *size_runtime_kind = self_size_runtime_kind;
                }
                ValueKind::Element(self_runtime_kind) => {
                    *content_runtime_kind = self_runtime_kind;
                    // When we project from an element variant to an array variant, we assume the size of the
                    // array is statically sized because we rely on the dynamically sized arrays runtime feature
                    // flag to detect such cases.
                    *size_runtime_kind = RuntimeKind::Static;
                }
            },
            ValueKind::Element(runtime_kind) => {
                // We should resolve to an element value kind variant.
                *runtime_kind = if self.is_dynamic() {
                    RuntimeKind::Dynamic
                } else {
                    RuntimeKind::Static
                };
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RuntimeKind {
    Static,
    Dynamic,
}

impl Display for RuntimeKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            RuntimeKind::Static => {
                write!(f, "Static")?;
            }
            RuntimeKind::Dynamic => {
                write!(f, "Dynamic")?;
            }
        }
        Ok(())
    }
}

impl RuntimeKind {
    pub(crate) fn aggregate(self, value: RuntimeKind) -> Self {
        match value {
            Self::Static => self,
            Self::Dynamic => Self::Dynamic,
        }
    }
}

bitflags! {
    /// Runtime features represent anything a program can do that is more complex than executing quantum operations on
    /// statically allocated qubits and using constant arguments.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct RuntimeFeatureFlags: u32 {
        /// Use of a dynamic `Bool`.
        const UseOfDynamicBool = 1 << 0;
        /// Use of a dynamic `Int`.
        const UseOfDynamicInt = 1 << 1;
        /// Use of a dynamic `Pauli`.
        const UseOfDynamicPauli = 1 << 2;
        /// Use of a dynamic `Range`.
        const UseOfDynamicRange = 1 << 3;
        /// Use of a dynamic `Double`.
        const UseOfDynamicDouble = 1 << 4;
        /// Use of a dynamic `Qubit`.
        const UseOfDynamicQubit = 1 << 5;
        /// Use of a dynamic `BigInt`.
        const UseOfDynamicBigInt = 1 << 6;
        /// Use of a dynamic `String`.
        const UseOfDynamicString = 1 << 7;
        /// Use of a dynamic array.
        const UseOfDynamicallySizedArray = 1 << 8;
        /// Use of a dynamic UDT.
        const UseOfDynamicUdt = 1 << 9;
        /// Use of a dynamic arrow function.
        const UseOfDynamicArrowFunction = 1 << 10;
        /// Use of a dynamic arrow operation.
        const UseOfDynamicArrowOperation = 1 << 11;
        /// A function with cycles used with a dynamic argument.
        const CallToCyclicFunctionWithDynamicArg = 1 << 12;
        /// An operation specialization with cycles exists.
        const CyclicOperationSpec = 1 << 13;
        /// A call to an operation with cycles.
        const CallToCyclicOperation = 1 << 14;
        /// A callee expression is dynamic.
        const CallToDynamicCallee = 1 << 15;
        /// A callee expression could not be resolved to a specific callable.
        const CallToUnresolvedCallee = 1 << 16;
        /// Performing a measurement within a dynamic scope.
        const MeasurementWithinDynamicScope = 1 << 17;
        /// Use of a dynamic index to access or update an array.
        const UseOfDynamicIndex = 1 << 18;
        /// A return expression within a dynamic scope.
        const ReturnWithinDynamicScope = 1 << 19;
        /// A loop with a dynamic condition.
        const LoopWithDynamicCondition = 1 << 20;
        /// Use of an advanced type as output of a computation.
        const UseOfAdvancedOutput = 1 << 21;
        /// Use of a `Bool` as output of a computation.
        const UseOfBoolOutput = 1 << 22;
        /// Use of a `Double` as output of a computation.
        const UseOfDoubleOutput = 1 << 23;
        /// Use of an `Int` as output of a computation.
        const UseOfIntOutput = 1 << 24;
        /// Use of a dynamic exponent in a computation.
        const UseOfDynamicExponent = 1 << 25;
        /// Use of a dynamic `Result` variable in a computation.
        const UseOfDynamicResult = 1 << 26;
        /// Use of a dynamic tuple variable.
        const UseOfDynamicTuple = 1 << 27;
        /// A callee expression to a measurement.
        const CallToCustomMeasurement = 1 << 28;
        /// A callee expression to a reset.
        const CallToCustomReset = 1 << 29;
    }
}

impl RuntimeFeatureFlags {
    /// Determines the runtime features that contribute to the provided target capabilities.
    #[must_use]
    pub fn contributing_features(&self, capabilities: TargetCapabilityFlags) -> Self {
        let mut contributing_features = Self::empty();
        for feature in self.iter() {
            if feature.target_capabilities().intersects(capabilities) {
                contributing_features |= feature;
            }
        }

        contributing_features
    }

    /// Maps program constructs to target capabilities.
    #[must_use]
    pub fn target_capabilities(&self) -> TargetCapabilityFlags {
        let mut capabilities = TargetCapabilityFlags::empty();
        if self.contains(RuntimeFeatureFlags::UseOfDynamicBool) {
            capabilities |= TargetCapabilityFlags::Adaptive;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicInt) {
            capabilities |= TargetCapabilityFlags::IntegerComputations;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicPauli) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicRange) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicDouble) {
            capabilities |= TargetCapabilityFlags::FloatingPointComputations;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicQubit) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicBigInt) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicString) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicallySizedArray) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicUdt) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicArrowFunction) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicArrowOperation) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::CallToCyclicFunctionWithDynamicArg) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::CyclicOperationSpec) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::CallToCyclicOperation) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::CallToDynamicCallee) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::MeasurementWithinDynamicScope) {
            capabilities |= TargetCapabilityFlags::Adaptive;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicIndex) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::ReturnWithinDynamicScope) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::LoopWithDynamicCondition) {
            capabilities |= TargetCapabilityFlags::BackwardsBranching;
        }
        if self.contains(RuntimeFeatureFlags::UseOfBoolOutput) {
            capabilities |= TargetCapabilityFlags::Adaptive;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDoubleOutput) {
            capabilities |= TargetCapabilityFlags::FloatingPointComputations;
        }
        if self.contains(RuntimeFeatureFlags::UseOfIntOutput) {
            capabilities |= TargetCapabilityFlags::IntegerComputations;
        }
        if self.contains(RuntimeFeatureFlags::UseOfAdvancedOutput) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicExponent) {
            capabilities |= TargetCapabilityFlags::BackwardsBranching;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicResult) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicTuple) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::CallToCustomMeasurement) {
            capabilities |= TargetCapabilityFlags::Adaptive;
        }
        if self.contains(RuntimeFeatureFlags::CallToCustomReset) {
            capabilities |= TargetCapabilityFlags::Adaptive;
        }
        capabilities
    }

    #[must_use]
    pub fn output_recording_flags() -> RuntimeFeatureFlags {
        RuntimeFeatureFlags::UseOfIntOutput
            | RuntimeFeatureFlags::UseOfDoubleOutput
            | RuntimeFeatureFlags::UseOfBoolOutput
            | RuntimeFeatureFlags::UseOfAdvancedOutput
    }
}
