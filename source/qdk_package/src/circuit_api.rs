// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Python binding for the dense circuit store.

use std::str::FromStr;

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyType};

use qsc::TargetCapabilityFlags;
use qsc::target::Profile;
use qsc_circuit_store::{
    AngleOp, BlockId, CircuitStore, Clbit, ControlFlow, Expr, ExprId, Literal, LoopIndex,
    PackedOperation, ParamValue, ParameterId, Qubit, StandardGate, StoreError, TimeUnit, VarId,
    VarType,
};

use crate::graph_api::QuantumGraph;
use crate::interpreter::TargetProfile;

fn api_err(error: qdk_circuit_api::CircuitError) -> PyErr {
    match error {
        qdk_circuit_api::CircuitError::Invalid(message) => PyValueError::new_err(message),
        error => PyValueError::new_err(error.to_string()),
    }
}

fn store_err(error: &StoreError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// Resolves a position within a register to an absolute index.
fn register_index(name: &str, start: u32, size: u32, index: i64) -> PyResult<u32> {
    let width = i64::from(size);
    let resolved = if index < 0 { width + index } else { index };
    if resolved < 0 || resolved >= width {
        return Err(PyIndexError::new_err(format!(
            "index {index} is outside register '{name}' of size {size}"
        )));
    }
    Ok(start + u32::try_from(resolved).expect("bounds checked above"))
}

/// A named, non-overlapping view over a contiguous range of qubit indices.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct QuantumRegister {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    start: u32,
    #[pyo3(get)]
    size: u32,
}

#[pymethods]
impl QuantumRegister {
    #[new]
    fn new(name: String, size: u32) -> Self {
        Self {
            name,
            start: 0,
            size,
        }
    }

    fn __len__(&self) -> usize {
        self.size as usize
    }

    /// Absolute qubit index for a position in this register.
    fn __getitem__(&self, index: i64) -> PyResult<u32> {
        register_index(&self.name, self.start, self.size, index)
    }

    fn __repr__(&self) -> String {
        format!("QuantumRegister('{}', {})", self.name, self.size)
    }
}

/// A named, non-overlapping view over a contiguous range of classical bits.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct ClassicalRegister {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    start: u32,
    #[pyo3(get)]
    size: u32,
}

#[pymethods]
impl ClassicalRegister {
    #[new]
    fn new(name: String, size: u32) -> Self {
        Self {
            name,
            start: 0,
            size,
        }
    }

    fn __len__(&self) -> usize {
        self.size as usize
    }

    /// Absolute classical bit index for a position in this register.
    fn __getitem__(&self, index: i64) -> PyResult<u32> {
        register_index(&self.name, self.start, self.size, index)
    }

    fn __repr__(&self) -> String {
        format!("ClassicalRegister('{}', {})", self.name, self.size)
    }
}

/// A register supplied when building a circuit, of either kind.
#[derive(FromPyObject)]
enum RegisterSpec {
    Quantum(QuantumRegister),
    Classical(ClassicalRegister),
}

/// A compile-time symbolic value, bound before compilation.
///
/// Parameters belong to the circuit that created them. That is deliberately
/// narrower than a free-floating symbol, because it removes the question of
/// what it means to bind a symbol shared between two unrelated circuits.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct Parameter {
    #[pyo3(get)]
    name: String,
    id: u32,
}

#[pymethods]
impl Parameter {
    fn __repr__(&self) -> String {
        format!("Parameter('{}')", self.name)
    }

    fn __eq__(&self, other: &Parameter) -> bool {
        self.id == other.id && self.name == other.name
    }

    fn __hash__(&self) -> u64 {
        u64::from(self.id)
    }

    fn __add__(&self, other: Angle) -> AngleExpression {
        combine(
            &self.as_expression(),
            AngleOp::Add,
            &other.into_expression(),
        )
    }

    fn __radd__(&self, other: Angle) -> AngleExpression {
        combine(
            &other.into_expression(),
            AngleOp::Add,
            &self.as_expression(),
        )
    }

    fn __sub__(&self, other: Angle) -> AngleExpression {
        combine(
            &self.as_expression(),
            AngleOp::Sub,
            &other.into_expression(),
        )
    }

    fn __rsub__(&self, other: Angle) -> AngleExpression {
        combine(
            &other.into_expression(),
            AngleOp::Sub,
            &self.as_expression(),
        )
    }

    fn __mul__(&self, other: Angle) -> AngleExpression {
        combine(
            &self.as_expression(),
            AngleOp::Mul,
            &other.into_expression(),
        )
    }

    fn __rmul__(&self, other: Angle) -> AngleExpression {
        combine(
            &other.into_expression(),
            AngleOp::Mul,
            &self.as_expression(),
        )
    }

    fn __truediv__(&self, other: Angle) -> AngleExpression {
        combine(
            &self.as_expression(),
            AngleOp::Div,
            &other.into_expression(),
        )
    }

    fn __rtruediv__(&self, other: Angle) -> AngleExpression {
        combine(
            &other.into_expression(),
            AngleOp::Div,
            &self.as_expression(),
        )
    }

    fn __neg__(&self) -> AngleExpression {
        negated(&self.as_expression())
    }
}

impl Parameter {
    fn as_expression(&self) -> AngleExpression {
        AngleExpression {
            inner: qdk_circuit_api::AngleExpression::parameter(
                ParameterId(self.id),
                self.name.clone(),
            ),
        }
    }
}

/// A loop index read while the program runs.
///
/// The counterpart to a parameter standing in for an angle. A parameter is
/// resolved before compilation, so it can be an angle but cannot vary inside a
/// loop the target runs itself. A variable is present at run time, so it can be
/// tested in a condition but cannot stand in for an angle.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct LoopVariable {
    #[pyo3(get)]
    name: String,
    id: u32,
}

#[pymethods]
impl LoopVariable {
    fn __repr__(&self) -> String {
        format!("LoopVariable('{}')", self.name)
    }
}

/// What a condition tests.
#[derive(FromPyObject)]
enum ConditionSubject {
    Variable(LoopVariable),
    Register(ClassicalRegister),
    Clbit(i64),
}

/// A value a for loop iterates over.
#[derive(FromPyObject)]
enum LoopValue {
    Int(i64),
    Float(f64),
}

impl LoopValue {
    const fn into_literal(self) -> Literal {
        match self {
            Self::Int(value) => Literal::Int(value),
            Self::Float(value) => Literal::Float(value),
        }
    }
}

/// Arithmetic over angles, built from parameters and numbers.
///
/// Resolved before compilation, so an expression never reaches the target as a
/// computation and never raises the capability a circuit demands.
///
/// Held flat, root last, rather than as a tree of boxes. Expressions are built
/// by user code that can nest them arbitrarily, and a flat form costs no stack
/// to build, combine, store, or drop.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct AngleExpression {
    inner: qdk_circuit_api::AngleExpression,
}

fn combine(left: &AngleExpression, op: AngleOp, right: &AngleExpression) -> AngleExpression {
    AngleExpression {
        inner: qdk_circuit_api::combine(&left.inner, op, &right.inner),
    }
}

fn negated(value: &AngleExpression) -> AngleExpression {
    AngleExpression {
        inner: qdk_circuit_api::negated(&value.inner),
    }
}

#[pymethods]
impl AngleExpression {
    fn __add__(&self, other: Angle) -> AngleExpression {
        combine(self, AngleOp::Add, &other.into_expression())
    }

    fn __radd__(&self, other: Angle) -> AngleExpression {
        combine(&other.into_expression(), AngleOp::Add, self)
    }

    fn __sub__(&self, other: Angle) -> AngleExpression {
        combine(self, AngleOp::Sub, &other.into_expression())
    }

    fn __rsub__(&self, other: Angle) -> AngleExpression {
        combine(&other.into_expression(), AngleOp::Sub, self)
    }

    fn __mul__(&self, other: Angle) -> AngleExpression {
        combine(self, AngleOp::Mul, &other.into_expression())
    }

    fn __rmul__(&self, other: Angle) -> AngleExpression {
        combine(&other.into_expression(), AngleOp::Mul, self)
    }

    fn __truediv__(&self, other: Angle) -> AngleExpression {
        combine(self, AngleOp::Div, &other.into_expression())
    }

    fn __rtruediv__(&self, other: Angle) -> AngleExpression {
        combine(&other.into_expression(), AngleOp::Div, self)
    }

    fn __neg__(&self) -> AngleExpression {
        negated(self)
    }

    fn __repr__(&self) -> String {
        format!("AngleExpression('{}')", self.inner.text())
    }
}

/// An angle argument: a fixed value, a parameter, or arithmetic over those.
#[derive(FromPyObject)]
enum Angle {
    Literal(f64),
    Symbol(Parameter),
    Expression(AngleExpression),
}

impl Angle {
    fn into_expression(self) -> AngleExpression {
        match self {
            Self::Literal(value) => AngleExpression {
                inner: qdk_circuit_api::AngleExpression::literal(value),
            },
            Self::Symbol(parameter) => parameter.as_expression(),
            Self::Expression(expression) => expression,
        }
    }

    /// Resolves to a stored angle, interning arithmetic into the circuit.
    fn into_param_value(self, store: &mut CircuitStore) -> ParamValue {
        match self {
            Self::Literal(value) => ParamValue::Literal(value),
            Self::Symbol(parameter) => ParamValue::Parameter(ParameterId(parameter.id)),
            Self::Expression(expression) => ParamValue::Expr(expression.inner.intern(store)),
        }
    }
}

/// A stored angle as seen from Python: a float, the parameter still standing
/// in for one, or the arithmetic that produces it.
#[derive(Clone, IntoPyObject)]
enum StoredAngle {
    Literal(f64),
    Symbol(Parameter),
    Expression(AngleExpression),
}

/// A classical value as seen from Python.
#[derive(Clone, IntoPyObject)]
enum StoredLiteral {
    Bool(bool),
    Int(i64),
    Float(f64),
}

impl From<Literal> for StoredLiteral {
    fn from(value: Literal) -> Self {
        match value {
            Literal::Bool(value) => Self::Bool(value),
            Literal::Int(value) => Self::Int(value),
            Literal::Float(value) => Self::Float(value),
        }
    }
}

/// A condition as a readable tree.
///
/// Reported rather than reduced to a subject and a value, because a condition
/// can be built at a level below this surface and reporting only the shapes
/// this surface happens to produce would misdescribe the rest.
#[pyclass(module = "qdk.circuit", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct ConditionView {
    /// One of `literal`, `clbit`, `variable`, `not`, or `binary`.
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    op: Option<String>,
    #[pyo3(get)]
    clbit: Option<u32>,
    #[pyo3(get)]
    name: Option<String>,
    #[pyo3(get)]
    value: Option<StoredLiteral>,
    #[pyo3(get)]
    operands: Vec<ConditionView>,
}

#[pymethods]
impl ConditionView {
    fn __repr__(&self) -> String {
        format!("ConditionView(kind='{}')", self.kind)
    }
}

impl From<qdk_circuit_api::ConditionView> for ConditionView {
    fn from(value: qdk_circuit_api::ConditionView) -> Self {
        Self {
            kind: value.kind,
            op: value.op,
            clbit: value.clbit,
            name: value.name,
            value: value.value.map(Into::into),
            operands: value.operands.into_iter().map(Into::into).collect(),
        }
    }
}

/// A read-only view of one stored instruction.
///
/// Mutating this object does not change the circuit it came from. Handles are
/// views onto internal data, and the circuit owns the invariants that data
/// depends on, so mutation goes through the circuit rather than the view.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct InstructionView {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    qubits: Vec<u32>,
    #[pyo3(get)]
    clbits: Vec<u32>,
    #[pyo3(get)]
    params: Vec<StoredAngle>,
    /// Blocks a control-flow construct transfers into, empty for anything else.
    ///
    /// Reported as identifiers rather than as their contents, so that reading
    /// an instruction costs the same whatever it contains.
    #[pyo3(get)]
    blocks: Vec<u32>,
    #[pyo3(get)]
    condition: Option<ConditionView>,
    /// Values a for loop runs over.
    #[pyo3(get)]
    values: Vec<StoredLiteral>,
    /// Name of the index a for loop binds.
    #[pyo3(get)]
    loop_index: Option<String>,
    /// Label carried by a box.
    #[pyo3(get)]
    label: Option<String>,
    /// Values each case of a switch matches, aligned with the leading blocks.
    #[pyo3(get)]
    cases: Vec<Vec<StoredLiteral>>,
    /// Block a switch takes when no case matches.
    #[pyo3(get)]
    default_block: Option<u32>,
}

#[pymethods]
impl InstructionView {
    fn __repr__(&self) -> String {
        format!(
            "InstructionView(name='{}', qubits={:?}, clbits={:?})",
            self.name, self.qubits, self.clbits
        )
    }
}

impl From<qdk_circuit_api::InstructionView> for InstructionView {
    fn from(value: qdk_circuit_api::InstructionView) -> Self {
        Self {
            name: value.name,
            qubits: value.qubits,
            clbits: value.clbits,
            params: value
                .params
                .into_iter()
                .map(|angle| match angle {
                    qdk_circuit_api::StoredAngle::Literal(value) => StoredAngle::Literal(value),
                    qdk_circuit_api::StoredAngle::Symbol { id, name } => {
                        StoredAngle::Symbol(Parameter { id: id.0, name })
                    }
                    qdk_circuit_api::StoredAngle::Expression(inner) => {
                        StoredAngle::Expression(AngleExpression { inner })
                    }
                })
                .collect(),
            blocks: value.blocks,
            condition: value.condition.map(Into::into),
            values: value.values.into_iter().map(Into::into).collect(),
            loop_index: value.loop_index,
            label: value.label,
            cases: value
                .cases
                .into_iter()
                .map(|values| values.into_iter().map(Into::into).collect())
                .collect(),
            default_block: value.default_block,
        }
    }
}

/// A control-flow body that retains its Python circuit.
#[pyclass(module = "qdk.circuit")]
pub struct BlockBuilder {
    circuit: Py<QuantumCircuit>,
    body: qdk_circuit_api::BodyBuilder,
}

#[pymethods]
impl BlockBuilder {
    fn __enter__(&mut self, py: Python<'_>) -> PyResult<Py<QuantumCircuit>> {
        self.body
            .enter(&mut self.circuit.borrow_mut(py).circuit)
            .map_err(api_err)?;
        Ok(self.circuit.clone_ref(py))
    }

    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(signature = (exc_type = None, exc_value = None, traceback = None))]
    fn __exit__(
        &mut self,
        py: Python<'_>,
        exc_type: Option<Py<PyAny>>,
        exc_value: Option<Py<PyAny>>,
        traceback: Option<Py<PyAny>>,
    ) -> PyResult<bool> {
        let _ = (exc_value, traceback);
        let mut circuit = self.circuit.borrow_mut(py);
        if exc_type.is_some() {
            self.body.cancel(&mut circuit.circuit)
        } else {
            self.body.finish(&mut circuit.circuit)
        }
        .map_err(api_err)?;
        Ok(false)
    }

    fn otherwise(&self, py: Python<'_>) -> PyResult<Self> {
        let body = self
            .body
            .otherwise(&mut self.circuit.borrow_mut(py).circuit)
            .map_err(api_err)?;
        Ok(Self {
            circuit: self.circuit.clone_ref(py),
            body,
        })
    }
}

/// A for-loop body retaining both its circuit and Python index object.
#[pyclass(module = "qdk.circuit")]
pub struct ForLoopBuilder {
    circuit: Py<QuantumCircuit>,
    body: qdk_circuit_api::BodyBuilder,
    handle: Py<PyAny>,
}

#[pymethods]
impl ForLoopBuilder {
    #[getter]
    fn index(&self, py: Python<'_>) -> Py<PyAny> {
        self.handle.clone_ref(py)
    }

    fn __enter__(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.body
            .enter(&mut self.circuit.borrow_mut(py).circuit)
            .map_err(api_err)?;
        Ok(self.handle.clone_ref(py))
    }

    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(signature = (exc_type = None, exc_value = None, traceback = None))]
    fn __exit__(
        &mut self,
        py: Python<'_>,
        exc_type: Option<Py<PyAny>>,
        exc_value: Option<Py<PyAny>>,
        traceback: Option<Py<PyAny>>,
    ) -> PyResult<bool> {
        let _ = (exc_value, traceback);
        let mut circuit = self.circuit.borrow_mut(py);
        if exc_type.is_some() {
            self.body.cancel(&mut circuit.circuit)
        } else {
            self.body.finish(&mut circuit.circuit)
        }
        .map_err(api_err)?;
        Ok(false)
    }
}

/// A switch that retains its Python circuit.
#[pyclass(module = "qdk.circuit")]
pub struct SwitchBuilder {
    circuit: Py<QuantumCircuit>,
    builder: qdk_circuit_api::SwitchBuilder,
}

#[pymethods]
impl SwitchBuilder {
    fn __enter__<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let circuit = slf.circuit.clone_ref(py);
        slf.builder
            .enter(&mut circuit.borrow_mut(py).circuit)
            .map_err(api_err)?;
        Ok(slf)
    }

    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(signature = (*values))]
    fn case(slf: PyRef<'_, Self>, py: Python<'_>, values: Vec<i64>) -> PyResult<SwitchCaseBuilder> {
        Self::open(
            slf,
            py,
            Some(values.into_iter().map(Literal::Int).collect()),
        )
    }

    #[allow(clippy::needless_pass_by_value)]
    fn default(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<SwitchCaseBuilder> {
        Self::open(slf, py, None)
    }

    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(signature = (exc_type = None, exc_value = None, traceback = None))]
    fn __exit__(
        &mut self,
        py: Python<'_>,
        exc_type: Option<Py<PyAny>>,
        exc_value: Option<Py<PyAny>>,
        traceback: Option<Py<PyAny>>,
    ) -> PyResult<bool> {
        let _ = (exc_value, traceback);
        let mut circuit = self.circuit.borrow_mut(py);
        if exc_type.is_some() {
            self.builder.cancel(&mut circuit.circuit)
        } else {
            self.builder.finish(&mut circuit.circuit)
        }
        .map_err(api_err)?;
        Ok(false)
    }
}

impl SwitchBuilder {
    fn open(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        matched: Option<Vec<Literal>>,
    ) -> PyResult<SwitchCaseBuilder> {
        let circuit = slf.circuit.clone_ref(py);
        let builder = slf
            .builder
            .case(&mut circuit.borrow_mut(py).circuit, matched)
            .map_err(api_err)?;
        Ok(SwitchCaseBuilder {
            switch: slf.into_pyobject(py)?.unbind(),
            circuit,
            builder,
        })
    }
}

/// A switch case retaining both its enclosing switch and circuit.
#[pyclass(module = "qdk.circuit")]
pub struct SwitchCaseBuilder {
    switch: Py<SwitchBuilder>,
    circuit: Py<QuantumCircuit>,
    builder: qdk_circuit_api::SwitchCaseBuilder,
}

#[pymethods]
impl SwitchCaseBuilder {
    fn __enter__(&mut self, py: Python<'_>) -> PyResult<Py<QuantumCircuit>> {
        self.builder
            .enter(&mut self.circuit.borrow_mut(py).circuit)
            .map_err(api_err)?;
        Ok(self.circuit.clone_ref(py))
    }

    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(signature = (exc_type = None, exc_value = None, traceback = None))]
    fn __exit__(
        &mut self,
        py: Python<'_>,
        exc_type: Option<Py<PyAny>>,
        exc_value: Option<Py<PyAny>>,
        traceback: Option<Py<PyAny>>,
    ) -> PyResult<bool> {
        let _ = (exc_value, traceback);
        let mut circuit = self.circuit.borrow_mut(py);
        if exc_type.is_some() {
            self.builder.cancel(&mut circuit.circuit)
        } else {
            self.builder.finish(
                &mut circuit.circuit,
                &mut self.switch.borrow_mut(py).builder,
            )
        }
        .map_err(api_err)?;
        Ok(false)
    }
}

/// Which kind of circuit construct a graph node came from.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum OriginKind {
    /// An instruction at a position in the circuit's sequence.
    Instruction,
    /// The declaration and lifetime of a qubit.
    Qubit,
    /// The declaration of a classical bit.
    Clbit,
    /// The circuit's global phase.
    GlobalPhase,
}

/// The circuit construct a raised operation came from.
#[pyclass(module = "qdk.circuit", frozen, from_py_object, eq)]
#[derive(Clone, Debug, PartialEq)]
pub struct CircuitOrigin {
    #[pyo3(get)]
    pub(crate) kind: OriginKind,
    /// The instruction position, qubit index, or classical bit index. The
    /// global phase has none.
    #[pyo3(get)]
    pub(crate) index: Option<usize>,
}

#[pymethods]
impl CircuitOrigin {
    #[new]
    #[pyo3(signature = (kind, index = None))]
    fn new(kind: OriginKind, index: Option<usize>) -> Self {
        Self { kind, index }
    }

    fn __repr__(&self) -> String {
        format!(
            "CircuitOrigin(kind={:?}, index={:?})",
            self.kind, self.index
        )
    }
}

impl From<qsc_codegen::circuit_qhdg::CircuitOrigin> for CircuitOrigin {
    fn from(value: qsc_codegen::circuit_qhdg::CircuitOrigin) -> Self {
        use qsc_codegen::circuit_qhdg::CircuitOrigin as Origin;
        let (kind, index) = match value {
            Origin::Instruction(index) => (OriginKind::Instruction, Some(index)),
            Origin::Qubit(index) => (OriginKind::Qubit, Some(index as usize)),
            Origin::Clbit(index) => (OriginKind::Clbit, Some(index as usize)),
            Origin::GlobalPhase => (OriginKind::GlobalPhase, None),
        };
        Self { kind, index }
    }
}

/// A circuit recovered from a graph, with what each instruction came from.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct RaisedCircuit {
    circuit: Py<QuantumCircuit>,
    /// One entry per recovered instruction, in program order.
    #[pyo3(get)]
    origins: Vec<Option<CircuitOrigin>>,
}

#[pymethods]
impl RaisedCircuit {
    #[getter]
    fn circuit(&self, py: Python<'_>) -> Py<QuantumCircuit> {
        self.circuit.clone_ref(py)
    }

    fn __repr__(&self) -> String {
        let attributed = self
            .origins
            .iter()
            .filter(|origin| origin.is_some())
            .count();
        format!(
            "RaisedCircuit({} instructions, {attributed} attributed)",
            self.origins.len()
        )
    }
}

/// A quantum circuit.
#[pyclass(module = "qdk.circuit")]
pub struct QuantumCircuit {
    circuit: qdk_circuit_api::Circuit,
}

impl QuantumCircuit {
    fn append(
        &mut self,
        operation: PackedOperation,
        qubits: &[Qubit],
        clbits: &[Clbit],
        params: Option<Box<[ParamValue]>>,
    ) -> Result<(), StoreError> {
        self.circuit.append(operation, qubits, clbits, params)
    }

    pub(crate) fn of(store: CircuitStore) -> Self {
        Self {
            circuit: qdk_circuit_api::Circuit::of(store),
        }
    }

    pub(crate) fn closed_store(&self, verb: &str) -> PyResult<&CircuitStore> {
        self.circuit.closed_store(verb).map_err(api_err)
    }

    /// Builds the value a switch selects on.
    fn selector(&mut self, subject: &ConditionSubject) -> PyResult<ExprId> {
        Ok(match subject {
            ConditionSubject::Clbit(clbit) => {
                let width = self.circuit.store().clbit_count();
                let bit = clbit_operand(*clbit, width)?;
                self.circuit.store_mut().push_expr(Expr::Clbit(bit.0))
            }
            ConditionSubject::Variable(variable) => self
                .circuit
                .store_mut()
                .push_expr(Expr::Var(VarId(variable.id))),
            // A condition on a register is written as one test per bit, which
            // is a truth value rather than the number a switch selects on.
            ConditionSubject::Register(register) => {
                return Err(PyValueError::new_err(format!(
                    "a switch selects on a classical bit or a loop variable, not on register '{}'",
                    register.name
                )));
            }
        })
    }

    /// Builds an equality condition between a runtime value and a number.
    fn equality_condition(&mut self, subject: &ConditionSubject, value: i64) -> PyResult<ExprId> {
        let left = match subject {
            ConditionSubject::Clbit(clbit) => {
                let width = self.circuit.store().clbit_count();
                let bit = clbit_operand(*clbit, width)?;
                self.circuit.store_mut().push_expr(Expr::Clbit(bit.0))
            }
            ConditionSubject::Variable(variable) => self
                .circuit
                .store_mut()
                .push_expr(Expr::Var(VarId(variable.id))),
            ConditionSubject::Register(register) => {
                return self
                    .circuit
                    .register_condition(&register.name, value)
                    .map_err(api_err);
            }
        };
        let right = self
            .circuit
            .store_mut()
            .push_expr(Expr::Literal(Literal::Int(value)));
        Ok(self.circuit.store_mut().push_expr(Expr::Binary(
            qsc_circuit_store::BinaryOp::Eq,
            left,
            right,
        )))
    }

    fn view_of(&self, instruction: &qsc_circuit_store::Instruction) -> InstructionView {
        self.circuit.view_of(instruction).into()
    }
}

/// Resolves a Python operand, which may be an index or a register position.
fn operand(value: i64, width: u32) -> PyResult<Qubit> {
    let width_signed = i64::from(width);
    let resolved = if value < 0 {
        width_signed + value
    } else {
        value
    };
    if resolved < 0 || resolved >= width_signed {
        return Err(PyIndexError::new_err(format!(
            "qubit index {value} is outside the circuit's {width} qubits"
        )));
    }
    Ok(Qubit(
        u32::try_from(resolved).expect("bounds checked above"),
    ))
}

fn clbit_operand(value: i64, width: u32) -> PyResult<Clbit> {
    let width_signed = i64::from(width);
    let resolved = if value < 0 {
        width_signed + value
    } else {
        value
    };
    if resolved < 0 || resolved >= width_signed {
        return Err(PyIndexError::new_err(format!(
            "clbit index {value} is outside the circuit's {width} classical bits"
        )));
    }
    Ok(Clbit(
        u32::try_from(resolved).expect("bounds checked above"),
    ))
}

/// Appends a rotation carrying one angle.
fn push_rotation<'py>(
    mut slf: PyRefMut<'py, QuantumCircuit>,
    gate: StandardGate,
    angle: Angle,
    qubits: &[i64],
) -> PyResult<PyRefMut<'py, QuantumCircuit>> {
    let width = slf.circuit.store().qubit_count();
    let targets: Vec<Qubit> = qubits
        .iter()
        .map(|value| operand(*value, width))
        .collect::<PyResult<_>>()?;
    let value = angle.into_param_value(slf.circuit.store_mut());
    slf.append(
        PackedOperation::Standard(gate),
        &targets,
        &[],
        Some(Box::new([value])),
    )
    .map_err(|e| store_err(&e))?;
    Ok(slf)
}

/// Appends a one-qubit standard gate, returning the circuit for chaining.
fn push_one(
    mut slf: PyRefMut<'_, QuantumCircuit>,
    gate: StandardGate,
    qubit: i64,
) -> PyResult<PyRefMut<'_, QuantumCircuit>> {
    let width = slf.circuit.store().qubit_count();
    let target = operand(qubit, width)?;
    slf.append(PackedOperation::Standard(gate), &[target], &[], None)
        .map_err(|e| store_err(&e))?;
    Ok(slf)
}

/// Appends a two-qubit standard gate, returning the circuit for chaining.
fn push_two(
    mut slf: PyRefMut<'_, QuantumCircuit>,
    gate: StandardGate,
    first: i64,
    second: i64,
) -> PyResult<PyRefMut<'_, QuantumCircuit>> {
    let width = slf.circuit.store().qubit_count();
    let a = operand(first, width)?;
    let b = operand(second, width)?;
    slf.append(PackedOperation::Standard(gate), &[a, b], &[], None)
        .map_err(|e| store_err(&e))?;
    Ok(slf)
}

/// Appends a measurement recording into a classical bit.
fn push_measurement(
    mut slf: PyRefMut<'_, QuantumCircuit>,
    gate: StandardGate,
    qubit: i64,
    clbit: i64,
) -> PyResult<PyRefMut<'_, QuantumCircuit>> {
    let qubit_width = slf.circuit.store().qubit_count();
    let clbit_width = slf.circuit.store().clbit_count();
    let target = operand(qubit, qubit_width)?;
    let destination = clbit_operand(clbit, clbit_width)?;
    slf.append(
        PackedOperation::Standard(gate),
        &[target],
        &[destination],
        None,
    )
    .map_err(|e| store_err(&e))?;
    Ok(slf)
}

#[pymethods]
impl QuantumCircuit {
    #[new]
    #[pyo3(signature = (qubits, clbits = 0))]
    fn new(qubits: u32, clbits: u32) -> Self {
        Self {
            circuit: qdk_circuit_api::Circuit::new(qubits, clbits),
        }
    }

    /// Builds a circuit from named registers laid out end to end.
    #[classmethod]
    #[pyo3(signature = (*registers, clbits = 0))]
    fn from_registers(
        _cls: &Bound<'_, PyType>,
        registers: Vec<RegisterSpec>,
        clbits: u32,
    ) -> PyResult<Self> {
        let registers = registers.into_iter().map(|register| match register {
            RegisterSpec::Quantum(register) => qdk_circuit_api::RegisterSpec::Quantum {
                name: register.name,
                size: register.size,
            },
            RegisterSpec::Classical(register) => qdk_circuit_api::RegisterSpec::Classical {
                name: register.name,
                size: register.size,
            },
        });
        Ok(Self {
            circuit: qdk_circuit_api::Circuit::from_registers(registers, clbits)
                .map_err(api_err)?,
        })
    }

    #[getter]
    fn num_qubits(&self) -> u32 {
        self.circuit.store().qubit_count()
    }

    #[getter]
    fn num_clbits(&self) -> u32 {
        self.circuit.store().clbit_count()
    }

    /// The phase applied to the circuit as a whole, in radians.
    ///
    /// Unobservable when the circuit is run on its own, and observable when it
    /// is controlled or used as a subroutine.
    #[getter]
    fn global_phase(&self) -> f64 {
        self.circuit.store().global_phase()
    }

    #[setter]
    fn set_global_phase(&mut self, phase: f64) {
        self.circuit.store_mut().set_global_phase(phase);
    }

    /// Qubit registers declared for this circuit, as non-overlapping ranges.
    #[getter]
    fn quantum_registers(&self) -> Vec<QuantumRegister> {
        self.circuit
            .quantum_registers()
            .iter()
            .map(|r| QuantumRegister {
                name: r.name.clone(),
                start: r.start,
                size: r.size,
            })
            .collect()
    }

    /// Classical bit registers declared for this circuit.
    #[getter]
    fn classical_registers(&self) -> Vec<ClassicalRegister> {
        self.circuit
            .classical_registers()
            .iter()
            .map(|r| ClassicalRegister {
                name: r.name.clone(),
                start: r.start,
                size: r.size,
            })
            .collect()
    }

    fn __len__(&self) -> usize {
        self.circuit.store().len()
    }

    /// Returns a read-only view of the instruction at `index`.
    fn __getitem__(&self, index: i64) -> PyResult<InstructionView> {
        let len = i64::try_from(self.circuit.store().len()).unwrap_or(i64::MAX);
        let resolved = if index < 0 { len + index } else { index };
        if resolved < 0 || resolved >= len {
            return Err(PyIndexError::new_err(format!(
                "instruction index {index} is outside the circuit's {len} instructions"
            )));
        }
        let position = usize::try_from(resolved).expect("bounds checked above");
        let instruction = self
            .circuit
            .store()
            .instruction(position)
            .ok_or_else(|| PyIndexError::new_err("instruction disappeared"))?;
        Ok(self.view_of(instruction))
    }

    /// Reserves room for `additional` further instructions.
    fn reserve(mut slf: PyRefMut<'_, Self>, additional: usize) -> PyRefMut<'_, Self> {
        slf.circuit.store_mut().reserve(additional);
        slf
    }

    fn i(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::I, qubit)
    }

    fn x(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::X, qubit)
    }

    fn y(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::Y, qubit)
    }

    fn z(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::Z, qubit)
    }

    fn h(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::H, qubit)
    }

    fn s(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::S, qubit)
    }

    fn sdg(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::SDag, qubit)
    }

    fn sx(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::SqrtX, qubit)
    }

    fn sxdg(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::SqrtXDag, qubit)
    }

    fn t(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::T, qubit)
    }

    fn tdg(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::TDag, qubit)
    }

    fn cx(slf: PyRefMut<'_, Self>, control: i64, target: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_two(slf, StandardGate::Cx, control, target)
    }

    fn cy(slf: PyRefMut<'_, Self>, control: i64, target: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_two(slf, StandardGate::Cy, control, target)
    }

    fn cz(slf: PyRefMut<'_, Self>, control: i64, target: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_two(slf, StandardGate::Cz, control, target)
    }

    fn swap(slf: PyRefMut<'_, Self>, first: i64, second: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_two(slf, StandardGate::Swap, first, second)
    }

    fn cswap(
        mut slf: PyRefMut<'_, Self>,
        control: i64,
        first: i64,
        second: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        let width = slf.circuit.store().qubit_count();
        let operands = [
            operand(control, width)?,
            operand(first, width)?,
            operand(second, width)?,
        ];
        slf.append(
            PackedOperation::Standard(StandardGate::Cswap),
            &operands,
            &[],
            None,
        )
        .map_err(|e| store_err(&e))?;
        Ok(slf)
    }

    /// Prepares a qubit in the plus-one eigenstate of the X observable.
    fn prepare_x(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::PrepareX, qubit)
    }

    /// Prepares a qubit in the plus-one eigenstate of the Y observable.
    fn prepare_y(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::PrepareY, qubit)
    }

    /// Prepares a qubit in the plus-one eigenstate of the Z observable.
    fn prepare_z(slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_one(slf, StandardGate::PrepareZ, qubit)
    }

    /// Declares a symbolic parameter owned by this circuit.
    fn parameter(mut slf: PyRefMut<'_, Self>, name: String) -> Parameter {
        let id = slf.circuit.store_mut().declare_parameter(name.clone());
        Parameter { name, id: id.0 }
    }

    /// Parameters referenced by at least one instruction, in declaration order.
    ///
    /// A declared but unused parameter is omitted, because binding it would
    /// have no effect and demanding a value for it would surprise the caller.
    #[getter]
    fn parameters(&self) -> Vec<Parameter> {
        self.circuit
            .store()
            .referenced_parameters()
            .into_iter()
            .map(|id| Parameter {
                name: self.circuit.store().parameters()[id.0 as usize]
                    .name
                    .clone(),
                id: id.0,
            })
            .collect()
    }

    /// Returns a new circuit with every referenced parameter replaced by a value.
    fn assign_parameters(&self, values: Vec<(Parameter, f64)>) -> PyResult<Self> {
        let bindings: Vec<(ParameterId, f64)> = values
            .into_iter()
            .map(|(parameter, value)| (ParameterId(parameter.id), value))
            .collect();
        Ok(Self {
            circuit: self.circuit.bind_parameters(&bindings).map_err(api_err)?,
        })
    }

    fn rx(slf: PyRefMut<'_, Self>, angle: Angle, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Rx, angle, &[qubit])
    }

    fn ry(slf: PyRefMut<'_, Self>, angle: Angle, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Ry, angle, &[qubit])
    }

    fn rz(slf: PyRefMut<'_, Self>, angle: Angle, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Rz, angle, &[qubit])
    }

    fn crx(
        slf: PyRefMut<'_, Self>,
        angle: Angle,
        control: i64,
        target: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Crx, angle, &[control, target])
    }

    fn cry(
        slf: PyRefMut<'_, Self>,
        angle: Angle,
        control: i64,
        target: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Cry, angle, &[control, target])
    }

    fn crz(
        slf: PyRefMut<'_, Self>,
        angle: Angle,
        control: i64,
        target: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Crz, angle, &[control, target])
    }

    fn rxx(
        slf: PyRefMut<'_, Self>,
        angle: Angle,
        first: i64,
        second: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Rxx, angle, &[first, second])
    }

    fn ryy(
        slf: PyRefMut<'_, Self>,
        angle: Angle,
        first: i64,
        second: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Ryy, angle, &[first, second])
    }

    fn rzz(
        slf: PyRefMut<'_, Self>,
        angle: Angle,
        first: i64,
        second: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        push_rotation(slf, StandardGate::Rzz, angle, &[first, second])
    }

    fn ccx(
        mut slf: PyRefMut<'_, Self>,
        control_a: i64,
        control_b: i64,
        target: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        let width = slf.circuit.store().qubit_count();
        let operands = [
            operand(control_a, width)?,
            operand(control_b, width)?,
            operand(target, width)?,
        ];
        slf.append(
            PackedOperation::Standard(StandardGate::Ccx),
            &operands,
            &[],
            None,
        )
        .map_err(|e| store_err(&e))?;
        Ok(slf)
    }

    fn measure(
        mut slf: PyRefMut<'_, Self>,
        qubit: i64,
        clbit: i64,
    ) -> PyResult<PyRefMut<'_, Self>> {
        let qubit_width = slf.circuit.store().qubit_count();
        let clbit_width = slf.circuit.store().clbit_count();
        let target = operand(qubit, qubit_width)?;
        let destination = clbit_operand(clbit, clbit_width)?;
        slf.append(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[target],
            &[destination],
            None,
        )
        .map_err(|e| store_err(&e))?;
        Ok(slf)
    }

    /// Measures a qubit in the X basis, leaving it in the observed eigenstate.
    fn measure_x(slf: PyRefMut<'_, Self>, qubit: i64, clbit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_measurement(slf, StandardGate::MeasureX, qubit, clbit)
    }

    /// Measures a qubit in the Y basis, leaving it in the observed eigenstate.
    fn measure_y(slf: PyRefMut<'_, Self>, qubit: i64, clbit: i64) -> PyResult<PyRefMut<'_, Self>> {
        push_measurement(slf, StandardGate::MeasureY, qubit, clbit)
    }

    fn reset(mut slf: PyRefMut<'_, Self>, qubit: i64) -> PyResult<PyRefMut<'_, Self>> {
        let width = slf.circuit.store().qubit_count();
        let target = operand(qubit, width)?;
        slf.append(PackedOperation::Reset, &[target], &[], None)
            .map_err(|e| store_err(&e))?;
        Ok(slf)
    }

    #[pyo3(signature = (qubits = None))]
    fn barrier(
        mut slf: PyRefMut<'_, Self>,
        qubits: Option<Vec<i64>>,
    ) -> PyResult<PyRefMut<'_, Self>> {
        let width = slf.circuit.store().qubit_count();
        let targets: Vec<Qubit> = match qubits {
            Some(values) => values
                .into_iter()
                .map(|value| operand(value, width))
                .collect::<PyResult<_>>()?,
            None => (0..width).map(Qubit).collect(),
        };
        let arity = u8::try_from(targets.len())
            .map_err(|_| PyValueError::new_err("barrier spans too many qubits"))?;
        slf.append(
            PackedOperation::Barrier { qubit_arity: arity },
            &targets,
            &[],
            None,
        )
        .map_err(|e| store_err(&e))?;
        Ok(slf)
    }

    /// Holds a qubit idle for a duration, measured in the named time unit.
    // Signature is fixed by pymethods.
    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(signature = (duration, qubit, unit = "dt".to_string()))]
    fn delay(
        mut slf: PyRefMut<'_, Self>,
        duration: f64,
        qubit: i64,
        unit: String,
    ) -> PyResult<PyRefMut<'_, Self>> {
        let measured = TimeUnit::from_str(&unit).map_err(|()| {
            PyValueError::new_err(format!(
                "'{unit}' is not a time unit; use one of {}",
                TimeUnit::ALL.map(TimeUnit::name).join(", ")
            ))
        })?;
        if !duration.is_finite() || duration < 0.0 {
            return Err(PyValueError::new_err(
                "a delay lasts a finite, non-negative duration",
            ));
        }
        let width = slf.circuit.store().qubit_count();
        let target = operand(qubit, width)?;
        slf.append(
            PackedOperation::Delay {
                duration,
                unit: measured,
            },
            &[target],
            &[],
            None,
        )
        .map_err(|e| store_err(&e))?;
        Ok(slf)
    }

    /// Opens a conditional on a measurement result or a loop index.
    ///
    /// Use as a context manager. The returned builder also opens the
    /// alternative branch through `otherwise`.
    // Signature is fixed by pymethods.
    #[allow(clippy::needless_pass_by_value)]
    fn if_test(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        subject: ConditionSubject,
        value: i64,
    ) -> PyResult<BlockBuilder> {
        let condition = slf.equality_condition(&subject, value)?;
        let body = slf
            .circuit
            .body(qdk_circuit_api::BodyKind::IfThen { condition });
        Ok(BlockBuilder {
            circuit: slf.into_pyobject(py)?.unbind(),
            body,
        })
    }

    /// Opens a loop that repeats while a measurement result holds.
    // Signature is fixed by pymethods.
    #[allow(clippy::needless_pass_by_value)]
    fn while_loop(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        subject: ConditionSubject,
        value: i64,
    ) -> PyResult<BlockBuilder> {
        let condition = slf.equality_condition(&subject, value)?;
        let body = slf
            .circuit
            .body(qdk_circuit_api::BodyKind::While { condition });
        Ok(BlockBuilder {
            circuit: slf.into_pyobject(py)?.unbind(),
            body,
        })
    }

    /// Opens a loop over a compile-time known sequence of values.
    ///
    /// The index is handed to the body, so `with circuit.for_loop(...) as i`
    /// binds it. By default it is a parameter, which can be used as an angle
    /// and requires the loop to be unrolled. Passing `runtime=True` makes it a
    /// variable instead, which can be tested in a condition and lets the target
    /// run the loop itself.
    #[pyo3(signature = (values, name = None, runtime = false))]
    fn for_loop(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        values: Vec<LoopValue>,
        name: Option<String>,
        runtime: bool,
    ) -> PyResult<ForLoopBuilder> {
        let name = match name {
            Some(name) => name,
            None => slf.circuit.next_loop_name(),
        };
        let (index, handle) = if runtime {
            let id = slf
                .circuit
                .store_mut()
                .declare_var(name.clone(), VarType::Int);
            let variable = LoopVariable { name, id: id.0 };
            (
                LoopIndex::Var(id),
                variable.into_pyobject(py)?.into_any().unbind(),
            )
        } else {
            let id = slf.circuit.store_mut().declare_parameter(name.clone());
            let parameter = Parameter { name, id: id.0 };
            (
                LoopIndex::Parameter(id),
                parameter.into_pyobject(py)?.into_any().unbind(),
            )
        };
        let body = slf.circuit.body(qdk_circuit_api::BodyKind::For {
            index,
            values: values.into_iter().map(LoopValue::into_literal).collect(),
        });
        Ok(ForLoopBuilder {
            circuit: slf.into_pyobject(py)?.unbind(),
            body,
            handle,
        })
    }

    /// Opens a group of instructions that run at the same time.
    ///
    /// Unlike a box, which only groups, a moment asserts simultaneity, so two
    /// instructions inside it may not touch the same qubit.
    fn moment(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<BlockBuilder> {
        let body = slf.circuit.body(qdk_circuit_api::BodyKind::Moment);
        Ok(BlockBuilder {
            circuit: slf.into_pyobject(py)?.unbind(),
            body,
        })
    }

    /// Opens a grouped region of instructions.
    #[pyo3(signature = (label = None))]
    fn box_(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        label: Option<String>,
    ) -> PyResult<BlockBuilder> {
        let body = slf.circuit.body(qdk_circuit_api::BodyKind::Box { label });
        Ok(BlockBuilder {
            circuit: slf.into_pyobject(py)?.unbind(),
            body,
        })
    }

    /// Opens a multi-way branch on a classical bit or a loop variable.
    ///
    /// Use as a context manager. The cases are opened from the builder it
    /// yields, and the construct is emitted when the switch itself closes.
    // Signature is fixed by pymethods.
    #[allow(clippy::needless_pass_by_value)]
    fn switch(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        subject: ConditionSubject,
    ) -> PyResult<SwitchBuilder> {
        let selector = slf.selector(&subject)?;
        let builder = slf.circuit.switch(selector);
        Ok(SwitchBuilder {
            circuit: slf.into_pyobject(py)?.unbind(),
            builder,
        })
    }

    /// Leaves the loop this instruction sits in.
    fn break_loop(mut slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        slf.circuit
            .append_jump(ControlFlow::Break, "break")
            .map_err(api_err)?;
        Ok(slf)
    }

    /// Starts the next iteration of the loop this instruction sits in.
    fn continue_loop(mut slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        slf.circuit
            .append_jump(ControlFlow::Continue, "continue")
            .map_err(api_err)?;
        Ok(slf)
    }

    /// Instructions inside a control-flow body, for inspection.
    /// Instructions in a block, addressed by an identifier from a view.
    ///
    /// Reading a construct's branches one at a time keeps them distinguishable,
    /// which concatenating them would not.
    fn block(&self, block: u32) -> PyResult<Vec<InstructionView>> {
        let body = self
            .circuit
            .store()
            .block(BlockId(block))
            .ok_or_else(|| PyIndexError::new_err(format!("block {block} does not exist")))?;
        Ok(body
            .instructions
            .iter()
            .map(|stored| self.view_of(stored))
            .collect())
    }

    /// Instructions inside a control-flow body, for inspection.
    fn body_of(&self, index: i64) -> PyResult<Vec<InstructionView>> {
        let len = i64::try_from(self.circuit.store().len()).unwrap_or(i64::MAX);
        let resolved = if index < 0 { len + index } else { index };
        if resolved < 0 || resolved >= len {
            return Err(PyIndexError::new_err("instruction does not exist"));
        }
        let position = usize::try_from(resolved).expect("bounds checked above");
        let instruction = self
            .circuit
            .store()
            .instruction(position)
            .ok_or_else(|| PyIndexError::new_err("instruction does not exist"))?;
        let PackedOperation::ControlFlow(control) = &instruction.operation else {
            return Err(PyValueError::new_err(
                "instruction is not a control-flow construct",
            ));
        };
        let mut views = Vec::new();
        for block in control.blocks() {
            let Some(body) = self.circuit.store().block(block) else {
                continue;
            };
            for stored in &body.instructions {
                views.push(self.view_of(stored));
            }
        }
        Ok(views)
    }

    fn __repr__(&self) -> String {
        format!(
            "QuantumCircuit({}, {}) with {} instructions",
            self.circuit.store().qubit_count(),
            self.circuit.store().clbit_count(),
            self.circuit.store().len()
        )
    }

    /// Converts to the package's circuit diagram type for rendering.
    ///
    /// The diagram is a display format rather than a program representation, so
    /// the conversion is lossy by design and states where.
    fn to_diagram(&self) -> crate::interpreter::Circuit {
        crate::interpreter::Circuit(self.circuit.build_diagram())
    }

    /// Renders the circuit as a text diagram.
    ///
    /// Source locations are suppressed, matching how the diagram type renders
    /// itself for users.
    fn __str__(&self) -> String {
        self.circuit
            .build_diagram()
            .display_no_locations()
            .to_string()
    }

    /// Compiles the circuit to QIR for a target profile.
    ///
    /// The profile decides what the circuit is allowed to contain: `Base`
    /// takes a straight-line program, and the adaptive profiles take
    /// measurement feedback. A circuit that asks for more than the profile
    /// offers is refused with the construct and the missing capability named,
    /// rather than being lowered into something the target cannot run.
    fn to_qir(&self, target_profile: TargetProfile) -> PyResult<String> {
        let capabilities: TargetCapabilityFlags = Into::<Profile>::into(target_profile).into();
        self.circuit.to_qir(capabilities).map_err(api_err)
    }

    /// Exports the circuit as OpenQASM 3 source.
    ///
    /// Qubits and classical bits are written as one register each, so register
    /// names given to this circuit do not appear in the output.
    fn to_qasm(&self) -> PyResult<String> {
        self.circuit.to_qasm().map_err(api_err)
    }

    /// Builds a circuit from OpenQASM 3 source.
    ///
    /// Registers are flattened into the circuit's qubit and classical bit
    /// indices in declaration order, and their names are not carried over.
    #[classmethod]
    fn from_qasm(_cls: &Bound<'_, PyType>, source: &str) -> PyResult<Self> {
        Ok(Self {
            circuit: qdk_circuit_api::Circuit::from_qasm(source).map_err(api_err)?,
        })
    }

    /// Lowers the circuit into the graph representation.
    ///
    /// Symbolic angles and conditionals are supported. Other constructs,
    /// including loops and delays, are refused with the construct named.
    fn to_graph(&self) -> PyResult<QuantumGraph> {
        let graph = self.circuit.to_graph().map_err(api_err)?;
        Ok(QuantumGraph { graph })
    }

    /// Raises a graph back to a circuit.
    ///
    /// This is the only best-effort crossing in the API. The graph expresses
    /// programs the circuit has no form for, and one of those is refused with
    /// the operation named, so a caller can tell which side of the subset they
    /// landed on. Registers are not recovered, because the graph never held
    /// them.
    #[classmethod]
    fn from_graph(_cls: &Bound<'_, PyType>, graph: &QuantumGraph) -> PyResult<Self> {
        let store = qsc_codegen::circuit_qhdg::graph_to_circuit(&graph.graph)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self::of(store))
    }

    /// Raises a graph back to a circuit, reporting what each recovered
    /// instruction came from.
    ///
    /// Attribution is read from the origin each node has carried since it was
    /// created, so a pass that removes operations leaves the survivors naming
    /// the instructions they came from rather than their new positions. A node
    /// that carries no circuit origin reports nothing rather than a guess,
    /// which is what a graph built anywhere else looks like from here.
    #[classmethod]
    fn from_graph_with_origins(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        graph: &QuantumGraph,
    ) -> PyResult<RaisedCircuit> {
        let raised = qsc_codegen::circuit_qhdg::graph_to_circuit_with_origins(&graph.graph)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let circuit = Py::new(py, Self::of(raised.circuit))?;
        Ok(RaisedCircuit {
            circuit,
            origins: raised
                .origins
                .into_iter()
                .map(|origin| origin.map(CircuitOrigin::from))
                .collect(),
        })
    }

    /// Saves the circuit as a QDK-native Cap'n Proto envelope.
    ///
    /// Save and restore is exact: every circuit that can be built can be
    /// written and read back with the same instructions, operands, parameters,
    /// classical variables, control flow, and registers.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        Ok(PyBytes::new(py, &self.circuit.to_bytes().map_err(api_err)?))
    }

    /// Saves the same model as an S-expression envelope for inspection.
    fn to_text(&self) -> PyResult<String> {
        self.circuit.to_text().map_err(api_err)
    }

    /// Restores a circuit from either snapshot envelope format.
    #[classmethod]
    fn from_bytes(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        Ok(Self {
            circuit: qdk_circuit_api::Circuit::from_bytes(data).map_err(api_err)?,
        })
    }

    /// Restores a circuit saved by `to_text`.
    #[classmethod]
    fn from_text(_cls: &Bound<'_, PyType>, text: &str) -> PyResult<Self> {
        Ok(Self {
            circuit: qdk_circuit_api::Circuit::from_text(text).map_err(api_err)?,
        })
    }
}
