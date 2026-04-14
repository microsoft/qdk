// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::fmt::Write;

use crate::model::Type;
use crate::model::{
    Attribute, AttributeGroup, BasicBlock, BinOpKind, CastKind, Constant, FloatPredicate, Function,
    GlobalVariable, Instruction, IntPredicate, Linkage, MetadataNode, MetadataValue, Module,
    NamedMetadata, Operand, StructType,
};

#[must_use]
pub fn write_module_to_string(module: &Module) -> String {
    let mut buf = String::new();
    write_module(&mut buf, module).expect("writing to string should succeed");
    buf
}

pub fn write_module(w: &mut dyn Write, module: &Module) -> Result<(), std::fmt::Error> {
    // 1. source_filename
    if let Some(ref name) = module.source_filename {
        writeln!(w, "source_filename = \"{name}\"")?;
    }

    // 2. struct types
    for st in &module.struct_types {
        write_struct_type(w, st)?;
        writeln!(w)?;
    }

    // 3. blank line + globals
    if !module.globals.is_empty() {
        if !module.struct_types.is_empty() {
            writeln!(w)?;
        }
        for g in &module.globals {
            write_global(w, g)?;
            writeln!(w)?;
        }
    }

    // 4. functions (declarations and definitions in original order)
    if !module.functions.is_empty() {
        writeln!(w)?;
    }

    for f in &module.functions {
        write_function(w, f)?;
        writeln!(w)?;
        writeln!(w)?;
    }

    // 6. attribute groups
    for ag in &module.attribute_groups {
        write_attribute_group(w, ag)?;
        writeln!(w)?;
    }

    // 7. named metadata header comment + named metadata
    if !module.named_metadata.is_empty() {
        writeln!(w)?;
        writeln!(w, "; module flags")?;
        writeln!(w)?;
        for nm in &module.named_metadata {
            write_named_metadata(w, nm)?;
            writeln!(w)?;
        }
    }

    // 8. metadata nodes
    if !module.metadata_nodes.is_empty() {
        writeln!(w)?;
        for node in &module.metadata_nodes {
            write_metadata_node(w, node)?;
            writeln!(w)?;
        }
    }

    Ok(())
}

fn write_struct_type(w: &mut dyn Write, st: &StructType) -> Result<(), std::fmt::Error> {
    if st.is_opaque {
        write!(w, "%{} = type opaque", st.name)
    } else {
        write!(w, "%{} = type {{}}", st.name)
    }
}

fn write_global(w: &mut dyn Write, g: &GlobalVariable) -> Result<(), std::fmt::Error> {
    let linkage = match g.linkage {
        Linkage::Internal => "internal",
        Linkage::External => "external",
    };
    let kind = if g.is_constant { "constant" } else { "global" };
    write!(w, "@{} = {linkage} {kind} {}", g.name, g.ty)?;
    if let Some(ref init) = g.initializer {
        write!(w, " ")?;
        write_constant(w, init)?;
    }
    Ok(())
}

fn write_constant(w: &mut dyn Write, c: &Constant) -> Result<(), std::fmt::Error> {
    match c {
        Constant::CString(s) => {
            write!(w, "c\"{s}\\00\"")
        }
        Constant::Int(i) => write!(w, "{i}"),
        Constant::Float(_, f) => write_float(w, *f),
        Constant::Null => write!(w, "null"),
    }
}

fn write_float(w: &mut dyn Write, f: f64) -> Result<(), std::fmt::Error> {
    if (f.floor() - f.ceil()).abs() < f64::EPSILON {
        write!(w, "{f:.1}")
    } else {
        write!(w, "{f}")
    }
}

fn write_function(w: &mut dyn Write, f: &Function) -> Result<(), std::fmt::Error> {
    if f.is_declaration {
        write!(w, "declare {} @{}(", f.return_type, f.name)?;
        write_param_list(w, f)?;
        write!(w, ")")?;
        for attr_ref in &f.attribute_group_refs {
            write!(w, " #{attr_ref}")?;
        }
    } else {
        write!(w, "define {} @{}(", f.return_type, f.name)?;
        write_param_list(w, f)?;
        write!(w, ")")?;
        for attr_ref in &f.attribute_group_refs {
            write!(w, " #{attr_ref}")?;
        }
        writeln!(w, " {{")?;
        for bb in &f.basic_blocks {
            write_basic_block(w, bb)?;
        }
        write!(w, "}}")?;
    }
    Ok(())
}

fn write_param_list(w: &mut dyn Write, f: &Function) -> Result<(), std::fmt::Error> {
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            write!(w, ", ")?;
        }
        write!(w, "{}", p.ty)?;
        if let Some(ref name) = p.name {
            write!(w, " %{name}")?;
        }
    }
    Ok(())
}

fn write_basic_block(w: &mut dyn Write, bb: &BasicBlock) -> Result<(), std::fmt::Error> {
    writeln!(w, "{}:", bb.name)?;
    for instr in &bb.instructions {
        write_instruction(w, instr)?;
        writeln!(w)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn write_instruction(w: &mut dyn Write, instr: &Instruction) -> Result<(), std::fmt::Error> {
    match instr {
        Instruction::Ret(None) => write!(w, "  ret void"),
        Instruction::Ret(Some(operand)) => {
            write!(w, "  ret ")?;
            write_typed_operand(w, operand)
        }
        Instruction::Br {
            cond_ty,
            cond,
            true_dest,
            false_dest,
        } => {
            write!(w, "  br {cond_ty} ")?;
            write_untyped_operand(w, cond)?;
            write!(w, ", label %{true_dest}, label %{false_dest}")
        }
        Instruction::Jump { dest } => {
            write!(w, "  br label %{dest}")
        }
        Instruction::BinOp {
            op,
            ty,
            lhs,
            rhs,
            result,
        } => {
            let op_str = binop_name(op);
            write!(w, "  %{result} = {op_str} {ty} ")?;
            write_untyped_operand(w, lhs)?;
            write!(w, ", ")?;
            write_untyped_operand(w, rhs)
        }
        Instruction::ICmp {
            pred,
            ty,
            lhs,
            rhs,
            result,
        } => {
            let pred_str = icmp_pred_name(pred);
            write!(w, "  %{result} = icmp {pred_str} {ty} ")?;
            write_untyped_operand(w, lhs)?;
            write!(w, ", ")?;
            write_untyped_operand(w, rhs)
        }
        Instruction::FCmp {
            pred,
            ty,
            lhs,
            rhs,
            result,
        } => {
            let pred_str = fcmp_pred_name(pred);
            write!(w, "  %{result} = fcmp {pred_str} {ty} ")?;
            write_untyped_operand(w, lhs)?;
            write!(w, ", ")?;
            write_untyped_operand(w, rhs)
        }
        Instruction::Cast {
            op,
            from_ty,
            to_ty,
            value,
            result,
        } => {
            let op_str = cast_name(op);
            write!(w, "  %{result} = {op_str} {from_ty} ")?;
            write_untyped_operand(w, value)?;
            write!(w, " to {to_ty}")
        }
        Instruction::Call {
            return_ty,
            callee,
            args,
            result,
            attr_refs,
        } => {
            let ret_ty = return_ty.as_ref().map_or(Type::Void, Clone::clone);
            if let Some(r) = result {
                write!(w, "  %{r} = call {ret_ty} @{callee}(")?;
            } else {
                write!(w, "  call {ret_ty} @{callee}(")?;
            }
            for (i, (ty, op)) in args.iter().enumerate() {
                if i > 0 {
                    write!(w, ", ")?;
                }
                write!(w, "{ty} ")?;
                write_untyped_operand(w, op)?;
            }
            write!(w, ")")?;
            for attr_ref in attr_refs {
                write!(w, " #{attr_ref}")?;
            }
            Ok(())
        }
        Instruction::Phi {
            ty,
            incoming,
            result,
        } => {
            write!(w, "  %{result} = phi {ty} ")?;
            for (i, (val, block)) in incoming.iter().enumerate() {
                if i > 0 {
                    write!(w, ", ")?;
                }
                write!(w, "[")?;
                write_untyped_operand(w, val)?;
                write!(w, ", %{block}]")?;
            }
            Ok(())
        }
        Instruction::Alloca { ty, result } => {
            write!(w, "  %{result} = alloca {ty}")
        }
        Instruction::Load {
            ty,
            ptr_ty,
            ptr,
            result,
        } => {
            write!(w, "  %{result} = load {ty}, {ptr_ty} ")?;
            write_untyped_operand(w, ptr)
        }
        Instruction::Store {
            ty,
            value,
            ptr_ty,
            ptr,
        } => {
            write!(w, "  store {ty} ")?;
            write_untyped_operand(w, value)?;
            write!(w, ", {ptr_ty} ")?;
            write_untyped_operand(w, ptr)
        }
        Instruction::Select {
            cond,
            true_val,
            false_val,
            ty,
            result,
        } => {
            write!(w, "  %{result} = select i1 ")?;
            write_untyped_operand(w, cond)?;
            write!(w, ", {ty} ")?;
            write_untyped_operand(w, true_val)?;
            write!(w, ", {ty} ")?;
            write_untyped_operand(w, false_val)
        }
        Instruction::Switch {
            ty,
            value,
            default_dest,
            cases,
        } => {
            write!(w, "  switch {ty} ")?;
            write_untyped_operand(w, value)?;
            writeln!(w, ", label %{default_dest} [")?;
            for (val, dest) in cases {
                writeln!(w, "    {ty} {val}, label %{dest}")?;
            }
            write!(w, "  ]")
        }
        Instruction::Unreachable => write!(w, "  unreachable"),
        Instruction::GetElementPtr {
            inbounds,
            pointee_ty,
            ptr_ty,
            ptr,
            indices,
            result,
        } => {
            write!(w, "  %{result} = getelementptr ")?;
            if *inbounds {
                write!(w, "inbounds ")?;
            }
            write!(w, "{pointee_ty}, {ptr_ty} ")?;
            write_untyped_operand(w, ptr)?;
            for idx in indices {
                write!(w, ", ")?;
                write_typed_operand(w, idx)?;
            }
            Ok(())
        }
    }
}

fn write_typed_operand(w: &mut dyn Write, op: &Operand) -> Result<(), std::fmt::Error> {
    match op {
        Operand::LocalRef(name) => {
            write!(w, "%{name}")
        }
        Operand::TypedLocalRef(name, ty) => {
            write!(w, "{ty} %{name}")
        }
        Operand::IntConst(ty, val) => {
            write!(w, "{ty} ")?;
            write_int_value(w, ty, *val)
        }
        Operand::FloatConst(ty, f) => {
            write!(w, "{ty} ")?;
            write_float(w, *f)
        }
        Operand::NullPtr => write!(w, "ptr null"),
        Operand::IntToPtr(val, ty) => {
            write!(w, "{ty} inttoptr (i64 {val} to {ty})")
        }
        Operand::GetElementPtr {
            ty,
            ptr,
            ptr_ty,
            indices,
        } => {
            write!(w, "{ptr_ty} getelementptr inbounds ({ty}, {ptr_ty} @{ptr}")?;
            for idx in indices {
                write!(w, ", ")?;
                write_typed_operand(w, idx)?;
            }
            write!(w, ")")
        }
        Operand::GlobalRef(name) => write!(w, "ptr @{name}"),
    }
}

fn write_untyped_operand(w: &mut dyn Write, op: &Operand) -> Result<(), std::fmt::Error> {
    match op {
        Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => write!(w, "%{name}"),
        Operand::IntConst(ty, val) => write_int_value(w, ty, *val),
        Operand::FloatConst(_, f) => write_float(w, *f),
        Operand::NullPtr => write!(w, "null"),
        Operand::IntToPtr(val, ty) => {
            write!(w, "inttoptr (i64 {val} to {ty})")
        }
        Operand::GetElementPtr {
            ty,
            ptr,
            ptr_ty,
            indices,
        } => {
            write!(w, "getelementptr inbounds ({ty}, {ptr_ty} @{ptr}")?;
            for idx in indices {
                write!(w, ", ")?;
                write_typed_operand(w, idx)?;
            }
            write!(w, ")")
        }
        Operand::GlobalRef(name) => write!(w, "@{name}"),
    }
}

fn write_int_value(w: &mut dyn Write, ty: &Type, val: i64) -> Result<(), std::fmt::Error> {
    if let Type::Integer(1) = ty {
        if val == 0 {
            write!(w, "false")
        } else {
            write!(w, "true")
        }
    } else {
        write!(w, "{val}")
    }
}

fn write_attribute_group(w: &mut dyn Write, ag: &AttributeGroup) -> Result<(), std::fmt::Error> {
    write!(w, "attributes #{} = {{ ", ag.id)?;
    for (i, attr) in ag.attributes.iter().enumerate() {
        if i > 0 {
            write!(w, " ")?;
        }
        match attr {
            Attribute::StringAttr(s) => write!(w, "\"{s}\"")?,
            Attribute::KeyValue(k, v) => write!(w, "\"{k}\"=\"{v}\"")?,
        }
    }
    write!(w, " }}")
}

fn write_named_metadata(w: &mut dyn Write, nm: &NamedMetadata) -> Result<(), std::fmt::Error> {
    write!(w, "!{} = !{{", nm.name)?;
    for (i, node_ref) in nm.node_refs.iter().enumerate() {
        if i > 0 {
            write!(w, ", ")?;
        }
        write!(w, "!{node_ref}")?;
    }
    write!(w, "}}")
}

fn write_metadata_node(w: &mut dyn Write, node: &MetadataNode) -> Result<(), std::fmt::Error> {
    write!(w, "!{} = !{{", node.id)?;
    for (i, val) in node.values.iter().enumerate() {
        if i > 0 {
            write!(w, ", ")?;
        }
        write_metadata_value(w, val)?;
    }
    write!(w, "}}")
}

fn write_metadata_value(w: &mut dyn Write, val: &MetadataValue) -> Result<(), std::fmt::Error> {
    match val {
        MetadataValue::Int(ty, v) => {
            if let Type::Integer(1) = ty {
                if *v == 0 {
                    write!(w, "{ty} false")
                } else {
                    write!(w, "{ty} true")
                }
            } else {
                write!(w, "{ty} {v}")
            }
        }
        MetadataValue::String(s) => write!(w, "!\"{s}\""),
        MetadataValue::NodeRef(id) => write!(w, "!{id}"),
        MetadataValue::SubList(vals) => {
            write!(w, "!{{")?;
            for (i, v) in vals.iter().enumerate() {
                if i > 0 {
                    write!(w, ", ")?;
                }
                write_metadata_value(w, v)?;
            }
            write!(w, "}}")
        }
    }
}

fn binop_name(op: &BinOpKind) -> &'static str {
    match op {
        BinOpKind::Add => "add",
        BinOpKind::Sub => "sub",
        BinOpKind::Mul => "mul",
        BinOpKind::Sdiv => "sdiv",
        BinOpKind::Srem => "srem",
        BinOpKind::Shl => "shl",
        BinOpKind::Ashr => "ashr",
        BinOpKind::And => "and",
        BinOpKind::Or => "or",
        BinOpKind::Xor => "xor",
        BinOpKind::Fadd => "fadd",
        BinOpKind::Fsub => "fsub",
        BinOpKind::Fmul => "fmul",
        BinOpKind::Fdiv => "fdiv",
        BinOpKind::Udiv => "udiv",
        BinOpKind::Urem => "urem",
        BinOpKind::Lshr => "lshr",
    }
}

fn icmp_pred_name(pred: &IntPredicate) -> &'static str {
    match pred {
        IntPredicate::Eq => "eq",
        IntPredicate::Ne => "ne",
        IntPredicate::Sgt => "sgt",
        IntPredicate::Sge => "sge",
        IntPredicate::Slt => "slt",
        IntPredicate::Sle => "sle",
        IntPredicate::Ult => "ult",
        IntPredicate::Ule => "ule",
        IntPredicate::Ugt => "ugt",
        IntPredicate::Uge => "uge",
    }
}

fn fcmp_pred_name(pred: &FloatPredicate) -> &'static str {
    match pred {
        FloatPredicate::Oeq => "oeq",
        FloatPredicate::Ogt => "ogt",
        FloatPredicate::Oge => "oge",
        FloatPredicate::Olt => "olt",
        FloatPredicate::Ole => "ole",
        FloatPredicate::One => "one",
        FloatPredicate::Ord => "ord",
        FloatPredicate::Uno => "uno",
        FloatPredicate::Ueq => "ueq",
        FloatPredicate::Ugt => "ugt",
        FloatPredicate::Uge => "uge",
        FloatPredicate::Ult => "ult",
        FloatPredicate::Ule => "ule",
        FloatPredicate::Une => "une",
    }
}

fn cast_name(op: &CastKind) -> &'static str {
    match op {
        CastKind::Sitofp => "sitofp",
        CastKind::Fptosi => "fptosi",
        CastKind::Zext => "zext",
        CastKind::Sext => "sext",
        CastKind::Trunc => "trunc",
        CastKind::FpExt => "fpext",
        CastKind::FpTrunc => "fptrunc",
        CastKind::IntToPtr => "inttoptr",
        CastKind::PtrToInt => "ptrtoint",
        CastKind::Bitcast => "bitcast",
    }
}
