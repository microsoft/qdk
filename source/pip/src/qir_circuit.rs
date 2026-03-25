// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Provides a `#[pyfunction]` that accepts QIR program data (blocks and
//! instructions expressed as Python primitives) and produces a `Circuit`.
//!
//! The heavy lifting is done by the existing `rir_to_circuit` infrastructure
//! in the `qsc_circuit` crate.  This module bridges the Python ↔ Rust gap
//! by defining a `QirQuantumProgram` struct that implements the
//! `QuantumProgram` trait using data supplied from the Python side.

use std::rc::Rc;

use pyo3::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
use qsc::circuit::{
    DbgInfo, LexicalScope, LogicalStackEntryLocation, LoopIdCache, PackageOffset, QuantumProgram,
    Scope, SourceLocation, SourceLookup, TracerConfig,
    instruction_types::{
        BinOpKind, BlockIdx, DbgLocationIdx, FcmpCondition, IcmpCondition, Instr, Lit, Opr, Var,
        VarTy,
    },
    rir_to_circuit,
};

use crate::interpreter::{Circuit, CircuitConfig};

// ---------------------------------------------------------------------------
// QirQuantumProgram – implements QuantumProgram from Python-supplied data
// ---------------------------------------------------------------------------

struct QirQuantumProgram {
    entry_block: BlockIdx,
    num_qubits: usize,
    blocks: Vec<(BlockIdx, Vec<Instr>)>,
    dbg_info: DbgInfo,
}

impl QuantumProgram for QirQuantumProgram {
    fn entry_block_id(&self) -> BlockIdx {
        self.entry_block
    }

    fn num_qubits(&self) -> usize {
        self.num_qubits
    }

    fn get_block_instructions(&self, id: BlockIdx) -> Vec<Instr> {
        self.blocks
            .iter()
            .find(|(bid, _)| *bid == id)
            .map(|(_, instrs)| instrs.clone())
            .unwrap_or_default()
    }

    fn block_ids(&self) -> Vec<BlockIdx> {
        self.blocks.iter().map(|(id, _)| *id).collect()
    }

    fn dbg_info(&self) -> &DbgInfo {
        &self.dbg_info
    }
}

// ---------------------------------------------------------------------------
// NoOpSourceLookup – stub SourceLookup for QIR (no Q# sources)
// ---------------------------------------------------------------------------

struct NoOpSourceLookup;

impl SourceLookup for NoOpSourceLookup {
    fn resolve_package_offset(&self, _package_offset: &PackageOffset) -> SourceLocation {
        SourceLocation {
            file: String::new(),
            line: 0,
            column: 0,
        }
    }

    fn resolve_scope(&self, scope: &Scope, _loop_id_cache: &mut LoopIdCache) -> LexicalScope {
        let name: Rc<str> = match scope {
            Scope::Top => "top".into(),
            Scope::ClassicallyControlled { label, .. } => label.clone().into(),
            _ => "scope".into(),
        };
        LexicalScope {
            location: None,
            name,
            is_adjoint: false,
            is_classically_controlled: matches!(scope, Scope::ClassicallyControlled { .. }),
        }
    }

    fn resolve_logical_stack_entry_location(
        &self,
        _location: LogicalStackEntryLocation,
        _loop_id_cache: &mut LoopIdCache,
    ) -> Option<PackageOffset> {
        None
    }
}

// ---------------------------------------------------------------------------
// Python → Rust instruction conversion helpers
// ---------------------------------------------------------------------------

/// Convert a Python dict representing a variable to `Var`.
fn py_to_var(obj: &Bound<'_, PyAny>) -> PyResult<Var> {
    let id: usize = obj.get_item("id")?.extract()?;
    let ty_str: String = obj.get_item("ty")?.extract()?;
    let ty = match ty_str.as_str() {
        "Qubit" => VarTy::Qubit,
        "Result" => VarTy::Result,
        "Boolean" => VarTy::Boolean,
        "Integer" => VarTy::Integer,
        "Double" => VarTy::Double,
        "Pointer" => VarTy::Pointer,
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown variable type: {other}"
            )));
        }
    };
    Ok(Var { id, ty })
}

/// Convert a Python dict representing an operand to `Opr`.
fn py_to_opr(obj: &Bound<'_, PyAny>) -> PyResult<Opr> {
    let kind: String = obj.get_item("kind")?.extract()?;
    match kind.as_str() {
        "var" => Ok(Opr::Variable(py_to_var(&obj.get_item("var")?)?)),
        "lit" => {
            let lit = py_to_lit(&obj.get_item("lit")?)?;
            Ok(Opr::Literal(lit))
        }
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown operand kind: {other}"
        ))),
    }
}

/// Convert a Python dict representing a literal to `Lit`.
fn py_to_lit(obj: &Bound<'_, PyAny>) -> PyResult<Lit> {
    let kind: String = obj.get_item("kind")?.extract()?;
    match kind.as_str() {
        "Qubit" => Ok(Lit::Qubit(obj.get_item("value")?.extract()?)),
        "Result" => Ok(Lit::Result(obj.get_item("value")?.extract()?)),
        "Bool" => Ok(Lit::Bool(obj.get_item("value")?.extract()?)),
        "Integer" => Ok(Lit::Integer(obj.get_item("value")?.extract()?)),
        "Double" => Ok(Lit::Double(obj.get_item("value")?.extract()?)),
        "Pointer" => Ok(Lit::Pointer),
        "Tag" => {
            let idx: usize = obj.get_item("idx")?.extract()?;
            let len: usize = obj.get_item("len")?.extract()?;
            Ok(Lit::Tag(idx, len))
        }
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown literal kind: {other}"
        ))),
    }
}

/// Convert a Python list of operands to `Vec<Opr>`.
fn py_to_oprs(list: &Bound<'_, PyList>) -> PyResult<Vec<Opr>> {
    list.iter().map(|item| py_to_opr(&item)).collect()
}

fn py_to_icmp(s: &str) -> PyResult<IcmpCondition> {
    match s {
        "Eq" => Ok(IcmpCondition::Eq),
        "Ne" => Ok(IcmpCondition::Ne),
        "Slt" => Ok(IcmpCondition::Slt),
        "Sle" => Ok(IcmpCondition::Sle),
        "Sgt" => Ok(IcmpCondition::Sgt),
        "Sge" => Ok(IcmpCondition::Sge),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown icmp condition: {other}"
        ))),
    }
}

fn py_to_fcmp(s: &str) -> PyResult<FcmpCondition> {
    match s {
        "False" => Ok(FcmpCondition::False),
        "OrderedAndEqual" => Ok(FcmpCondition::OrderedAndEqual),
        "OrderedAndGreaterThan" => Ok(FcmpCondition::OrderedAndGreaterThan),
        "OrderedAndGreaterThanOrEqual" => Ok(FcmpCondition::OrderedAndGreaterThanOrEqual),
        "OrderedAndLessThan" => Ok(FcmpCondition::OrderedAndLessThan),
        "OrderedAndLessThanOrEqual" => Ok(FcmpCondition::OrderedAndLessThanOrEqual),
        "OrderedAndNotEqual" => Ok(FcmpCondition::OrderedAndNotEqual),
        "Ordered" => Ok(FcmpCondition::Ordered),
        "UnorderedOrEqual" => Ok(FcmpCondition::UnorderedOrEqual),
        "UnorderedOrGreaterThan" => Ok(FcmpCondition::UnorderedOrGreaterThan),
        "UnorderedOrGreaterThanOrEqual" => Ok(FcmpCondition::UnorderedOrGreaterThanOrEqual),
        "UnorderedOrLessThan" => Ok(FcmpCondition::UnorderedOrLessThan),
        "UnorderedOrLessThanOrEqual" => Ok(FcmpCondition::UnorderedOrLessThanOrEqual),
        "UnorderedOrNotEqual" => Ok(FcmpCondition::UnorderedOrNotEqual),
        "Unordered" => Ok(FcmpCondition::Unordered),
        "True" => Ok(FcmpCondition::True),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown fcmp condition: {other}"
        ))),
    }
}

fn py_to_binop_kind(s: &str) -> PyResult<BinOpKind> {
    match s {
        "Add" => Ok(BinOpKind::Add),
        "Sub" => Ok(BinOpKind::Sub),
        "Mul" => Ok(BinOpKind::Mul),
        "Sdiv" => Ok(BinOpKind::Sdiv),
        "Srem" => Ok(BinOpKind::Srem),
        "Shl" => Ok(BinOpKind::Shl),
        "Ashr" => Ok(BinOpKind::Ashr),
        "Fadd" => Ok(BinOpKind::Fadd),
        "Fsub" => Ok(BinOpKind::Fsub),
        "Fmul" => Ok(BinOpKind::Fmul),
        "Fdiv" => Ok(BinOpKind::Fdiv),
        "LogicalAnd" => Ok(BinOpKind::LogicalAnd),
        "LogicalOr" => Ok(BinOpKind::LogicalOr),
        "BitwiseAnd" => Ok(BinOpKind::BitwiseAnd),
        "BitwiseOr" => Ok(BinOpKind::BitwiseOr),
        "BitwiseXor" => Ok(BinOpKind::BitwiseXor),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown binop kind: {other}"
        ))),
    }
}

/// Convert a Python dict representing an instruction to `Instr`.
fn py_to_instr(obj: &Bound<'_, PyAny>) -> PyResult<Instr> {
    let kind: String = obj.get_item("kind")?.extract()?;
    match kind.as_str() {
        "Call" => {
            let callable_name: String = obj.get_item("callable_name")?.extract()?;
            let args = py_to_oprs(obj.get_item("args")?.cast::<PyList>()?)?;
            let output = if obj.get_item("output")?.is_none() {
                None
            } else {
                Some(py_to_var(&obj.get_item("output")?)?)
            };
            let dbg_location: Option<DbgLocationIdx> = obj
                .get_item("dbg_location")
                .ok()
                .and_then(|v| v.extract().ok());
            Ok(Instr::Call {
                callable_name,
                args,
                output,
                dbg_location,
            })
        }
        "Jump" => {
            let target: BlockIdx = obj.get_item("target")?.extract()?;
            Ok(Instr::Jump(target))
        }
        "Branch" => {
            let condition = py_to_var(&obj.get_item("condition")?)?;
            let true_block: BlockIdx = obj.get_item("true_block")?.extract()?;
            let false_block: BlockIdx = obj.get_item("false_block")?.extract()?;
            let dbg_location: Option<DbgLocationIdx> = obj
                .get_item("dbg_location")
                .ok()
                .and_then(|v| v.extract().ok());
            Ok(Instr::Branch {
                condition,
                true_block,
                false_block,
                dbg_location,
            })
        }
        "Return" => Ok(Instr::Return),
        "Icmp" => {
            let cc: String = obj.get_item("condition")?.extract()?;
            let a = py_to_opr(&obj.get_item("operand0")?)?;
            let b = py_to_opr(&obj.get_item("operand1")?)?;
            let v = py_to_var(&obj.get_item("variable")?)?;
            Ok(Instr::Icmp(py_to_icmp(&cc)?, a, b, v))
        }
        "Fcmp" => {
            let cc: String = obj.get_item("condition")?.extract()?;
            let a = py_to_opr(&obj.get_item("operand0")?)?;
            let b = py_to_opr(&obj.get_item("operand1")?)?;
            let v = py_to_var(&obj.get_item("variable")?)?;
            Ok(Instr::Fcmp(py_to_fcmp(&cc)?, a, b, v))
        }
        "Phi" => {
            let pres_list = obj.get_item("predecessors")?.cast::<PyList>()?.clone();
            let mut pres = Vec::new();
            for item in pres_list.iter() {
                let tup = item.cast::<PyTuple>()?;
                let opr = py_to_opr(&tup.get_item(0)?)?;
                let block: BlockIdx = tup.get_item(1)?.extract()?;
                pres.push((opr, block));
            }
            let v = py_to_var(&obj.get_item("variable")?)?;
            Ok(Instr::Phi(pres, v))
        }
        "BinOp" => {
            let op: String = obj.get_item("op")?.extract()?;
            let a = py_to_opr(&obj.get_item("operand0")?)?;
            let b = py_to_opr(&obj.get_item("operand1")?)?;
            let v = py_to_var(&obj.get_item("variable")?)?;
            Ok(Instr::BinOp(py_to_binop_kind(&op)?, a, b, v))
        }
        "LogicalNot" => {
            let a = py_to_opr(&obj.get_item("operand")?)?;
            let v = py_to_var(&obj.get_item("variable")?)?;
            Ok(Instr::LogicalNot(a, v))
        }
        "Convert" => {
            let a = py_to_opr(&obj.get_item("operand")?)?;
            let v = py_to_var(&obj.get_item("variable")?)?;
            Ok(Instr::Convert(a, v))
        }
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown instruction kind: {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Public PyO3 function
// ---------------------------------------------------------------------------

/// Generate a circuit diagram from a QIR program represented as
/// blocks and instructions.
///
/// This is the Rust entry point called from `qsharp._qir_circuit`.
/// The Python side walks a `pyqir.Module`, converts its instructions
/// to a list of dicts, and passes them here.
#[pyfunction]
#[pyo3(signature = (entry_block_id, num_qubits, blocks, config))]
pub(crate) fn circuit_from_qir_program(
    py: Python<'_>,
    entry_block_id: usize,
    num_qubits: usize,
    blocks: &Bound<'_, PyList>,
    config: &CircuitConfig,
) -> PyResult<Py<PyAny>> {
    // Parse blocks: list of (block_id, [instr_dict, ...])
    let mut parsed_blocks: Vec<(BlockIdx, Vec<Instr>)> = Vec::new();
    for block_item in blocks.iter() {
        let tup = block_item.cast::<PyTuple>()?;
        let block_id: BlockIdx = tup.get_item(0)?.extract()?;
        let instrs_list = tup.get_item(1)?.cast::<PyList>()?.clone();
        let mut instrs = Vec::new();
        for instr_item in instrs_list.iter() {
            instrs.push(py_to_instr(&instr_item)?);
        }
        parsed_blocks.push((block_id, instrs));
    }

    let program = QirQuantumProgram {
        entry_block: entry_block_id,
        num_qubits,
        blocks: parsed_blocks,
        dbg_info: DbgInfo::default(),
    };

    let tracer_config = TracerConfig {
        max_operations: config
            .max_operations
            .unwrap_or(TracerConfig::DEFAULT_MAX_OPERATIONS),
        source_locations: config.source_locations,
        group_by_scope: config.group_by_scope,
        prune_classical_qubits: config.prune_classical_qubits,
    };

    match rir_to_circuit(&program, tracer_config, &[], &NoOpSourceLookup) {
        Ok(circuit) => Circuit(circuit).into_py_any(py),
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "circuit generation error: {e}"
        ))),
    }
}
