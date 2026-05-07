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
#[cfg(debug_assertions)]
mod invariants;
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
            inherent: ComputeKind::Static,
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
    pub fn generate_application_compute_kind(
        &self,
        args_compute_kinds: &[ComputeKind],
    ) -> ComputeKind {
        // RCA generators record one `ParamApplication` per flattened input
        // parameter of the owning callable. The runtime arg vector must match
        // exactly; any skew indicates a bug in the analyzer's recording path.
        assert!(
            self.dynamic_param_applications.len() == args_compute_kinds.len(),
            "application generator recorded {} parameter applications for {} runtime arguments",
            self.dynamic_param_applications.len(),
            args_compute_kinds.len()
        );
        let mut compute_kind = self.inherent;
        for (arg_compute_kind, param_application) in args_compute_kinds
            .iter()
            .zip(self.dynamic_param_applications.iter())
        {
            match param_application {
                ParamApplication::Element(param_compute_kind) => {
                    if arg_compute_kind.is_variable_value_kind() {
                        compute_kind = compute_kind.aggregate(*param_compute_kind);
                    }
                }
                ParamApplication::Array(array_param_application) => {
                    if let ComputeKind::Dynamic {
                        runtime_features,
                        value_kind,
                    } = arg_compute_kind
                    {
                        match value_kind {
                            ValueKind::Variable
                                if runtime_features
                                    .contains(RuntimeFeatureFlags::UseOfDynamicallySizedArray) =>
                            {
                                compute_kind =
                                    compute_kind.aggregate(array_param_application.dynamic_size);
                            }
                            ValueKind::Variable => {
                                compute_kind =
                                    compute_kind.aggregate(array_param_application.static_size);
                            }
                            ValueKind::Constant => {
                                // No aggregation needed for static arrays.
                            }
                        }
                    }
                }
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
    pub static_size: ComputeKind,
    pub dynamic_size: ComputeKind,
}

impl Display for ArrayParamApplication {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut indent = set_indentation(indented(f), 0);
        write!(indent, "ArrayParamApplication:",)?;
        indent = set_indentation(indent, 1);
        write!(indent, "\nstatic_size: {}", self.static_size)?;
        write!(indent, "\ndynamic_size: {}", self.dynamic_size)?;
        Ok(())
    }
}

/// The two computation kinds for a program element: static or dynamic. These correspond to the
/// concepts in Partial Evaluation, where "static" refers to computations that can be performed
/// at code generation time and "dynamic" refers to computations that must occur during runtime
/// and are emitted into the generated code.
#[derive(Clone, Copy, Debug)]
pub enum ComputeKind {
    // An element that will be fully computed during code generation and does not affect the required capabilities of
    // the runtime environment for emitted code.
    Static,
    // An element that will be emitted into the generated code and may require additional capabilities in the runtime
    // environment.
    Dynamic {
        /// The runtime features used by the program element.
        runtime_features: RuntimeFeatureFlags,
        /// The kind of value produced by the program element.
        value_kind: ValueKind,
    },
}

impl Display for ComputeKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            ComputeKind::Dynamic {
                runtime_features,
                value_kind,
            } => {
                let mut indent = set_indentation(indented(f), 0);
                write!(indent, "Dynamic:",)?;
                indent = set_indentation(indent, 1);
                write!(indent, "\nruntime_features: {runtime_features:?}")?;
                write!(indent, "\nvalue_kind: {value_kind}")?;
            }
            ComputeKind::Static => write!(f, "Static")?,
        }
        Ok(())
    }
}

impl ComputeKind {
    pub(crate) fn aggregate(self, value: Self) -> Self {
        let ComputeKind::Dynamic {
            runtime_features,
            value_kind,
        } = value
        else {
            // A static compute kind has nothing to aggregate so just return self with no changes.
            return self;
        };

        // Determine the aggregated runtime features and value kind.
        let (runtime_features, value_kind) = match self {
            Self::Static => (runtime_features, value_kind),
            Self::Dynamic {
                runtime_features: self_runtime_features,
                value_kind: self_value_kind,
            } => (
                self_runtime_features | runtime_features,
                self_value_kind.aggregate(value_kind),
            ),
        };

        // Return the aggregated compute kind.
        ComputeKind::Dynamic {
            runtime_features,
            value_kind,
        }
    }

    pub(crate) fn aggregate_runtime_features(
        self,
        value: ComputeKind,
        default_value_kind: ValueKind,
    ) -> Self {
        let Self::Dynamic {
            runtime_features, ..
        } = value
        else {
            // A static compute kind has nothing to aggregate so just return the self with no changes.
            return self;
        };

        // Determine the aggregated runtime features, use the value kind equivalent from self or the default.
        let (runtime_features, value_kind) = match self {
            Self::Static => (runtime_features, default_value_kind),
            Self::Dynamic {
                runtime_features: self_runtime_features,
                value_kind: self_value_kind,
            } => (self_runtime_features | runtime_features, self_value_kind),
        };

        // Return the aggregated compute kind.
        ComputeKind::Dynamic {
            runtime_features,
            value_kind,
        }
    }

    pub(crate) fn aggregate_value_kind(&mut self, value: ValueKind) {
        let Self::Dynamic { value_kind, .. } = self else {
            panic!("a value kind can only be aggregated to a compute kind of the dynamic variant");
        };

        *value_kind = value_kind.aggregate(value);
    }

    pub(crate) fn set_variable_value_kind(&mut self) {
        let Self::Dynamic { value_kind, .. } = self else {
            panic!(
                "a value kind can only be set to variable for a compute kind of the dynamic variant"
            );
        };

        *value_kind = ValueKind::Variable;
    }

    #[must_use]
    pub fn is_variable_value_kind(self) -> bool {
        matches!(
            self,
            Self::Dynamic {
                value_kind: ValueKind::Variable,
                ..
            }
        )
    }
}

/// The kind of value corresponding to a program element, indicating whether it is constant over the course of runtime or can vary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    // A value that is constant during execution.
    Constant,
    // A value that can vary during execution, which may require additional capabilities and support in the runtime environment.
    Variable,
}

impl Display for ValueKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self {
            ValueKind::Constant => {
                write!(f, "Constant")?;
            }
            ValueKind::Variable => {
                write!(f, "Variable")?;
            }
        }
        Ok(())
    }
}

impl ValueKind {
    pub(crate) fn aggregate(self, value: ValueKind) -> Self {
        match value {
            Self::Constant => self,
            Self::Variable => Self::Variable,
        }
    }

    pub(crate) fn new_variable_from_type(ty: &Ty) -> Self {
        if *ty == Ty::UNIT {
            // The associated value kind for a unit type is always constant.
            Self::Constant
        } else {
            Self::Variable
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
        /// Use of a dynamic array with static size.
        const UseOfDynamicArray = 1 << 8;
        /// Use of a dynamic array with dynamic size.
        const UseOfDynamicallySizedArray = 1 << 9;
        /// Use of a dynamic UDT.
        const UseOfDynamicUdt = 1 << 10;
        /// Use of a dynamic arrow function.
        const UseOfDynamicArrowFunction = 1 << 11;
        /// Use of a dynamic arrow operation.
        const UseOfDynamicArrowOperation = 1 << 12;
        /// A function with cycles used with a dynamic argument.
        const CallToCyclicFunctionWithDynamicArg = 1 << 13;
        /// An operation specialization with cycles exists.
        const CyclicOperationSpec = 1 << 14;
        /// A call to an operation with cycles.
        const CallToCyclicOperation = 1 << 15;
        /// A callee expression is dynamic.
        const CallToDynamicCallee = 1 << 16;
        /// A callee expression could not be resolved to a specific callable.
        const CallToUnresolvedCallee = 1 << 17;
        /// Performing a measurement within a dynamic scope.
        const MeasurementWithinDynamicScope = 1 << 18;
        /// Use of a dynamic index to access or update an array.
        const UseOfDynamicIndex = 1 << 19;
        /// A return expression within a dynamic scope.
        const ReturnWithinDynamicScope = 1 << 20;
        /// A loop with a dynamic condition.
        const LoopWithDynamicCondition = 1 << 21;
        /// Use of an advanced type as output of a computation.
        const UseOfAdvancedOutput = 1 << 22;
        /// Use of a `Bool` as output of a computation.
        const UseOfBoolOutput = 1 << 23;
        /// Use of a `Double` as output of a computation.
        const UseOfDoubleOutput = 1 << 24;
        /// Use of an `Int` as output of a computation.
        const UseOfIntOutput = 1 << 25;
        /// Use of a dynamic exponent in a computation.
        const UseOfDynamicExponent = 1 << 26;
        /// Use of a dynamic `Result` variable in a computation.
        const UseOfDynamicResult = 1 << 27;
        /// Use of a dynamic tuple variable.
        const UseOfDynamicTuple = 1 << 28;
        /// A callee expression to a measurement.
        const CallToCustomMeasurement = 1 << 29;
        /// A callee expression to a reset.
        const CallToCustomReset = 1 << 30;
        /// Use of a dynamic generic parameter.
        const UseOfDynamicGeneric = 1 << 31;
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
            capabilities |= TargetCapabilityFlags::StaticSizedArrays;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicBigInt) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicString) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
        }
        if self.contains(RuntimeFeatureFlags::UseOfDynamicArray) {
            // capabilities |= TargetCapabilityFlags::StaticSizedArrays;

            // For now, we are treating any dynamic array as requiriing higher level constructs,
            // so that we can reject loops over arrays with dynamic contents.
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
            capabilities |= TargetCapabilityFlags::StaticSizedArrays;
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
            // capabilities |= TargetCapabilityFlags::BackwardsBranching;

            // For now, we are mapping use of a dynamic exponent to higher level constructs
            // until we support emiting the equivalent loop.
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
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
        if self.contains(RuntimeFeatureFlags::UseOfDynamicGeneric) {
            capabilities |= TargetCapabilityFlags::HigherLevelConstructs;
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
