// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Python binding for the QHDG graph.
//!
//! The graph is addressed by plain integers on this side. A region, a node, and
//! a port each answer to the identifier the builder handed back, which keeps the
//! surface small and matches how the circuit surface already addresses qubits.
//!
//! Nothing here hands out a borrow. The graph's own transaction type carries a
//! lifetime tied to the graph it mutates, so it could never be held across a
//! call from Python, and the builder is the only way to change a graph from
//! here.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyType};
use std::collections::BTreeMap;

use qsc::TargetCapabilityFlags;
use qsc::target::Profile;
use qsc_qhdg::{
    Angle as QhdgAngle, AngleOp as QhdgAngleOp, Direction, Edge, EdgeKind as QhdgEdgeKind,
    EffectDomain as QhdgEffectDomain, EffectEvent as QhdgEffectEvent, EffectMode as QhdgEffectMode,
    EffectResource, Graph, GraphBuilder as QhdgGraphBuilder,
    InterfaceBinding as QhdgInterfaceBinding, InterfaceRole as QhdgInterfaceRole, NodeId,
    NodeOrigin as QhdgNodeOrigin, NodePayload as QhdgNodePayload, NodeValue as QhdgNodeValue,
    PortId, PortKind as QhdgPortKind, Real, RegionId, RegionKind as QhdgRegionKind, Stage,
    SymbolId as QhdgSymbolId, Type, Usage,
};

use crate::circuit_api::{CircuitOrigin, OriginKind};
use crate::interpreter::TargetProfile;

/// The graph's own origin for a Python-supplied one.
///
/// Every kind but the global phase names an index, and the graph has no form
/// for one that does not.
fn node_origin(origin: &CircuitOrigin) -> PyResult<QhdgNodeOrigin> {
    let index = || {
        origin
            .index
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| PyValueError::new_err("this origin kind needs an index"))
    };
    Ok(match origin.kind {
        OriginKind::Instruction => QhdgNodeOrigin::Instruction(index()?),
        OriginKind::Qubit => QhdgNodeOrigin::Qubit(index()?),
        OriginKind::Clbit => QhdgNodeOrigin::Clbit(index()?),
        OriginKind::GlobalPhase => QhdgNodeOrigin::GlobalPhase,
    })
}

/// What a region stands for.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum RegionKind {
    /// The whole program.
    ModuleOmega,
    /// A callable body.
    Lambda,
    /// A branch arm.
    Gamma,
    /// A loop body.
    Theta,
    /// A recursive body.
    Recursion,
    /// An effect handler body.
    Handler,
    /// A compilation stage boundary.
    Stage,
    /// A control-flow graph that survived to the residual stage.
    ResidualCfg,
}

/// Which way a port faces.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum PortDirection {
    Input,
    Output,
}

/// What a port carries.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum PortKind {
    /// A value that may be read more than once.
    Value,
    /// A capability standing for an owned resource, such as a qubit.
    Capability,
    /// An observable effect, which is what keeps operations ordered.
    HardEffect,
    /// A control exit from a region.
    ControlExit,
}

/// When a port's value is known.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum PortStage {
    /// Known while the program is being generated.
    Generation,
    /// Not known until the program runs.
    Residual,
}

/// How many times a port's value may be used.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum PortUsage {
    /// Any number of times.
    Copyable,
    /// At most once.
    Affine,
    /// Exactly once.
    Linear,
}

/// What an edge carries between two ports.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum EdgeKind {
    CopyableValue,
    AffineCapability,
    HardEffect,
    ControlExit,
}

/// What an effect acts on.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum EffectDomain {
    Memory,
    Quantum,
    Result,
    Allocation,
    Output,
    Abort,
    Host,
    Noise,
    Unknown,
}

/// What an effect does to what it acts on.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum EffectMode {
    Read,
    Write,
    Irreversible,
    Allocate,
    Release,
    Emit,
    Abort,
    Unknown,
}

/// What a bound port is to the region boundary it crosses.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum InterfaceRole {
    Argument,
    Result,
    Exit,
    Capture,
    Carried,
}

impl From<RegionKind> for QhdgRegionKind {
    fn from(value: RegionKind) -> Self {
        match value {
            RegionKind::ModuleOmega => Self::ModuleOmega,
            RegionKind::Lambda => Self::Lambda,
            RegionKind::Gamma => Self::Gamma,
            RegionKind::Theta => Self::Theta,
            RegionKind::Recursion => Self::Recursion,
            RegionKind::Handler => Self::Handler,
            RegionKind::Stage => Self::Stage,
            RegionKind::ResidualCfg => Self::ResidualCfg,
        }
    }
}

impl From<QhdgRegionKind> for RegionKind {
    fn from(value: QhdgRegionKind) -> Self {
        match value {
            QhdgRegionKind::ModuleOmega => Self::ModuleOmega,
            QhdgRegionKind::Lambda => Self::Lambda,
            QhdgRegionKind::Gamma => Self::Gamma,
            QhdgRegionKind::Theta => Self::Theta,
            QhdgRegionKind::Recursion => Self::Recursion,
            QhdgRegionKind::Handler => Self::Handler,
            QhdgRegionKind::Stage => Self::Stage,
            QhdgRegionKind::ResidualCfg => Self::ResidualCfg,
        }
    }
}

impl From<PortDirection> for Direction {
    fn from(value: PortDirection) -> Self {
        match value {
            PortDirection::Input => Self::Input,
            PortDirection::Output => Self::Output,
        }
    }
}

impl From<Direction> for PortDirection {
    fn from(value: Direction) -> Self {
        match value {
            Direction::Input => Self::Input,
            Direction::Output => Self::Output,
        }
    }
}

impl From<PortKind> for QhdgPortKind {
    fn from(value: PortKind) -> Self {
        match value {
            PortKind::Value => Self::Value,
            PortKind::Capability => Self::Capability,
            PortKind::HardEffect => Self::HardEffect,
            PortKind::ControlExit => Self::ControlExit,
        }
    }
}

impl From<QhdgPortKind> for PortKind {
    fn from(value: QhdgPortKind) -> Self {
        match value {
            QhdgPortKind::Value => Self::Value,
            QhdgPortKind::Capability => Self::Capability,
            QhdgPortKind::HardEffect => Self::HardEffect,
            QhdgPortKind::ControlExit => Self::ControlExit,
        }
    }
}

impl From<PortStage> for Stage {
    fn from(value: PortStage) -> Self {
        match value {
            PortStage::Generation => Self::Generation,
            PortStage::Residual => Self::Residual,
        }
    }
}

impl From<Stage> for PortStage {
    fn from(value: Stage) -> Self {
        match value {
            Stage::Generation => Self::Generation,
            Stage::Residual => Self::Residual,
        }
    }
}

impl From<PortUsage> for Usage {
    fn from(value: PortUsage) -> Self {
        match value {
            PortUsage::Copyable => Self::Copyable,
            PortUsage::Affine => Self::Affine,
            PortUsage::Linear => Self::Linear,
        }
    }
}

impl From<Usage> for PortUsage {
    fn from(value: Usage) -> Self {
        match value {
            Usage::Copyable => Self::Copyable,
            Usage::Affine => Self::Affine,
            Usage::Linear => Self::Linear,
        }
    }
}

impl From<EdgeKind> for QhdgEdgeKind {
    fn from(value: EdgeKind) -> Self {
        match value {
            EdgeKind::CopyableValue => Self::CopyableValue,
            EdgeKind::AffineCapability => Self::AffineCapability,
            EdgeKind::HardEffect => Self::HardEffect,
            EdgeKind::ControlExit => Self::ControlExit,
        }
    }
}

impl From<QhdgEdgeKind> for EdgeKind {
    fn from(value: QhdgEdgeKind) -> Self {
        match value {
            QhdgEdgeKind::CopyableValue => Self::CopyableValue,
            QhdgEdgeKind::AffineCapability => Self::AffineCapability,
            QhdgEdgeKind::HardEffect => Self::HardEffect,
            QhdgEdgeKind::ControlExit => Self::ControlExit,
        }
    }
}

impl From<EffectDomain> for QhdgEffectDomain {
    fn from(value: EffectDomain) -> Self {
        match value {
            EffectDomain::Memory => Self::Memory,
            EffectDomain::Quantum => Self::Quantum,
            EffectDomain::Result => Self::Result,
            EffectDomain::Allocation => Self::Allocation,
            EffectDomain::Output => Self::Output,
            EffectDomain::Abort => Self::Abort,
            EffectDomain::Host => Self::Host,
            EffectDomain::Noise => Self::Noise,
            EffectDomain::Unknown => Self::Unknown,
        }
    }
}

impl From<QhdgEffectDomain> for EffectDomain {
    fn from(value: QhdgEffectDomain) -> Self {
        match value {
            QhdgEffectDomain::Memory => Self::Memory,
            QhdgEffectDomain::Quantum => Self::Quantum,
            QhdgEffectDomain::Result => Self::Result,
            QhdgEffectDomain::Allocation => Self::Allocation,
            QhdgEffectDomain::Output => Self::Output,
            QhdgEffectDomain::Abort => Self::Abort,
            QhdgEffectDomain::Host => Self::Host,
            QhdgEffectDomain::Noise => Self::Noise,
            QhdgEffectDomain::Unknown => Self::Unknown,
        }
    }
}

impl From<EffectMode> for QhdgEffectMode {
    fn from(value: EffectMode) -> Self {
        match value {
            EffectMode::Read => Self::Read,
            EffectMode::Write => Self::Write,
            EffectMode::Irreversible => Self::Irreversible,
            EffectMode::Allocate => Self::Allocate,
            EffectMode::Release => Self::Release,
            EffectMode::Emit => Self::Emit,
            EffectMode::Abort => Self::Abort,
            EffectMode::Unknown => Self::Unknown,
        }
    }
}

impl From<QhdgEffectMode> for EffectMode {
    fn from(value: QhdgEffectMode) -> Self {
        match value {
            QhdgEffectMode::Read => Self::Read,
            QhdgEffectMode::Write => Self::Write,
            QhdgEffectMode::Irreversible => Self::Irreversible,
            QhdgEffectMode::Allocate => Self::Allocate,
            QhdgEffectMode::Release => Self::Release,
            QhdgEffectMode::Emit => Self::Emit,
            QhdgEffectMode::Abort => Self::Abort,
            QhdgEffectMode::Unknown => Self::Unknown,
        }
    }
}

impl From<InterfaceRole> for QhdgInterfaceRole {
    fn from(value: InterfaceRole) -> Self {
        match value {
            InterfaceRole::Argument => Self::Argument,
            InterfaceRole::Result => Self::Result,
            InterfaceRole::Exit => Self::Exit,
            InterfaceRole::Capture => Self::Capture,
            InterfaceRole::Carried => Self::Carried,
        }
    }
}

/// The type a port carries.
///
/// Built rather than named, because three of the forms hold another type.
#[pyclass(module = "qdk.circuit", frozen, from_py_object, eq)]
#[derive(Clone, PartialEq)]
pub struct ValueType {
    pub(crate) ty: Type,
}

#[pymethods]
impl ValueType {
    #[staticmethod]
    fn unit() -> Self {
        Self { ty: Type::Unit }
    }

    #[staticmethod]
    #[pyo3(name = "bool")]
    fn boolean() -> Self {
        Self { ty: Type::Bool }
    }

    #[staticmethod]
    #[pyo3(name = "int")]
    fn integer() -> Self {
        Self { ty: Type::Int }
    }

    #[staticmethod]
    fn double() -> Self {
        Self { ty: Type::Double }
    }

    #[staticmethod]
    fn string() -> Self {
        Self { ty: Type::String }
    }

    #[staticmethod]
    fn result() -> Self {
        Self { ty: Type::Result }
    }

    #[staticmethod]
    fn qubit() -> Self {
        Self { ty: Type::Qubit }
    }

    #[staticmethod]
    fn unknown() -> Self {
        Self { ty: Type::Unknown }
    }

    #[staticmethod]
    fn array(element: &ValueType) -> Self {
        Self {
            ty: Type::Array(Box::new(element.ty.clone())),
        }
    }

    #[staticmethod]
    #[pyo3(name = "tuple")]
    fn tuple_of(elements: Vec<ValueType>) -> Self {
        Self {
            ty: Type::Tuple(elements.into_iter().map(|value| value.ty).collect()),
        }
    }

    #[staticmethod]
    fn location(target: &ValueType) -> Self {
        Self {
            ty: Type::Location(Box::new(target.ty.clone())),
        }
    }

    fn __repr__(&self) -> String {
        format!("ValueType({:?})", self.ty)
    }
}

/// The source-level item a node came from.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct SymbolId {
    #[pyo3(get)]
    package: String,
    #[pyo3(get)]
    item: String,
}

#[pymethods]
impl SymbolId {
    #[new]
    fn new(package: String, item: String) -> Self {
        Self { package, item }
    }

    fn __repr__(&self) -> String {
        format!("SymbolId({:?}, {:?})", self.package, self.item)
    }
}

/// One observable effect a node performs.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct EffectEvent {
    #[pyo3(get)]
    domain: EffectDomain,
    #[pyo3(get)]
    mode: EffectMode,
    /// The named resource acted on, or `None` when it is not known.
    #[pyo3(get)]
    resource: Option<String>,
    #[pyo3(get)]
    observable: bool,
}

#[pymethods]
impl EffectEvent {
    #[new]
    #[pyo3(signature = (domain, mode, resource = None, observable = true))]
    fn new(
        domain: EffectDomain,
        mode: EffectMode,
        resource: Option<String>,
        observable: bool,
    ) -> Self {
        Self {
            domain,
            mode,
            resource,
            observable,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "EffectEvent(domain={:?}, mode={:?}, resource={:?}, observable={})",
            self.domain, self.mode, self.resource, self.observable
        )
    }
}

impl From<&EffectEvent> for QhdgEffectEvent {
    fn from(value: &EffectEvent) -> Self {
        Self {
            domain: value.domain.into(),
            mode: value.mode.into(),
            resource: match &value.resource {
                Some(name) => EffectResource::Named(name.clone()),
                None => EffectResource::Unknown,
            },
            observable: value.observable,
        }
    }
}

impl From<&QhdgEffectEvent> for EffectEvent {
    fn from(value: &QhdgEffectEvent) -> Self {
        Self {
            domain: value.domain.into(),
            mode: value.mode.into(),
            resource: match &value.resource {
                EffectResource::Named(name) => Some(name.clone()),
                EffectResource::Unknown => None,
            },
            observable: value.observable,
        }
    }
}

/// A child region port bound to the parent port it crosses to.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct InterfaceBinding {
    #[pyo3(get)]
    outer: u32,
    #[pyo3(get)]
    inner: u32,
    #[pyo3(get)]
    role: InterfaceRole,
}

#[pymethods]
impl InterfaceBinding {
    #[new]
    fn new(outer: u32, inner: u32, role: InterfaceRole) -> Self {
        Self { outer, inner, role }
    }

    fn __repr__(&self) -> String {
        format!(
            "InterfaceBinding(outer={}, inner={}, role={:?})",
            self.outer, self.inner, self.role
        )
    }
}

/// A region as the graph holds it.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct RegionView {
    #[pyo3(get)]
    id: u32,
    /// The enclosing region, or `None` for the outermost one.
    #[pyo3(get)]
    parent: Option<u32>,
    #[pyo3(get)]
    kind: RegionKind,
    /// The node this region is a body of, when one declared it.
    #[pyo3(get)]
    owner: Option<u32>,
}

#[pymethods]
impl RegionView {
    fn __repr__(&self) -> String {
        format!(
            "RegionView(id={}, parent={:?}, kind={:?})",
            self.id, self.parent, self.kind
        )
    }
}

/// Arithmetic available in a symbolic angle.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum AngleOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl From<AngleOp> for QhdgAngleOp {
    fn from(value: AngleOp) -> Self {
        match value {
            AngleOp::Add => Self::Add,
            AngleOp::Sub => Self::Sub,
            AngleOp::Mul => Self::Mul,
            AngleOp::Div => Self::Div,
        }
    }
}

/// An angle a node carries, which may still name a parameter.
///
/// A parameter is named rather than indexed, so an angle read from one graph
/// can be written to another without the circuit that produced either. Named
/// apart from the circuit surface's `Angle`, which is what a gate accepts as an
/// argument rather than what a node holds.
#[pyclass(module = "qdk.circuit", frozen, from_py_object, eq)]
#[derive(Clone, Debug, PartialEq)]
pub struct NodeAngle {
    pub(crate) inner: QhdgAngle,
}

#[pymethods]
impl NodeAngle {
    #[staticmethod]
    fn literal(value: f64) -> Self {
        Self {
            inner: QhdgAngle::literal(value),
        }
    }

    #[staticmethod]
    fn parameter(name: String) -> Self {
        Self {
            inner: QhdgAngle::Parameter { name },
        }
    }

    #[staticmethod]
    fn negate(operand: &Self) -> Self {
        Self {
            inner: QhdgAngle::Neg {
                operand: Box::new(operand.inner.clone()),
            },
        }
    }

    #[staticmethod]
    fn binary(op: AngleOp, left: &Self, right: &Self) -> Self {
        Self {
            inner: QhdgAngle::Binary {
                op: op.into(),
                left: Box::new(left.inner.clone()),
                right: Box::new(right.inner.clone()),
            },
        }
    }

    /// The fixed value, or `None` while this angle still names a parameter.
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.as_literal()
    }

    /// Every parameter this angle reads, in first-seen order.
    #[getter]
    fn parameters(&self) -> Vec<String> {
        self.inner
            .parameters()
            .into_iter()
            .map(ToString::to_string)
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("NodeAngle({:?})", self.inner)
    }
}

/// A node as the graph holds it.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct NodeView {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    region: u32,
    #[pyo3(get)]
    operation: String,
    #[pyo3(get)]
    inputs: Vec<u32>,
    #[pyo3(get)]
    outputs: Vec<u32>,
    #[pyo3(get)]
    effects: Vec<EffectEvent>,
    /// The source-level item this node came from, when one was declared.
    #[pyo3(get)]
    symbol: Option<SymbolId>,
    /// The real constant this node carries, when it carries one.
    #[pyo3(get)]
    number: Option<f64>,
    /// The classical bit this node targets, when it targets one.
    #[pyo3(get)]
    classical_bit: Option<u32>,
    /// The angle this node carries, when it still names a parameter.
    #[pyo3(get)]
    angle: Option<NodeAngle>,
    /// The construct this node came from, when a producer recorded one.
    #[pyo3(get)]
    origin: Option<CircuitOrigin>,
}

#[pymethods]
impl NodeView {
    fn __repr__(&self) -> String {
        format!("NodeView(id={}, operation={:?})", self.id, self.operation)
    }
}

/// A port as the graph holds it.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct PortView {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    node: u32,
    #[pyo3(get)]
    region: u32,
    #[pyo3(get)]
    direction: PortDirection,
    #[pyo3(get)]
    kind: PortKind,
    #[pyo3(get)]
    value_type: ValueType,
    #[pyo3(get)]
    stage: PortStage,
    #[pyo3(get)]
    usage: PortUsage,
}

#[pymethods]
impl PortView {
    fn __repr__(&self) -> String {
        format!(
            "PortView(id={}, node={}, direction={:?}, kind={:?})",
            self.id, self.node, self.direction, self.kind
        )
    }
}

/// An edge as the graph holds it.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct EdgeView {
    #[pyo3(get)]
    source: u32,
    #[pyo3(get)]
    target: u32,
    #[pyo3(get)]
    kind: EdgeKind,
}

#[pymethods]
impl EdgeView {
    fn __repr__(&self) -> String {
        format!(
            "EdgeView(source={}, target={}, kind={:?})",
            self.source, self.target, self.kind
        )
    }
}

/// A program in the graph representation.
///
/// The handle is read-only. Reading answers with plain records rather than live
/// views, so nothing here borrows the graph, and changing a graph means building
/// a new one.
#[pyclass(module = "qdk.circuit")]
pub struct QuantumGraph {
    pub(crate) graph: Graph,
}

#[pymethods]
impl QuantumGraph {
    /// Saves a QDK-native Cap'n Proto graph envelope.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = qsc_qhdg::canonical_graph_snapshot(&self.graph)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Saves the same graph snapshot as an S-expression envelope.
    fn to_text(&self) -> PyResult<String> {
        qsc_qhdg::graph_snapshot_text(&self.graph)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Restores and verifies either graph snapshot envelope format.
    #[classmethod]
    fn from_bytes(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        Ok(Self {
            graph: qsc_qhdg::decode_graph_snapshot(data)
                .map_err(|error| PyValueError::new_err(error.to_string()))?,
        })
    }

    #[classmethod]
    fn from_text(cls: &Bound<'_, PyType>, text: &str) -> PyResult<Self> {
        Self::from_bytes(cls, text.as_bytes())
    }

    #[getter]
    fn regions(&self) -> Vec<RegionView> {
        self.graph
            .regions()
            .map(|region| RegionView {
                id: region.id.0,
                parent: region.parent.map(|parent| parent.0),
                kind: region.kind.into(),
                owner: region.owner.map(|owner| owner.0),
            })
            .collect()
    }

    #[getter]
    fn nodes(&self) -> Vec<NodeView> {
        self.graph
            .nodes()
            .map(|node| NodeView {
                id: node.id.0,
                region: node.region.0,
                operation: node.operation.0.clone(),
                inputs: node.inputs.iter().map(|port| port.0).collect(),
                outputs: node.outputs.iter().map(|port| port.0).collect(),
                effects: node.effects.iter().map(EffectEvent::from).collect(),
                symbol: node.symbol.as_ref().map(|symbol| SymbolId {
                    package: symbol.package.clone(),
                    item: symbol.item.clone(),
                }),
                number: node.payload.number_value(),
                classical_bit: node.payload.classical_bit_value(),
                angle: node.payload.angle_value().map(|angle| NodeAngle {
                    inner: angle.clone(),
                }),
                origin: node.payload.origin.and_then(|origin| {
                    qsc_codegen::circuit_qhdg::CircuitOrigin::try_from(origin)
                        .ok()
                        .map(CircuitOrigin::from)
                }),
            })
            .collect()
    }

    #[getter]
    fn ports(&self) -> Vec<PortView> {
        self.graph
            .ports()
            .map(|port| PortView {
                id: port.id.0,
                node: port.node.0,
                region: port.region.0,
                direction: port.direction.into(),
                kind: port.kind.into(),
                value_type: ValueType {
                    ty: port.ty.clone(),
                },
                stage: port.stage.into(),
                usage: port.usage.into(),
            })
            .collect()
    }

    #[getter]
    fn edges(&self) -> Vec<EdgeView> {
        self.graph
            .edges()
            .iter()
            .map(|edge| EdgeView {
                source: edge.source.0,
                target: edge.target.0,
                kind: edge.kind.into(),
            })
            .collect()
    }

    /// Reports what the graph's own verification says about this graph.
    ///
    /// An empty list means the graph satisfies every rule. A graph built here
    /// can fail verification, because the builder checks each edit in isolation
    /// and the rules are about the whole program.
    fn diagnostics(&self) -> Vec<String> {
        qsc_qhdg::verify(&self.graph)
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message))
            .collect()
    }

    /// Compiles the graph to QIR for a target profile.
    ///
    /// This is a path of its own rather than a raise followed by the circuit's
    /// compilation, so a program a pass produced reaches QIR whether or not a
    /// circuit could still express it. The profile names the same thing it
    /// names on a circuit.
    ///
    /// Only the vocabulary the circuit lowering emits compiles here, and a
    /// graph outside it is refused with the operation named.
    fn to_qir(&self, target_profile: TargetProfile) -> PyResult<String> {
        let capabilities: TargetCapabilityFlags = Into::<Profile>::into(target_profile).into();
        qsc_codegen::graph::graph_to_qir(&self.graph, capabilities)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Exports the graph as OpenQASM 3 source.
    ///
    /// The graph raises to a circuit first and the circuit's own exporter
    /// writes it, so a graph outside the circuit-expressible subset is refused
    /// by the raise rather than by a second writer that would be free to
    /// disagree with the first.
    fn to_qasm(&self) -> PyResult<String> {
        let store = qsc_codegen::circuit_qhdg::graph_to_circuit(&self.graph)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        qsc_codegen::circuit_qasm::circuit_to_qasm(&store)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Substitutes values for the parameters this graph's angles name.
    ///
    /// Returns a new graph, so the symbolic program survives and can be bound
    /// again with the next set of values, which is the shape a variational loop
    /// needs. Node, port, and edge identity carry across, so a rebind is a value
    /// update rather than a second lowering.
    ///
    /// A parameter this call does not name survives unbound, and a name no
    /// angle reads changes nothing.
    fn bind_parameters(&self, values: &Bound<'_, PyDict>) -> PyResult<Self> {
        let values: BTreeMap<String, f64> = values.extract()?;
        let mut graph = self.graph.clone();
        graph
            .rebind_parameters(&values)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { graph })
    }

    fn __repr__(&self) -> String {
        format!(
            "QuantumGraph({} nodes, {} edges)",
            self.graph.nodes().count(),
            self.graph.edges().len()
        )
    }

    /// Runs a patch-returning pass over the operations it names.
    ///
    /// The pass is called once for each matched node with a [`PassMatch`], and
    /// returns a [`PassPatch`] or `None`. Matching and application stay native,
    /// so the cost at the boundary follows how much the pass changes rather
    /// than how large the graph is.
    ///
    /// Raises `ValueError` when a decision cannot be applied, naming the
    /// operation and the reason.
    #[pyo3(signature = (operations, pass_, budget = None))]
    fn rewrite(
        &self,
        py: Python<'_>,
        operations: Vec<String>,
        pass_: &Bound<'_, PyAny>,
        budget: Option<usize>,
    ) -> PyResult<crate::pass_api::PassResult> {
        crate::pass_api::run_patches(
            py,
            &self.graph,
            &operations.into_iter().collect(),
            pass_,
            budget,
        )
    }

    /// Runs a fragment-returning pass over the operations it names.
    ///
    /// The pass is called once for each matched node and returns the sequence
    /// of operations that stands in its place, or `None`. This is separate from
    /// [`Self::rewrite`] because a patch cannot create a node, so a
    /// decomposition that grows the program has no patch form.
    ///
    /// Raises `ValueError` when a node cannot be stood in for, naming the
    /// operation and the reason.
    fn expand(
        &self,
        py: Python<'_>,
        operations: Vec<String>,
        pass_: &Bound<'_, PyAny>,
    ) -> PyResult<crate::pass_api::PassResult> {
        crate::pass_api::run_fragments(py, &self.graph, &operations.into_iter().collect(), pass_)
    }
}

/// Builds a graph region by region, node by node, port by port, and edge by
/// edge.
///
/// Each call is checked as it is made, so an edge between ports of different
/// types is refused where it is written rather than at the end. Rules about the
/// whole program are not checked here, which is what `QuantumGraph.diagnostics`
/// reports.
#[pyclass(module = "qdk.circuit")]
pub struct GraphBuilder {
    /// Taken by `finish`, so a builder cannot produce two graphs.
    builder: Option<QhdgGraphBuilder>,
}

impl GraphBuilder {
    fn active(&mut self) -> PyResult<&mut QhdgGraphBuilder> {
        self.builder
            .as_mut()
            .ok_or_else(|| PyValueError::new_err("this builder has already produced a graph"))
    }
}

fn build_err(error: &qsc_qhdg::BuildError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pymethods]
impl GraphBuilder {
    #[new]
    fn new() -> Self {
        Self {
            builder: Some(QhdgGraphBuilder::new()),
        }
    }

    /// Adds a region, optionally inside another one.
    #[pyo3(signature = (kind, parent = None))]
    fn add_region(&mut self, kind: RegionKind, parent: Option<u32>) -> PyResult<u32> {
        let region = self.active()?.add_region(parent.map(RegionId), kind.into());
        Ok(region.0)
    }

    /// Adds a node to a region, named by the operation it performs.
    #[pyo3(signature = (region, operation, effects = None))]
    fn add_node(
        &mut self,
        region: u32,
        operation: String,
        effects: Option<Vec<EffectEvent>>,
    ) -> PyResult<u32> {
        let effects = effects
            .unwrap_or_default()
            .iter()
            .map(QhdgEffectEvent::from)
            .collect();
        self.active()?
            .add_node(RegionId(region), operation, effects)
            .map(|node| node.0)
            .map_err(|error| build_err(&error))
    }

    /// Adds a port to a node.
    fn add_port(
        &mut self,
        node: u32,
        direction: PortDirection,
        kind: PortKind,
        value_type: &ValueType,
        stage: PortStage,
        usage: PortUsage,
    ) -> PyResult<u32> {
        self.active()?
            .add_port(
                NodeId(node),
                direction.into(),
                kind.into(),
                value_type.ty.clone(),
                stage.into(),
                usage.into(),
            )
            .map(|port| port.0)
            .map_err(|error| build_err(&error))
    }

    /// Connects an output port to an input port.
    fn connect(&mut self, source: u32, target: u32, kind: EdgeKind) -> PyResult<()> {
        self.active()?
            .connect(Edge {
                source: PortId(source),
                target: PortId(target),
                kind: kind.into(),
            })
            .map_err(|error| build_err(&error))
    }

    /// Declares which source-level item a node came from.
    fn declare_symbol(&mut self, node: u32, symbol: &SymbolId) -> PyResult<()> {
        self.active()?
            .declare_symbol(
                NodeId(node),
                QhdgSymbolId {
                    package: symbol.package.clone(),
                    item: symbol.item.clone(),
                },
            )
            .map_err(|error| build_err(&error))
    }

    /// Declares the value a node carries and the construct it came from.
    ///
    /// A pass that rebuilds a graph calls this to carry forward what each node
    /// held, since a node's value and origin are not recoverable from its name.
    #[pyo3(signature = (node, number = None, classical_bit = None, angle = None, origin = None))]
    fn declare_payload(
        &mut self,
        node: u32,
        number: Option<f64>,
        classical_bit: Option<u32>,
        angle: Option<&NodeAngle>,
        origin: Option<&CircuitOrigin>,
    ) -> PyResult<()> {
        let value = match (number, classical_bit, angle) {
            (None, None, None) => None,
            (Some(number), None, None) => Some(QhdgNodeValue::Number(Real::new(number))),
            (None, Some(bit), None) => Some(QhdgNodeValue::ClassicalBit(bit)),
            (None, None, Some(angle)) => Some(QhdgNodeValue::Angle(angle.inner.clone())),
            _ => {
                return Err(PyValueError::new_err(
                    "a node carries one value, so number, classical_bit, and angle cannot be combined",
                ));
            }
        };
        let origin = origin.map(node_origin).transpose()?;
        self.active()?
            .declare_payload(NodeId(node), QhdgNodePayload { value, origin })
            .map_err(|error| build_err(&error))
    }

    /// Binds a port inside a region to the parent port it crosses to.
    fn bind_interface(&mut self, region: u32, binding: &InterfaceBinding) -> PyResult<()> {
        self.active()?
            .bind_interface(
                RegionId(region),
                QhdgInterfaceBinding {
                    outer: PortId(binding.outer),
                    inner: PortId(binding.inner),
                    role: binding.role.into(),
                },
            )
            .map_err(|error| build_err(&error))
    }

    /// Declares the node a region is a body of.
    ///
    /// Branch regions are siblings under one parent, so nesting alone cannot
    /// say which node they belong to or which predicate selects between them.
    fn declare_region_owner(&mut self, region: u32, node: u32) -> PyResult<()> {
        self.active()?
            .declare_region_owner(RegionId(region), NodeId(node))
            .map_err(|error| build_err(&error))
    }

    /// Marks a node as demanded, so that analysis treats it as needed.
    fn demand(&mut self, node: u32) -> PyResult<()> {
        self.active()?.demand(NodeId(node));
        Ok(())
    }

    /// Produces the graph, leaving the builder spent.
    fn finish(&mut self) -> PyResult<QuantumGraph> {
        let builder = self
            .builder
            .take()
            .ok_or_else(|| PyValueError::new_err("this builder has already produced a graph"))?;
        Ok(QuantumGraph {
            graph: builder.finish(),
        })
    }
}
