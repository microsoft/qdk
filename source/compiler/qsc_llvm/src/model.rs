// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod test_helpers;

pub mod builder;

use half::f16;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Integer(u32),
    Half,
    Float,
    Double,
    Label,
    Ptr,
    NamedPtr(String),
    TypedPtr(Box<Type>),
    Array(u64, Box<Type>),
    Function(Box<Type>, Vec<Type>),
    Named(String),
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Void => write!(f, "void"),
            Type::Integer(n) => write!(f, "i{n}"),
            Type::Half => write!(f, "half"),
            Type::Float => write!(f, "float"),
            Type::Double => write!(f, "double"),
            Type::Label => write!(f, "label"),
            Type::Ptr => write!(f, "ptr"),
            Type::NamedPtr(s) => write!(f, "%{s}*"),
            Type::TypedPtr(inner) => write!(f, "{inner}*"),
            Type::Array(n, ty) => write!(f, "[{n} x {ty}]"),
            Type::Function(ret, params) => {
                write!(f, "{ret} (")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ")")
            }
            Type::Named(s) => write!(f, "%{s}"),
        }
    }
}

impl Type {
    #[must_use]
    pub fn is_floating_point(&self) -> bool {
        matches!(self, Self::Half | Self::Float | Self::Double)
    }

    #[must_use]
    pub fn floating_point_bit_width(&self) -> Option<u32> {
        match self {
            Self::Half => Some(16),
            Self::Float => Some(32),
            Self::Double => Some(64),
            _ => None,
        }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn canonicalize_float_value(&self, value: f64) -> Option<f64> {
        match self {
            Self::Half => Some(f16::from_f64(value).to_f64()),
            Self::Float => Some(f64::from(value as f32)),
            Self::Double => Some(value),
            _ => None,
        }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn encode_float_bits(&self, value: f64) -> Option<u64> {
        let canonical = self.canonicalize_float_value(value)?;
        match self {
            Self::Half => Some(u64::from(f16::from_f64(canonical).to_bits())),
            Self::Float => Some(u64::from((canonical as f32).to_bits())),
            Self::Double => Some(canonical.to_bits()),
            _ => None,
        }
    }

    #[must_use]
    pub fn decode_float_bits(&self, bits: u64) -> Option<f64> {
        match self {
            Self::Half => Some(f16::from_bits(u16::try_from(bits).ok()?).to_f64()),
            Self::Float => Some(f64::from(f32::from_bits(u32::try_from(bits).ok()?))),
            Self::Double => Some(f64::from_bits(bits)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub source_filename: Option<String>,
    pub target_datalayout: Option<String>,
    pub target_triple: Option<String>,
    pub struct_types: Vec<StructType>,
    pub globals: Vec<GlobalVariable>,
    pub functions: Vec<Function>,
    pub attribute_groups: Vec<AttributeGroup>,
    pub named_metadata: Vec<NamedMetadata>,
    pub metadata_nodes: Vec<MetadataNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructType {
    pub name: String,
    pub is_opaque: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalVariable {
    pub name: String,
    pub ty: Type,
    pub linkage: Linkage,
    pub is_constant: bool,
    pub initializer: Option<Constant>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Linkage {
    Internal,
    External,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    CString(String),
    Int(i64),
    Float(Type, f64),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub return_type: Type,
    pub params: Vec<Param>,
    pub is_declaration: bool,
    pub attribute_group_refs: Vec<u32>,
    pub basic_blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub ty: Type,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub name: String,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    Ret(Option<Operand>),
    Br {
        cond_ty: Type,
        cond: Operand,
        true_dest: String,
        false_dest: String,
    },
    Jump {
        dest: String,
    },
    BinOp {
        op: BinOpKind,
        ty: Type,
        lhs: Operand,
        rhs: Operand,
        result: String,
    },
    ICmp {
        pred: IntPredicate,
        ty: Type,
        lhs: Operand,
        rhs: Operand,
        result: String,
    },
    FCmp {
        pred: FloatPredicate,
        ty: Type,
        lhs: Operand,
        rhs: Operand,
        result: String,
    },
    Cast {
        op: CastKind,
        from_ty: Type,
        to_ty: Type,
        value: Operand,
        result: String,
    },
    Call {
        return_ty: Option<Type>,
        callee: String,
        args: Vec<(Type, Operand)>,
        result: Option<String>,
        attr_refs: Vec<u32>,
    },
    Phi {
        ty: Type,
        incoming: Vec<(Operand, String)>,
        result: String,
    },
    Alloca {
        ty: Type,
        result: String,
    },
    Load {
        ty: Type,
        ptr_ty: Type,
        ptr: Operand,
        result: String,
    },
    Store {
        ty: Type,
        value: Operand,
        ptr_ty: Type,
        ptr: Operand,
    },
    Select {
        cond: Operand,
        true_val: Operand,
        false_val: Operand,
        ty: Type,
        result: String,
    },
    Switch {
        ty: Type,
        value: Operand,
        default_dest: String,
        cases: Vec<(i64, String)>,
    },
    GetElementPtr {
        inbounds: bool,
        pointee_ty: Type,
        ptr_ty: Type,
        ptr: Operand,
        indices: Vec<Operand>,
        result: String,
    },
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Sdiv,
    Srem,
    Shl,
    Ashr,
    And,
    Or,
    Xor,
    Fadd,
    Fsub,
    Fmul,
    Fdiv,
    Udiv,
    Urem,
    Lshr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IntPredicate {
    Eq,
    Ne,
    Sgt,
    Sge,
    Slt,
    Sle,
    Ult,
    Ule,
    Ugt,
    Uge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FloatPredicate {
    Oeq,
    Ogt,
    Oge,
    Olt,
    Ole,
    One,
    Ord,
    Uno,
    Ueq,
    Ugt,
    Uge,
    Ult,
    Ule,
    Une,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CastKind {
    Sitofp,
    Fptosi,
    Zext,
    Sext,
    Trunc,
    FpExt,
    FpTrunc,
    IntToPtr,
    PtrToInt,
    Bitcast,
}

#[derive(Debug, Clone)]
pub enum Operand {
    LocalRef(String),
    TypedLocalRef(String, Type),
    IntConst(Type, i64),
    FloatConst(Type, f64),
    NullPtr,
    IntToPtr(i64, Type),
    GetElementPtr {
        ty: Type,
        ptr: String,
        ptr_ty: Type,
        indices: Vec<Operand>,
    },
    GlobalRef(String),
}

impl PartialEq for Operand {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::TypedLocalRef(lhs_name, lhs_ty), Self::TypedLocalRef(rhs_name, rhs_ty)) => {
                lhs_name == rhs_name && lhs_ty == rhs_ty
            }
            (Self::IntConst(lhs_ty, lhs_val), Self::IntConst(rhs_ty, rhs_val)) => {
                lhs_ty == rhs_ty && lhs_val == rhs_val
            }
            (Self::FloatConst(lhs_ty, lhs), Self::FloatConst(rhs_ty, rhs)) => {
                lhs_ty == rhs_ty && lhs == rhs
            }
            (Self::NullPtr, Self::NullPtr) => true,
            (Self::IntToPtr(lhs_val, lhs_ty), Self::IntToPtr(rhs_val, rhs_ty)) => {
                lhs_val == rhs_val && lhs_ty == rhs_ty
            }
            (
                Self::GetElementPtr {
                    ty: lhs_ty,
                    ptr: lhs_ptr,
                    ptr_ty: lhs_ptr_ty,
                    indices: lhs_indices,
                },
                Self::GetElementPtr {
                    ty: rhs_ty,
                    ptr: rhs_ptr,
                    ptr_ty: rhs_ptr_ty,
                    indices: rhs_indices,
                },
            ) => {
                lhs_ty == rhs_ty
                    && lhs_ptr == rhs_ptr
                    && lhs_ptr_ty == rhs_ptr_ty
                    && lhs_indices == rhs_indices
            }
            (Self::LocalRef(lhs) | Self::TypedLocalRef(lhs, _), Self::LocalRef(rhs))
            | (Self::LocalRef(lhs), Self::TypedLocalRef(rhs, _))
            | (Self::GlobalRef(lhs), Self::GlobalRef(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

impl Constant {
    #[must_use]
    pub fn float(ty: Type, value: f64) -> Self {
        let value = ty.canonicalize_float_value(value).unwrap_or(value);
        Self::Float(ty, value)
    }
}

impl Operand {
    #[must_use]
    pub fn float_const(ty: Type, value: f64) -> Self {
        let value = ty.canonicalize_float_value(value).unwrap_or(value);
        Self::FloatConst(ty, value)
    }

    #[must_use]
    pub fn int_to_named_ptr<S: Into<String>>(value: i64, name: S) -> Self {
        Self::IntToPtr(value, Type::NamedPtr(name.into()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeGroup {
    pub id: u32,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Attribute {
    StringAttr(String),
    KeyValue(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedMetadata {
    pub name: String,
    pub node_refs: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetadataNode {
    pub id: u32,
    pub values: Vec<MetadataValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataValue {
    Int(Type, i64),
    String(String),
    NodeRef(u32),
    SubList(Vec<MetadataValue>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModuleFlagNodeIssue {
    DanglingReference { node_ref: u32 },
    MalformedEntry { node_ref: u32, reason: &'static str },
}

#[derive(Debug, Clone)]
pub(crate) struct ModuleFlagNode<'a> {
    pub(crate) node_id: u32,
    pub(crate) behavior: &'a MetadataValue,
    pub(crate) key: &'a str,
    pub(crate) value: &'a MetadataValue,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ModuleFlagAudit<'a> {
    pub(crate) entries: Vec<ModuleFlagNode<'a>>,
    pub(crate) issues: Vec<ModuleFlagNodeIssue>,
}

impl Module {
    #[must_use]
    pub(crate) fn audit_module_flags(&self) -> ModuleFlagAudit<'_> {
        let Some(named_metadata) = self
            .named_metadata
            .iter()
            .find(|metadata| metadata.name == "llvm.module.flags")
        else {
            return ModuleFlagAudit::default();
        };

        let mut audit = ModuleFlagAudit::default();

        for &node_ref in &named_metadata.node_refs {
            let Some(node) = self
                .metadata_nodes
                .iter()
                .find(|candidate| candidate.id == node_ref)
            else {
                audit
                    .issues
                    .push(ModuleFlagNodeIssue::DanglingReference { node_ref });
                continue;
            };

            if node.values.len() < 3 {
                audit.issues.push(ModuleFlagNodeIssue::MalformedEntry {
                    node_ref: node.id,
                    reason: "module flag nodes must contain behavior, name, and value operands",
                });
                continue;
            }

            let MetadataValue::String(key) = &node.values[1] else {
                audit.issues.push(ModuleFlagNodeIssue::MalformedEntry {
                    node_ref: node.id,
                    reason: "module flag names must be metadata strings",
                });
                continue;
            };

            audit.entries.push(ModuleFlagNode {
                node_id: node.id,
                behavior: &node.values[0],
                key,
                value: &node.values[2],
            });
        }

        audit
    }

    /// Retrieves a module flag value by key from `!llvm.module.flags` named metadata.
    #[must_use]
    pub fn get_flag(&self, key: &str) -> Option<&MetadataValue> {
        self.audit_module_flags()
            .entries
            .into_iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.value)
    }
}
