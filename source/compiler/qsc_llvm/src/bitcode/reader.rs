// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Winnow-based LLVM bitcode reader.
//!
//! Two-layer design:
//!   Layer 1 (unchanged): `BitstreamReader` handles bit-level I/O, VBR encoding,
//!                         block/sub-block navigation, and abbreviation management.
//!   Layer 2 (this file): winnow combinators parse record-value slices (`&[u64]`)
//!                         into LLVM IR model types.

#[cfg(test)]
mod tests;

use super::bitstream::BitstreamReader;
use crate::model::Type;
use crate::model::{
    Attribute, AttributeGroup, BasicBlock, BinOpKind, CastKind, Constant, FloatPredicate, Function,
    GlobalVariable, Instruction, IntPredicate, Linkage, MetadataNode, MetadataValue, Module,
    NamedMetadata, Operand, Param,
};
use crate::{ReadDiagnostic, ReadDiagnosticKind, ReadPolicy, ReadReport};
use rustc_hash::FxHashMap;
use std::{cell::RefCell, fmt};
use winnow::combinator::opt;
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::{any, rest};

/// Converts a `ParseError` into a winnow `PResult`-compatible error.
fn map_parse_err<T>(result: Result<T, ParseError>) -> PResult<T> {
    result.map_err(|_| ErrMode::Cut(ContextError::new()))
}

// ---------------------------------------------------------------------------
// Winnow type aliases — &[u64] is a native winnow Stream (Token = u64)
// ---------------------------------------------------------------------------

type RecordInput<'a> = &'a [u64];
type PResult<T> = winnow::ModalResult<T, ContextError>;

// ---------------------------------------------------------------------------
// Constants — block IDs and record codes
// ---------------------------------------------------------------------------

use super::constants::*;

const BLOCKINFO_BLOCK_ID: u32 = 0;
const BLOCKINFO_CODE_SETBID: u32 = 1;

// ---------------------------------------------------------------------------
// ParseError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind: ReadDiagnosticKind,
    pub context: &'static str,
    pub offset: usize,
    pub message: String,
}

impl ParseError {
    fn malformed(offset: usize, context: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ReadDiagnosticKind::MalformedInput,
            context,
            offset,
            message: message.into(),
        }
    }

    fn unsupported(offset: usize, context: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ReadDiagnosticKind::UnsupportedSemanticConstruct,
            context,
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bitcode parse error at byte {}: {}",
            self.offset, self.message
        )
    }
}

impl std::error::Error for ParseError {}

impl From<ParseError> for ReadDiagnostic {
    fn from(error: ParseError) -> Self {
        Self {
            kind: error.kind,
            offset: Some(error.offset),
            context: error.context,
            message: error.message,
        }
    }
}

impl From<ReadDiagnostic> for ParseError {
    fn from(diagnostic: ReadDiagnostic) -> Self {
        Self {
            kind: diagnostic.kind,
            context: diagnostic.context,
            offset: diagnostic.offset.unwrap_or_default(),
            message: diagnostic.message,
        }
    }
}

// ---------------------------------------------------------------------------
// Value tracking types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum ValueEntry {
    Global(String),
    Function(String),
    Constant(Type, Constant),
    Local(String, Type),
    Param(String, Type),
    IntToPtrConst(i64, Type),
    GepConst {
        source_ty: Type,
        ptr_name: String,
        ptr_ty: Type,
        indices: Vec<Operand>,
    },
}

#[derive(Debug, Clone)]
enum MetadataSlotEntry {
    String(String),
    Value(Type, i64),
    Node(u32),
}

#[derive(Debug, Clone)]
struct FuncProto {
    #[allow(dead_code)]
    func_type_id: u32,
    is_declaration: bool,
    #[allow(dead_code)]
    paramattr_index: u32,
}

#[derive(Debug, Clone, Copy)]
struct PendingGlobalInitializer {
    global_index: usize,
    value_id: u32,
}

#[derive(Debug, Clone, Copy)]
struct PendingStrtabName {
    value_id: usize,
    offset: usize,
    size: usize,
}

// ---------------------------------------------------------------------------
// InstrContext — carries resolution state for instruction-level winnow parsers
// ---------------------------------------------------------------------------

struct InstrContext<'a> {
    global_value_table: &'a [ValueEntry],
    local_values: &'a [ValueEntry],
    type_table: &'a [Type],
    paramattr_lists: &'a [Vec<u32>],
    bb_names: &'a FxHashMap<u32, String>,
    diagnostics: &'a RefCell<Vec<ReadDiagnostic>>,
    current_value_id: u32,
    byte_offset: usize,
    policy: ReadPolicy,
}

impl InstrContext<'_> {
    fn record_compatibility_diagnostic(
        &self,
        kind: ReadDiagnosticKind,
        context: &'static str,
        message: impl Into<String>,
    ) {
        if self.policy == ReadPolicy::Compatibility {
            self.diagnostics.borrow_mut().push(ReadDiagnostic {
                kind,
                offset: Some(self.byte_offset),
                context,
                message: message.into(),
            });
        }
    }

    fn resolve_known_global_name(&self, value_id: usize) -> Option<String> {
        match self.global_value_table.get(value_id) {
            Some(ValueEntry::Global(name) | ValueEntry::Function(name)) => Some(name.clone()),
            _ => None,
        }
    }

    fn resolve_operand(&self, relative_id: u64) -> Result<Operand, ParseError> {
        if relative_id > u64::from(self.current_value_id) {
            return Err(ParseError::malformed(
                self.byte_offset,
                "value resolution",
                format!(
                    "unresolvable relative value ID: {relative_id} exceeds current value ID {}",
                    self.current_value_id
                ),
            ));
        }
        let absolute_id = self.current_value_id - relative_id as u32;
        let global_count = self.global_value_table.len() as u32;

        if absolute_id < global_count {
            match &self.global_value_table[absolute_id as usize] {
                ValueEntry::Global(name) | ValueEntry::Function(name) => {
                    Ok(Operand::GlobalRef(name.clone()))
                }
                _ => Err(ParseError::malformed(
                    self.byte_offset,
                    "value resolution",
                    format!(
                        "global value ID {absolute_id} does not reference a global or function"
                    ),
                )),
            }
        } else {
            let local_idx = (absolute_id - global_count) as usize;
            if local_idx < self.local_values.len() {
                match &self.local_values[local_idx] {
                    ValueEntry::Constant(ty, Constant::Int(val)) => {
                        Ok(Operand::IntConst(ty.clone(), *val))
                    }
                    ValueEntry::Constant(_, Constant::Float(ty, val)) => {
                        Ok(Operand::FloatConst(ty.clone(), *val))
                    }
                    ValueEntry::Constant(_, Constant::Null) => Ok(Operand::NullPtr),
                    ValueEntry::Constant(_, Constant::CString(_)) => Ok(Operand::NullPtr),
                    ValueEntry::IntToPtrConst(val, ty) => Ok(Operand::IntToPtr(*val, ty.clone())),
                    ValueEntry::GepConst {
                        source_ty,
                        ptr_name,
                        ptr_ty,
                        indices,
                    } => Ok(Operand::GetElementPtr {
                        ty: source_ty.clone(),
                        ptr: ptr_name.clone(),
                        ptr_ty: ptr_ty.clone(),
                        indices: indices.clone(),
                    }),
                    ValueEntry::Local(name, ty) => {
                        Ok(Operand::TypedLocalRef(name.clone(), ty.clone()))
                    }
                    ValueEntry::Param(name, ty) => {
                        Ok(Operand::TypedLocalRef(name.clone(), ty.clone()))
                    }
                    ValueEntry::Global(name) | ValueEntry::Function(name) => {
                        Ok(Operand::GlobalRef(name.clone()))
                    }
                }
            } else {
                Err(ParseError::malformed(
                    self.byte_offset,
                    "value resolution",
                    format!(
                        "local value index {local_idx} out of range (have {} local values)",
                        self.local_values.len()
                    ),
                ))
            }
        }
    }

    fn resolve_phi_operand(&self, encoded_delta: u64, ty: &Type) -> Result<Operand, ParseError> {
        let delta = sign_unrotate(encoded_delta);
        let absolute_id_i64 = i64::from(self.current_value_id) - delta;
        if absolute_id_i64 < 0 {
            return Err(ParseError::malformed(
                self.byte_offset,
                "phi instruction",
                format!(
                    "unresolvable PHI value delta {delta} for current value ID {}",
                    self.current_value_id
                ),
            ));
        }

        let absolute_id = absolute_id_i64 as u32;
        let global_count = self.global_value_table.len() as u32;

        if absolute_id < global_count {
            return match &self.global_value_table[absolute_id as usize] {
                ValueEntry::Global(name) | ValueEntry::Function(name) => {
                    Ok(Operand::GlobalRef(name.clone()))
                }
                _ => Err(ParseError::malformed(
                    self.byte_offset,
                    "phi instruction",
                    format!("PHI value ID {absolute_id} does not reference a global or function"),
                )),
            };
        }

        let local_idx = (absolute_id - global_count) as usize;
        if local_idx < self.local_values.len() {
            return match &self.local_values[local_idx] {
                ValueEntry::Constant(inner_ty, Constant::Int(val)) => {
                    Ok(Operand::IntConst(inner_ty.clone(), *val))
                }
                ValueEntry::Constant(_, Constant::Float(ty, val)) => {
                    Ok(Operand::FloatConst(ty.clone(), *val))
                }
                ValueEntry::Constant(_, Constant::Null | Constant::CString(_)) => {
                    Ok(Operand::NullPtr)
                }
                ValueEntry::IntToPtrConst(val, target_ty) => {
                    Ok(Operand::IntToPtr(*val, target_ty.clone()))
                }
                ValueEntry::GepConst {
                    source_ty,
                    ptr_name,
                    ptr_ty,
                    indices,
                } => Ok(Operand::GetElementPtr {
                    ty: source_ty.clone(),
                    ptr: ptr_name.clone(),
                    ptr_ty: ptr_ty.clone(),
                    indices: indices.clone(),
                }),
                ValueEntry::Local(name, inner_ty) => {
                    Ok(Operand::TypedLocalRef(name.clone(), inner_ty.clone()))
                }
                ValueEntry::Param(name, inner_ty) => {
                    Ok(Operand::TypedLocalRef(name.clone(), inner_ty.clone()))
                }
                ValueEntry::Global(name) | ValueEntry::Function(name) => {
                    Ok(Operand::GlobalRef(name.clone()))
                }
            };
        }

        Ok(Operand::TypedLocalRef(
            format!("val_{absolute_id}"),
            ty.clone(),
        ))
    }

    fn resolve_call_target_name(&self, encoded_id: u64) -> Result<String, ParseError> {
        let relative_target = self.resolve_operand(encoded_id).ok();

        if let Some(Operand::GlobalRef(name)) = &relative_target {
            return Ok(name.clone());
        }

        if let Some(name) = self.resolve_known_global_name(encoded_id as usize) {
            return Ok(name);
        }

        match relative_target {
            Some(Operand::LocalRef(name) | Operand::TypedLocalRef(name, _))
                if self.policy == ReadPolicy::Compatibility =>
            {
                self.record_compatibility_diagnostic(
                    ReadDiagnosticKind::UnsupportedSemanticConstruct,
                    "call instruction",
                    format!(
                        "call target value {encoded_id} resolved to a local value and was imported using placeholder callee `{name}`"
                    ),
                );
                Ok(name)
            }
            _ if self.policy == ReadPolicy::Compatibility => {
                self.record_compatibility_diagnostic(
                    ReadDiagnosticKind::UnsupportedSemanticConstruct,
                    "call instruction",
                    format!(
                        "call target value {encoded_id} does not resolve to a known function and was imported as `unknown_{encoded_id}`"
                    ),
                );
                Ok(format!("unknown_{encoded_id}"))
            }
            _ => Err(ParseError::unsupported(
                self.byte_offset,
                "call instruction",
                format!("call target value {encoded_id} does not resolve to a known function"),
            )),
        }
    }

    fn resolve_type(&self, type_id: u32) -> Result<Type, ParseError> {
        self.type_table
            .get(type_id as usize)
            .cloned()
            .ok_or_else(|| {
                ParseError::malformed(
                    self.byte_offset,
                    "type resolution",
                    format!(
                        "invalid type ID {type_id} (type table has {} entries)",
                        self.type_table.len()
                    ),
                )
            })
    }

    fn resolve_function_type(&self, type_id: u32) -> Result<(Type, Vec<Type>), ParseError> {
        match self.type_table.get(type_id as usize) {
            Some(Type::Function(ret, params)) => Ok((ret.as_ref().clone(), params.clone())),
            _ if self.policy == ReadPolicy::Compatibility => {
                self.record_compatibility_diagnostic(
                    ReadDiagnosticKind::UnsupportedSemanticConstruct,
                    "call instruction",
                    format!(
                        "type ID {type_id} does not resolve to a function type and was imported as `void ()`"
                    ),
                );
                Ok((Type::Void, Vec::new()))
            }
            _ => Err(ParseError::unsupported(
                self.byte_offset,
                "call instruction",
                format!("type ID {type_id} does not resolve to a function type"),
            )),
        }
    }

    fn resolve_bb_name(&self, bb_id: u32) -> String {
        self.bb_names
            .get(&bb_id)
            .cloned()
            .unwrap_or_else(|| format!("bb_{bb_id}"))
    }

    fn infer_type(&self, op: &Operand) -> Type {
        match op {
            Operand::IntConst(ty, _) => ty.clone(),
            Operand::FloatConst(ty, _) => ty.clone(),
            Operand::NullPtr => Type::Ptr,
            Operand::IntToPtr(_, ty) => ty.clone(),
            Operand::TypedLocalRef(_, ty) => ty.clone(),
            Operand::LocalRef(name) => {
                // Look up tracked type from value table
                for entry in self.local_values {
                    match entry {
                        ValueEntry::Local(n, ty) if n == name => return ty.clone(),
                        _ => {}
                    }
                }
                for entry in self.local_values {
                    if let ValueEntry::Param(param_name, ty) = entry
                        && param_name == name
                    {
                        return ty.clone();
                    }
                }
                Type::Integer(64)
            }
            Operand::GlobalRef(_) => Type::Ptr,
            Operand::GetElementPtr { ty, .. } => ty.clone(),
        }
    }

    fn result_name(&self) -> String {
        format!("val_{}", self.current_value_id)
    }
}

// ---------------------------------------------------------------------------
// Primitive winnow parsers on &[u64] record values
// ---------------------------------------------------------------------------

fn parse_char_string(input: &mut RecordInput<'_>) -> PResult<String> {
    let values = rest.parse_next(input)?;
    Ok(values.iter().map(|&v| v as u8 as char).collect())
}

fn remap_block_name(name: &str, bb_names: &FxHashMap<u32, String>) -> String {
    let Some(id) = name.strip_prefix("bb_") else {
        return name.to_string();
    };

    let Ok(bb_id) = id.parse::<u32>() else {
        return name.to_string();
    };

    bb_names
        .get(&bb_id)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

fn remap_instruction_block_names(instr: &mut Instruction, bb_names: &FxHashMap<u32, String>) {
    match instr {
        Instruction::Jump { dest } => {
            *dest = remap_block_name(dest, bb_names);
        }
        Instruction::Br {
            true_dest,
            false_dest,
            ..
        } => {
            *true_dest = remap_block_name(true_dest, bb_names);
            *false_dest = remap_block_name(false_dest, bb_names);
        }
        Instruction::Phi { incoming, .. } => {
            for (_, block) in incoming {
                *block = remap_block_name(block, bb_names);
            }
        }
        Instruction::Switch {
            default_dest,
            cases,
            ..
        } => {
            *default_dest = remap_block_name(default_dest, bb_names);
            for (_, dest) in cases {
                *dest = remap_block_name(dest, bb_names);
            }
        }
        Instruction::Ret(_)
        | Instruction::BinOp { .. }
        | Instruction::ICmp { .. }
        | Instruction::FCmp { .. }
        | Instruction::Cast { .. }
        | Instruction::Call { .. }
        | Instruction::Alloca { .. }
        | Instruction::Load { .. }
        | Instruction::Store { .. }
        | Instruction::Select { .. }
        | Instruction::Unreachable
        | Instruction::GetElementPtr { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// Type record winnow parsers
// ---------------------------------------------------------------------------

fn parse_type_integer(input: &mut RecordInput<'_>) -> PResult<Type> {
    let width = opt(any).map(|v| v.unwrap_or(32) as u32).parse_next(input)?;
    Ok(Type::Integer(width))
}

// ---------------------------------------------------------------------------
// Module record winnow parsers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ParsedGlobalVarRecord {
    is_const: bool,
    init_value_id: Option<u32>,
    linkage: Linkage,
    elem_type_id: Option<u32>,
    legacy_placeholder: bool,
}

fn decode_global_linkage(encoded: u64) -> Linkage {
    if encoded == 3 {
        Linkage::Internal
    } else {
        Linkage::External
    }
}

fn parse_global_var_record(input: &mut RecordInput<'_>) -> PResult<ParsedGlobalVarRecord> {
    let fields = rest.parse_next(input)?;

    if fields.len() >= 18 {
        let Some(elem_type_id) = fields.get(2).copied() else {
            return Err(ErrMode::Cut(ContextError::new()));
        };
        let Some(flags) = fields.get(3).copied() else {
            return Err(ErrMode::Cut(ContextError::new()));
        };
        let Some(init_raw) = fields.get(4).copied() else {
            return Err(ErrMode::Cut(ContextError::new()));
        };
        let Some(linkage) = fields.get(5).copied() else {
            return Err(ErrMode::Cut(ContextError::new()));
        };

        Ok(ParsedGlobalVarRecord {
            is_const: flags & 1 != 0,
            init_value_id: init_raw
                .checked_sub(1)
                .map(|value| u32::try_from(value).map_err(|_| ErrMode::Cut(ContextError::new())))
                .transpose()?,
            linkage: decode_global_linkage(linkage),
            elem_type_id: Some(
                u32::try_from(elem_type_id).map_err(|_| ErrMode::Cut(ContextError::new()))?,
            ),
            legacy_placeholder: false,
        })
    } else {
        if fields.len() < 5 {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        let raw_init = u32::try_from(fields[3]).map_err(|_| ErrMode::Cut(ContextError::new()))?;
        Ok(ParsedGlobalVarRecord {
            is_const: fields[2] != 0,
            init_value_id: if raw_init > 1 {
                Some(raw_init - 1)
            } else {
                None
            },
            linkage: decode_global_linkage(fields[4]),
            elem_type_id: if fields.len() >= 14 {
                Some(u32::try_from(fields[13]).map_err(|_| ErrMode::Cut(ContextError::new()))?)
            } else {
                None
            },
            legacy_placeholder: raw_init == 1,
        })
    }
}

fn parse_function_record(input: &mut RecordInput<'_>) -> PResult<(u32, bool, u32)> {
    let func_type_id = any.map(|v: u64| v as u32).parse_next(input)?;
    let _cc = any.parse_next(input)?;
    let is_declaration = any.map(|v: u64| v != 0).parse_next(input)?;
    let _linkage = opt(any).parse_next(input)?;
    let paramattr = opt(any).map(|v| v.unwrap_or(0) as u32).parse_next(input)?;
    Ok((func_type_id, is_declaration, paramattr))
}

// ---------------------------------------------------------------------------
// Instruction winnow parsers
// ---------------------------------------------------------------------------

fn parse_ret_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let maybe_val = opt(any).parse_next(input)?;
    match maybe_val {
        None => Ok(Instruction::Ret(None)),
        Some(val_id) => {
            let op = map_parse_err(ctx.resolve_operand(val_id))?;
            Ok(Instruction::Ret(Some(op)))
        }
    }
}

fn parse_br_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let first = any.parse_next(input)?;
    let second = opt(any).parse_next(input)?;
    match second {
        None => {
            let dest = ctx.resolve_bb_name(first as u32);
            Ok(Instruction::Jump { dest })
        }
        Some(false_val) => {
            let cond_id = any.parse_next(input)?;
            let true_dest = ctx.resolve_bb_name(first as u32);
            let false_dest = ctx.resolve_bb_name(false_val as u32);
            let cond = map_parse_err(ctx.resolve_operand(cond_id))?;
            Ok(Instruction::Br {
                cond_ty: Type::Integer(1),
                cond,
                true_dest,
                false_dest,
            })
        }
    }
}

fn parse_binop_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let lhs_id = any.parse_next(input)?;
    let rhs_id = any.parse_next(input)?;
    let opcode = any.parse_next(input)?;
    let lhs = map_parse_err(ctx.resolve_operand(lhs_id))?;
    let rhs = map_parse_err(ctx.resolve_operand(rhs_id))?;
    let ty = ctx.infer_type(&lhs);
    let op = map_parse_err(opcode_to_binop(opcode, &ty))?;
    Ok(Instruction::BinOp {
        op,
        ty,
        lhs,
        rhs,
        result: ctx.result_name(),
    })
}

fn parse_cmp2_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let lhs_id = any.parse_next(input)?;
    let rhs_id = any.parse_next(input)?;
    let pred_code = any.parse_next(input)?;
    let lhs = map_parse_err(ctx.resolve_operand(lhs_id))?;
    let rhs = map_parse_err(ctx.resolve_operand(rhs_id))?;
    let ty = ctx.infer_type(&lhs);
    let result = ctx.result_name();

    if pred_code >= 32 {
        Ok(Instruction::ICmp {
            pred: map_parse_err(icmp_code_to_predicate(pred_code))?,
            ty,
            lhs,
            rhs,
            result,
        })
    } else {
        Ok(Instruction::FCmp {
            pred: map_parse_err(fcmp_code_to_predicate(pred_code))?,
            ty,
            lhs,
            rhs,
            result,
        })
    }
}

fn parse_cast_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let val_id = any.parse_next(input)?;
    let to_ty_id = any.parse_next(input)? as u32;
    let cast_opcode = any.parse_next(input)?;
    let value = map_parse_err(ctx.resolve_operand(val_id))?;
    let from_ty = ctx.infer_type(&value);
    let to_ty = map_parse_err(ctx.resolve_type(to_ty_id))?;
    Ok(Instruction::Cast {
        op: map_parse_err(opcode_to_cast(cast_opcode))?,
        from_ty,
        to_ty,
        value,
        result: ctx.result_name(),
    })
}

fn parse_call_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let paramattr = any.parse_next(input)?;
    let _packed_call_cc_info = any.parse_next(input)?;
    let func_ty_id = any.parse_next(input)? as u32;
    let callee_val_id = any.parse_next(input)?;

    let callee_name = map_parse_err(ctx.resolve_call_target_name(callee_val_id))?;
    let (return_type, param_types) = map_parse_err(ctx.resolve_function_type(func_ty_id))?;

    let has_result = !matches!(return_type, Type::Void);
    let result = if has_result {
        Some(ctx.result_name())
    } else {
        None
    };
    let return_ty = if has_result { Some(return_type) } else { None };

    let remaining = rest.parse_next(input)?;
    let mut args = Vec::with_capacity(remaining.len());
    for (index, &rel_id) in remaining.iter().enumerate() {
        let ty = if let Some(ty) = param_types.get(index).cloned() {
            ty
        } else if ctx.policy == ReadPolicy::Compatibility {
            ctx.record_compatibility_diagnostic(
                ReadDiagnosticKind::UnsupportedSemanticConstruct,
                "call instruction",
                format!(
                    "call argument {index} exceeds imported function signature with {} parameter(s) and was imported as `ptr`",
                    param_types.len()
                ),
            );
            Type::Ptr
        } else {
            return map_parse_err(Err(ParseError::unsupported(
                ctx.byte_offset,
                "call instruction",
                format!(
                    "call argument {index} exceeds imported function signature with {} parameter(s)",
                    param_types.len()
                ),
            )));
        };
        let op = map_parse_err(ctx.resolve_operand(rel_id))?;
        args.push((ty, op));
    }

    Ok(Instruction::Call {
        return_ty,
        callee: callee_name,
        args,
        result,
        attr_refs: if paramattr == 0 {
            Vec::new()
        } else {
            ctx.paramattr_lists
                .get((paramattr - 1) as usize)
                .cloned()
                .unwrap_or_default()
        },
    })
}

fn parse_phi_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let ty_id = any.parse_next(input)? as u32;
    let ty = map_parse_err(ctx.resolve_type(ty_id))?;
    let remaining = rest.parse_next(input)?;
    let mut incoming = Vec::new();
    let mut i = 0;
    while i + 1 < remaining.len() {
        let val_op = map_parse_err(ctx.resolve_phi_operand(remaining[i], &ty))?;
        let bb_id = remaining[i + 1] as u32;
        incoming.push((val_op, ctx.resolve_bb_name(bb_id)));
        i += 2;
    }
    Ok(Instruction::Phi {
        ty,
        incoming,
        result: ctx.result_name(),
    })
}

fn parse_alloca_record(
    ctx: &InstrContext<'_>,
    input: &mut RecordInput<'_>,
) -> PResult<Instruction> {
    let ty_id = any.parse_next(input)? as u32;
    let ty = map_parse_err(ctx.resolve_type(ty_id))?;
    Ok(Instruction::Alloca {
        ty,
        result: ctx.result_name(),
    })
}

fn parse_load_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let ptr_id = any.parse_next(input)?;
    let ty_id = any.parse_next(input)? as u32;
    let ptr = map_parse_err(ctx.resolve_operand(ptr_id))?;
    let ty = map_parse_err(ctx.resolve_type(ty_id))?;
    let ptr_ty = ctx.infer_type(&ptr);
    Ok(Instruction::Load {
        ty,
        ptr_ty,
        ptr,
        result: ctx.result_name(),
    })
}

fn parse_store_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let ptr_id = any.parse_next(input)?;
    let value_id = any.parse_next(input)?;
    let ptr = map_parse_err(ctx.resolve_operand(ptr_id))?;
    let value = map_parse_err(ctx.resolve_operand(value_id))?;
    let ty = ctx.infer_type(&value);
    let ptr_ty = ctx.infer_type(&ptr);
    Ok(Instruction::Store {
        ty,
        value,
        ptr_ty,
        ptr,
    })
}

fn parse_select_record(
    ctx: &InstrContext<'_>,
    input: &mut RecordInput<'_>,
) -> PResult<Instruction> {
    let true_id = any.parse_next(input)?;
    let false_id = any.parse_next(input)?;
    let cond_id = any.parse_next(input)?;
    let true_val = map_parse_err(ctx.resolve_operand(true_id))?;
    let false_val = map_parse_err(ctx.resolve_operand(false_id))?;
    let cond = map_parse_err(ctx.resolve_operand(cond_id))?;
    let ty = ctx.infer_type(&true_val);
    Ok(Instruction::Select {
        cond,
        true_val,
        false_val,
        ty,
        result: ctx.result_name(),
    })
}

fn parse_switch_record(
    ctx: &InstrContext<'_>,
    input: &mut RecordInput<'_>,
) -> PResult<Instruction> {
    let ty_id = any.parse_next(input)? as u32;
    let ty = map_parse_err(ctx.resolve_type(ty_id))?;
    let value_id = any.parse_next(input)?;
    let value = map_parse_err(ctx.resolve_operand(value_id))?;
    let default_id = any.parse_next(input)? as u32;
    let default_dest = ctx.resolve_bb_name(default_id);

    let remaining = rest.parse_next(input)?;
    let mut cases = Vec::new();
    let mut i = 0;
    while i + 1 < remaining.len() {
        let case_val = sign_unrotate(remaining[i]);
        let dest_id = remaining[i + 1] as u32;
        cases.push((case_val, ctx.resolve_bb_name(dest_id)));
        i += 2;
    }
    Ok(Instruction::Switch {
        ty,
        value,
        default_dest,
        cases,
    })
}

fn parse_gep_record(ctx: &InstrContext<'_>, input: &mut RecordInput<'_>) -> PResult<Instruction> {
    let inbounds = any.map(|v: u64| v != 0).parse_next(input)?;
    let pointee_type_id = any.parse_next(input)? as u32;
    let pointee_ty = map_parse_err(ctx.resolve_type(pointee_type_id))?;
    let ptr_id = any.parse_next(input)?;
    let ptr = map_parse_err(ctx.resolve_operand(ptr_id))?;
    let ptr_ty = ctx.infer_type(&ptr);

    let remaining = rest.parse_next(input)?;
    let indices = remaining
        .iter()
        .map(|&idx_id| map_parse_err(ctx.resolve_operand(idx_id)))
        .collect::<PResult<Vec<Operand>>>()?;

    Ok(Instruction::GetElementPtr {
        inbounds,
        pointee_ty,
        ptr_ty,
        ptr,
        indices,
        result: ctx.result_name(),
    })
}

// ---------------------------------------------------------------------------
// Instruction dispatch
// ---------------------------------------------------------------------------

fn dispatch_instruction(
    code: u32,
    ctx: &InstrContext<'_>,
    values: &[u64],
    byte_offset: usize,
) -> Result<Option<Instruction>, ParseError> {
    let mut input: RecordInput<'_> = values;
    let make_err = |msg: &str| ParseError::malformed(byte_offset, "instruction record", msg);

    match code {
        FUNC_CODE_INST_RET => parse_ret_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("RET record malformed")),
        FUNC_CODE_INST_BR => parse_br_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("BR record has invalid number of values")),
        FUNC_CODE_INST_BINOP => parse_binop_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("BINOP record too short")),
        FUNC_CODE_INST_CMP2 => parse_cmp2_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("CMP2 record too short")),
        FUNC_CODE_INST_CAST => parse_cast_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("CAST record too short")),
        FUNC_CODE_INST_CALL => parse_call_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("CALL record too short")),
        FUNC_CODE_INST_PHI => parse_phi_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("PHI record too short")),
        FUNC_CODE_INST_ALLOCA => parse_alloca_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("ALLOCA record too short")),
        FUNC_CODE_INST_LOAD => parse_load_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("LOAD record too short")),
        FUNC_CODE_INST_STORE => parse_store_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("STORE record too short")),
        FUNC_CODE_INST_SELECT => parse_select_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("SELECT record too short")),
        FUNC_CODE_INST_SWITCH => parse_switch_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("SWITCH record too short")),
        FUNC_CODE_INST_GEP => parse_gep_record(ctx, &mut input)
            .map(Some)
            .map_err(|_| make_err("GEP record too short")),
        FUNC_CODE_INST_UNREACHABLE => Ok(Some(Instruction::Unreachable)),
        _ if ctx.policy == ReadPolicy::Compatibility => {
            ctx.record_compatibility_diagnostic(
                ReadDiagnosticKind::UnsupportedSemanticConstruct,
                "instruction record",
                format!("unsupported instruction record code {code} was dropped during import"),
            );
            Ok(None)
        }
        _ => Err(ParseError::unsupported(
            byte_offset,
            "instruction record",
            format!("unsupported instruction record code {code}"),
        )),
    }
}

fn instruction_produces_value(code: u32, _values: &[u64], instr: &Instruction) -> bool {
    match code {
        FUNC_CODE_INST_BINOP
        | FUNC_CODE_INST_CAST
        | FUNC_CODE_INST_CMP2
        | FUNC_CODE_INST_PHI
        | FUNC_CODE_INST_ALLOCA
        | FUNC_CODE_INST_LOAD
        | FUNC_CODE_INST_SELECT
        | FUNC_CODE_INST_GEP => true,
        FUNC_CODE_INST_CALL => matches!(
            instr,
            Instruction::Call {
                result: Some(_),
                ..
            }
        ),
        _ => false,
    }
}

fn instruction_result_type(instr: &Instruction) -> Type {
    match instr {
        Instruction::BinOp { ty, .. } => ty.clone(),
        Instruction::ICmp { .. } | Instruction::FCmp { .. } => Type::Integer(1),
        Instruction::Call { return_ty, .. } => return_ty.clone().unwrap_or(Type::Void),
        Instruction::Cast { to_ty, .. } => to_ty.clone(),
        Instruction::Alloca { .. } => Type::Ptr,
        Instruction::Load { ty, .. } => ty.clone(),
        Instruction::Phi { ty, .. } => ty.clone(),
        Instruction::Select { ty, .. } => ty.clone(),
        Instruction::GetElementPtr { .. } => Type::Ptr,
        _ => Type::Void,
    }
}

// ---------------------------------------------------------------------------
// BlockReader — block-level traversal using BitstreamReader
// ---------------------------------------------------------------------------

struct BlockReader<'a> {
    reader: BitstreamReader<'a>,
    policy: ReadPolicy,
    diagnostics: RefCell<Vec<ReadDiagnostic>>,
    type_table: Vec<Type>,
    source_filename: Option<String>,
    target_triple: Option<String>,
    target_datalayout: Option<String>,
    global_value_table: Vec<ValueEntry>,
    globals: Vec<GlobalVariable>,
    pending_global_initializers: Vec<PendingGlobalInitializer>,
    func_protos: Vec<FuncProto>,
    functions: Vec<Function>,
    pending_struct_name: Option<String>,
    struct_types: Vec<String>,
    attribute_groups: Vec<AttributeGroup>,
    paramattr_lists: Vec<Vec<u32>>,
    named_metadata: Vec<NamedMetadata>,
    metadata_nodes: Vec<MetadataNode>,
    metadata_slot_map: Vec<MetadataSlotEntry>,
    module_constants: Vec<(Type, Constant)>,
    module_constant_value_offset: u32,
    module_version: u32,
    pending_strtab_names: Vec<PendingStrtabName>,
    string_table: Vec<u8>,
}

impl<'a> BlockReader<'a> {
    fn new(data: &'a [u8], policy: ReadPolicy) -> Result<Self, ParseError> {
        if data.len() < 4 {
            return Err(ParseError::malformed(
                0,
                "bitcode header",
                "data too short for magic bytes",
            ));
        }
        if data[0] != 0x42 || data[1] != 0x43 || data[2] != 0xC0 || data[3] != 0xDE {
            return Err(ParseError::malformed(
                0,
                "bitcode header",
                "invalid bitcode magic bytes",
            ));
        }
        let mut reader = BitstreamReader::new(data);
        reader.read_bits(32);
        Ok(Self {
            reader,
            policy,
            diagnostics: RefCell::new(Vec::new()),
            type_table: Vec::new(),
            source_filename: None,
            target_triple: None,
            target_datalayout: None,
            global_value_table: Vec::new(),
            globals: Vec::new(),
            pending_global_initializers: Vec::new(),
            func_protos: Vec::new(),
            functions: Vec::new(),
            pending_struct_name: None,
            struct_types: Vec::new(),
            attribute_groups: Vec::new(),
            paramattr_lists: Vec::new(),
            named_metadata: Vec::new(),
            metadata_nodes: Vec::new(),
            metadata_slot_map: Vec::new(),
            module_constants: Vec::new(),
            module_constant_value_offset: 0,
            module_version: 0,
            pending_strtab_names: Vec::new(),
            string_table: Vec::new(),
        })
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::malformed(self.reader.byte_position(), "bitcode reader", message)
    }

    fn unsupported(&self, context: &'static str, message: impl Into<String>) -> ParseError {
        ParseError::unsupported(self.reader.byte_position(), context, message)
    }

    fn record_compatibility_diagnostic(
        &self,
        kind: ReadDiagnosticKind,
        context: &'static str,
        message: impl Into<String>,
    ) {
        if self.policy == ReadPolicy::Compatibility {
            self.diagnostics.borrow_mut().push(ReadDiagnostic {
                kind,
                offset: Some(self.reader.byte_position()),
                context,
                message: message.into(),
            });
        }
    }

    fn unsupported_or_recover<T>(
        &self,
        context: &'static str,
        strict_message: impl Into<String>,
        compatibility_message: impl Into<String>,
        fallback: T,
    ) -> Result<T, ParseError> {
        let strict_message = strict_message.into();
        let compatibility_message = compatibility_message.into();

        match self.policy {
            ReadPolicy::Compatibility => {
                self.record_compatibility_diagnostic(
                    ReadDiagnosticKind::UnsupportedSemanticConstruct,
                    context,
                    compatibility_message,
                );
                Ok(fallback)
            }
            ReadPolicy::QirSubsetStrict => Err(self.unsupported(context, strict_message)),
        }
    }

    fn resolve_constant_type(
        &self,
        type_id: u32,
        context: &'static str,
        description: &'static str,
        fallback: Type,
    ) -> Result<Type, ParseError> {
        match self.type_table.get(type_id as usize).cloned() {
            Some(ty) => Ok(ty),
            None => self.unsupported_or_recover(
                context,
                format!("{description} references unknown type ID {type_id}"),
                format!(
                    "{description} references unknown type ID {type_id} and was imported as `{fallback}`"
                ),
                fallback,
            ),
        }
    }

    fn remap_operand_names(operand: &mut Operand, name_remap: &FxHashMap<String, String>) {
        match operand {
            Operand::GlobalRef(name) => {
                if let Some(final_name) = name_remap.get(name.as_str()) {
                    name.clone_from(final_name);
                }
            }
            Operand::GetElementPtr { ptr, indices, .. } => {
                if let Some(final_name) = name_remap.get(ptr.as_str()) {
                    ptr.clone_from(final_name);
                }
                for index in indices {
                    Self::remap_operand_names(index, name_remap);
                }
            }
            Operand::LocalRef(_)
            | Operand::TypedLocalRef(_, _)
            | Operand::IntConst(_, _)
            | Operand::FloatConst(_, _)
            | Operand::NullPtr
            | Operand::IntToPtr(_, _) => {}
        }
    }

    fn remap_instruction_names(instr: &mut Instruction, name_remap: &FxHashMap<String, String>) {
        match instr {
            Instruction::Ret(Some(value)) => Self::remap_operand_names(value, name_remap),
            Instruction::Br { cond, .. } => Self::remap_operand_names(cond, name_remap),
            Instruction::BinOp { lhs, rhs, .. }
            | Instruction::ICmp { lhs, rhs, .. }
            | Instruction::FCmp { lhs, rhs, .. } => {
                Self::remap_operand_names(lhs, name_remap);
                Self::remap_operand_names(rhs, name_remap);
            }
            Instruction::Cast { value, .. } => Self::remap_operand_names(value, name_remap),
            Instruction::Call { callee, args, .. } => {
                if let Some(final_name) = name_remap.get(callee.as_str()) {
                    callee.clone_from(final_name);
                }
                for (_, operand) in args {
                    Self::remap_operand_names(operand, name_remap);
                }
            }
            Instruction::Phi { incoming, .. } => {
                for (operand, _) in incoming {
                    Self::remap_operand_names(operand, name_remap);
                }
            }
            Instruction::Load { ptr, .. } => Self::remap_operand_names(ptr, name_remap),
            Instruction::Store { value, ptr, .. } => {
                Self::remap_operand_names(value, name_remap);
                Self::remap_operand_names(ptr, name_remap);
            }
            Instruction::Select {
                cond,
                true_val,
                false_val,
                ..
            } => {
                Self::remap_operand_names(cond, name_remap);
                Self::remap_operand_names(true_val, name_remap);
                Self::remap_operand_names(false_val, name_remap);
            }
            Instruction::Switch { value, .. } => Self::remap_operand_names(value, name_remap),
            Instruction::GetElementPtr { ptr, indices, .. } => {
                Self::remap_operand_names(ptr, name_remap);
                for index in indices {
                    Self::remap_operand_names(index, name_remap);
                }
            }
            Instruction::Ret(None)
            | Instruction::Jump { .. }
            | Instruction::Alloca { .. }
            | Instruction::Unreachable => {}
        }
    }

    fn remap_local_operand_names(operand: &mut Operand, name_remap: &FxHashMap<String, String>) {
        match operand {
            Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => {
                if let Some(final_name) = name_remap.get(name.as_str()) {
                    name.clone_from(final_name);
                }
            }
            Operand::GetElementPtr { indices, .. } => {
                for index in indices {
                    Self::remap_local_operand_names(index, name_remap);
                }
            }
            Operand::IntConst(_, _)
            | Operand::FloatConst(_, _)
            | Operand::NullPtr
            | Operand::IntToPtr(_, _)
            | Operand::GlobalRef(_) => {}
        }
    }

    fn remap_local_instruction_names(
        instr: &mut Instruction,
        name_remap: &FxHashMap<String, String>,
    ) {
        match instr {
            Instruction::Ret(Some(value)) => Self::remap_local_operand_names(value, name_remap),
            Instruction::Br { cond, .. } => Self::remap_local_operand_names(cond, name_remap),
            Instruction::BinOp {
                lhs, rhs, result, ..
            }
            | Instruction::ICmp {
                lhs, rhs, result, ..
            }
            | Instruction::FCmp {
                lhs, rhs, result, ..
            } => {
                Self::remap_local_operand_names(lhs, name_remap);
                Self::remap_local_operand_names(rhs, name_remap);
                if let Some(final_name) = name_remap.get(result.as_str()) {
                    result.clone_from(final_name);
                }
            }
            Instruction::Cast { value, result, .. } => {
                Self::remap_local_operand_names(value, name_remap);
                if let Some(final_name) = name_remap.get(result.as_str()) {
                    result.clone_from(final_name);
                }
            }
            Instruction::Call { args, result, .. } => {
                for (_, operand) in args {
                    Self::remap_local_operand_names(operand, name_remap);
                }
                if let Some(name) = result
                    && let Some(final_name) = name_remap.get(name.as_str())
                {
                    name.clone_from(final_name);
                }
            }
            Instruction::Phi {
                incoming, result, ..
            } => {
                for (operand, _) in incoming {
                    Self::remap_local_operand_names(operand, name_remap);
                }
                if let Some(final_name) = name_remap.get(result.as_str()) {
                    result.clone_from(final_name);
                }
            }
            Instruction::Alloca { result, .. } => {
                if let Some(final_name) = name_remap.get(result.as_str()) {
                    result.clone_from(final_name);
                }
            }
            Instruction::Load { ptr, result, .. } => {
                Self::remap_local_operand_names(ptr, name_remap);
                if let Some(final_name) = name_remap.get(result.as_str()) {
                    result.clone_from(final_name);
                }
            }
            Instruction::Store { value, ptr, .. } => {
                Self::remap_local_operand_names(value, name_remap);
                Self::remap_local_operand_names(ptr, name_remap);
            }
            Instruction::Select {
                cond,
                true_val,
                false_val,
                result,
                ..
            } => {
                Self::remap_local_operand_names(cond, name_remap);
                Self::remap_local_operand_names(true_val, name_remap);
                Self::remap_local_operand_names(false_val, name_remap);
                if let Some(final_name) = name_remap.get(result.as_str()) {
                    result.clone_from(final_name);
                }
            }
            Instruction::Switch { value, .. } => {
                Self::remap_local_operand_names(value, name_remap);
            }
            Instruction::GetElementPtr {
                ptr,
                indices,
                result,
                ..
            } => {
                Self::remap_local_operand_names(ptr, name_remap);
                for index in indices {
                    Self::remap_local_operand_names(index, name_remap);
                }
                if let Some(final_name) = name_remap.get(result.as_str()) {
                    result.clone_from(final_name);
                }
            }
            Instruction::Ret(None) | Instruction::Jump { .. } | Instruction::Unreachable => {}
        }
    }

    fn remap_module_symbol_uses(&mut self, name_remap: &FxHashMap<String, String>) {
        if name_remap.is_empty() {
            return;
        }

        for function in &mut self.functions {
            for block in &mut function.basic_blocks {
                for instruction in &mut block.instructions {
                    Self::remap_instruction_names(instruction, name_remap);
                }
            }
        }
    }

    fn resolve_strtab_name(&self, offset: usize, size: usize) -> Option<String> {
        let end = offset.checked_add(size)?;
        let bytes = self.string_table.get(offset..end)?;
        String::from_utf8(bytes.to_vec()).ok()
    }

    fn apply_module_value_name(
        &mut self,
        value_id: usize,
        name: String,
        name_remap: &mut FxHashMap<String, String>,
    ) {
        if value_id >= self.global_value_table.len() {
            return;
        }

        match self.global_value_table[value_id].clone() {
            ValueEntry::Global(old_name) => {
                let global_idx = self.global_index_from_value_id(value_id);
                if global_idx < self.globals.len() {
                    self.globals[global_idx].name.clone_from(&name);
                }
                self.global_value_table[value_id] = ValueEntry::Global(name.clone());
                if old_name != name {
                    name_remap.insert(old_name, name);
                }
            }
            ValueEntry::Function(old_name) => {
                let func_idx = self.func_index_from_value_id(value_id);
                if func_idx < self.functions.len() {
                    self.functions[func_idx].name.clone_from(&name);
                }
                self.global_value_table[value_id] = ValueEntry::Function(name.clone());
                if old_name != name {
                    name_remap.insert(old_name, name);
                }
            }
            _ => {}
        }
    }

    fn apply_pending_strtab_names(&mut self) {
        if self.pending_strtab_names.is_empty() || self.string_table.is_empty() {
            return;
        }

        let mut name_remap = FxHashMap::default();
        let pending_names = std::mem::take(&mut self.pending_strtab_names);
        for pending in pending_names {
            let Some(name) = self.resolve_strtab_name(pending.offset, pending.size) else {
                continue;
            };

            self.apply_module_value_name(pending.value_id, name, &mut name_remap);
        }

        self.remap_module_symbol_uses(&name_remap);
    }

    fn read_record(&mut self, abbrev_id: u32) -> Result<(u32, Vec<u64>), ParseError> {
        if abbrev_id == 3 {
            Ok(self.reader.read_unabbrev_record())
        } else {
            self.reader
                .read_abbreviated_record(abbrev_id)
                .map_err(|e| self.error(e))
        }
    }

    // -------------------------------------------------------------------
    // Top-level reading
    // -------------------------------------------------------------------

    fn read_top_level(&mut self) -> Result<(), ParseError> {
        let top_abbrev = 2;
        while !self.reader.at_end() {
            let abbrev_id = self.reader.read_abbrev_id(top_abbrev);
            match abbrev_id {
                0 => break,
                1 => {
                    let (block_id, new_abbrev, block_len) = self.reader.enter_subblock();
                    match block_id {
                        BLOCKINFO_BLOCK_ID => self.read_blockinfo_block(new_abbrev)?,
                        IDENTIFICATION_BLOCK_ID => self.reader.skip_block(block_len),
                        MODULE_BLOCK_ID => self.read_module_block(new_abbrev)?,
                        STRTAB_BLOCK_ID => self.read_strtab_block(new_abbrev)?,
                        _ => self.reader.skip_block(block_len),
                    }
                }
                _ => {
                    return Err(self.error(format!("unexpected top-level abbrev id {abbrev_id}")));
                }
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Module block
    // -------------------------------------------------------------------

    fn read_module_block(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(MODULE_BLOCK_ID);
        let mut func_body_index: usize = 0;

        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (block_id, new_abbrev, block_len) = self.reader.enter_subblock();
                    match block_id {
                        BLOCKINFO_BLOCK_ID => self.read_blockinfo_block(new_abbrev)?,
                        TYPE_BLOCK_ID_NEW => self.read_type_block(new_abbrev)?,
                        FUNCTION_BLOCK_ID => {
                            while func_body_index < self.func_protos.len()
                                && self.func_protos[func_body_index].is_declaration
                            {
                                func_body_index += 1;
                            }
                            if func_body_index < self.func_protos.len() {
                                self.read_function_block(new_abbrev, func_body_index)?;
                                func_body_index += 1;
                            } else {
                                self.reader.skip_block(block_len);
                            }
                        }
                        VALUE_SYMTAB_BLOCK_ID => self.read_module_vst(new_abbrev)?,
                        CONSTANTS_BLOCK_ID => {
                            self.read_module_constants(new_abbrev)?;
                        }
                        PARAMATTR_BLOCK_ID => self.read_paramattr_block(new_abbrev)?,
                        PARAMATTR_GROUP_BLOCK_ID => {
                            self.read_paramattr_group_block(new_abbrev)?;
                        }
                        METADATA_BLOCK_ID => self.read_metadata_block(new_abbrev)?,
                        _ => self.reader.skip_block(block_len),
                    }
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    self.handle_module_record(code, &values)?;
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    fn handle_module_record(&mut self, code: u32, values: &[u64]) -> Result<(), ParseError> {
        match code {
            MODULE_CODE_VERSION => {
                self.module_version = values.first().copied().unwrap_or(0) as u32;
            }
            MODULE_CODE_TRIPLE => {
                let s = parse_char_string
                    .parse(values)
                    .map_err(|_| self.error("failed to parse triple"))?;
                self.target_triple = Some(s);
            }
            MODULE_CODE_DATALAYOUT => {
                let s = parse_char_string
                    .parse(values)
                    .map_err(|_| self.error("failed to parse datalayout"))?;
                self.target_datalayout = Some(s);
            }
            MODULE_CODE_GLOBALVAR => {
                let mut input: RecordInput<'_> = values;
                let parsed_record = parse_global_var_record(&mut input)
                    .map_err(|_| self.error("global var record too short"))?;
                let ty = match parsed_record.elem_type_id {
                    Some(type_id) => match self.type_table.get(type_id as usize).cloned() {
                        Some(ty) => ty,
                        None if self.policy == ReadPolicy::Compatibility => {
                            self.record_compatibility_diagnostic(
                                ReadDiagnosticKind::UnsupportedSemanticConstruct,
                                "global variable",
                                format!(
                                    "global variable references unknown element type ID {type_id} and was imported as `ptr`"
                                ),
                            );
                            Type::Ptr
                        }
                        None => {
                            return Err(self.unsupported(
                                "global variable",
                                format!(
                                    "global variable references unknown element type ID {type_id}"
                                ),
                            ));
                        }
                    },
                    None => Type::Ptr,
                };
                let initializer = if parsed_record.legacy_placeholder {
                    Some(self.unsupported_or_recover(
                        "global variable",
                        "legacy placeholder global initializer encoding is not supported",
                        "legacy placeholder global initializer encoding was imported as null",
                        Constant::Null,
                    )?)
                } else {
                    None
                };
                let global = GlobalVariable {
                    name: String::new(),
                    ty,
                    linkage: parsed_record.linkage,
                    is_constant: parsed_record.is_const,
                    initializer,
                };
                let idx = self.globals.len();
                self.globals.push(global);
                if let Some(value_id) = parsed_record.init_value_id {
                    self.pending_global_initializers
                        .push(PendingGlobalInitializer {
                            global_index: idx,
                            value_id,
                        });
                }
                self.global_value_table
                    .push(ValueEntry::Global(format!("__global_{idx}")));
            }
            MODULE_CODE_VSTOFFSET => {}
            MODULE_CODE_SOURCE_FILENAME => {
                let s = parse_char_string
                    .parse(values)
                    .map_err(|_| self.error("failed to parse source_filename"))?;
                self.source_filename = Some(s);
            }
            MODULE_CODE_FUNCTION => {
                let legacy_type_is_function = values
                    .first()
                    .and_then(|value| self.type_table.get(*value as usize))
                    .is_some_and(|ty| matches!(ty, Type::Function(_, _)));
                let modern_type_is_function = values
                    .get(2)
                    .and_then(|value| self.type_table.get(*value as usize))
                    .is_some_and(|ty| matches!(ty, Type::Function(_, _)));
                let use_modern_v2_layout =
                    self.module_version >= 2 && modern_type_is_function && !legacy_type_is_function;

                let (func_type_id, is_declaration, paramattr, pending_name) =
                    if use_modern_v2_layout {
                        let func_type_id = values.get(2).copied().unwrap_or(0) as u32;
                        let is_declaration = values.get(4).copied().unwrap_or(0) != 0;
                        let paramattr = values.get(6).copied().unwrap_or(0) as u32;
                        let pending_name = Some((
                            values.first().copied().unwrap_or(0) as usize,
                            values.get(1).copied().unwrap_or(0) as usize,
                        ));
                        (func_type_id, is_declaration, paramattr, pending_name)
                    } else {
                        let mut input: RecordInput<'_> = values;
                        let (func_type_id, is_declaration, paramattr) =
                            parse_function_record(&mut input)
                                .map_err(|_| self.error("function record too short"))?;
                        (func_type_id, is_declaration, paramattr, None)
                    };

                let proto = FuncProto {
                    func_type_id,
                    is_declaration,
                    paramattr_index: paramattr,
                };

                let (return_type, param_types) = match self.type_table.get(func_type_id as usize) {
                    Some(Type::Function(ret, params)) => (ret.as_ref().clone(), params.clone()),
                    _ if self.policy == ReadPolicy::Compatibility => {
                        self.record_compatibility_diagnostic(
                            ReadDiagnosticKind::UnsupportedSemanticConstruct,
                            "function declaration",
                            format!(
                                "function record references non-function type ID {func_type_id} and was imported as `void ()`"
                            ),
                        );
                        (Type::Void, Vec::new())
                    }
                    _ => {
                        return Err(self.unsupported(
                            "function declaration",
                            format!(
                                "function record references non-function type ID {func_type_id}"
                            ),
                        ));
                    }
                };

                // Resolve attribute group refs from paramattr index (1-based; 0 = no attrs)
                let attribute_group_refs = if paramattr > 0 {
                    self.paramattr_lists
                        .get((paramattr - 1) as usize)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };

                let func = Function {
                    name: String::new(),
                    return_type,
                    params: param_types
                        .into_iter()
                        .enumerate()
                        .map(|(index, ty)| Param {
                            ty,
                            name: Some(format!("param_{index}")),
                        })
                        .collect(),
                    is_declaration,
                    attribute_group_refs,
                    basic_blocks: Vec::new(),
                };

                let func_idx = self.functions.len();
                let value_id = self.global_value_table.len();
                self.functions.push(func);
                self.func_protos.push(proto);
                self.global_value_table
                    .push(ValueEntry::Function(format!("__func_{func_idx}")));
                if let Some((offset, size)) = pending_name {
                    self.pending_strtab_names.push(PendingStrtabName {
                        value_id,
                        offset,
                        size,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Type block
    // -------------------------------------------------------------------

    fn read_type_block(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(TYPE_BLOCK_ID_NEW);
        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    self.handle_type_record(code, &values)?;
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    fn handle_type_record(&mut self, code: u32, values: &[u64]) -> Result<(), ParseError> {
        let mut input: RecordInput<'_> = values;
        match code {
            TYPE_CODE_NUMENTRY => {
                if let Some(&count) = values.first() {
                    self.type_table.reserve(count as usize);
                }
            }
            TYPE_CODE_VOID => self.type_table.push(Type::Void),
            TYPE_CODE_HALF => self.type_table.push(Type::Half),
            TYPE_CODE_FLOAT => self.type_table.push(Type::Float),
            TYPE_CODE_DOUBLE => self.type_table.push(Type::Double),
            TYPE_CODE_LABEL => self.type_table.push(Type::Label),
            TYPE_CODE_INTEGER => {
                let ty = parse_type_integer(&mut input)
                    .map_err(|_| self.error("INTEGER type record malformed"))?;
                self.type_table.push(ty);
            }
            TYPE_CODE_OPAQUE_POINTER => self.type_table.push(Type::Ptr),
            TYPE_CODE_POINTER => {
                let Some(inner_id) = values.first().copied() else {
                    self.type_table.push(Type::Ptr);
                    return Ok(());
                };
                let inner_id = inner_id as u32;
                let inner = match self.type_table.get(inner_id as usize).cloned() {
                    Some(ty) => ty,
                    None if self.policy == ReadPolicy::Compatibility => {
                        self.record_compatibility_diagnostic(
                            ReadDiagnosticKind::UnsupportedSemanticConstruct,
                            "type record",
                            format!(
                                "pointer type references unknown element type ID {inner_id} and was imported as `void`"
                            ),
                        );
                        Type::Void
                    }
                    None => {
                        return Err(self.unsupported(
                            "type record",
                            format!("pointer type references unknown element type ID {inner_id}"),
                        ));
                    }
                };
                let ty = match inner {
                    Type::Named(name) => Type::NamedPtr(name),
                    other => Type::TypedPtr(Box::new(other)),
                };
                self.type_table.push(ty);
            }
            TYPE_CODE_STRUCT_NAME => {
                let name = parse_char_string(&mut input)
                    .map_err(|_| self.error("STRUCT_NAME record malformed"))?;
                self.pending_struct_name = Some(name);
            }
            TYPE_CODE_OPAQUE => {
                let name = self
                    .pending_struct_name
                    .take()
                    .unwrap_or_else(|| "unknown".to_string());
                self.struct_types.push(name.clone());
                self.type_table.push(Type::Named(name));
            }
            TYPE_CODE_ARRAY => {
                if values.len() < 2 {
                    return Err(self.error("ARRAY type record too short"));
                }
                let len = values[0];
                let elem_id = values[1] as u32;
                let elem = match self.type_table.get(elem_id as usize).cloned() {
                    Some(ty) => ty,
                    None if self.policy == ReadPolicy::Compatibility => {
                        self.record_compatibility_diagnostic(
                            ReadDiagnosticKind::UnsupportedSemanticConstruct,
                            "type record",
                            format!(
                                "array type references unknown element type ID {elem_id} and was imported as `void`"
                            ),
                        );
                        Type::Void
                    }
                    None => {
                        return Err(self.unsupported(
                            "type record",
                            format!("array type references unknown element type ID {elem_id}"),
                        ));
                    }
                };
                let ty = Type::Array(len, Box::new(elem));
                self.type_table.push(ty);
            }
            TYPE_CODE_FUNCTION_TYPE => {
                if values.len() < 2 {
                    return Err(self.error("FUNCTION type record too short"));
                }
                let ret_id = values[1] as u32;
                let ret = match self.type_table.get(ret_id as usize).cloned() {
                    Some(ty) => ty,
                    None if self.policy == ReadPolicy::Compatibility => {
                        self.record_compatibility_diagnostic(
                            ReadDiagnosticKind::UnsupportedSemanticConstruct,
                            "type record",
                            format!(
                                "function type references unknown return type ID {ret_id} and was imported as `void`"
                            ),
                        );
                        Type::Void
                    }
                    None => {
                        return Err(self.unsupported(
                            "type record",
                            format!("function type references unknown return type ID {ret_id}"),
                        ));
                    }
                };
                let mut param_types = Vec::with_capacity(values.len().saturating_sub(2));
                for &param_id in &values[2..] {
                    let param_id = param_id as u32;
                    let param_ty = match self.type_table.get(param_id as usize).cloned() {
                        Some(ty) => ty,
                        None if self.policy == ReadPolicy::Compatibility => {
                            self.record_compatibility_diagnostic(
                                ReadDiagnosticKind::UnsupportedSemanticConstruct,
                                "type record",
                                format!(
                                    "function type references unknown parameter type ID {param_id} and was imported as `void`"
                                ),
                            );
                            Type::Void
                        }
                        None => {
                            return Err(self.unsupported(
                                "type record",
                                format!(
                                    "function type references unknown parameter type ID {param_id}"
                                ),
                            ));
                        }
                    };
                    param_types.push(param_ty);
                }
                let ty = Type::Function(Box::new(ret), param_types);
                self.type_table.push(ty);
            }
            _ if self.policy == ReadPolicy::Compatibility => {
                self.record_compatibility_diagnostic(
                    ReadDiagnosticKind::UnsupportedSemanticConstruct,
                    "type record",
                    format!("unsupported type record code {code} was imported as `void`"),
                );
                self.type_table.push(Type::Void);
            }
            _ => {
                return Err(self.unsupported(
                    "type record",
                    format!("unsupported type record code {code}"),
                ));
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Function block
    // -------------------------------------------------------------------

    fn read_function_block(
        &mut self,
        abbrev_width: u32,
        func_index: usize,
    ) -> Result<(), ParseError> {
        self.reader.push_block_scope(FUNCTION_BLOCK_ID);
        let mut local_values: Vec<ValueEntry> = Vec::new();

        let func = &self.functions[func_index];
        for p in &func.params {
            let name = p
                .name
                .clone()
                .unwrap_or_else(|| format!("param_{}", local_values.len()));
            local_values.push(ValueEntry::Param(name, p.ty.clone()));
        }

        let mut num_bbs: usize = 0;
        let mut current_bb: usize = 0;
        let mut basic_blocks: Vec<BasicBlock> = Vec::new();
        let mut current_instructions: Vec<Instruction> = Vec::new();
        let mut bb_names: FxHashMap<u32, String> = FxHashMap::default();
        let mut local_name_entries: FxHashMap<u32, String> = FxHashMap::default();
        let mut next_result_id: u32 =
            self.global_value_table.len() as u32 + local_values.len() as u32;

        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    if !current_instructions.is_empty() || current_bb < num_bbs {
                        let bb_name = bb_names
                            .get(&(current_bb as u32))
                            .cloned()
                            .unwrap_or_else(|| format!("bb_{current_bb}"));
                        basic_blocks.push(BasicBlock {
                            name: bb_name,
                            instructions: std::mem::take(&mut current_instructions),
                        });
                    }
                    break;
                }
                1 => {
                    let (block_id, new_abbrev, block_len) = self.reader.enter_subblock();
                    match block_id {
                        CONSTANTS_BLOCK_ID => {
                            self.read_function_constants(
                                new_abbrev,
                                &mut local_values,
                                &mut next_result_id,
                            )?;
                        }
                        VALUE_SYMTAB_BLOCK_ID => {
                            self.read_function_vst(
                                new_abbrev,
                                &mut bb_names,
                                &mut local_name_entries,
                            )?;
                        }
                        _ => self.reader.skip_block(block_len),
                    }
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    if code == FUNC_CODE_DECLAREBLOCKS {
                        num_bbs = values.first().copied().unwrap_or(1) as usize;
                    } else {
                        let is_terminator = matches!(
                            code,
                            FUNC_CODE_INST_RET
                                | FUNC_CODE_INST_BR
                                | FUNC_CODE_INST_SWITCH
                                | FUNC_CODE_INST_UNREACHABLE
                        );
                        let byte_offset = self.reader.byte_position();

                        let ctx = InstrContext {
                            global_value_table: &self.global_value_table,
                            local_values: &local_values,
                            type_table: &self.type_table,
                            paramattr_lists: &self.paramattr_lists,
                            bb_names: &bb_names,
                            diagnostics: &self.diagnostics,
                            current_value_id: next_result_id,
                            byte_offset,
                            policy: self.policy,
                        };

                        if let Some(instr) = dispatch_instruction(code, &ctx, &values, byte_offset)?
                        {
                            let produces_value = instruction_produces_value(code, &values, &instr);

                            if produces_value {
                                let result_ty = instruction_result_type(&instr);
                                local_values.push(ValueEntry::Local(
                                    format!("val_{next_result_id}"),
                                    result_ty,
                                ));
                                next_result_id += 1;
                            }

                            current_instructions.push(instr);
                        }

                        if is_terminator && current_bb < num_bbs {
                            let bb_name = bb_names
                                .get(&(current_bb as u32))
                                .cloned()
                                .unwrap_or_else(|| format!("bb_{current_bb}"));
                            basic_blocks.push(BasicBlock {
                                name: bb_name,
                                instructions: std::mem::take(&mut current_instructions),
                            });
                            current_bb += 1;
                        }
                    }
                }
            }
        }

        let mut local_name_remap = FxHashMap::default();
        let global_value_count = self.global_value_table.len() as u32;
        let uses_absolute_local_ids = local_name_entries
            .keys()
            .any(|value_id| *value_id >= local_values.len() as u32);
        for (value_id, name) in local_name_entries {
            let local_id = if uses_absolute_local_ids {
                let Some(local_id) = value_id.checked_sub(global_value_count) else {
                    continue;
                };
                local_id
            } else {
                value_id
            };
            let Some(entry) = local_values.get(local_id as usize) else {
                continue;
            };

            match entry {
                ValueEntry::Local(old_name, _) | ValueEntry::Param(old_name, _) => {
                    if old_name != &name {
                        local_name_remap.insert(old_name.clone(), name);
                    }
                }
                _ => {}
            }
        }

        for (i, bb) in basic_blocks.iter_mut().enumerate() {
            if let Some(name) = bb_names.get(&(i as u32)) {
                bb.name.clone_from(name);
            }

            for instruction in &mut bb.instructions {
                Self::remap_local_instruction_names(instruction, &local_name_remap);
                remap_instruction_block_names(instruction, &bb_names);
            }
        }

        let func = &mut self.functions[func_index];
        for param in &mut func.params {
            if let Some(name) = &mut param.name
                && let Some(final_name) = local_name_remap.get(name.as_str())
            {
                name.clone_from(final_name);
            }
        }
        func.basic_blocks = basic_blocks;
        self.reader.pop_block_scope();
        Ok(())
    }

    // -------------------------------------------------------------------
    // Attribute blocks
    // -------------------------------------------------------------------

    const PARAMATTR_GRP_CODE_ENTRY: u32 = 3;
    const PARAMATTR_CODE_ENTRY: u32 = 2;

    fn read_paramattr_group_block(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(PARAMATTR_GROUP_BLOCK_ID);
        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    if code == Self::PARAMATTR_GRP_CODE_ENTRY && values.len() >= 2 {
                        let group_id = values[0] as u32;
                        // values[1] = param_index (0xFFFFFFFF for function attrs); not stored
                        let attributes = Self::parse_attr_encodings(
                            &values[2..],
                            self.policy,
                            self.reader.byte_position(),
                            &self.diagnostics,
                        )?;
                        self.attribute_groups.push(AttributeGroup {
                            id: group_id,
                            attributes,
                        });
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    fn parse_attr_encodings(
        values: &[u64],
        policy: ReadPolicy,
        byte_offset: usize,
        diagnostics: &RefCell<Vec<ReadDiagnostic>>,
    ) -> Result<Vec<Attribute>, ParseError> {
        let mut attrs = Vec::new();
        let mut i = 0;
        while i < values.len() {
            let kind = values[i];
            i += 1;
            match kind {
                // Code 3 = string attribute: null-terminated string
                3 => {
                    let mut s = String::new();
                    while i < values.len() && values[i] != 0 {
                        s.push(values[i] as u8 as char);
                        i += 1;
                    }
                    i += 1; // skip null terminator
                    attrs.push(Attribute::StringAttr(s));
                }
                // Code 4 = string key/value: key (null-terminated), value (null-terminated)
                4 => {
                    let mut key = String::new();
                    while i < values.len() && values[i] != 0 {
                        key.push(values[i] as u8 as char);
                        i += 1;
                    }
                    i += 1; // skip null terminator
                    let mut val = String::new();
                    while i < values.len() && values[i] != 0 {
                        val.push(values[i] as u8 as char);
                        i += 1;
                    }
                    i += 1; // skip null terminator
                    attrs.push(Attribute::KeyValue(key, val));
                }
                // Skip enum (0/1) and other attribute kinds for v1
                _ if policy == ReadPolicy::Compatibility => {
                    diagnostics.borrow_mut().push(ReadDiagnostic {
                        kind: ReadDiagnosticKind::UnsupportedSemanticConstruct,
                        offset: Some(byte_offset),
                        context: "attribute group",
                        message: format!(
                            "attribute group contains unsupported encoded attribute kind {kind}; remaining attributes were skipped"
                        ),
                    });
                    break;
                }
                _ => {
                    return Err(ParseError::unsupported(
                        byte_offset,
                        "attribute group",
                        format!(
                            "attribute group contains unsupported encoded attribute kind {kind}"
                        ),
                    ));
                }
            }
        }
        Ok(attrs)
    }

    fn read_paramattr_block(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(PARAMATTR_BLOCK_ID);
        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    if code == Self::PARAMATTR_CODE_ENTRY {
                        self.paramattr_lists
                            .push(values.iter().map(|&v| v as u32).collect());
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    // -------------------------------------------------------------------
    // Metadata block
    // -------------------------------------------------------------------

    fn read_metadata_block(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(METADATA_BLOCK_ID);
        let mut pending_named_metadata_name: Option<String> = None;
        let mut pending_metadata_nodes: Vec<Vec<usize>> = Vec::new();
        let mut pending_named_metadata_nodes: Vec<(String, Vec<usize>)> = Vec::new();

        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    match code {
                        METADATA_STRING_OLD => {
                            let s: String = values.iter().map(|&v| v as u8 as char).collect();
                            self.metadata_slot_map.push(MetadataSlotEntry::String(s));
                        }
                        METADATA_VALUE => {
                            if values.len() < 2 {
                                return Err(self.error("METADATA_VALUE record too short"));
                            }
                            let type_id = values[0] as usize;
                            let value_id = values[1] as usize;
                            let ty = match self.type_table.get(type_id).cloned() {
                                Some(ty) => ty,
                                None if self.policy == ReadPolicy::Compatibility => {
                                    self.record_compatibility_diagnostic(
                                        ReadDiagnosticKind::UnsupportedSemanticConstruct,
                                        "metadata",
                                        format!(
                                            "metadata value references unknown type ID {type_id} and was imported as `void`"
                                        ),
                                    );
                                    Type::Void
                                }
                                None => {
                                    return Err(self.unsupported(
                                        "metadata",
                                        format!(
                                            "metadata value references unknown type ID {type_id}"
                                        ),
                                    ));
                                }
                            };
                            // Look up the value from global_value_table
                            // (module constants are stored there)
                            let val = match self.global_value_table.get(value_id) {
                                Some(ValueEntry::Constant(_, Constant::Int(v))) => *v,
                                _ if self.policy == ReadPolicy::Compatibility => {
                                    self.record_compatibility_diagnostic(
                                        ReadDiagnosticKind::UnsupportedSemanticConstruct,
                                        "metadata",
                                        format!(
                                            "metadata value {value_id} is not an integer constant and was normalized to `0` during import"
                                        ),
                                    );
                                    0
                                }
                                _ => {
                                    return Err(self.unsupported(
                                        "metadata",
                                        format!(
                                            "metadata value {value_id} is not an integer constant and would be normalized during import"
                                        ),
                                    ));
                                }
                            };
                            self.metadata_slot_map
                                .push(MetadataSlotEntry::Value(ty, val));
                        }
                        METADATA_NODE => {
                            let node_id = self.metadata_nodes.len() as u32;
                            pending_metadata_nodes.push(
                                values
                                    .iter()
                                    .map(|&operand_ref| operand_ref as usize)
                                    .collect(),
                            );
                            self.metadata_nodes.push(MetadataNode {
                                id: node_id,
                                values: Vec::new(),
                            });
                            self.metadata_slot_map
                                .push(MetadataSlotEntry::Node(node_id));
                        }
                        METADATA_NAME => {
                            let s: String = values.iter().map(|&v| v as u8 as char).collect();
                            pending_named_metadata_name = Some(s);
                        }
                        METADATA_NAMED_NODE => {
                            let name = pending_named_metadata_name.take().unwrap_or_default();
                            pending_named_metadata_nodes.push((
                                name,
                                values.iter().map(|&slot_ref| slot_ref as usize).collect(),
                            ));
                        }
                        _ => {} // skip other metadata codes
                    }
                }
            }
        }
        self.reader.pop_block_scope();

        for (node, operand_slots) in self.metadata_nodes.iter_mut().zip(pending_metadata_nodes) {
            let mut node_values = Vec::new();
            for slot_idx in operand_slots {
                match self.metadata_slot_map.get(slot_idx) {
                    Some(MetadataSlotEntry::String(s)) => {
                        node_values.push(MetadataValue::String(s.clone()));
                    }
                    Some(MetadataSlotEntry::Value(ty, val)) => {
                        node_values.push(MetadataValue::Int(ty.clone(), *val));
                    }
                    Some(MetadataSlotEntry::Node(child_id)) => {
                        node_values.push(MetadataValue::NodeRef(*child_id));
                    }
                    None => {}
                }
            }
            node.values = node_values;
        }

        for (name, slot_refs) in pending_named_metadata_nodes {
            let mut node_refs = Vec::new();
            for slot_idx in slot_refs {
                if let Some(MetadataSlotEntry::Node(node_id)) = self.metadata_slot_map.get(slot_idx)
                {
                    node_refs.push(*node_id);
                }
            }
            self.named_metadata.push(NamedMetadata { name, node_refs });
        }

        // Post-process: reconstruct SubList values from synthetic child nodes
        self.reconstruct_sublists();

        Ok(())
    }

    /// Reconstructs `MetadataValue::SubList` from synthetic child nodes.
    ///
    /// Nodes that are referenced only from other nodes' operands (not from
    /// named_metadata directly) are "synthetic child" nodes. We replace
    /// their parent's `NodeRef` with `SubList` containing the child's values,
    /// then remove the synthetic children and re-number remaining nodes.
    fn reconstruct_sublists(&mut self) {
        use rustc_hash::FxHashSet;

        fn expand_synthetic_value(
            value: &MetadataValue,
            synthetic_ids: &FxHashSet<u32>,
            child_values: &FxHashMap<u32, Vec<MetadataValue>>,
        ) -> MetadataValue {
            match value {
                MetadataValue::NodeRef(child_id) if synthetic_ids.contains(child_id) => {
                    let Some(children) = child_values.get(child_id) else {
                        return MetadataValue::NodeRef(*child_id);
                    };

                    MetadataValue::SubList(
                        children
                            .iter()
                            .map(|child| expand_synthetic_value(child, synthetic_ids, child_values))
                            .collect(),
                    )
                }
                MetadataValue::SubList(children) => MetadataValue::SubList(
                    children
                        .iter()
                        .map(|child| expand_synthetic_value(child, synthetic_ids, child_values))
                        .collect(),
                ),
                MetadataValue::Int(ty, value) => MetadataValue::Int(ty.clone(), *value),
                MetadataValue::String(text) => MetadataValue::String(text.clone()),
                MetadataValue::NodeRef(node_id) => MetadataValue::NodeRef(*node_id),
            }
        }

        fn remap_node_refs(values: &mut [MetadataValue], id_remap: &FxHashMap<u32, u32>) {
            for value in values {
                match value {
                    MetadataValue::NodeRef(node_id) => {
                        if let Some(remapped) = id_remap.get(node_id) {
                            *node_id = *remapped;
                        }
                    }
                    MetadataValue::SubList(children) => remap_node_refs(children, id_remap),
                    MetadataValue::Int(_, _) | MetadataValue::String(_) => {}
                }
            }
        }

        // Collect node IDs referenced directly by named_metadata
        let directly_referenced: FxHashSet<u32> = self
            .named_metadata
            .iter()
            .flat_map(|nm| nm.node_refs.iter().copied())
            .collect();

        // Collect node IDs referenced from other nodes' operands
        let mut node_ref_parents: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
        for node in &self.metadata_nodes {
            for val in &node.values {
                if let MetadataValue::NodeRef(child_id) = val {
                    node_ref_parents.entry(*child_id).or_default().push(node.id);
                }
            }
        }

        // Identify synthetic child nodes: referenced from other nodes only,
        // not from named_metadata
        let synthetic_ids: FxHashSet<u32> = node_ref_parents
            .keys()
            .copied()
            .filter(|id| !directly_referenced.contains(id))
            .collect();

        if synthetic_ids.is_empty() {
            return;
        }

        // Build a map from node ID to its values for synthetic children
        let child_values: FxHashMap<u32, Vec<MetadataValue>> = self
            .metadata_nodes
            .iter()
            .filter(|n| synthetic_ids.contains(&n.id))
            .map(|n| (n.id, n.values.clone()))
            .collect();

        // Replace synthetic NodeRef operands with recursively reconstructed SubList values.
        for node in &mut self.metadata_nodes {
            node.values = node
                .values
                .iter()
                .map(|value| expand_synthetic_value(value, &synthetic_ids, &child_values))
                .collect();
        }

        // Remove synthetic child nodes
        self.metadata_nodes
            .retain(|n| !synthetic_ids.contains(&n.id));

        // Re-number remaining node IDs sequentially
        let old_ids: Vec<u32> = self.metadata_nodes.iter().map(|n| n.id).collect();
        let id_remap: FxHashMap<u32, u32> = old_ids
            .iter()
            .enumerate()
            .map(|(new_idx, &old_id)| (old_id, new_idx as u32))
            .collect();

        for node in &mut self.metadata_nodes {
            node.id = id_remap[&node.id];
            remap_node_refs(&mut node.values, &id_remap);
        }

        // Update named_metadata node_refs
        for nm in &mut self.named_metadata {
            nm.node_refs = nm
                .node_refs
                .iter()
                .filter_map(|old_id| id_remap.get(old_id).copied())
                .collect();
        }
    }

    // -------------------------------------------------------------------
    // Constant expression helpers
    // -------------------------------------------------------------------

    /// Resolve an absolute value ID to a global name from the global value table.
    fn resolve_global_name_by_id(
        &self,
        value_id: u32,
        context: &'static str,
        description: &'static str,
    ) -> Result<String, ParseError> {
        match self.global_value_table.get(value_id as usize) {
            Some(ValueEntry::Global(name) | ValueEntry::Function(name)) => Ok(name.clone()),
            _ => self.unsupported_or_recover(
                context,
                format!("{description} value ID {value_id} does not resolve to a global value"),
                format!(
                    "{description} value ID {value_id} does not resolve to a global value and was imported as `unknown_{value_id}`"
                ),
                format!("unknown_{value_id}"),
            ),
        }
    }

    /// Resolve an absolute value ID to an integer constant from the global table.
    fn resolve_constant_int_from_global_table(
        &self,
        value_id: u32,
        context: &'static str,
        description: &'static str,
    ) -> Result<i64, ParseError> {
        match self.global_value_table.get(value_id as usize) {
            Some(ValueEntry::Constant(_, Constant::Int(val))) => Ok(*val),
            _ => self.unsupported_or_recover(
                context,
                format!("{description} value ID {value_id} is not an integer constant"),
                format!(
                    "{description} value ID {value_id} is not an integer constant and was normalized to `0` during import"
                ),
                0,
            ),
        }
    }

    /// Resolve an absolute value ID to an integer constant, checking both
    /// global and function-local value tables.
    fn resolve_constant_int_from_tables(
        &self,
        value_id: u32,
        local_values: &[ValueEntry],
        context: &'static str,
        description: &'static str,
    ) -> Result<i64, ParseError> {
        let global_count = self.global_value_table.len() as u32;
        if value_id < global_count {
            self.resolve_constant_int_from_global_table(value_id, context, description)
        } else {
            let local_idx = (value_id - global_count) as usize;
            if let Some(ValueEntry::Constant(_, Constant::Int(val))) = local_values.get(local_idx) {
                Ok(*val)
            } else {
                self.unsupported_or_recover(
                    context,
                    format!("{description} value ID {value_id} is not an integer constant"),
                    format!(
                        "{description} value ID {value_id} is not an integer constant and was normalized to `0` during import"
                    ),
                    0,
                )
            }
        }
    }

    /// Parse a GEP CE record and push the result into the global value table.
    fn parse_gep_ce_into_global_table(&mut self, values: &[u64]) -> Result<(), ParseError> {
        let mut op_idx = 0;
        // If odd number of values, first element is the pointee type ID
        let source_type_id = if values.len() % 2 == 1 {
            let id = values[op_idx] as u32;
            op_idx += 1;
            id
        } else {
            0
        };
        let source_ty = self.resolve_constant_type(
            source_type_id,
            "constant expression",
            "getelementptr constant source type",
            Type::Void,
        )?;

        // First pair: pointer type + pointer value
        if op_idx + 1 >= values.len() {
            return Err(self.error("getelementptr constant expression record too short"));
        }
        let ptr_type_id = values[op_idx] as u32;
        let ptr_value_id = values[op_idx + 1] as u32;
        op_idx += 2;

        let ptr_ty = self.resolve_constant_type(
            ptr_type_id,
            "constant expression",
            "getelementptr constant pointer type",
            Type::Ptr,
        )?;
        let ptr_name = self.resolve_global_name_by_id(
            ptr_value_id,
            "constant expression",
            "getelementptr constant base pointer",
        )?;

        // Remaining pairs: index type + index value
        let mut indices = Vec::new();
        while op_idx + 1 < values.len() {
            let idx_type_id = values[op_idx] as u32;
            let idx_value_id = values[op_idx + 1] as u32;
            let idx_ty = self.resolve_constant_type(
                idx_type_id,
                "constant expression",
                "getelementptr constant index type",
                Type::Integer(64),
            )?;
            let idx_val = self.resolve_constant_int_from_global_table(
                idx_value_id,
                "constant expression",
                "getelementptr constant index",
            )?;
            indices.push(Operand::IntConst(idx_ty, idx_val));
            op_idx += 2;
        }

        self.global_value_table.push(ValueEntry::GepConst {
            source_ty,
            ptr_name,
            ptr_ty,
            indices,
        });

        Ok(())
    }

    /// Parse a GEP CE record and push the result into function-local values.
    fn parse_gep_ce_into_local_values(
        &self,
        values: &[u64],
        local_values: &mut Vec<ValueEntry>,
    ) -> Result<(), ParseError> {
        let mut op_idx = 0;
        // If odd number of values, first element is the pointee type ID
        let source_type_id = if values.len() % 2 == 1 {
            let id = values[op_idx] as u32;
            op_idx += 1;
            id
        } else {
            0
        };
        let source_ty = self.resolve_constant_type(
            source_type_id,
            "constant expression",
            "getelementptr constant source type",
            Type::Void,
        )?;

        // First pair: pointer type + pointer value
        if op_idx + 1 >= values.len() {
            return Err(self.error("getelementptr constant expression record too short"));
        }
        let ptr_type_id = values[op_idx] as u32;
        let ptr_value_id = values[op_idx + 1] as u32;
        op_idx += 2;

        let ptr_ty = self.resolve_constant_type(
            ptr_type_id,
            "constant expression",
            "getelementptr constant pointer type",
            Type::Ptr,
        )?;
        let global_count = self.global_value_table.len() as u32;
        let ptr_name = if ptr_value_id < global_count {
            self.resolve_global_name_by_id(
                ptr_value_id,
                "constant expression",
                "getelementptr constant base pointer",
            )?
        } else {
            let local_idx = (ptr_value_id - global_count) as usize;
            match local_values.get(local_idx) {
                Some(ValueEntry::Global(name) | ValueEntry::Function(name)) => name.clone(),
                _ => {
                    self.unsupported_or_recover(
                        "constant expression",
                        format!(
                            "getelementptr constant base pointer value ID {ptr_value_id} does not resolve to a global value"
                        ),
                        format!(
                            "getelementptr constant base pointer value ID {ptr_value_id} does not resolve to a global value and was imported as `unknown_{ptr_value_id}`"
                        ),
                        format!("unknown_{ptr_value_id}"),
                    )?
                }
            }
        };

        // Remaining pairs: index type + index value
        let mut indices = Vec::new();
        while op_idx + 1 < values.len() {
            let idx_type_id = values[op_idx] as u32;
            let idx_value_id = values[op_idx + 1] as u32;
            let idx_ty = self.resolve_constant_type(
                idx_type_id,
                "constant expression",
                "getelementptr constant index type",
                Type::Integer(64),
            )?;
            let idx_val = self.resolve_constant_int_from_tables(
                idx_value_id,
                local_values,
                "constant expression",
                "getelementptr constant index",
            )?;
            indices.push(Operand::IntConst(idx_ty, idx_val));
            op_idx += 2;
        }

        local_values.push(ValueEntry::GepConst {
            source_ty,
            ptr_name,
            ptr_ty,
            indices,
        });

        Ok(())
    }

    // -------------------------------------------------------------------
    // Constants block
    // -------------------------------------------------------------------

    fn read_module_constants(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.module_constant_value_offset = self.global_value_table.len() as u32;
        self.reader.push_block_scope(CONSTANTS_BLOCK_ID);
        let mut current_type_id: u32 = 0;

        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    match code {
                        CST_CODE_SETTYPE => {
                            let Some(type_id) = values.first().copied() else {
                                return Err(self.error("SETTYPE constant record too short"));
                            };
                            current_type_id = type_id as u32;
                        }
                        CST_CODE_INTEGER => {
                            let Some(encoded) = values.first().copied() else {
                                return Err(self.error("INTEGER constant record too short"));
                            };
                            let val = sign_unrotate(encoded);
                            let ty = self.resolve_constant_type(
                                current_type_id,
                                "constant record",
                                "integer constant current type",
                                Type::Void,
                            )?;
                            self.module_constants.push((ty.clone(), Constant::Int(val)));
                            self.global_value_table
                                .push(ValueEntry::Constant(ty, Constant::Int(val)));
                        }
                        CST_CODE_FLOAT => {
                            let Some(bits) = values.first().copied() else {
                                return Err(self.error("FLOAT constant record too short"));
                            };
                            let ty = self.resolve_constant_type(
                                current_type_id,
                                "constant record",
                                "floating constant current type",
                                Type::Void,
                            )?;
                            let Some(val) = ty.decode_float_bits(bits) else {
                                return Err(self
                                    .error("FLOAT constant record has non-floating current type"));
                            };
                            self.module_constants
                                .push((ty.clone(), Constant::float(ty.clone(), val)));
                            self.global_value_table
                                .push(ValueEntry::Constant(ty.clone(), Constant::float(ty, val)));
                        }
                        CST_CODE_CSTRING => {
                            let ty = self.resolve_constant_type(
                                current_type_id,
                                "constant record",
                                "cstring constant current type",
                                Type::Void,
                            )?;
                            let bytes = values
                                .iter()
                                .map(|value| {
                                    u8::try_from(*value).map_err(|_| {
                                        self.error("CSTRING constant contains out-of-range byte")
                                    })
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            let text = String::from_utf8(bytes).map_err(|_| {
                                self.error("CSTRING constant contains invalid UTF-8")
                            })?;
                            self.module_constants
                                .push((ty.clone(), Constant::CString(text.clone())));
                            self.global_value_table
                                .push(ValueEntry::Constant(ty, Constant::CString(text)));
                        }
                        CST_CODE_NULL => {
                            let ty = self.resolve_constant_type(
                                current_type_id,
                                "constant record",
                                "null constant current type",
                                Type::Void,
                            )?;
                            self.module_constants.push((ty.clone(), Constant::Null));
                            self.global_value_table
                                .push(ValueEntry::Constant(ty, Constant::Null));
                        }
                        CST_CODE_CE_CAST => {
                            if values.len() >= 3 && values[0] == 10 {
                                let src_value_id = values[2] as u32;
                                let int_val = self.resolve_constant_int_from_global_table(
                                    src_value_id,
                                    "constant expression",
                                    "inttoptr constant source",
                                )?;
                                let target_ty = self.resolve_constant_type(
                                    current_type_id,
                                    "constant expression",
                                    "inttoptr constant target type",
                                    Type::Ptr,
                                )?;
                                self.global_value_table
                                    .push(ValueEntry::IntToPtrConst(int_val, target_ty));
                            }
                        }
                        CST_CODE_CE_INBOUNDS_GEP => {
                            self.parse_gep_ce_into_global_table(&values)?;
                        }
                        _ => {}
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    fn read_function_constants(
        &mut self,
        abbrev_width: u32,
        local_values: &mut Vec<ValueEntry>,
        next_result_id: &mut u32,
    ) -> Result<(), ParseError> {
        self.reader.push_block_scope(CONSTANTS_BLOCK_ID);
        let mut current_type_id: u32 = 0;

        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    match code {
                        CST_CODE_SETTYPE => {
                            let Some(type_id) = values.first().copied() else {
                                return Err(self.error("SETTYPE constant record too short"));
                            };
                            current_type_id = type_id as u32;
                        }
                        CST_CODE_INTEGER => {
                            let Some(encoded) = values.first().copied() else {
                                return Err(self.error("INTEGER constant record too short"));
                            };
                            let val = sign_unrotate(encoded);
                            let ty = self.resolve_constant_type(
                                current_type_id,
                                "constant record",
                                "integer constant current type",
                                Type::Void,
                            )?;
                            local_values.push(ValueEntry::Constant(ty, Constant::Int(val)));
                            *next_result_id += 1;
                        }
                        CST_CODE_FLOAT => {
                            let Some(bits) = values.first().copied() else {
                                return Err(self.error("FLOAT constant record too short"));
                            };
                            let ty = self.resolve_constant_type(
                                current_type_id,
                                "constant record",
                                "floating constant current type",
                                Type::Void,
                            )?;
                            let Some(val) = ty.decode_float_bits(bits) else {
                                return Err(self
                                    .error("FLOAT constant record has non-floating current type"));
                            };
                            local_values
                                .push(ValueEntry::Constant(ty.clone(), Constant::float(ty, val)));
                            *next_result_id += 1;
                        }
                        CST_CODE_NULL => {
                            let ty = self.resolve_constant_type(
                                current_type_id,
                                "constant record",
                                "null constant current type",
                                Type::Void,
                            )?;
                            local_values.push(ValueEntry::Constant(ty, Constant::Null));
                            *next_result_id += 1;
                        }
                        CST_CODE_CE_CAST => {
                            if values.len() >= 3 && values[0] == 10 {
                                let src_value_id = values[2] as u32;
                                let int_val = self.resolve_constant_int_from_tables(
                                    src_value_id,
                                    local_values,
                                    "constant expression",
                                    "inttoptr constant source",
                                )?;
                                let target_ty = self.resolve_constant_type(
                                    current_type_id,
                                    "constant expression",
                                    "inttoptr constant target type",
                                    Type::Ptr,
                                )?;
                                local_values.push(ValueEntry::IntToPtrConst(int_val, target_ty));
                                *next_result_id += 1;
                            }
                        }
                        CST_CODE_CE_INBOUNDS_GEP => {
                            self.parse_gep_ce_into_local_values(&values, local_values)?;
                            *next_result_id += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    // -------------------------------------------------------------------
    // Value symbol tables
    // -------------------------------------------------------------------

    fn read_function_vst(
        &mut self,
        abbrev_width: u32,
        bb_names: &mut FxHashMap<u32, String>,
        local_names: &mut FxHashMap<u32, String>,
    ) -> Result<(), ParseError> {
        self.reader.push_block_scope(VALUE_SYMTAB_BLOCK_ID);
        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    if !values.is_empty() {
                        let name: String = values[1..].iter().map(|&v| v as u8 as char).collect();
                        match code {
                            VST_CODE_ENTRY => {
                                local_names.insert(values[0] as u32, name);
                            }
                            VST_CODE_BBENTRY => {
                                bb_names.insert(values[0] as u32, name);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    fn read_module_vst(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(VALUE_SYMTAB_BLOCK_ID);
        let mut name_remap = FxHashMap::default();
        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    match code {
                        VST_CODE_ENTRY if values.len() > 1 => {
                            let value_id = values[0] as usize;
                            let name: String =
                                values[1..].iter().map(|&v| v as u8 as char).collect();
                            self.apply_module_value_name(value_id, name, &mut name_remap);
                        }
                        VST_CODE_FNENTRY if values.len() > 2 => {
                            let value_id = values[0] as usize;
                            let name: String =
                                values[2..].iter().map(|&v| v as u8 as char).collect();
                            self.apply_module_value_name(value_id, name, &mut name_remap);
                        }
                        _ => {}
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        self.remap_module_symbol_uses(&name_remap);
        Ok(())
    }

    fn read_strtab_block(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(STRTAB_BLOCK_ID);
        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    self.reader
                        .read_define_abbrev()
                        .map_err(|e| self.error(e))?;
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    if code == STRTAB_BLOB {
                        self.string_table = values.iter().map(|&value| value as u8).collect();
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        self.apply_pending_strtab_names();
        Ok(())
    }

    // -------------------------------------------------------------------
    // Blockinfo block
    // -------------------------------------------------------------------

    fn read_blockinfo_block(&mut self, abbrev_width: u32) -> Result<(), ParseError> {
        self.reader.push_block_scope(BLOCKINFO_BLOCK_ID);
        let mut current_block_id: Option<u32> = None;

        loop {
            if self.reader.at_end() {
                break;
            }
            let abbrev_id = self.reader.read_abbrev_id(abbrev_width);
            match abbrev_id {
                0 => {
                    self.reader.align32();
                    break;
                }
                1 => {
                    let (_, _, block_len) = self.reader.enter_subblock();
                    self.reader.skip_block(block_len);
                }
                2 => {
                    if let Some(target_id) = current_block_id {
                        self.reader
                            .read_blockinfo_abbrev(target_id)
                            .map_err(|e| self.error(e))?;
                    } else {
                        return Err(self.error("DEFINE_ABBREV in BLOCKINFO before SETBID"));
                    }
                }
                id => {
                    let (code, values) = self.read_record(id)?;
                    if code == BLOCKINFO_CODE_SETBID
                        && let Some(&block_id_val) = values.first()
                    {
                        current_block_id = Some(block_id_val as u32);
                    }
                }
            }
        }
        self.reader.pop_block_scope();
        Ok(())
    }

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

    fn global_index_from_value_id(&self, value_id: usize) -> usize {
        let mut count = 0;
        for (i, entry) in self.global_value_table.iter().enumerate() {
            if i == value_id {
                return count;
            }
            if matches!(entry, ValueEntry::Global(_)) {
                count += 1;
            }
        }
        count
    }

    fn local_value_name_exists(local_values: &[ValueEntry], name: &str) -> bool {
        local_values.iter().any(|entry| match entry {
            ValueEntry::Local(entry_name, _) | ValueEntry::Param(entry_name, _) => {
                entry_name == name
            }
            ValueEntry::Global(_)
            | ValueEntry::Function(_)
            | ValueEntry::Constant(_, _)
            | ValueEntry::IntToPtrConst(_, _)
            | ValueEntry::GepConst { .. } => false,
        })
    }

    fn validate_phi_operands_resolved(
        &self,
        basic_blocks: &[BasicBlock],
        local_values: &[ValueEntry],
    ) -> Result<(), ParseError> {
        for block in basic_blocks {
            for instruction in &block.instructions {
                let Instruction::Phi { incoming, .. } = instruction else {
                    continue;
                };

                for (operand, _) in incoming {
                    let (Operand::LocalRef(name) | Operand::TypedLocalRef(name, _)) = operand
                    else {
                        continue;
                    };

                    if Self::local_value_name_exists(local_values, name) {
                        continue;
                    }

                    self.unsupported_or_recover(
                        "phi instruction",
                        format!(
                            "PHI incoming value `{name}` could not be resolved during bitcode import"
                        ),
                        format!(
                            "PHI incoming value `{name}` could not be resolved during bitcode import and was preserved as a placeholder"
                        ),
                        (),
                    )?;
                }
            }
        }

        Ok(())
    }

    fn func_index_from_value_id(&self, value_id: usize) -> usize {
        let mut count = 0;
        for (i, entry) in self.global_value_table.iter().enumerate() {
            if i == value_id {
                return count;
            }
            if matches!(entry, ValueEntry::Function(_)) {
                count += 1;
            }
        }
        count
    }

    fn resolve_pending_global_initializers(&mut self) -> Result<(), ParseError> {
        let pending = std::mem::take(&mut self.pending_global_initializers);

        for PendingGlobalInitializer {
            global_index,
            value_id,
        } in pending
        {
            let initializer = match self.global_value_table.get(value_id as usize).cloned() {
                Some(ValueEntry::Constant(
                    _,
                    constant @ (Constant::CString(_)
                    | Constant::Int(_)
                    | Constant::Float(_, _)
                    | Constant::Null),
                )) => constant,
                Some(_) => self.unsupported_or_recover(
                    "global variable",
                    format!(
                        "global initializer value ID {value_id} resolves to an unsupported initializer form"
                    ),
                    format!(
                        "global initializer value ID {value_id} resolves to an unsupported initializer form and was imported as null"
                    ),
                    Constant::Null,
                )?,
                None => self.unsupported_or_recover(
                    "global variable",
                    format!(
                        "global initializer value ID {value_id} could not be resolved during bitcode import"
                    ),
                    format!(
                        "global initializer value ID {value_id} could not be resolved during bitcode import and was imported as null"
                    ),
                    Constant::Null,
                )?,
            };

            self.globals[global_index].initializer = Some(initializer);
        }

        Ok(())
    }

    fn build_module(self) -> Module {
        use crate::model::StructType as ModelStructType;

        let struct_types: Vec<ModelStructType> = self
            .struct_types
            .into_iter()
            .map(|name| ModelStructType {
                name,
                is_opaque: true,
            })
            .collect();

        Module {
            source_filename: self.source_filename,
            target_datalayout: self.target_datalayout,
            target_triple: self.target_triple,
            struct_types,
            globals: self.globals,
            functions: self.functions,
            attribute_groups: self.attribute_groups,
            named_metadata: self.named_metadata,
            metadata_nodes: self.metadata_nodes,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn sign_unrotate(val: u64) -> i64 {
    if val & 1 == 0 {
        (val >> 1) as i64
    } else if val == 1 {
        i64::MIN
    } else {
        -((val >> 1) as i64)
    }
}

fn opcode_to_binop(opcode: u64, ty: &Type) -> Result<BinOpKind, ParseError> {
    let is_fp = ty.is_floating_point();
    match opcode {
        0 => {
            if is_fp {
                Ok(BinOpKind::Fadd)
            } else {
                Ok(BinOpKind::Add)
            }
        }
        1 => {
            if is_fp {
                Ok(BinOpKind::Fsub)
            } else {
                Ok(BinOpKind::Sub)
            }
        }
        2 => {
            if is_fp {
                Ok(BinOpKind::Fmul)
            } else {
                Ok(BinOpKind::Mul)
            }
        }
        3 => Ok(BinOpKind::Udiv),
        4 => {
            if is_fp {
                Ok(BinOpKind::Fdiv)
            } else {
                Ok(BinOpKind::Sdiv)
            }
        }
        5 => Ok(BinOpKind::Urem),
        6 => Ok(BinOpKind::Srem),
        7 => Ok(BinOpKind::Shl),
        8 => Ok(BinOpKind::Lshr),
        9 => Ok(BinOpKind::Ashr),
        10 => Ok(BinOpKind::And),
        11 => Ok(BinOpKind::Or),
        12 => Ok(BinOpKind::Xor),
        _ => Err(ParseError::malformed(
            0,
            "instruction record",
            format!("unknown binop opcode: {opcode}"),
        )),
    }
}

fn icmp_code_to_predicate(code: u64) -> Result<IntPredicate, ParseError> {
    match code {
        32 => Ok(IntPredicate::Eq),
        33 => Ok(IntPredicate::Ne),
        34 => Ok(IntPredicate::Ugt),
        35 => Ok(IntPredicate::Uge),
        36 => Ok(IntPredicate::Ult),
        37 => Ok(IntPredicate::Ule),
        38 => Ok(IntPredicate::Sgt),
        39 => Ok(IntPredicate::Sge),
        40 => Ok(IntPredicate::Slt),
        41 => Ok(IntPredicate::Sle),
        _ => Err(ParseError::malformed(
            0,
            "instruction record",
            format!("unknown icmp predicate code: {code}"),
        )),
    }
}

fn fcmp_code_to_predicate(code: u64) -> Result<FloatPredicate, ParseError> {
    match code {
        1 => Ok(FloatPredicate::Oeq),
        2 => Ok(FloatPredicate::Ogt),
        3 => Ok(FloatPredicate::Oge),
        4 => Ok(FloatPredicate::Olt),
        5 => Ok(FloatPredicate::Ole),
        6 => Ok(FloatPredicate::One),
        7 => Ok(FloatPredicate::Ord),
        8 => Ok(FloatPredicate::Uno),
        9 => Ok(FloatPredicate::Ueq),
        10 => Ok(FloatPredicate::Ugt),
        11 => Ok(FloatPredicate::Uge),
        12 => Ok(FloatPredicate::Ult),
        13 => Ok(FloatPredicate::Ule),
        14 => Ok(FloatPredicate::Une),
        _ => Err(ParseError::malformed(
            0,
            "instruction record",
            format!("unknown fcmp predicate code: {code}"),
        )),
    }
}

fn opcode_to_cast(opcode: u64) -> Result<CastKind, ParseError> {
    match opcode {
        0 => Ok(CastKind::Trunc),
        1 => Ok(CastKind::Zext),
        2 => Ok(CastKind::Sext),
        4 => Ok(CastKind::FpTrunc),
        5 => Ok(CastKind::FpExt),
        6 => Ok(CastKind::Sitofp),
        7 => Ok(CastKind::Fptosi),
        9 => Ok(CastKind::PtrToInt),
        10 => Ok(CastKind::IntToPtr),
        11 => Ok(CastKind::Bitcast),
        _ => Err(ParseError::malformed(
            0,
            "instruction record",
            format!("unknown cast opcode: {opcode}"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

fn parse_bitcode_with_policy(data: &[u8], policy: ReadPolicy) -> Result<Module, ParseError> {
    let mut reader = BlockReader::new(data, policy)?;
    reader.read_top_level()?;
    reader.resolve_pending_global_initializers()?;
    Ok(reader.build_module())
}

fn parse_bitcode_report_with_policy(
    data: &[u8],
    policy: ReadPolicy,
) -> Result<ReadReport, Vec<ReadDiagnostic>> {
    let mut reader = BlockReader::new(data, policy).map_err(|error| vec![error.into()])?;
    match reader.read_top_level() {
        Ok(()) => {
            if let Err(error) = reader.resolve_pending_global_initializers() {
                let mut diagnostics = std::mem::take(reader.diagnostics.get_mut());
                diagnostics.push(error.into());
                return Err(diagnostics);
            }
            let diagnostics = std::mem::take(reader.diagnostics.get_mut());
            let module = reader.build_module();
            Ok(ReadReport {
                module,
                diagnostics,
            })
        }
        Err(error) => {
            let mut diagnostics = std::mem::take(reader.diagnostics.get_mut());
            diagnostics.push(error.into());
            Err(diagnostics)
        }
    }
}

pub fn parse_bitcode_detailed(
    data: &[u8],
    policy: ReadPolicy,
) -> Result<Module, Vec<ReadDiagnostic>> {
    let report = parse_bitcode_report_with_policy(data, policy)?;
    if policy == ReadPolicy::Compatibility && !report.diagnostics.is_empty() {
        Err(report.diagnostics)
    } else {
        Ok(report.module)
    }
}

/// Parses LLVM bitcode data into a `Module`.
pub fn parse_bitcode(data: &[u8]) -> Result<Module, ParseError> {
    parse_bitcode_with_policy(data, ReadPolicy::QirSubsetStrict)
}

pub fn parse_bitcode_compatibility(data: &[u8]) -> Result<Module, ParseError> {
    parse_bitcode_detailed(data, ReadPolicy::Compatibility)
        .map_err(|mut diagnostics| diagnostics.remove(0).into())
}

pub fn parse_bitcode_compatibility_report(data: &[u8]) -> Result<ReadReport, Vec<ReadDiagnostic>> {
    parse_bitcode_report_with_policy(data, ReadPolicy::Compatibility)
}
