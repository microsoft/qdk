// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use thiserror::Error;

use super::bitstream::{AbbrevDef, AbbrevOperand, BitstreamWriter};
use crate::model::Type;
use crate::model::{
    Attribute, BinOpKind, CastKind, Constant, FloatPredicate, Function, Instruction, IntPredicate,
    Linkage, MetadataNode, MetadataValue, Module, Operand,
};
use crate::qir::{QIR_MAJOR_VERSION_KEY, QirEmitTarget};

use super::constants::*;

// Identification block codes
const IDENTIFICATION_CODE_STRING: u32 = 1;
const IDENTIFICATION_CODE_EPOCH: u32 = 2;

// Attribute record codes
const PARAMATTR_GRP_CODE_ENTRY: u32 = 3;
const PARAMATTR_CODE_ENTRY: u32 = 2;

// Fixed abbreviation width for all our blocks
const ABBREV_WIDTH: u32 = 4;
const TOP_LEVEL_ABBREV_WIDTH: u32 = 2;
const SYNTHETIC_METADATA_NODE_START: u32 = u32::MAX / 2;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum WriteError {
    #[error("bitcode writer could not resolve operand `{operand}` in {context}")]
    UnresolvedOperand { context: String, operand: String },

    #[error("bitcode writer could not resolve basic block `{block}` in {context}")]
    MissingBasicBlock { context: String, block: String },

    #[error("bitcode writer could not resolve callee `@{callee}`")]
    UnknownCallee { callee: String },

    #[error("bitcode writer could not resolve attribute refs {attr_refs:?} in {context}")]
    MissingAttributeList {
        context: String,
        attr_refs: Vec<u32>,
    },

    #[error("bitcode writer could not encode floating constant `{value}` as `{ty}`")]
    InvalidFloatingConstant { ty: Type, value: f64 },

    #[error("bitcode writer could not resolve metadata constant `{ty} {value}`")]
    MissingMetadataConstant { ty: Type, value: i64 },

    #[error("bitcode writer could not resolve metadata node `!{node_id}`")]
    MissingMetadataNode { node_id: u32 },

    #[error("bitcode writer could not resolve module constant for {context}")]
    MissingModuleConstant { context: String },
}

impl WriteError {
    fn unresolved_operand(context: impl Into<String>, operand: &Operand) -> Self {
        Self::UnresolvedOperand {
            context: context.into(),
            operand: format_operand(operand),
        }
    }

    fn missing_basic_block(context: impl Into<String>, block: &str) -> Self {
        Self::MissingBasicBlock {
            context: context.into(),
            block: block.to_string(),
        }
    }
}

fn format_operand(operand: &Operand) -> String {
    match operand {
        Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => format!("%{name}"),
        Operand::IntConst(ty, value) => format!("{ty} {value}"),
        Operand::FloatConst(ty, value) => format!("{ty} {value}"),
        Operand::NullPtr => "null".to_string(),
        Operand::IntToPtr(value, ty) => format!("inttoptr (i64 {value} to {ty})"),
        Operand::GetElementPtr { ptr, .. } => format!("getelementptr from {ptr}"),
        Operand::GlobalRef(name) => format!("@{name}"),
    }
}

/// Writes a `Module` as LLVM bitcode.
pub fn write_bitcode(module: &Module) -> Vec<u8> {
    try_write_bitcode(module)
        .unwrap_or_else(|error| panic!("failed to write LLVM bitcode: {error}"))
}

/// Writes a `Module` as LLVM bitcode for the requested QIR compatibility target.
pub fn write_bitcode_for_target(module: &Module, emit_target: QirEmitTarget) -> Vec<u8> {
    try_write_bitcode_for_target(module, emit_target)
        .unwrap_or_else(|error| panic!("failed to write LLVM bitcode: {error}"))
}

/// Writes a `Module` as LLVM bitcode, returning a structured error if emission fails.
pub fn try_write_bitcode(module: &Module) -> Result<Vec<u8>, WriteError> {
    try_write_bitcode_for_target(module, infer_emit_target(module))
}

/// Writes a `Module` as LLVM bitcode for the requested QIR compatibility target.
pub fn try_write_bitcode_for_target(
    module: &Module,
    emit_target: QirEmitTarget,
) -> Result<Vec<u8>, WriteError> {
    let mut ctx = WriteContext::new(module, emit_target);
    ctx.write()?;
    Ok(ctx.writer.finish())
}

struct TypeTable {
    types: Vec<Type>,
    map: FxHashMap<Type, u32>,
}

impl TypeTable {
    fn new() -> Self {
        Self {
            types: Vec::new(),
            map: FxHashMap::default(),
        }
    }

    fn get_or_insert(&mut self, ty: &Type) -> u32 {
        if let Some(&id) = self.map.get(ty) {
            return id;
        }
        let id = self.types.len() as u32;
        self.types.push(ty.clone());
        self.map.insert(ty.clone(), id);
        id
    }
}

#[derive(Debug, Clone)]
enum MetadataSlotKind {
    String(String),
    Value(Type, i64),
    Node(u32),
}

#[derive(Debug, Clone)]
enum LoweredMetadataValue {
    String(String),
    Int(Type, i64),
    NodeRef(u32),
}

#[derive(Debug, Clone)]
struct LoweredMetadataNode {
    id: u32,
    values: Vec<LoweredMetadataValue>,
}

struct WriteContext<'a> {
    module: &'a Module,
    emit_target: QirEmitTarget,
    writer: BitstreamWriter,
    type_table: TypeTable,
    // Value enumeration: maps (name) -> value_id at module scope
    global_value_ids: FxHashMap<String, u32>,
    next_global_value_id: u32,
    attr_list_table: Vec<Vec<u32>>,
    metadata_slots: Vec<MetadataSlotKind>,
    module_constant_ids: FxHashMap<ModuleConstantKey, u32>,
    module_constants: Vec<(Type, Constant)>,
    module_strtab: Vec<u8>,
    module_function_name_offsets: FxHashMap<String, (u32, u32)>,
    function_word_offsets: FxHashMap<u32, u32>,
    module_vst_offset_placeholder_bit: Option<usize>,
}

/// Describes a constant expression to be emitted in pass 2 of the constants
/// block, after all regular (non-CE) constants have been assigned value IDs.
#[derive(Debug, Clone)]
enum PendingCE {
    IntToPtr {
        val: i64,
    },
    InboundsGep {
        source_ty: Type,
        ptr_name: String,
        indices: Vec<Operand>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ModuleConstantKey {
    Int(Type, i64),
    Float(Type, u64),
    Null(Type),
    CString(Type, String),
}

fn lower_metadata_graph(
    nodes: &[MetadataNode],
) -> (Vec<LoweredMetadataNode>, Vec<LoweredMetadataNode>) {
    let mut synthetic_nodes = Vec::new();
    let mut visible_nodes = Vec::with_capacity(nodes.len());
    let mut next_synthetic_id = SYNTHETIC_METADATA_NODE_START;

    for node in nodes {
        let values =
            lower_metadata_values(&node.values, &mut next_synthetic_id, &mut synthetic_nodes);
        visible_nodes.push(LoweredMetadataNode {
            id: node.id,
            values,
        });
    }

    (synthetic_nodes, visible_nodes)
}

fn lower_metadata_values(
    values: &[MetadataValue],
    next_synthetic_id: &mut u32,
    synthetic_nodes: &mut Vec<LoweredMetadataNode>,
) -> Vec<LoweredMetadataValue> {
    let mut lowered = Vec::with_capacity(values.len());

    for value in values {
        match value {
            MetadataValue::String(text) => {
                lowered.push(LoweredMetadataValue::String(text.clone()));
            }
            MetadataValue::Int(ty, value) => {
                lowered.push(LoweredMetadataValue::Int(ty.clone(), *value));
            }
            MetadataValue::NodeRef(node_id) => {
                lowered.push(LoweredMetadataValue::NodeRef(*node_id));
            }
            MetadataValue::SubList(children) => {
                let synthetic_id = *next_synthetic_id;
                *next_synthetic_id += 1;
                let lowered_children =
                    lower_metadata_values(children, next_synthetic_id, synthetic_nodes);
                synthetic_nodes.push(LoweredMetadataNode {
                    id: synthetic_id,
                    values: lowered_children,
                });
                lowered.push(LoweredMetadataValue::NodeRef(synthetic_id));
            }
        }
    }

    lowered
}

fn encode_metadata_operands(
    values: &[LoweredMetadataValue],
    find_string_slot: &impl Fn(&str) -> Option<usize>,
    find_value_slot: &impl Fn(&Type, i64) -> Option<usize>,
    find_node_slot: &impl Fn(u32) -> Option<usize>,
) -> Result<Vec<u64>, WriteError> {
    let mut operands = Vec::with_capacity(values.len());

    for value in values {
        match value {
            LoweredMetadataValue::String(text) => {
                if let Some(idx) = find_string_slot(text) {
                    operands.push(idx as u64);
                }
            }
            LoweredMetadataValue::Int(ty, value) => {
                if let Some(idx) = find_value_slot(ty, *value) {
                    operands.push(idx as u64);
                }
            }
            LoweredMetadataValue::NodeRef(node_id) => {
                let idx = find_node_slot(*node_id)
                    .ok_or(WriteError::MissingMetadataNode { node_id: *node_id })?;
                operands.push(idx as u64);
            }
        }
    }

    Ok(operands)
}

impl<'a> WriteContext<'a> {
    fn new(module: &'a Module, emit_target: QirEmitTarget) -> Self {
        Self {
            module,
            emit_target,
            writer: BitstreamWriter::new(),
            type_table: TypeTable::new(),
            global_value_ids: FxHashMap::default(),
            next_global_value_id: 0,
            attr_list_table: Vec::new(),
            metadata_slots: Vec::new(),
            module_constant_ids: FxHashMap::default(),
            module_constants: Vec::new(),
            module_strtab: Vec::new(),
            module_function_name_offsets: FxHashMap::default(),
            function_word_offsets: FxHashMap::default(),
            module_vst_offset_placeholder_bit: None,
        }
    }

    fn write(&mut self) -> Result<(), WriteError> {
        // 1. Emit magic
        self.writer.emit_bits(0x42, 8);
        self.writer.emit_bits(0x43, 8);
        self.writer.emit_bits(0xC0, 8);
        self.writer.emit_bits(0xDE, 8);

        // 2. Write identification block
        self.write_identification_block();

        // 3. Collect all types from the module
        self.collect_types();

        // 4. Enumerate global values
        self.enumerate_global_values();

        // Reserve module-scope constant IDs before globals need to reference
        // them as initializers and before metadata needs to reference them.
        self.collect_module_constants()?;

        // 4b. Build the module string table payload used by opaque-lane
        // modern function naming records.
        self.build_module_function_strtab();

        // 5. Enter MODULE_BLOCK
        self.writer
            .enter_subblock(MODULE_BLOCK_ID, ABBREV_WIDTH, TOP_LEVEL_ABBREV_WIDTH);

        // Emit the module layout version that matches the records written
        // below for the selected compatibility lane.
        self.writer.emit_record(
            MODULE_CODE_VERSION,
            &[self.emit_target.module_bitcode_version()],
            ABBREV_WIDTH,
        );

        // 7. Write triple/datalayout if present
        if let Some(ref triple) = self.module.target_triple {
            let chars: Vec<u64> = triple.bytes().map(u64::from).collect();
            self.writer
                .emit_record(MODULE_CODE_TRIPLE, &chars, ABBREV_WIDTH);
        }
        if let Some(ref dl) = self.module.target_datalayout {
            let chars: Vec<u64> = dl.bytes().map(u64::from).collect();
            self.writer
                .emit_record(MODULE_CODE_DATALAYOUT, &chars, ABBREV_WIDTH);
        }
        if let Some(ref sf) = self.module.source_filename {
            let chars: Vec<u64> = sf.bytes().map(u64::from).collect();
            self.writer
                .emit_record(MODULE_CODE_SOURCE_FILENAME, &chars, ABBREV_WIDTH);
        }

        // 7b. Build and write attribute blocks (before type block per LLVM convention)
        self.build_attr_list_table();
        self.write_paramattr_group_block();
        self.write_paramattr_block();

        // 7c. Pre-register metadata constant types so they appear in the type block.
        self.register_metadata_constant_types();

        // 8. Write type block
        self.write_type_block();

        // 9. Write global variables
        self.write_global_vars()?;

        // 10. Write function prototypes (declarations and definition headers)
        self.write_function_protos()?;

        if self.uses_modern_function_naming_container() {
            self.write_module_vst_offset_placeholder();
        }

        // 11. Write module-scope constants after globals and prototypes.
        if !self.module_constants.is_empty() {
            self.write_module_constants_block()?;
        }

        // 12. Build and write metadata block
        self.build_metadata_slots();
        self.write_metadata_block()?;

        // 13. Write function bodies
        for func in &self.module.functions.clone() {
            if !func.is_declaration {
                self.write_function_body(func)?;
            }
        }

        // 14. Write value symbol table
        self.write_value_symtab();

        // 15. Exit MODULE_BLOCK
        self.writer.exit_block(ABBREV_WIDTH);

        if self.uses_modern_function_naming_container() {
            self.write_module_strtab_block();
        }

        Ok(())
    }

    fn uses_modern_function_naming_container(&self) -> bool {
        self.emit_target == QirEmitTarget::QirV2Opaque
    }

    fn build_module_function_strtab(&mut self) {
        self.module_strtab.clear();
        self.module_function_name_offsets.clear();

        if !self.uses_modern_function_naming_container() {
            return;
        }

        for function in &self.module.functions {
            let offset = self.module_strtab.len() as u32;
            let bytes = function.name.as_bytes();
            self.module_strtab.extend_from_slice(bytes);
            self.module_function_name_offsets
                .insert(function.name.clone(), (offset, bytes.len() as u32));
        }
    }

    fn module_function_name_range(&self, name: &str) -> (u32, u32) {
        self.module_function_name_offsets
            .get(name)
            .copied()
            .unwrap_or((0, 0))
    }

    fn write_module_vst_offset_placeholder(&mut self) {
        let abbrev = AbbrevDef {
            operands: vec![
                AbbrevOperand::Literal(u64::from(MODULE_CODE_VSTOFFSET)),
                AbbrevOperand::Fixed(32),
            ],
        };
        let abbrev_id = self.writer.emit_define_abbrev(&abbrev, ABBREV_WIDTH);
        self.writer
            .emit_abbreviated_record(abbrev_id, &[0], ABBREV_WIDTH);
        self.module_vst_offset_placeholder_bit = Some(self.writer.bit_position() - 32);
    }

    fn patch_module_vst_offset(&mut self) {
        let Some(patch_bit_position) = self.module_vst_offset_placeholder_bit.take() else {
            return;
        };

        let vst_bit_position = self.writer.bit_position();
        self.writer
            .patch_u32_bits(patch_bit_position, (vst_bit_position / 32) as u32);
    }

    fn write_module_strtab_block(&mut self) {
        self.writer
            .enter_subblock(STRTAB_BLOCK_ID, ABBREV_WIDTH, TOP_LEVEL_ABBREV_WIDTH);

        let abbrev = AbbrevDef {
            operands: vec![
                AbbrevOperand::Literal(u64::from(STRTAB_BLOB)),
                AbbrevOperand::Blob,
            ],
        };
        let abbrev_id = self.writer.emit_define_abbrev(&abbrev, ABBREV_WIDTH);

        let mut fields = Vec::with_capacity(self.module_strtab.len() + 1);
        fields.push(self.module_strtab.len() as u64);
        fields.extend(self.module_strtab.iter().map(|&byte| u64::from(byte)));
        self.writer
            .emit_abbreviated_record(abbrev_id, &fields, ABBREV_WIDTH);

        self.writer.exit_block(ABBREV_WIDTH);
    }

    fn write_identification_block(&mut self) {
        self.writer
            .enter_subblock(IDENTIFICATION_BLOCK_ID, ABBREV_WIDTH, 2);
        let producer = "qsc_codegen";
        let chars: Vec<u64> = producer.bytes().map(u64::from).collect();
        self.writer
            .emit_record(IDENTIFICATION_CODE_STRING, &chars, ABBREV_WIDTH);
        // epoch = 0 (current)
        self.writer
            .emit_record(IDENTIFICATION_CODE_EPOCH, &[0], ABBREV_WIDTH);
        self.writer.exit_block(ABBREV_WIDTH);
    }

    fn build_attr_list_table(&mut self) {
        fn insert_attr_list(
            attr_list_table: &mut Vec<Vec<u32>>,
            seen: &mut FxHashMap<Vec<u32>, usize>,
            attr_refs: &[u32],
            normalize: bool,
        ) {
            if attr_refs.is_empty() {
                return;
            }

            let mut key = attr_refs.to_vec();
            if normalize {
                key.sort_unstable();
            }

            if !seen.contains_key(&key) {
                let idx = attr_list_table.len();
                attr_list_table.push(key.clone());
                seen.insert(key, idx);
            }
        }

        let mut seen: FxHashMap<Vec<u32>, usize> = FxHashMap::default();
        for f in &self.module.functions {
            insert_attr_list(
                &mut self.attr_list_table,
                &mut seen,
                &f.attribute_group_refs,
                true,
            );

            for block in &f.basic_blocks {
                for instruction in &block.instructions {
                    if let Instruction::Call { attr_refs, .. } = instruction {
                        insert_attr_list(&mut self.attr_list_table, &mut seen, attr_refs, false);
                    }
                }
            }
        }
    }

    fn write_paramattr_group_block(&mut self) {
        // Collect the set of attribute group IDs actually referenced by functions or calls.
        let used_ids: rustc_hash::FxHashSet<u32> =
            self.attr_list_table.iter().flatten().copied().collect();

        let groups: Vec<_> = self
            .module
            .attribute_groups
            .iter()
            .filter(|g| used_ids.contains(&g.id))
            .collect();

        if groups.is_empty() {
            return;
        }

        self.writer
            .enter_subblock(PARAMATTR_GROUP_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        for group in &groups {
            let mut values: Vec<u64> = vec![
                u64::from(group.id),
                0xFFFF_FFFF, // function-level attributes
            ];
            for attr in &group.attributes {
                match attr {
                    Attribute::StringAttr(s) => {
                        values.push(3); // string attr code
                        for ch in s.bytes() {
                            values.push(u64::from(ch));
                        }
                        values.push(0); // null terminator
                    }
                    Attribute::KeyValue(key, val) => {
                        values.push(4); // key/value attr code
                        for ch in key.bytes() {
                            values.push(u64::from(ch));
                        }
                        values.push(0); // null terminator
                        for ch in val.bytes() {
                            values.push(u64::from(ch));
                        }
                        values.push(0); // null terminator
                    }
                }
            }
            self.writer
                .emit_record(PARAMATTR_GRP_CODE_ENTRY, &values, ABBREV_WIDTH);
        }

        self.writer.exit_block(ABBREV_WIDTH);
    }

    fn write_paramattr_block(&mut self) {
        if self.attr_list_table.is_empty() {
            return;
        }

        self.writer
            .enter_subblock(PARAMATTR_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        for group_ids in &self.attr_list_table {
            let values: Vec<u64> = group_ids.iter().map(|&id| u64::from(id)).collect();
            self.writer
                .emit_record(PARAMATTR_CODE_ENTRY, &values, ABBREV_WIDTH);
        }

        self.writer.exit_block(ABBREV_WIDTH);
    }

    fn collect_types(&mut self) {
        // Collect all types used in the module
        for st in &self.module.struct_types {
            if st.is_opaque {
                self.type_table.get_or_insert(&Type::Named(st.name.clone()));
            }
        }

        for g in &self.module.globals {
            self.collect_type(&g.ty);
            // Globals are accessed via pointer
            let global_ptr_ty = self.emit_target.pointer_type_for_pointee(&g.ty);
            self.type_table.get_or_insert(&global_ptr_ty);
        }

        for f in &self.module.functions {
            self.collect_type(&f.return_type);
            for p in &f.params {
                self.collect_type(&p.ty);
            }
            // Function type
            let func_ty = Type::Function(
                Box::new(f.return_type.clone()),
                f.params.iter().map(|p| p.ty.clone()).collect(),
            );
            self.collect_type(&func_ty);
            // Pointer to function
            let function_ptr_ty = self.emit_target.pointer_type_for_pointee(&func_ty);
            self.type_table.get_or_insert(&function_ptr_ty);

            for bb in &f.basic_blocks {
                for instr in &bb.instructions {
                    self.collect_instruction_types(instr);
                }
            }
        }

        // Collect types used in metadata values
        for node in &self.module.metadata_nodes {
            self.collect_metadata_value_types(&node.values);
        }
    }

    fn collect_metadata_value_types(&mut self, values: &[MetadataValue]) {
        for val in values {
            match val {
                MetadataValue::Int(ty, _) => {
                    self.collect_type(ty);
                }
                MetadataValue::SubList(children) => {
                    self.collect_metadata_value_types(children);
                }
                MetadataValue::String(_) | MetadataValue::NodeRef(_) => {}
            }
        }
    }

    fn collect_type(&mut self, ty: &Type) {
        match ty {
            Type::Function(ret, params) => {
                self.collect_type(ret);
                for p in params {
                    self.collect_type(p);
                }
                self.type_table.get_or_insert(ty);
            }
            Type::Array(_, elem) => {
                self.collect_type(elem);
                self.type_table.get_or_insert(ty);
            }
            Type::TypedPtr(inner) => {
                self.collect_type(inner);
                self.type_table.get_or_insert(ty);
            }
            Type::NamedPtr(name) => {
                self.type_table.get_or_insert(&Type::Named(name.clone()));
                self.type_table.get_or_insert(ty);
            }
            _ => {
                self.type_table.get_or_insert(ty);
            }
        }
    }

    fn collect_instruction_types(&mut self, instr: &Instruction) {
        match instr {
            Instruction::Ret(Some(op)) => self.collect_operand_types(op),
            Instruction::Ret(None) => {}
            Instruction::Br { cond_ty, cond, .. } => {
                self.collect_type(cond_ty);
                self.collect_operand_types(cond);
            }
            Instruction::Jump { .. } => {}
            Instruction::BinOp { ty, lhs, rhs, .. } => {
                self.collect_type(ty);
                self.collect_operand_types(lhs);
                self.collect_operand_types(rhs);
            }
            Instruction::ICmp { ty, lhs, rhs, .. } | Instruction::FCmp { ty, lhs, rhs, .. } => {
                self.collect_type(ty);
                self.collect_operand_types(lhs);
                self.collect_operand_types(rhs);
                self.type_table.get_or_insert(&Type::Integer(1));
            }
            Instruction::Cast {
                from_ty,
                to_ty,
                value,
                ..
            } => {
                self.collect_type(from_ty);
                self.collect_type(to_ty);
                self.collect_operand_types(value);
            }
            Instruction::Call {
                return_ty, args, ..
            } => {
                if let Some(ret) = return_ty {
                    self.collect_type(ret);
                }
                for (ty, op) in args {
                    self.collect_type(ty);
                    self.collect_operand_types(op);
                }
            }
            Instruction::Phi { ty, incoming, .. } => {
                self.collect_type(ty);
                for (op, _) in incoming {
                    self.collect_operand_types(op);
                }
            }
            Instruction::Alloca { ty, .. } => {
                self.collect_type(ty);
                let alloca_ptr_ty = self.emit_target.pointer_type_for_pointee(ty);
                self.type_table.get_or_insert(&alloca_ptr_ty);
            }
            Instruction::Load {
                ty, ptr_ty, ptr, ..
            } => {
                self.collect_type(ty);
                self.collect_type(ptr_ty);
                self.collect_operand_types(ptr);
            }
            Instruction::Store {
                ty,
                value,
                ptr_ty,
                ptr,
            } => {
                self.collect_type(ty);
                self.collect_operand_types(value);
                self.collect_type(ptr_ty);
                self.collect_operand_types(ptr);
            }
            Instruction::Select {
                cond,
                true_val,
                false_val,
                ty,
                ..
            } => {
                self.collect_type(&Type::Integer(1));
                self.collect_operand_types(cond);
                self.collect_type(ty);
                self.collect_operand_types(true_val);
                self.collect_operand_types(false_val);
            }
            Instruction::Switch { ty, value, .. } => {
                self.collect_type(ty);
                self.collect_operand_types(value);
            }
            Instruction::GetElementPtr {
                pointee_ty,
                ptr_ty,
                ptr,
                indices,
                ..
            } => {
                self.collect_type(pointee_ty);
                self.collect_type(ptr_ty);
                self.collect_operand_types(ptr);
                for idx in indices {
                    self.collect_operand_types(idx);
                }
            }
            Instruction::Unreachable => {}
        }
    }

    fn collect_operand_types(&mut self, op: &Operand) {
        match op {
            Operand::IntConst(ty, _) => self.collect_type(ty),
            Operand::FloatConst(ty, _) => self.collect_type(ty),
            Operand::NullPtr => {
                let null_ptr_ty = self.emit_target.default_pointer_type();
                self.type_table.get_or_insert(&null_ptr_ty);
            }
            Operand::IntToPtr(_, ty) => {
                self.collect_type(&Type::Integer(64));
                self.collect_type(ty);
            }
            Operand::GetElementPtr {
                ty,
                ptr_ty,
                indices,
                ..
            } => {
                self.collect_type(ty);
                self.collect_type(ptr_ty);
                for index in indices {
                    self.collect_operand_types(index);
                }
            }
            Operand::LocalRef(_) | Operand::TypedLocalRef(_, _) | Operand::GlobalRef(_) => {}
        }
    }

    fn enumerate_global_values(&mut self) {
        // Globals first
        for g in &self.module.globals {
            let id = self.next_global_value_id;
            self.global_value_ids.insert(g.name.clone(), id);
            self.next_global_value_id += 1;
            // Suppress unused variable warning
            let _ = &g.ty;
        }
        // Then functions
        for f in &self.module.functions {
            let id = self.next_global_value_id;
            self.global_value_ids.insert(f.name.clone(), id);
            self.next_global_value_id += 1;
        }
    }

    fn module_constant_key(
        ty: &Type,
        constant: &Constant,
    ) -> Result<ModuleConstantKey, WriteError> {
        match constant {
            Constant::Int(value) => Ok(ModuleConstantKey::Int(ty.clone(), *value)),
            Constant::Float(_, value) => {
                let bits = ty.encode_float_bits(*value).ok_or_else(|| {
                    WriteError::InvalidFloatingConstant {
                        ty: ty.clone(),
                        value: *value,
                    }
                })?;
                Ok(ModuleConstantKey::Float(ty.clone(), bits))
            }
            Constant::Null => Ok(ModuleConstantKey::Null(ty.clone())),
            Constant::CString(text) => Ok(ModuleConstantKey::CString(ty.clone(), text.clone())),
        }
    }

    fn push_module_constant(&mut self, ty: Type, constant: Constant) -> Result<(), WriteError> {
        let key = Self::module_constant_key(&ty, &constant)?;
        if self.module_constant_ids.contains_key(&key) {
            return Ok(());
        }

        let index =
            u32::try_from(self.module_constants.len()).expect("module constant count exceeded u32");
        self.module_constant_ids
            .insert(key, self.next_global_value_id + index);
        self.module_constants.push((ty, constant));
        Ok(())
    }

    fn resolve_module_constant_value_id(
        &self,
        ty: &Type,
        constant: &Constant,
        context: impl Into<String>,
    ) -> Result<u32, WriteError> {
        let key = Self::module_constant_key(ty, constant)?;
        self.module_constant_ids.get(&key).copied().ok_or_else(|| {
            WriteError::MissingModuleConstant {
                context: context.into(),
            }
        })
    }

    fn write_type_block(&mut self) {
        self.writer
            .enter_subblock(TYPE_BLOCK_ID_NEW, ABBREV_WIDTH, ABBREV_WIDTH);

        let num_types = self.type_table.types.len() as u64;
        self.writer
            .emit_record(TYPE_CODE_NUMENTRY, &[num_types], ABBREV_WIDTH);

        let types = self.type_table.types.clone();
        for ty in &types {
            self.write_type_record(ty);
        }

        self.writer.exit_block(ABBREV_WIDTH);
    }

    fn write_type_record(&mut self, ty: &Type) {
        match ty {
            Type::Void => {
                self.writer.emit_record(TYPE_CODE_VOID, &[], ABBREV_WIDTH);
            }
            Type::Integer(width) => {
                self.writer
                    .emit_record(TYPE_CODE_INTEGER, &[u64::from(*width)], ABBREV_WIDTH);
            }
            Type::Half => {
                self.writer.emit_record(TYPE_CODE_HALF, &[], ABBREV_WIDTH);
            }
            Type::Float => {
                self.writer.emit_record(TYPE_CODE_FLOAT, &[], ABBREV_WIDTH);
            }
            Type::Double => {
                self.writer.emit_record(TYPE_CODE_DOUBLE, &[], ABBREV_WIDTH);
            }
            Type::Label => {
                self.writer.emit_record(TYPE_CODE_LABEL, &[], ABBREV_WIDTH);
            }
            Type::Ptr => {
                self.writer
                    .emit_record(TYPE_CODE_OPAQUE_POINTER, &[0], ABBREV_WIDTH);
            }
            Type::Named(name) => {
                // Emit struct name then opaque
                let chars: Vec<u64> = name.bytes().map(u64::from).collect();
                self.writer
                    .emit_record(TYPE_CODE_STRUCT_NAME, &chars, ABBREV_WIDTH);
                self.writer
                    .emit_record(TYPE_CODE_OPAQUE, &[0], ABBREV_WIDTH);
            }
            Type::NamedPtr(name) => {
                if self.emit_target.uses_typed_pointers() {
                    let inner_id = self.type_table.get_or_insert(&Type::Named(name.clone()));
                    self.writer.emit_record(
                        TYPE_CODE_POINTER,
                        &[u64::from(inner_id), 0],
                        ABBREV_WIDTH,
                    );
                } else {
                    self.writer
                        .emit_record(TYPE_CODE_OPAQUE_POINTER, &[0], ABBREV_WIDTH);
                }
            }
            Type::TypedPtr(inner) => {
                if self.emit_target.uses_typed_pointers() {
                    let inner_id = self.type_table.get_or_insert(inner);
                    self.writer.emit_record(
                        TYPE_CODE_POINTER,
                        &[u64::from(inner_id), 0],
                        ABBREV_WIDTH,
                    );
                } else {
                    self.writer
                        .emit_record(TYPE_CODE_OPAQUE_POINTER, &[0], ABBREV_WIDTH);
                }
            }
            Type::Array(len, elem) => {
                let elem_id = self.type_table.get_or_insert(elem);
                self.writer
                    .emit_record(TYPE_CODE_ARRAY, &[*len, u64::from(elem_id)], ABBREV_WIDTH);
            }
            Type::Function(ret, params) => {
                let ret_id = self.type_table.get_or_insert(ret);
                let mut values = vec![0u64, u64::from(ret_id)]; // 0 = not vararg
                for p in params {
                    let p_id = self.type_table.get_or_insert(p);
                    values.push(u64::from(p_id));
                }
                self.writer
                    .emit_record(TYPE_CODE_FUNCTION_TYPE, &values, ABBREV_WIDTH);
            }
        }
    }

    fn write_global_vars(&mut self) -> Result<(), WriteError> {
        for g in &self.module.globals {
            let ty_id = self.type_table.get_or_insert(&g.ty);
            let global_ptr_ty = self.emit_target.pointer_type_for_pointee(&g.ty);
            let ptr_ty_id = self.type_table.get_or_insert(&global_ptr_ty);
            let linkage: u64 = match g.linkage {
                Linkage::External => 0,
                Linkage::Internal => 3,
            };
            let is_const: u64 = u64::from(g.is_constant);
            let init_id = if let Some(initializer) = &g.initializer {
                u64::from(
                    self.resolve_module_constant_value_id(
                        &g.ty,
                        initializer,
                        format!("global initializer @{}", g.name),
                    )? + 1,
                )
            } else {
                0
            };
            // MODULE_CODE_GLOBALVAR: [pointer_type, address_space, is_const, init_id, linkage, alignment, section, ...]
            // We append the actual element type ID as an extra trailing field
            // so our reader can recover the global's element type.
            let values = vec![
                u64::from(ptr_ty_id), // pointer type
                0,                    // address_space
                is_const,
                init_id, // init id (0 = none, otherwise value_id + 1)
                linkage,
                0,                // alignment
                0,                // section
                0,                // visibility
                0,                // thread_local
                0,                // unnamed_addr
                0,                // externally_initialized
                0,                // dso_local
                0,                // comdat
                u64::from(ty_id), // actual element type (our extension)
            ];
            self.writer
                .emit_record(MODULE_CODE_GLOBALVAR, &values, ABBREV_WIDTH);
        }

        Ok(())
    }

    fn write_function_protos(&mut self) -> Result<(), WriteError> {
        for f in &self.module.functions {
            let func_ty = Type::Function(
                Box::new(f.return_type.clone()),
                f.params.iter().map(|p| p.ty.clone()).collect(),
            );
            let func_ty_id = self.type_table.get_or_insert(&func_ty);
            let is_decl: u64 = u64::from(f.is_declaration);
            let linkage: u64 = 0; // external

            // Look up the 1-based paramattr index from the attr list table.
            // 0 means no attributes.
            let paramattr: u64 = if f.attribute_group_refs.is_empty() {
                0
            } else {
                self.resolve_attr_list_index(
                    &f.attribute_group_refs,
                    true,
                    format!("function prototype @{}", f.name),
                )?
            };

            let values = if self.uses_modern_function_naming_container() {
                let (name_offset, name_size) = self.module_function_name_range(&f.name);
                vec![
                    u64::from(name_offset),
                    u64::from(name_size),
                    u64::from(func_ty_id),
                    0,       // calling conv
                    is_decl, // isproto (1 = declaration)
                    linkage,
                    paramattr,
                    0, // alignment
                    0, // section
                    0, // visibility
                    0, // gc
                ]
            } else {
                // MODULE_CODE_FUNCTION: [type, callingconv, isproto, linkage, paramattr, alignment, section, visibility, gc, unnamed_addr, prologuedata, dllstorageclass, comdat, prefixdata, personalityfn, dso_local]
                vec![
                    u64::from(func_ty_id),
                    0,       // calling conv
                    is_decl, // isproto (1 = declaration)
                    linkage,
                    paramattr,
                    0, // alignment
                    0, // section
                    0, // visibility
                    0, // gc
                ]
            };
            self.writer
                .emit_record(MODULE_CODE_FUNCTION, &values, ABBREV_WIDTH);
        }

        Ok(())
    }

    fn register_metadata_constant_types(&mut self) {
        // Pre-register types used by metadata integer constants so they
        // appear in the type block. Does not allocate value IDs.
        for node in &self.module.metadata_nodes.clone() {
            Self::register_metadata_types_from_values(&node.values, &mut self.type_table);
        }
    }

    fn register_metadata_types_from_values(values: &[MetadataValue], type_table: &mut TypeTable) {
        for v in values {
            match v {
                MetadataValue::Int(ty, _) => {
                    type_table.get_or_insert(ty);
                }
                MetadataValue::SubList(sub) => {
                    Self::register_metadata_types_from_values(sub, type_table);
                }
                MetadataValue::String(_) | MetadataValue::NodeRef(_) => {}
            }
        }
    }

    fn collect_module_constants(&mut self) -> Result<(), WriteError> {
        fn visit_metadata_values(
            values: &[MetadataValue],
            writer: &mut WriteContext<'_>,
        ) -> Result<(), WriteError> {
            for v in values {
                match v {
                    MetadataValue::Int(ty, val) => {
                        writer.push_module_constant(ty.clone(), Constant::Int(*val))?;
                    }
                    MetadataValue::SubList(sub) => visit_metadata_values(sub, writer)?,
                    MetadataValue::String(_) | MetadataValue::NodeRef(_) => {}
                }
            }

            Ok(())
        }

        self.module_constant_ids.clear();
        self.module_constants.clear();

        for global in &self.module.globals {
            if let Some(initializer) = &global.initializer {
                self.push_module_constant(global.ty.clone(), initializer.clone())?;
            }
        }

        for node in &self.module.metadata_nodes.clone() {
            visit_metadata_values(&node.values, self)?;
        }

        self.next_global_value_id +=
            u32::try_from(self.module_constants.len()).expect("module constant count exceeded u32");

        Ok(())
    }

    fn write_module_constants_block(&mut self) -> Result<(), WriteError> {
        if self.module_constants.is_empty() {
            return Ok(());
        }

        self.writer
            .enter_subblock(CONSTANTS_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        let mut current_type: Option<u32> = None;

        let module_constants = self.module_constants.clone();
        for (ty, constant) in &module_constants {
            let ty_id = self.type_table.get_or_insert(ty);
            if current_type != Some(ty_id) {
                self.writer
                    .emit_record(CST_CODE_SETTYPE, &[u64::from(ty_id)], ABBREV_WIDTH);
                current_type = Some(ty_id);
            }

            match constant {
                Constant::Int(value) => {
                    let encoded = sign_rotate(*value);
                    self.writer
                        .emit_record(CST_CODE_INTEGER, &[encoded], ABBREV_WIDTH);
                }
                Constant::Float(_, value) => {
                    let bits = ty.encode_float_bits(*value).ok_or_else(|| {
                        WriteError::InvalidFloatingConstant {
                            ty: ty.clone(),
                            value: *value,
                        }
                    });
                    self.writer
                        .emit_record(CST_CODE_FLOAT, &[bits?], ABBREV_WIDTH);
                }
                Constant::Null => {
                    self.writer.emit_record(CST_CODE_NULL, &[], ABBREV_WIDTH);
                }
                Constant::CString(text) => {
                    let chars: Vec<u64> = text.bytes().map(u64::from).collect();
                    self.writer
                        .emit_record(CST_CODE_CSTRING, &chars, ABBREV_WIDTH);
                }
            }
        }

        self.writer.exit_block(ABBREV_WIDTH);

        Ok(())
    }

    fn build_metadata_slots(&mut self) {
        if self.module.metadata_nodes.is_empty() && self.module.named_metadata.is_empty() {
            return;
        }

        // Phase 1: Collect unique strings and values from all metadata nodes,
        // including SubList children.
        let mut string_set: Vec<String> = Vec::new();
        let mut value_set: Vec<(Type, i64)> = Vec::new();

        fn collect_leaf_entries(
            values: &[MetadataValue],
            strings: &mut Vec<String>,
            vals: &mut Vec<(Type, i64)>,
        ) {
            for v in values {
                match v {
                    MetadataValue::String(s) => {
                        if !strings.contains(s) {
                            strings.push(s.clone());
                        }
                    }
                    MetadataValue::Int(ty, val) => {
                        let key = (ty.clone(), *val);
                        if !vals.contains(&key) {
                            vals.push(key);
                        }
                    }
                    MetadataValue::SubList(sub) => {
                        collect_leaf_entries(sub, strings, vals);
                    }
                    MetadataValue::NodeRef(_) => {}
                }
            }
        }

        for node in &self.module.metadata_nodes {
            collect_leaf_entries(&node.values, &mut string_set, &mut value_set);
        }

        // Phase 2: Assign slots in order: strings, then values, then nodes.
        self.metadata_slots.clear();

        for s in &string_set {
            self.metadata_slots
                .push(MetadataSlotKind::String(s.clone()));
        }
        for (ty, val) in &value_set {
            self.metadata_slots
                .push(MetadataSlotKind::Value(ty.clone(), *val));
        }

        let (synthetic_nodes, _) = lower_metadata_graph(&self.module.metadata_nodes);

        // Add synthetic child node slots first
        for node in &synthetic_nodes {
            self.metadata_slots.push(MetadataSlotKind::Node(node.id));
        }

        // Then add visible metadata node slots
        for node in &self.module.metadata_nodes {
            self.metadata_slots.push(MetadataSlotKind::Node(node.id));
        }
    }

    fn write_metadata_block(&mut self) -> Result<(), WriteError> {
        if self.module.metadata_nodes.is_empty() && self.module.named_metadata.is_empty() {
            return Ok(());
        }

        self.writer
            .enter_subblock(METADATA_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        // Build helper structures for slot lookup
        let slots = self.metadata_slots.clone();

        // Helper: find slot index for a string
        let find_string_slot = |s: &str| -> Option<usize> {
            slots
                .iter()
                .position(|slot| matches!(slot, MetadataSlotKind::String(ss) if ss == s))
        };

        // Helper: find slot index for a value
        let find_value_slot = |ty: &Type, val: i64| -> Option<usize> {
            slots.iter().position(
                |slot| matches!(slot, MetadataSlotKind::Value(t, v) if t == ty && *v == val),
            )
        };

        let find_node_slot = |node_id: u32| -> Option<usize> {
            slots
                .iter()
                .position(|slot| matches!(slot, MetadataSlotKind::Node(id) if *id == node_id))
        };

        // Emit METADATA_STRING_OLD records
        for slot in &slots {
            if let MetadataSlotKind::String(s) = slot {
                let chars: Vec<u64> = s.bytes().map(u64::from).collect();
                self.writer
                    .emit_record(METADATA_STRING_OLD, &chars, ABBREV_WIDTH);
            }
        }

        // Emit METADATA_VALUE records
        for slot in &slots {
            if let MetadataSlotKind::Value(ty, val) = slot {
                let type_id = self.type_table.get_or_insert(ty);
                let key = Self::module_constant_key(ty, &Constant::Int(*val))?;
                let value_id = self.module_constant_ids.get(&key).copied().ok_or_else(|| {
                    WriteError::MissingMetadataConstant {
                        ty: ty.clone(),
                        value: *val,
                    }
                })?;
                self.writer.emit_record(
                    METADATA_VALUE,
                    &[u64::from(type_id), u64::from(value_id)],
                    ABBREV_WIDTH,
                );
            }
        }

        let (synthetic_nodes, visible_nodes) = lower_metadata_graph(&self.module.metadata_nodes);

        // Emit synthetic child METADATA_NODE records
        for node in &synthetic_nodes {
            let operands = encode_metadata_operands(
                &node.values,
                &find_string_slot,
                &find_value_slot,
                &find_node_slot,
            )?;
            self.writer
                .emit_record(METADATA_NODE, &operands, ABBREV_WIDTH);
        }

        // Emit visible METADATA_NODE records
        for node in &visible_nodes {
            let operands = encode_metadata_operands(
                &node.values,
                &find_string_slot,
                &find_value_slot,
                &find_node_slot,
            )?;
            self.writer
                .emit_record(METADATA_NODE, &operands, ABBREV_WIDTH);
        }

        // Emit named metadata
        for nm in &self.module.named_metadata {
            // METADATA_NAME
            let name_chars: Vec<u64> = nm.name.bytes().map(u64::from).collect();
            self.writer
                .emit_record(METADATA_NAME, &name_chars, ABBREV_WIDTH);

            // METADATA_NAMED_NODE — slot indexes for referenced visible nodes
            let mut node_slot_refs: Vec<u64> = Vec::new();
            for &node_ref in &nm.node_refs {
                let idx = find_node_slot(node_ref)
                    .ok_or(WriteError::MissingMetadataNode { node_id: node_ref })?;
                node_slot_refs.push(idx as u64);
            }
            self.writer
                .emit_record(METADATA_NAMED_NODE, &node_slot_refs, ABBREV_WIDTH);
        }

        self.writer.exit_block(ABBREV_WIDTH);

        Ok(())
    }

    fn write_function_body(&mut self, func: &Function) -> Result<(), WriteError> {
        if self.uses_modern_function_naming_container()
            && let Some(&value_id) = self.global_value_ids.get(&func.name)
        {
            let function_bit_position = self.writer.bit_position();
            self.function_word_offsets
                .insert(value_id, (function_bit_position / 32) as u32);
        }

        self.writer
            .enter_subblock(FUNCTION_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        // Declare number of basic blocks
        let num_bbs = func.basic_blocks.len() as u64;
        self.writer
            .emit_record(FUNC_CODE_DECLAREBLOCKS, &[num_bbs], ABBREV_WIDTH);

        // Build local value mapping for this function
        let mut local_value_ids = FxHashMap::default();
        let base_value_id = self.next_global_value_id;
        let mut next_value_id: u32 = base_value_id;

        // Parameters get value IDs first
        for p in &func.params {
            if let Some(ref name) = p.name {
                local_value_ids.insert(name.clone(), next_value_id);
            }
            next_value_id += 1;
        }

        // Build basic block name -> index map
        let bb_map = func
            .basic_blocks
            .iter()
            .enumerate()
            .map(|(i, bb)| (bb.name.clone(), i as u32))
            .collect::<FxHashMap<_, _>>();

        // Collect constants used in this function
        let (constants, ces) = self.collect_function_constants(func);
        if !constants.is_empty() || !ces.is_empty() {
            self.write_constants_block(&constants, &ces, &mut local_value_ids, &mut next_value_id)?;
        }

        let mut reserved_value_id = next_value_id;
        Self::reserve_function_result_ids(func, &mut local_value_ids, &mut reserved_value_id);

        // Write instructions
        for bb in &func.basic_blocks {
            for instr in &bb.instructions {
                self.write_instruction(instr, &mut local_value_ids, &mut next_value_id, &bb_map)?;
            }
        }

        // Write value symbol table for function locals
        let named_local_entries =
            Self::collect_named_function_vst_entries(func, &local_value_ids, base_value_id);
        self.write_function_vst(func, &bb_map, &named_local_entries);

        self.writer.exit_block(ABBREV_WIDTH);

        Ok(())
    }

    fn collect_named_function_vst_entries(
        func: &Function,
        local_value_ids: &FxHashMap<String, u32>,
        base_value_id: u32,
    ) -> Vec<(u32, String)> {
        let mut entries = Vec::new();

        for param in &func.params {
            let Some(name) = &param.name else {
                continue;
            };
            let Some(&value_id) = local_value_ids.get(name) else {
                continue;
            };
            entries.push((value_id - base_value_id, name.clone()));
        }

        for block in &func.basic_blocks {
            for instruction in &block.instructions {
                let Some(name) = Self::instruction_result_name(instruction) else {
                    continue;
                };
                let Some(&value_id) = local_value_ids.get(name) else {
                    continue;
                };
                entries.push((value_id - base_value_id, name.to_string()));
            }
        }

        entries
    }

    fn reserve_function_result_ids(
        func: &Function,
        local_value_ids: &mut FxHashMap<String, u32>,
        next_value_id: &mut u32,
    ) {
        for block in &func.basic_blocks {
            for instruction in &block.instructions {
                let Some(name) = Self::instruction_result_name(instruction) else {
                    continue;
                };
                local_value_ids.insert(name.to_string(), *next_value_id);
                *next_value_id += 1;
            }
        }
    }

    fn instruction_result_name(instr: &Instruction) -> Option<&str> {
        match instr {
            Instruction::BinOp { result, .. }
            | Instruction::ICmp { result, .. }
            | Instruction::FCmp { result, .. }
            | Instruction::Cast { result, .. }
            | Instruction::Phi { result, .. }
            | Instruction::Alloca { result, .. }
            | Instruction::Load { result, .. }
            | Instruction::Select { result, .. }
            | Instruction::GetElementPtr { result, .. }
            | Instruction::Call {
                result: Some(result),
                ..
            } => Some(result.as_str()),
            Instruction::Ret(_)
            | Instruction::Call { result: None, .. }
            | Instruction::Br { .. }
            | Instruction::Jump { .. }
            | Instruction::Store { .. }
            | Instruction::Switch { .. }
            | Instruction::Unreachable => None,
        }
    }

    fn collect_function_constants(
        &self,
        func: &Function,
    ) -> (Vec<(Type, Constant)>, Vec<(Type, PendingCE)>) {
        let mut constants: Vec<(Type, Constant)> = Vec::new();
        let mut ces: Vec<(Type, PendingCE)> = Vec::new();
        let mut seen: FxHashMap<(String, String), bool> = FxHashMap::default();

        for bb in &func.basic_blocks {
            for instr in &bb.instructions {
                self.collect_constants_from_instruction(instr, &mut constants, &mut ces, &mut seen);
            }
        }
        (constants, ces)
    }

    fn collect_constants_from_instruction(
        &self,
        instr: &Instruction,
        constants: &mut Vec<(Type, Constant)>,
        ces: &mut Vec<(Type, PendingCE)>,
        seen: &mut FxHashMap<(String, String), bool>,
    ) {
        match instr {
            Instruction::Ret(Some(op)) => {
                self.collect_constants_from_operand(op, constants, ces, seen);
            }
            Instruction::Br { cond, .. } => {
                self.collect_constants_from_operand(cond, constants, ces, seen);
            }
            Instruction::BinOp { lhs, rhs, .. } => {
                self.collect_constants_from_operand(lhs, constants, ces, seen);
                self.collect_constants_from_operand(rhs, constants, ces, seen);
            }
            Instruction::ICmp { lhs, rhs, .. } | Instruction::FCmp { lhs, rhs, .. } => {
                self.collect_constants_from_operand(lhs, constants, ces, seen);
                self.collect_constants_from_operand(rhs, constants, ces, seen);
            }
            Instruction::Cast { value, .. } => {
                self.collect_constants_from_operand(value, constants, ces, seen);
            }
            Instruction::Call { args, .. } => {
                for (_, op) in args {
                    self.collect_constants_from_operand(op, constants, ces, seen);
                }
            }
            Instruction::Phi { incoming, .. } => {
                for (op, _) in incoming {
                    self.collect_constants_from_operand(op, constants, ces, seen);
                }
            }
            Instruction::Load { ptr, .. } => {
                self.collect_constants_from_operand(ptr, constants, ces, seen);
            }
            Instruction::Store { value, ptr, .. } => {
                self.collect_constants_from_operand(value, constants, ces, seen);
                self.collect_constants_from_operand(ptr, constants, ces, seen);
            }
            Instruction::Select {
                cond,
                true_val,
                false_val,
                ..
            } => {
                self.collect_constants_from_operand(cond, constants, ces, seen);
                self.collect_constants_from_operand(true_val, constants, ces, seen);
                self.collect_constants_from_operand(false_val, constants, ces, seen);
            }
            Instruction::Switch { value, .. } => {
                self.collect_constants_from_operand(value, constants, ces, seen);
            }
            Instruction::GetElementPtr { ptr, indices, .. } => {
                self.collect_constants_from_operand(ptr, constants, ces, seen);
                for idx in indices {
                    self.collect_constants_from_operand(idx, constants, ces, seen);
                }
            }
            Instruction::Ret(None)
            | Instruction::Jump { .. }
            | Instruction::Alloca { .. }
            | Instruction::Unreachable => {}
        }
    }

    fn collect_constants_from_operand(
        &self,
        op: &Operand,
        constants: &mut Vec<(Type, Constant)>,
        ces: &mut Vec<(Type, PendingCE)>,
        seen: &mut FxHashMap<(String, String), bool>,
    ) {
        match op {
            Operand::IntConst(ty, val) => {
                let key = (ty.to_string(), format!("int:{val}"));
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                    e.insert(true);
                    constants.push((ty.clone(), Constant::Int(*val)));
                }
            }
            Operand::FloatConst(ty, val) => {
                let bits = ty.encode_float_bits(*val).unwrap_or_else(|| val.to_bits());
                let key = (ty.to_string(), format!("float:{bits}"));
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                    e.insert(true);
                    constants.push((ty.clone(), Constant::float(ty.clone(), *val)));
                }
            }
            Operand::NullPtr => {
                let null_ptr_ty = self.emit_target.default_pointer_type();
                let key = (null_ptr_ty.to_string(), "null".to_string());
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                    e.insert(true);
                    constants.push((null_ptr_ty, Constant::Null));
                }
            }
            Operand::IntToPtr(val, ty) => {
                // Collect the integer constant for the CE's source operand
                let int_ty = Type::Integer(64);
                let key = (int_ty.to_string(), format!("int:{val}"));
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                    e.insert(true);
                    constants.push((int_ty, Constant::Int(*val)));
                }
                // Track CE for pass 2 emission
                let ce_key = ("ce".to_string(), format!("inttoptr:{val}:{ty}"));
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(ce_key) {
                    e.insert(true);
                    ces.push((ty.clone(), PendingCE::IntToPtr { val: *val }));
                }
            }
            Operand::GetElementPtr {
                ty: source_ty,
                ptr: ptr_name,
                indices,
                ..
            } => {
                // Collect index constants for the CE's source operands
                for idx in indices {
                    self.collect_constants_from_operand(idx, constants, ces, seen);
                }
                // Track CE for pass 2 emission
                let idx_desc = indices
                    .iter()
                    .map(|i| format!("{i:?}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let ce_key = (
                    "ce".to_string(),
                    format!("gep:{source_ty}:{ptr_name}:{idx_desc}"),
                );
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(ce_key) {
                    e.insert(true);
                    ces.push((
                        Type::Ptr,
                        PendingCE::InboundsGep {
                            source_ty: source_ty.clone(),
                            ptr_name: ptr_name.clone(),
                            indices: indices.clone(),
                        },
                    ));
                }
            }
            Operand::LocalRef(_) | Operand::TypedLocalRef(_, _) | Operand::GlobalRef(_) => {}
        }
    }

    fn write_constants_block(
        &mut self,
        constants: &[(Type, Constant)],
        pending_ces: &[(Type, PendingCE)],
        local_value_ids: &mut FxHashMap<String, u32>,
        next_value_id: &mut u32,
    ) -> Result<(), WriteError> {
        self.writer
            .enter_subblock(CONSTANTS_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        let mut current_type: Option<u32> = None;

        // Pass 1: Emit regular constants
        for (ty, cst) in constants {
            let ty_id = self.type_table.get_or_insert(ty);

            // Emit SETTYPE if type changed
            if current_type != Some(ty_id) {
                self.writer
                    .emit_record(CST_CODE_SETTYPE, &[u64::from(ty_id)], ABBREV_WIDTH);
                current_type = Some(ty_id);
            }

            match cst {
                Constant::Int(val) => {
                    let encoded = sign_rotate(*val);
                    self.writer
                        .emit_record(CST_CODE_INTEGER, &[encoded], ABBREV_WIDTH);
                    let name = format!("__const_int_{ty}_{val}");
                    local_value_ids.insert(name, *next_value_id);
                }
                Constant::Float(float_ty, val) => {
                    let bits = float_ty.encode_float_bits(*val).ok_or_else(|| {
                        WriteError::InvalidFloatingConstant {
                            ty: float_ty.clone(),
                            value: *val,
                        }
                    })?;
                    self.writer
                        .emit_record(CST_CODE_FLOAT, &[bits], ABBREV_WIDTH);
                    let name = format!("__const_float_{float_ty}_{bits}");
                    local_value_ids.insert(name, *next_value_id);
                }
                Constant::Null => {
                    self.writer.emit_record(CST_CODE_NULL, &[], ABBREV_WIDTH);
                    let name = format!("__const_null_{ty}");
                    local_value_ids.insert(name, *next_value_id);
                }
                Constant::CString(_) => {
                    // CString constants are handled as global initializers, not in constant blocks
                }
            }
            *next_value_id += 1;
        }

        // Pass 2: Emit constant expressions with absolute value IDs
        for (result_ty, ce) in pending_ces {
            let result_ty_id = self.type_table.get_or_insert(result_ty);
            if current_type != Some(result_ty_id) {
                self.writer
                    .emit_record(CST_CODE_SETTYPE, &[u64::from(result_ty_id)], ABBREV_WIDTH);
                current_type = Some(result_ty_id);
            }

            match ce {
                PendingCE::IntToPtr { val } => {
                    let src_type = Type::Integer(64);
                    let src_type_id = self.type_table.get_or_insert(&src_type);
                    let src_key = format!("__const_int_{src_type}_{val}");
                    let src_value_id = local_value_ids.get(&src_key).copied().ok_or_else(|| {
                        WriteError::unresolved_operand(
                            "inttoptr constant expression source",
                            &Operand::IntConst(src_type.clone(), *val),
                        )
                    })?;
                    self.writer.emit_record(
                        CST_CODE_CE_CAST,
                        &[10, u64::from(src_type_id), u64::from(src_value_id)],
                        ABBREV_WIDTH,
                    );
                    let ce_key = format!("__ce_inttoptr_{val}_{result_ty}");
                    local_value_ids.insert(ce_key, *next_value_id);
                }
                PendingCE::InboundsGep {
                    source_ty,
                    ptr_name,
                    indices,
                } => {
                    let source_ty_id = self.type_table.get_or_insert(source_ty);
                    let gep_ptr_ty = self.emit_target.pointer_type_for_pointee(source_ty);
                    let ptr_type_id = self.type_table.get_or_insert(&gep_ptr_ty);
                    let ptr_value_id =
                        self.global_value_ids
                            .get(ptr_name)
                            .copied()
                            .ok_or_else(|| {
                                WriteError::unresolved_operand(
                                    "getelementptr constant expression base pointer",
                                    &Operand::GlobalRef(ptr_name.clone()),
                                )
                            })?;
                    // Record format (odd length → first is pointee type):
                    // [pointee_type_id, ptr_type_id, ptr_value_id, idx_type, idx_val, ...]
                    let mut record = vec![
                        u64::from(source_ty_id),
                        u64::from(ptr_type_id),
                        u64::from(ptr_value_id),
                    ];
                    for idx in indices {
                        if let Operand::IntConst(idx_ty, idx_val) = idx {
                            let idx_type_id = self.type_table.get_or_insert(idx_ty);
                            let idx_key = format!("__const_int_{idx_ty}_{idx_val}");
                            let idx_value_id =
                                local_value_ids.get(&idx_key).copied().ok_or_else(|| {
                                    WriteError::unresolved_operand(
                                        "getelementptr constant expression index",
                                        idx,
                                    )
                                })?;
                            record.push(u64::from(idx_type_id));
                            record.push(u64::from(idx_value_id));
                        }
                    }
                    self.writer
                        .emit_record(CST_CODE_CE_INBOUNDS_GEP, &record, ABBREV_WIDTH);
                    let ce_key = Self::gep_ce_key(source_ty, ptr_name, indices);
                    local_value_ids.insert(ce_key, *next_value_id);
                }
            }
            *next_value_id += 1;
        }

        self.writer.exit_block(ABBREV_WIDTH);

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn write_instruction(
        &mut self,
        instr: &Instruction,
        local_value_ids: &mut FxHashMap<String, u32>,
        next_value_id: &mut u32,
        bb_map: &FxHashMap<String, u32>,
    ) -> Result<(), WriteError> {
        match instr {
            Instruction::Ret(None) => {
                self.writer
                    .emit_record(FUNC_CODE_INST_RET, &[], ABBREV_WIDTH);
            }
            Instruction::Ret(Some(op)) => {
                let val = self.resolve_operand(
                    "return instruction",
                    op,
                    local_value_ids,
                    *next_value_id,
                )?;
                self.writer
                    .emit_record(FUNC_CODE_INST_RET, &[val], ABBREV_WIDTH);
            }
            Instruction::Jump { dest } => {
                let bb_id = bb_map
                    .get(dest)
                    .copied()
                    .ok_or_else(|| WriteError::missing_basic_block("unconditional branch", dest))?;
                self.writer
                    .emit_record(FUNC_CODE_INST_BR, &[u64::from(bb_id)], ABBREV_WIDTH);
            }
            Instruction::Br {
                cond,
                true_dest,
                false_dest,
                ..
            } => {
                let true_id = bb_map.get(true_dest).copied().ok_or_else(|| {
                    WriteError::missing_basic_block(
                        "conditional branch true destination",
                        true_dest,
                    )
                })?;
                let false_id = bb_map.get(false_dest).copied().ok_or_else(|| {
                    WriteError::missing_basic_block(
                        "conditional branch false destination",
                        false_dest,
                    )
                })?;
                let cond_val = self.resolve_operand(
                    "conditional branch condition",
                    cond,
                    local_value_ids,
                    *next_value_id,
                )?;
                self.writer.emit_record(
                    FUNC_CODE_INST_BR,
                    &[u64::from(true_id), u64::from(false_id), cond_val],
                    ABBREV_WIDTH,
                );
            }
            Instruction::BinOp {
                op,
                lhs,
                rhs,
                result,
                ..
            } => {
                let lhs_val = self.resolve_operand(
                    "binary operation lhs",
                    lhs,
                    local_value_ids,
                    *next_value_id,
                )?;
                let rhs_val = self.resolve_operand(
                    "binary operation rhs",
                    rhs,
                    local_value_ids,
                    *next_value_id,
                )?;
                let opcode = binop_to_opcode(op);
                self.writer.emit_record(
                    FUNC_CODE_INST_BINOP,
                    &[lhs_val, rhs_val, opcode],
                    ABBREV_WIDTH,
                );
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::ICmp {
                pred,
                lhs,
                rhs,
                result,
                ..
            } => {
                let lhs_val = self.resolve_operand(
                    "integer comparison lhs",
                    lhs,
                    local_value_ids,
                    *next_value_id,
                )?;
                let rhs_val = self.resolve_operand(
                    "integer comparison rhs",
                    rhs,
                    local_value_ids,
                    *next_value_id,
                )?;
                let pred_code = icmp_predicate_code(pred);
                self.writer.emit_record(
                    FUNC_CODE_INST_CMP2,
                    &[lhs_val, rhs_val, pred_code],
                    ABBREV_WIDTH,
                );
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::FCmp {
                pred,
                lhs,
                rhs,
                result,
                ..
            } => {
                let lhs_val = self.resolve_operand(
                    "floating comparison lhs",
                    lhs,
                    local_value_ids,
                    *next_value_id,
                )?;
                let rhs_val = self.resolve_operand(
                    "floating comparison rhs",
                    rhs,
                    local_value_ids,
                    *next_value_id,
                )?;
                let pred_code = fcmp_predicate_code(pred);
                self.writer.emit_record(
                    FUNC_CODE_INST_CMP2,
                    &[lhs_val, rhs_val, pred_code],
                    ABBREV_WIDTH,
                );
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::Cast {
                op: cast_op,
                to_ty,
                value,
                result,
                ..
            } => {
                let val =
                    self.resolve_operand("cast operand", value, local_value_ids, *next_value_id)?;
                let to_ty_id = self.type_table.get_or_insert(to_ty);
                let cast_opcode = cast_to_opcode(cast_op);
                self.writer.emit_record(
                    FUNC_CODE_INST_CAST,
                    &[val, u64::from(to_ty_id), cast_opcode],
                    ABBREV_WIDTH,
                );
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::Call {
                callee,
                args,
                result,
                attr_refs,
                ..
            } => {
                let call_context = format!("call instruction @{callee}");
                let func_ty = self.get_function_type(callee)?;
                let callee_operand = Operand::GlobalRef(callee.clone());
                let callee_val = self.resolve_operand(
                    call_context.clone(),
                    &callee_operand,
                    local_value_ids,
                    *next_value_id,
                )?;
                let func_ty_id = self.type_table.get_or_insert(&func_ty);
                let packed_call_cc_info = if self.emit_target == QirEmitTarget::QirV2Opaque {
                    CALL_EXPLICIT_TYPE_FLAG
                } else {
                    0
                };
                let paramattr = if attr_refs.is_empty() {
                    0
                } else {
                    self.resolve_attr_list_index(attr_refs, false, call_context.clone())?
                };

                // Opaque-pointer CALL records must set the explicit function-type flag
                // in the packed cc-info operand so external LLVM decodes the callee slot
                // using the modern layout.
                let mut values = vec![
                    paramattr,
                    packed_call_cc_info,
                    u64::from(func_ty_id), // function type
                    callee_val,            // callee value ID
                ];
                for (_, op) in args {
                    let arg_val = self.resolve_operand(
                        call_context.clone(),
                        op,
                        local_value_ids,
                        *next_value_id,
                    )?;
                    values.push(arg_val);
                }
                self.writer
                    .emit_record(FUNC_CODE_INST_CALL, &values, ABBREV_WIDTH);

                if let Some(res) = result {
                    local_value_ids.insert(res.clone(), *next_value_id);
                    *next_value_id += 1;
                }
            }
            Instruction::Phi {
                ty,
                incoming,
                result,
            } => {
                let ty_id = self.type_table.get_or_insert(ty);
                let mut values = vec![u64::from(ty_id)];
                for (op, bb_name) in incoming {
                    let val = self.resolve_phi_operand(
                        "phi incoming value",
                        op,
                        local_value_ids,
                        *next_value_id,
                    )?;
                    let bb_id = bb_map.get(bb_name).copied().ok_or_else(|| {
                        WriteError::missing_basic_block("phi incoming edge", bb_name)
                    })?;
                    values.push(val);
                    values.push(u64::from(bb_id));
                }
                self.writer
                    .emit_record(FUNC_CODE_INST_PHI, &values, ABBREV_WIDTH);
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::Alloca { ty, result } => {
                let ty_id = self.type_table.get_or_insert(ty);
                let alloca_ptr_ty = self.emit_target.pointer_type_for_pointee(ty);
                let ptr_ty_id = self.type_table.get_or_insert(&alloca_ptr_ty);
                // ALLOCA: [instty, opty, op, align]
                let i32_ty_id = self.type_table.get_or_insert(&Type::Integer(32));
                self.writer.emit_record(
                    FUNC_CODE_INST_ALLOCA,
                    &[
                        u64::from(ty_id),
                        u64::from(i32_ty_id),
                        0,
                        u64::from(ptr_ty_id),
                    ],
                    ABBREV_WIDTH,
                );
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::Load {
                ty, ptr, result, ..
            } => {
                let ptr_val =
                    self.resolve_operand("load pointer", ptr, local_value_ids, *next_value_id)?;
                let ty_id = self.type_table.get_or_insert(ty);
                // LOAD: [opty, op, ty, align, vol]
                self.writer.emit_record(
                    FUNC_CODE_INST_LOAD,
                    &[ptr_val, u64::from(ty_id), 0, 0],
                    ABBREV_WIDTH,
                );
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::Store { value, ptr, .. } => {
                let ptr_val =
                    self.resolve_operand("store pointer", ptr, local_value_ids, *next_value_id)?;
                let val =
                    self.resolve_operand("store value", value, local_value_ids, *next_value_id)?;
                // STORE: [ptrty, ptr, valty, val, align, vol]
                self.writer
                    .emit_record(FUNC_CODE_INST_STORE, &[ptr_val, val, 0, 0], ABBREV_WIDTH);
            }
            Instruction::Select {
                cond,
                true_val,
                false_val,
                result,
                ..
            } => {
                let true_v = self.resolve_operand(
                    "select true value",
                    true_val,
                    local_value_ids,
                    *next_value_id,
                )?;
                let false_v = self.resolve_operand(
                    "select false value",
                    false_val,
                    local_value_ids,
                    *next_value_id,
                )?;
                let cond_v = self.resolve_operand(
                    "select condition",
                    cond,
                    local_value_ids,
                    *next_value_id,
                )?;
                self.writer.emit_record(
                    FUNC_CODE_INST_SELECT,
                    &[true_v, false_v, cond_v],
                    ABBREV_WIDTH,
                );
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
            Instruction::Switch {
                ty,
                value,
                default_dest,
                cases,
            } => {
                let ty_id = self.type_table.get_or_insert(ty);
                let val = self.resolve_operand(
                    "switch selector",
                    value,
                    local_value_ids,
                    *next_value_id,
                )?;
                let default_id = bb_map.get(default_dest).copied().ok_or_else(|| {
                    WriteError::missing_basic_block("switch default destination", default_dest)
                })?;
                let mut values = vec![u64::from(ty_id), val, u64::from(default_id)];
                for (case_val, dest) in cases {
                    values.push(sign_rotate(*case_val));
                    let dest_id = bb_map.get(dest).copied().ok_or_else(|| {
                        WriteError::missing_basic_block("switch case destination", dest)
                    })?;
                    values.push(u64::from(dest_id));
                }
                self.writer
                    .emit_record(FUNC_CODE_INST_SWITCH, &values, ABBREV_WIDTH);
            }
            Instruction::Unreachable => {
                self.writer
                    .emit_record(FUNC_CODE_INST_UNREACHABLE, &[], ABBREV_WIDTH);
            }
            Instruction::GetElementPtr {
                inbounds,
                pointee_ty,
                ptr,
                indices,
                result,
                ..
            } => {
                let inbounds_flag = u64::from(*inbounds);
                let pointee_type_id = self.type_table.get_or_insert(pointee_ty);
                let ptr_val = self.resolve_operand(
                    "getelementptr base pointer",
                    ptr,
                    local_value_ids,
                    *next_value_id,
                )?;
                let mut values = vec![inbounds_flag, u64::from(pointee_type_id), ptr_val];
                for idx in indices {
                    let idx_val = self.resolve_operand(
                        "getelementptr index",
                        idx,
                        local_value_ids,
                        *next_value_id,
                    )?;
                    values.push(idx_val);
                }
                self.writer
                    .emit_record(FUNC_CODE_INST_GEP, &values, ABBREV_WIDTH);
                local_value_ids.insert(result.clone(), *next_value_id);
                *next_value_id += 1;
            }
        }

        Ok(())
    }

    fn resolve_operand(
        &self,
        context: impl Into<String>,
        op: &Operand,
        local_value_ids: &FxHashMap<String, u32>,
        current_value_id: u32,
    ) -> Result<u64, WriteError> {
        let context = context.into();
        let id = self.resolve_operand_value_id(&context, local_value_ids, op)?;
        current_value_id
            .checked_sub(id)
            .map(u64::from)
            .ok_or_else(|| WriteError::unresolved_operand(context, op))
    }

    fn resolve_phi_operand(
        &self,
        context: impl Into<String>,
        op: &Operand,
        local_value_ids: &FxHashMap<String, u32>,
        current_value_id: u32,
    ) -> Result<u64, WriteError> {
        let context = context.into();
        let id = self.resolve_operand_value_id(&context, local_value_ids, op)?;
        Ok(sign_rotate(i64::from(current_value_id) - i64::from(id)))
    }

    fn resolve_operand_value_id(
        &self,
        context: &str,
        local_value_ids: &FxHashMap<String, u32>,
        op: &Operand,
    ) -> Result<u32, WriteError> {
        let value_id = match op {
            Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => {
                local_value_ids.get(name).copied()
            }
            Operand::GlobalRef(name) => self.global_value_ids.get(name).copied(),
            Operand::IntConst(ty, val) => {
                let key = format!("__const_int_{ty}_{val}");
                local_value_ids.get(&key).copied()
            }
            Operand::FloatConst(ty, val) => {
                let bits = ty.encode_float_bits(*val).ok_or_else(|| {
                    WriteError::InvalidFloatingConstant {
                        ty: ty.clone(),
                        value: *val,
                    }
                })?;
                let key = format!("__const_float_{ty}_{bits}");
                local_value_ids.get(&key).copied()
            }
            Operand::NullPtr => {
                let key = format!("__const_null_{}", self.emit_target.default_pointer_type());
                local_value_ids.get(&key).copied()
            }
            Operand::IntToPtr(val, ty) => {
                let key = format!("__ce_inttoptr_{val}_{ty}");
                local_value_ids.get(&key).copied()
            }
            Operand::GetElementPtr {
                ty: source_ty,
                ptr: ptr_name,
                indices,
                ..
            } => {
                let key = Self::gep_ce_key(source_ty, ptr_name, indices);
                local_value_ids.get(&key).copied()
            }
        };

        value_id.ok_or_else(|| WriteError::unresolved_operand(context.to_string(), op))
    }

    fn gep_ce_key(source_ty: &Type, ptr_name: &str, indices: &[Operand]) -> String {
        let idx_desc = indices
            .iter()
            .map(|i| match i {
                Operand::IntConst(ty, val) => format!("{ty}:{val}"),
                _ => "?".to_string(),
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("__ce_gep_{source_ty}_{ptr_name}_{idx_desc}")
    }

    fn get_function_type(&self, name: &str) -> Result<Type, WriteError> {
        for f in &self.module.functions {
            if f.name == name {
                return Ok(Type::Function(
                    Box::new(f.return_type.clone()),
                    f.params.iter().map(|p| p.ty.clone()).collect(),
                ));
            }
        }
        Err(WriteError::UnknownCallee {
            callee: name.to_string(),
        })
    }

    fn resolve_attr_list_index(
        &self,
        attr_refs: &[u32],
        normalize: bool,
        context: String,
    ) -> Result<u64, WriteError> {
        let mut key = attr_refs.to_vec();
        if normalize {
            key.sort_unstable();
        }

        self.attr_list_table
            .iter()
            .position(|entry| *entry == key)
            .map(|idx| (idx + 1) as u64)
            .ok_or_else(|| WriteError::MissingAttributeList {
                context,
                attr_refs: attr_refs.to_vec(),
            })
    }

    fn write_value_symtab(&mut self) {
        if self.uses_modern_function_naming_container() {
            self.patch_module_vst_offset();

            self.writer
                .enter_subblock(VALUE_SYMTAB_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

            for global in &self.module.globals {
                let Some(&value_id) = self.global_value_ids.get(&global.name) else {
                    continue;
                };

                let mut values = vec![u64::from(value_id)];
                values.extend(global.name.bytes().map(u64::from));
                self.writer
                    .emit_record(VST_CODE_ENTRY, &values, ABBREV_WIDTH);
            }

            for function in &self.module.functions {
                if function.is_declaration {
                    continue;
                }

                let Some(&value_id) = self.global_value_ids.get(&function.name) else {
                    continue;
                };
                let Some(&function_word_offset) = self.function_word_offsets.get(&value_id) else {
                    continue;
                };

                let mut values = vec![u64::from(value_id), u64::from(function_word_offset)];
                values.extend(function.name.bytes().map(u64::from));
                self.writer
                    .emit_record(VST_CODE_FNENTRY, &values, ABBREV_WIDTH);
            }

            self.writer.exit_block(ABBREV_WIDTH);
            return;
        }

        let mut entries: Vec<(u32, String)> = Vec::new();

        // Add global names
        for g in &self.module.globals {
            if let Some(&id) = self.global_value_ids.get(&g.name) {
                entries.push((id, g.name.clone()));
            }
        }
        for f in &self.module.functions {
            if let Some(&id) = self.global_value_ids.get(&f.name) {
                entries.push((id, f.name.clone()));
            }
        }

        if entries.is_empty() {
            return;
        }

        self.writer
            .enter_subblock(VALUE_SYMTAB_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        for (id, name) in &entries {
            let mut values: Vec<u64> = vec![u64::from(*id)];
            for b in name.bytes() {
                values.push(u64::from(b));
            }
            self.writer
                .emit_record(VST_CODE_ENTRY, &values, ABBREV_WIDTH);
        }

        self.writer.exit_block(ABBREV_WIDTH);
    }

    fn write_function_vst(
        &mut self,
        func: &Function,
        bb_map: &FxHashMap<String, u32>,
        named_local_entries: &[(u32, String)],
    ) {
        // Write basic block entries in function VST
        let has_named_bbs = func.basic_blocks.iter().any(|bb| !bb.name.is_empty());
        if named_local_entries.is_empty() && !has_named_bbs {
            return;
        }

        self.writer
            .enter_subblock(VALUE_SYMTAB_BLOCK_ID, ABBREV_WIDTH, ABBREV_WIDTH);

        for (local_id, name) in named_local_entries {
            let mut values: Vec<u64> = vec![u64::from(*local_id)];
            for b in name.bytes() {
                values.push(u64::from(b));
            }
            self.writer
                .emit_record(VST_CODE_ENTRY, &values, ABBREV_WIDTH);
        }

        for bb in &func.basic_blocks {
            if !bb.name.is_empty()
                && let Some(&bb_id) = bb_map.get(&bb.name)
            {
                let mut values: Vec<u64> = vec![u64::from(bb_id)];
                for b in bb.name.bytes() {
                    values.push(u64::from(b));
                }
                self.writer
                    .emit_record(VST_CODE_BBENTRY, &values, ABBREV_WIDTH);
            }
        }

        self.writer.exit_block(ABBREV_WIDTH);
    }
}

fn infer_emit_target(module: &Module) -> QirEmitTarget {
    if let Some(MetadataValue::Int(_, major_version)) = module.get_flag(QIR_MAJOR_VERSION_KEY) {
        return match *major_version {
            1 => QirEmitTarget::QirV1Typed,
            _ => QirEmitTarget::QirV2Opaque,
        };
    }

    if module_contains_typed_pointers(module) {
        QirEmitTarget::QirV1Typed
    } else {
        QirEmitTarget::QirV2Opaque
    }
}

fn module_contains_typed_pointers(module: &Module) -> bool {
    module
        .globals
        .iter()
        .any(|global| type_contains_typed_pointers(&global.ty))
        || module
            .functions
            .iter()
            .any(function_contains_typed_pointers)
}

fn function_contains_typed_pointers(function: &Function) -> bool {
    type_contains_typed_pointers(&function.return_type)
        || function
            .params
            .iter()
            .any(|param| type_contains_typed_pointers(&param.ty))
        || function
            .basic_blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .any(instruction_contains_typed_pointers)
}

fn instruction_contains_typed_pointers(instruction: &Instruction) -> bool {
    match instruction {
        Instruction::Ret(Some(operand)) => operand_contains_typed_pointers(operand),
        Instruction::Ret(None) | Instruction::Jump { .. } | Instruction::Unreachable => false,
        Instruction::Br { cond_ty, cond, .. } => {
            type_contains_typed_pointers(cond_ty) || operand_contains_typed_pointers(cond)
        }
        Instruction::BinOp { ty, lhs, rhs, .. }
        | Instruction::ICmp { ty, lhs, rhs, .. }
        | Instruction::FCmp { ty, lhs, rhs, .. } => {
            type_contains_typed_pointers(ty)
                || operand_contains_typed_pointers(lhs)
                || operand_contains_typed_pointers(rhs)
        }
        Instruction::Cast {
            from_ty,
            to_ty,
            value,
            ..
        } => {
            type_contains_typed_pointers(from_ty)
                || type_contains_typed_pointers(to_ty)
                || operand_contains_typed_pointers(value)
        }
        Instruction::Call {
            return_ty, args, ..
        } => {
            return_ty.as_ref().is_some_and(type_contains_typed_pointers)
                || args.iter().any(|(ty, operand)| {
                    type_contains_typed_pointers(ty) || operand_contains_typed_pointers(operand)
                })
        }
        Instruction::Phi { ty, incoming, .. } => {
            type_contains_typed_pointers(ty)
                || incoming
                    .iter()
                    .any(|(operand, _)| operand_contains_typed_pointers(operand))
        }
        Instruction::Alloca { ty, .. } => type_contains_typed_pointers(ty),
        Instruction::Load {
            ty, ptr_ty, ptr, ..
        } => {
            type_contains_typed_pointers(ty)
                || type_contains_typed_pointers(ptr_ty)
                || operand_contains_typed_pointers(ptr)
        }
        Instruction::Store {
            ty,
            value,
            ptr_ty,
            ptr,
        } => {
            type_contains_typed_pointers(ty)
                || operand_contains_typed_pointers(value)
                || type_contains_typed_pointers(ptr_ty)
                || operand_contains_typed_pointers(ptr)
        }
        Instruction::Select {
            cond,
            true_val,
            false_val,
            ty,
            ..
        } => {
            operand_contains_typed_pointers(cond)
                || operand_contains_typed_pointers(true_val)
                || operand_contains_typed_pointers(false_val)
                || type_contains_typed_pointers(ty)
        }
        Instruction::Switch { ty, value, .. } => {
            type_contains_typed_pointers(ty) || operand_contains_typed_pointers(value)
        }
        Instruction::GetElementPtr {
            pointee_ty,
            ptr_ty,
            ptr,
            indices,
            ..
        } => {
            type_contains_typed_pointers(pointee_ty)
                || type_contains_typed_pointers(ptr_ty)
                || operand_contains_typed_pointers(ptr)
                || indices.iter().any(operand_contains_typed_pointers)
        }
    }
}

fn operand_contains_typed_pointers(operand: &Operand) -> bool {
    match operand {
        Operand::LocalRef(_) | Operand::GlobalRef(_) | Operand::NullPtr => false,
        Operand::TypedLocalRef(_, ty) | Operand::IntConst(ty, _) | Operand::FloatConst(ty, _) => {
            type_contains_typed_pointers(ty)
        }
        Operand::IntToPtr(_, ty) => type_contains_typed_pointers(ty),
        Operand::GetElementPtr {
            ty,
            ptr_ty,
            indices,
            ..
        } => {
            type_contains_typed_pointers(ty)
                || type_contains_typed_pointers(ptr_ty)
                || indices.iter().any(operand_contains_typed_pointers)
        }
    }
}

fn type_contains_typed_pointers(ty: &Type) -> bool {
    match ty {
        Type::NamedPtr(_) | Type::TypedPtr(_) => true,
        Type::Array(_, element) => type_contains_typed_pointers(element),
        Type::Function(result, params) => {
            type_contains_typed_pointers(result) || params.iter().any(type_contains_typed_pointers)
        }
        Type::Void
        | Type::Integer(_)
        | Type::Half
        | Type::Float
        | Type::Double
        | Type::Label
        | Type::Ptr
        | Type::Named(_) => false,
    }
}

fn sign_rotate(val: i64) -> u64 {
    let magnitude = val.unsigned_abs();
    if val >= 0 {
        magnitude << 1
    } else {
        (magnitude << 1) | 1
    }
}

fn binop_to_opcode(op: &BinOpKind) -> u64 {
    match op {
        BinOpKind::Add | BinOpKind::Fadd => 0,
        BinOpKind::Sub | BinOpKind::Fsub => 1,
        BinOpKind::Mul | BinOpKind::Fmul => 2,
        BinOpKind::Udiv => 3,
        BinOpKind::Sdiv | BinOpKind::Fdiv => 4,
        BinOpKind::Urem => 5,
        BinOpKind::Srem => 6,
        BinOpKind::Shl => 7,
        BinOpKind::Lshr => 8,
        BinOpKind::Ashr => 9,
        BinOpKind::And => 10,
        BinOpKind::Or => 11,
        BinOpKind::Xor => 12,
    }
}

fn icmp_predicate_code(pred: &IntPredicate) -> u64 {
    match pred {
        IntPredicate::Eq => 32,
        IntPredicate::Ne => 33,
        IntPredicate::Ugt => 34,
        IntPredicate::Uge => 35,
        IntPredicate::Ult => 36,
        IntPredicate::Ule => 37,
        IntPredicate::Sgt => 38,
        IntPredicate::Sge => 39,
        IntPredicate::Slt => 40,
        IntPredicate::Sle => 41,
    }
}

fn fcmp_predicate_code(pred: &FloatPredicate) -> u64 {
    match pred {
        FloatPredicate::Oeq => 1,
        FloatPredicate::Ogt => 2,
        FloatPredicate::Oge => 3,
        FloatPredicate::Olt => 4,
        FloatPredicate::Ole => 5,
        FloatPredicate::One => 6,
        FloatPredicate::Ord => 7,
        FloatPredicate::Uno => 8,
        FloatPredicate::Ueq => 9,
        FloatPredicate::Ugt => 10,
        FloatPredicate::Uge => 11,
        FloatPredicate::Ult => 12,
        FloatPredicate::Ule => 13,
        FloatPredicate::Une => 14,
    }
}

fn cast_to_opcode(op: &CastKind) -> u64 {
    match op {
        CastKind::Trunc => 0,
        CastKind::Zext => 1,
        CastKind::Sext => 2,
        CastKind::FpTrunc => 4,
        CastKind::FpExt => 5,
        CastKind::Sitofp => 6,
        CastKind::Fptosi => 7,
        CastKind::PtrToInt => 9,
        CastKind::IntToPtr => 10,
        CastKind::Bitcast => 11,
    }
}
