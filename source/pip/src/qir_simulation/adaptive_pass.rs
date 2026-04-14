// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Adaptive Profile Pass — walks adaptive-profile QIR and emits bytecode
//! consumed by the GPU/CPU parallel shot simulators.
//!
//! This is the Rust equivalent of `_adaptive_pass.py`. It takes a parsed
//! `Module`, traverses functions → basic blocks → instructions, and produces
//! the same dict-like structure that `AdaptiveProgram.as_dict()` returns.

use pyo3::{
    Bound, IntoPyObjectExt, PyResult, Python,
    exceptions::PyValueError,
    pyfunction,
    types::{PyAnyMethods, PyDict, PyDictMethods, PyList, PyListMethods, PyTuple},
};
use qsc_llvm::{
    model::Type,
    model::{
        Attribute, BinOpKind, CastKind, Constant, FloatPredicate, Function, Instruction,
        IntPredicate, Module, Operand,
    },
    parse_module,
    qir::{find_entry_point, get_function_attribute, qis, rt},
};
use rustc_hash::FxHashMap;

// ── Bytecode opcodes — must match `_adaptive_bytecode.py` and GPU shader ────

const FLAG_DST_IMM: u32 = 1 << 18;
const FLAG_SRC0_IMM: u32 = 1 << 16;
const FLAG_SRC1_IMM: u32 = 1 << 17;
const FLAG_AUX0_IMM: u32 = 1 << 19;
const FLAG_AUX1_IMM: u32 = 1 << 20;
const FLAG_AUX2_IMM: u32 = 1 << 21;
#[allow(dead_code)]
const FLAG_AUX3_IMM: u32 = 1 << 22;
const FLAG_FLOAT: u32 = 1 << 23;

const OP_RET: u32 = 0x02;
const OP_JUMP: u32 = 0x04;
const OP_BRANCH: u32 = 0x05;
const OP_SWITCH: u32 = 0x06;
const OP_CALL: u32 = 0x07;
const OP_CALL_RETURN: u32 = 0x08;

const OP_QUANTUM_GATE: u32 = 0x10;
const OP_MEASURE: u32 = 0x11;
const OP_RESET: u32 = 0x12;
const OP_READ_RESULT: u32 = 0x13;
const OP_RECORD_OUTPUT: u32 = 0x14;

const OP_ADD: u32 = 0x20;
const OP_SUB: u32 = 0x21;
const OP_MUL: u32 = 0x22;
const OP_UDIV: u32 = 0x23;
const OP_SDIV: u32 = 0x24;
const OP_UREM: u32 = 0x25;
const OP_SREM: u32 = 0x26;

const OP_AND: u32 = 0x28;
const OP_OR: u32 = 0x29;
const OP_XOR: u32 = 0x2A;
const OP_SHL: u32 = 0x2B;
const OP_LSHR: u32 = 0x2C;
const OP_ASHR: u32 = 0x2D;

const OP_ICMP: u32 = 0x30;
const OP_FCMP: u32 = 0x31;

const OP_FADD: u32 = 0x38;
const OP_FSUB: u32 = 0x39;
const OP_FMUL: u32 = 0x3A;
const OP_FDIV: u32 = 0x3B;

const OP_ZEXT: u32 = 0x40;
const OP_SEXT: u32 = 0x41;
const OP_TRUNC: u32 = 0x42;
const OP_FPEXT: u32 = 0x43;
const OP_FPTRUNC: u32 = 0x44;
#[allow(dead_code)]
const OP_INTTOPTR: u32 = 0x45;
const OP_FPTOSI: u32 = 0x46;
const OP_SITOFP: u32 = 0x47;

const OP_PHI: u32 = 0x50;
const OP_SELECT: u32 = 0x51;
const OP_MOV: u32 = 0x52;
const OP_CONST: u32 = 0x53;

// ICmp condition codes
const ICMP_EQ: u32 = 0;
const ICMP_NE: u32 = 1;
const ICMP_SLT: u32 = 2;
const ICMP_SLE: u32 = 3;
const ICMP_SGT: u32 = 4;
const ICMP_SGE: u32 = 5;
const ICMP_ULT: u32 = 6;
const ICMP_ULE: u32 = 7;
const ICMP_UGT: u32 = 8;
const ICMP_UGE: u32 = 9;

// FCmp condition codes
const FCMP_OEQ: u32 = 1;
const FCMP_OGT: u32 = 2;
const FCMP_OGE: u32 = 3;
const FCMP_OLT: u32 = 4;
const FCMP_OLE: u32 = 5;
const FCMP_ONE: u32 = 6;
const FCMP_ORD: u32 = 7;
const FCMP_UNO: u32 = 8;
const FCMP_UEQ: u32 = 9;
const FCMP_UGT: u32 = 10;
const FCMP_UGE: u32 = 11;
const FCMP_ULT: u32 = 12;
const FCMP_ULE: u32 = 13;
const FCMP_UNE: u32 = 14;

// Register type tags
const REG_TYPE_BOOL: u32 = 0;
const REG_TYPE_I32: u32 = 1;
const REG_TYPE_I64: u32 = 2;
const REG_TYPE_F32: u32 = 3;
const REG_TYPE_F64: u32 = 4;
const REG_TYPE_PTR: u32 = 5;

const VOID_RETURN: u32 = 0xFFFF_FFFF;

// Correlated noise op ID (must match shader_types.rs)
const CORRELATED_NOISE_OP_ID: u32 = 131;

// ── Gate mapping ────────────────────────────────────────────────────────────

fn gate_name_to_op_id(name: &str) -> Option<u32> {
    match name {
        "reset" => Some(1),
        "x" => Some(2),
        "y" => Some(3),
        "z" => Some(4),
        "h" => Some(5),
        "s" => Some(6),
        "s__adj" => Some(7),
        "t" => Some(8),
        "t__adj" => Some(9),
        "sx" => Some(10),
        "sx__adj" => Some(11),
        "rx" => Some(12),
        "ry" => Some(13),
        "rz" => Some(14),
        "cnot" | "cx" => Some(15),
        "cz" => Some(16),
        "cy" => Some(29),
        "rxx" => Some(17),
        "ryy" => Some(18),
        "rzz" => Some(19),
        "ccx" => Some(20),
        "m" | "mz" => Some(21),
        "mresetz" => Some(22),
        "swap" => Some(24),
        _ => None,
    }
}

fn is_measure_gate(name: &str) -> bool {
    matches!(name, "m" | "mz" | "mresetz")
}

fn is_reset_gate(name: &str) -> bool {
    name == "reset"
}

fn is_rotation_gate(name: &str) -> bool {
    matches!(name, "rx" | "ry" | "rz" | "rxx" | "ryy" | "rzz")
}

// ── Operand wrapper ─────────────────────────────────────────────────────────

/// An operand that either refers to a register or carries an immediate value.
#[derive(Clone, Copy)]
enum OpVal {
    Reg(u32),
    IntImm(u32),
    FloatImm(u32),
}

impl OpVal {
    fn raw(self) -> u32 {
        match self {
            OpVal::Reg(v) | OpVal::IntImm(v) | OpVal::FloatImm(v) => v,
        }
    }

    fn is_imm(self) -> bool {
        matches!(self, OpVal::IntImm(_) | OpVal::FloatImm(_))
    }
}

fn encode_float_as_bits(val: f64) -> u32 {
    (val as f32).to_bits()
}

fn i64_to_u32_masked(val: i64) -> u32 {
    (val as u32) & 0xFFFF_FFFF
}

// ── Block / instruction / quantum op tuples ─────────────────────────────────

#[derive(Clone, Copy)]
struct BcBlock {
    block_id: u32,
    instr_offset: u32,
    instr_count: u32,
}

#[derive(Clone, Copy)]
struct BcInstr {
    opcode: u32,
    dst: u32,
    src0: u32,
    src1: u32,
    aux0: u32,
    aux1: u32,
    aux2: u32,
    aux3: u32,
}

#[derive(Clone, Copy)]
struct BcQuantumOp {
    op_id: u32,
    q1: u32,
    q2: u32,
    q3: u32,
    angle: f64,
}

#[derive(Clone, Copy)]
struct BcFunction {
    entry_block: u32,
    num_params: u32,
    param_base: u32,
}

#[derive(Clone, Copy)]
struct BcPhiEntry {
    block_id: u32,
    val_reg: u32,
}

#[derive(Clone, Copy)]
struct BcSwitchCase {
    case_val: u32,
    target_block: u32,
}

// ── Pass state ──────────────────────────────────────────────────────────────

struct AdaptivePass<'m> {
    module: &'m Module,

    // Output tables
    blocks: Vec<BcBlock>,
    instructions: Vec<BcInstr>,
    quantum_ops: Vec<BcQuantumOp>,
    functions: Vec<BcFunction>,
    phi_entries: Vec<BcPhiEntry>,
    switch_cases: Vec<BcSwitchCase>,
    call_args: Vec<u32>,
    labels: Vec<String>,
    register_types: Vec<u32>,

    // Internal
    next_reg: u32,
    next_block: u32,
    next_qop: u32,
    /// SSA name (%name) → register ID
    value_to_reg: FxHashMap<String, u32>,
    /// Basic block name → block ID (function-qualified: "func::block")
    block_to_id: FxHashMap<String, u32>,
    /// Function name → function table index
    func_to_id: FxHashMap<String, u32>,
    current_func_is_entry: bool,
    current_func_name: String,
    noise_intrinsics: Option<FxHashMap<String, u32>>,
}

impl<'m> AdaptivePass<'m> {
    fn new(module: &'m Module, noise_intrinsics: Option<FxHashMap<String, u32>>) -> Self {
        Self {
            module,
            blocks: Vec::new(),
            instructions: Vec::new(),
            quantum_ops: Vec::new(),
            functions: Vec::new(),
            phi_entries: Vec::new(),
            switch_cases: Vec::new(),
            call_args: Vec::new(),
            labels: Vec::new(),
            register_types: Vec::new(),
            next_reg: 0,
            next_block: 0,
            next_qop: 0,
            value_to_reg: FxHashMap::default(),
            block_to_id: FxHashMap::default(),
            func_to_id: FxHashMap::default(),
            current_func_is_entry: true,
            current_func_name: String::new(),
            noise_intrinsics,
        }
    }

    fn run(&mut self, entry_idx: usize) -> PyResult<()> {
        // Check for arrays module flag
        if self.module.get_flag("arrays").is_some() {
            return Err(PyValueError::new_err(
                "QIR arrays are not currently supported.",
            ));
        }

        // Pass 1: assign block IDs and function IDs
        for func in &self.module.functions {
            if !func.basic_blocks.is_empty() {
                self.assign_function(func, entry_idx);
            }
        }

        // Pass 2: walk instructions and emit bytecode
        for (idx, func) in self.module.functions.iter().enumerate() {
            if !func.basic_blocks.is_empty() {
                self.walk_function(func, idx == entry_idx)?;
            }
        }

        Ok(())
    }

    // ── Register allocation ─────────────────────────────────────────────

    fn alloc_reg(&mut self, name: Option<&str>, type_tag: u32) -> u32 {
        if let Some(n) = name {
            if let Some(&existing) = self.value_to_reg.get(n) {
                return existing;
            }
        }
        let reg = self.next_reg;
        self.next_reg += 1;
        if let Some(n) = name {
            self.value_to_reg.insert(n.to_string(), reg);
        }
        self.register_types.push(type_tag);
        reg
    }

    /// Build a qualified name for a block: "func_name::block_name"
    fn qualified_block_name(func_name: &str, block_name: &str) -> String {
        let mut s = String::with_capacity(func_name.len() + 2 + block_name.len());
        s.push_str(func_name);
        s.push_str("::");
        s.push_str(block_name);
        s
    }

    // ── Instruction emission ────────────────────────────────────────────

    fn emit(
        &mut self,
        opcode: u32,
        dst: OpVal,
        src0: OpVal,
        src1: OpVal,
        aux0: OpVal,
        aux1: OpVal,
        aux2: OpVal,
        aux3: OpVal,
    ) {
        let mut flags: u32 = 0;
        if dst.is_imm() {
            flags |= FLAG_DST_IMM;
        }
        if src0.is_imm() {
            flags |= FLAG_SRC0_IMM;
        }
        if src1.is_imm() {
            flags |= FLAG_SRC1_IMM;
        }
        if aux0.is_imm() {
            flags |= FLAG_AUX0_IMM;
        }
        if aux1.is_imm() {
            flags |= FLAG_AUX1_IMM;
        }
        if aux2.is_imm() {
            flags |= FLAG_AUX2_IMM;
        }
        if aux3.is_imm() {
            // Note: FLAG_AUX3_IMM is not commonly used but is tracked
        }
        self.instructions.push(BcInstr {
            opcode: opcode | flags,
            dst: dst.raw(),
            src0: src0.raw(),
            src1: src1.raw(),
            aux0: aux0.raw(),
            aux1: aux1.raw(),
            aux2: aux2.raw(),
            aux3: aux3.raw(),
        });
    }

    /// Helper for common case: emit with minimal operands
    fn emit_simple(&mut self, opcode: u32, dst: OpVal, src0: OpVal, src1: OpVal) {
        let z = OpVal::Reg(0);
        self.emit(opcode, dst, src0, src1, z, z, z, z);
    }

    fn emit_quantum_op(&mut self, op_id: u32, q1: u32, q2: u32, q3: u32, angle: f64) -> u32 {
        let idx = self.next_qop;
        self.next_qop += 1;
        self.quantum_ops.push(BcQuantumOp {
            op_id,
            q1,
            q2,
            q3,
            angle,
        });
        idx
    }

    // ── Operand resolution ──────────────────────────────────────────────

    fn resolve_operand(&mut self, operand: &Operand) -> PyResult<OpVal> {
        match operand {
            Operand::LocalRef(name) | Operand::TypedLocalRef(name, _) => {
                if let Some(&reg) = self.value_to_reg.get(name.as_str()) {
                    Ok(OpVal::Reg(reg))
                } else {
                    // Forward reference — pre-allocate a register
                    let reg = self.alloc_reg(Some(name), REG_TYPE_I64);
                    Ok(OpVal::Reg(reg))
                }
            }
            Operand::IntConst(_, val) => Ok(OpVal::IntImm(i64_to_u32_masked(*val))),
            Operand::FloatConst(_, val) => Ok(OpVal::FloatImm(encode_float_as_bits(*val))),
            Operand::IntToPtr(val, _) => Ok(OpVal::IntImm(i64_to_u32_masked(*val))),
            Operand::NullPtr => {
                // Null pointer — materialize as register with value 0
                let reg = self.alloc_reg(None, REG_TYPE_PTR);
                self.emit(
                    OP_CONST | FLAG_SRC0_IMM,
                    OpVal::Reg(reg),
                    OpVal::IntImm(0),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                );
                Ok(OpVal::Reg(reg))
            }
            Operand::GlobalRef(_) => {
                // Global reference — look up string initializer for label extraction
                // Return as immediate 0 (globals are typically used for labels, not values)
                Ok(OpVal::IntImm(0))
            }
            Operand::GetElementPtr { .. } => Err(PyValueError::new_err(
                "GEP operands not supported in adaptive pass",
            )),
        }
    }

    fn type_tag(ty: &Type) -> u32 {
        match ty {
            Type::Integer(1) => REG_TYPE_BOOL,
            Type::Integer(w) if *w <= 32 => REG_TYPE_I32,
            Type::Integer(_) => REG_TYPE_I64,
            Type::Ptr | Type::NamedPtr(_) | Type::TypedPtr(_) => REG_TYPE_PTR,
            Type::Double => REG_TYPE_F64,
            _ => REG_TYPE_F32,
        }
    }

    // ── Function assignment (Pass 1) ────────────────────────────────────

    fn assign_function(&mut self, func: &Function, entry_idx: usize) {
        let is_entry = self
            .module
            .functions
            .iter()
            .position(|f| std::ptr::eq(f, func))
            .is_some_and(|idx| idx == entry_idx);

        if !is_entry && !self.func_to_id.contains_key(&func.name) {
            let func_id = self.func_to_id.len() as u32;
            self.func_to_id.insert(func.name.clone(), func_id);
        }

        for block in &func.basic_blocks {
            let qname = Self::qualified_block_name(&func.name, &block.name);
            let id = self.next_block;
            self.next_block += 1;
            self.block_to_id.insert(qname, id);
        }
    }

    // ── Function walking (Pass 2) ───────────────────────────────────────

    fn walk_function(&mut self, func: &Function, is_entry: bool) -> PyResult<()> {
        self.current_func_is_entry = is_entry;
        self.current_func_name = func.name.clone();

        // Clear per-function register name mapping so names from previous
        // functions don't leak (e.g. %q0 in one function != %q0 in another).
        self.value_to_reg.clear();

        // For non-entry functions, register parameters as registers
        if !is_entry {
            let param_base = self.next_reg;
            for param in &func.params {
                let name = param.name.as_deref();
                self.alloc_reg(name, REG_TYPE_PTR);
            }
            if let Some(&_func_id) = self.func_to_id.get(&func.name) {
                let func_entry_block = self.block_to_id
                    [&Self::qualified_block_name(&func.name, &func.basic_blocks[0].name)];
                self.functions.push(BcFunction {
                    entry_block: func_entry_block,
                    num_params: func.params.len() as u32,
                    param_base,
                });
            }
        }

        for block in &func.basic_blocks {
            let qname = Self::qualified_block_name(&func.name, &block.name);
            let block_id = self.block_to_id[&qname];
            let instr_offset = self.instructions.len() as u32;
            for instr in &block.instructions {
                self.on_instruction(instr)?;
            }
            let instr_count = self.instructions.len() as u32 - instr_offset;
            self.blocks.push(BcBlock {
                block_id,
                instr_offset,
                instr_count,
            });
        }

        Ok(())
    }

    // ── Instruction dispatch ────────────────────────────────────────────

    fn on_instruction(&mut self, instr: &Instruction) -> PyResult<()> {
        match instr {
            Instruction::Call {
                callee,
                args,
                result,
                return_ty,
                ..
            } => self.emit_call(callee, args, result.as_deref(), return_ty.as_ref()),
            Instruction::Phi {
                ty,
                incoming,
                result,
            } => self.emit_phi(ty, incoming, result),
            Instruction::ICmp {
                pred,
                ty: _,
                lhs,
                rhs,
                result,
            } => self.emit_icmp(pred, lhs, rhs, result),
            Instruction::FCmp {
                pred,
                ty: _,
                lhs,
                rhs,
                result,
            } => self.emit_fcmp(pred, lhs, rhs, result),
            Instruction::Switch {
                ty: _,
                value,
                default_dest,
                cases,
            } => self.emit_switch(value, default_dest, cases),
            Instruction::Br {
                cond,
                true_dest,
                false_dest,
                ..
            } => self.emit_cond_branch(cond, true_dest, false_dest),
            Instruction::Jump { dest } => self.emit_jump(dest),
            Instruction::Ret(operand) => self.emit_ret(operand.as_ref()),
            Instruction::Select {
                cond,
                true_val,
                false_val,
                ty,
                result,
            } => self.emit_select(cond, true_val, false_val, ty, result),
            Instruction::BinOp {
                op,
                ty,
                lhs,
                rhs,
                result,
            } => self.emit_binop(op, ty, lhs, rhs, result),
            Instruction::Cast {
                op,
                from_ty,
                to_ty,
                value,
                result,
            } => self.emit_cast(op, from_ty, to_ty, value, result),
            Instruction::Alloca { .. }
            | Instruction::Load { .. }
            | Instruction::Store { .. }
            | Instruction::GetElementPtr { .. } => {
                // Memory instructions not expected in adaptive profile QIR
                Err(PyValueError::new_err(format!(
                    "Unsupported memory instruction in adaptive pass: {instr:?}"
                )))
            }
            Instruction::Unreachable => Ok(()),
        }
    }

    // ── BinOp dispatch ──────────────────────────────────────────────────

    fn emit_binop(
        &mut self,
        op: &BinOpKind,
        ty: &Type,
        lhs: &Operand,
        rhs: &Operand,
        result: &str,
    ) -> PyResult<()> {
        let opcode = match op {
            BinOpKind::Add => OP_ADD,
            BinOpKind::Sub => OP_SUB,
            BinOpKind::Mul => OP_MUL,
            BinOpKind::Udiv => OP_UDIV,
            BinOpKind::Sdiv => OP_SDIV,
            BinOpKind::Urem => OP_UREM,
            BinOpKind::Srem => OP_SREM,
            BinOpKind::And => OP_AND,
            BinOpKind::Or => OP_OR,
            BinOpKind::Xor => OP_XOR,
            BinOpKind::Shl => OP_SHL,
            BinOpKind::Lshr => OP_LSHR,
            BinOpKind::Ashr => OP_ASHR,
            BinOpKind::Fadd => OP_FADD | FLAG_FLOAT,
            BinOpKind::Fsub => OP_FSUB | FLAG_FLOAT,
            BinOpKind::Fmul => OP_FMUL | FLAG_FLOAT,
            BinOpKind::Fdiv => OP_FDIV | FLAG_FLOAT,
        };
        let dst_reg = self.alloc_reg(Some(result), Self::type_tag(ty));
        let s0 = self.resolve_operand(lhs)?;
        let s1 = self.resolve_operand(rhs)?;
        self.emit_simple(opcode, OpVal::Reg(dst_reg), s0, s1);
        Ok(())
    }

    // ── Cast dispatch ───────────────────────────────────────────────────

    fn emit_cast(
        &mut self,
        op: &CastKind,
        from_ty: &Type,
        to_ty: &Type,
        value: &Operand,
        result: &str,
    ) -> PyResult<()> {
        match op {
            CastKind::Zext => {
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_ZEXT, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
            CastKind::Sext => {
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                let src_bits = match from_ty {
                    Type::Integer(w) => *w,
                    _ => 32,
                };
                let z = OpVal::Reg(0);
                self.emit(
                    OP_SEXT,
                    OpVal::Reg(dst),
                    src,
                    z,
                    OpVal::IntImm(src_bits),
                    z,
                    z,
                    z,
                );
            }
            CastKind::Trunc => {
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_TRUNC, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
            CastKind::FpExt => {
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_FPEXT | FLAG_FLOAT, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
            CastKind::FpTrunc => {
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_FPTRUNC | FLAG_FLOAT, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
            CastKind::Fptosi => {
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_FPTOSI, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
            CastKind::Sitofp => {
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_SITOFP | FLAG_FLOAT, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
            CastKind::IntToPtr => {
                // inttoptr is essentially a no-op cast; alias via MOV
                let dst = self.alloc_reg(Some(result), REG_TYPE_PTR);
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_MOV, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
            CastKind::PtrToInt | CastKind::Bitcast => {
                // Pass-through casts
                let dst = self.alloc_reg(Some(result), Self::type_tag(to_ty));
                let src = self.resolve_operand(value)?;
                self.emit_simple(OP_MOV, OpVal::Reg(dst), src, OpVal::Reg(0));
            }
        }
        Ok(())
    }

    // ── Call dispatch ───────────────────────────────────────────────────

    fn emit_call(
        &mut self,
        callee: &str,
        args: &[(Type, Operand)],
        result: Option<&str>,
        return_ty: Option<&Type>,
    ) -> PyResult<()> {
        match callee {
            qis::READ_RESULT | rt::READ_RESULT => {
                let dst = self.alloc_reg(result, REG_TYPE_BOOL);
                let result_reg = self.resolve_operand(&args[0].1)?;
                let z = OpVal::Reg(0);
                self.emit(OP_READ_RESULT, OpVal::Reg(dst), result_reg, z, z, z, z, z);
            }
            name if name.starts_with("__quantum__qis__") => {
                self.emit_quantum_call(name, args, result)?;
            }
            rt::RESULT_RECORD_OUTPUT => {
                let result_reg = self.resolve_operand(&args[0].1)?;
                let label_str = self.extract_label(&args[1].1);
                let label_idx = self.labels.len() as u32;
                self.labels.push(label_str);
                let z = OpVal::Reg(0);
                self.emit(
                    OP_RECORD_OUTPUT,
                    z,
                    result_reg,
                    z,
                    OpVal::IntImm(label_idx),
                    z,
                    z,
                    z,
                );
            }
            rt::ARRAY_RECORD_OUTPUT => {
                let count = match args[0].1 {
                    Operand::IntConst(_, v) => u32::try_from(v).expect("Array length out of range"),
                    _ => 0,
                };
                let label_str = self.extract_label(&args[1].1);
                let label_idx = self.labels.len() as u32;
                self.labels.push(label_str);
                let z = OpVal::Reg(0);
                self.emit(
                    OP_RECORD_OUTPUT,
                    z,
                    OpVal::IntImm(count),
                    z,
                    OpVal::IntImm(label_idx),
                    OpVal::IntImm(1), // aux1=1 → array
                    z,
                    z,
                );
            }
            rt::TUPLE_RECORD_OUTPUT => {
                let count = match args[0].1 {
                    Operand::IntConst(_, v) => u32::try_from(v).expect("Tuple length out of range"),
                    _ => 0,
                };
                let label_str = self.extract_label(&args[1].1);
                let label_idx = self.labels.len() as u32;
                self.labels.push(label_str);
                let z = OpVal::Reg(0);
                self.emit(
                    OP_RECORD_OUTPUT,
                    z,
                    OpVal::IntImm(count),
                    z,
                    OpVal::IntImm(label_idx),
                    OpVal::IntImm(2), // aux1=2 → tuple
                    z,
                    z,
                );
            }
            rt::BOOL_RECORD_OUTPUT => {
                let src = self.resolve_operand(&args[0].1)?;
                let label_str = self.extract_label(&args[1].1);
                let label_idx = self.labels.len() as u32;
                self.labels.push(label_str);
                let z = OpVal::Reg(0);
                self.emit(
                    OP_RECORD_OUTPUT,
                    z,
                    src,
                    z,
                    OpVal::IntImm(label_idx),
                    OpVal::IntImm(3), // aux1=3 → bool
                    z,
                    z,
                );
            }
            rt::INT_RECORD_OUTPUT => {
                let src = self.resolve_operand(&args[0].1)?;
                let label_str = self.extract_label(&args[1].1);
                let label_idx = self.labels.len() as u32;
                self.labels.push(label_str);
                let z = OpVal::Reg(0);
                self.emit(
                    OP_RECORD_OUTPUT,
                    z,
                    src,
                    z,
                    OpVal::IntImm(label_idx),
                    OpVal::IntImm(4), // aux1=4 → int
                    z,
                    z,
                );
            }
            rt::INITIALIZE
            | rt::BEGIN_PARALLEL
            | rt::END_PARALLEL
            | qis::BARRIER
            | rt::READ_LOSS => {
                // No-op
            }
            name if self.func_to_id.contains_key(name) => {
                self.emit_ir_function_call(name, args, result, return_ty)?;
            }
            name if self.is_noise_intrinsic(name) => {
                self.emit_noise_intrinsic_call(name, args)?;
            }
            _ => {
                return Err(PyValueError::new_err(format!("Unsupported call: {callee}")));
            }
        }
        Ok(())
    }

    // ── Quantum call ────────────────────────────────────────────────────

    fn emit_quantum_call(
        &mut self,
        callee_name: &str,
        args: &[(Type, Operand)],
        _result: Option<&str>,
    ) -> PyResult<()> {
        let gate_name = callee_name
            .replace("__quantum__qis__", "")
            .replace("__body", "");
        let op_id = gate_name_to_op_id(&gate_name)
            .ok_or_else(|| PyValueError::new_err(format!("Unknown quantum gate: {gate_name}")))?;

        if is_measure_gate(&gate_name) {
            let q = self.resolve_operand(&args[0].1)?;
            let r = self.resolve_operand(&args[1].1)?;
            let qop_idx = self.emit_quantum_op(op_id, q.raw(), r.raw(), 0, 0.0);
            let z = OpVal::Reg(0);
            self.emit(OP_MEASURE, z, z, z, OpVal::IntImm(qop_idx), q, r, z);
            return Ok(());
        }
        if is_reset_gate(&gate_name) {
            let q = self.resolve_operand(&args[0].1)?;
            let qop_idx = self.emit_quantum_op(op_id, q.raw(), 0, 0, 0.0);
            let z = OpVal::Reg(0);
            self.emit(OP_RESET, z, z, z, OpVal::IntImm(qop_idx), q, z, z);
            return Ok(());
        }

        let (qubit_offset, angle) = if is_rotation_gate(&gate_name) {
            let a = self.resolve_operand(&args[0].1)?;
            (1, a)
        } else {
            (0, OpVal::FloatImm(0))
        };

        let qubit_args = &args[qubit_offset..];
        let mut qs = [OpVal::IntImm(0); 3];
        for (i, (_, operand)) in qubit_args.iter().enumerate().take(3) {
            qs[i] = self.resolve_operand(operand)?;
        }

        let qop_idx = self.emit_quantum_op(
            op_id,
            qs[0].raw(),
            qs[1].raw(),
            qs[2].raw(),
            if angle.is_imm() {
                // Decode float bits back to f64 for the quantum op table
                f32::from_bits(angle.raw()).into()
            } else {
                0.0 // Register-based angle — will be handled at runtime
            },
        );
        let z = OpVal::Reg(0);
        self.emit(
            OP_QUANTUM_GATE,
            z,
            z,
            z,
            OpVal::IntImm(qop_idx),
            qs[0],
            qs[1],
            qs[2],
        );
        Ok(())
    }

    // ── Noise intrinsic call ────────────────────────────────────────────

    fn is_noise_intrinsic(&self, name: &str) -> bool {
        // Check if the callee has qdk_noise attribute
        self.module.functions.iter().any(|f| {
            f.name == name
                && f.attribute_group_refs.iter().any(|&group_ref| {
                    self.module
                        .attribute_groups
                        .iter()
                        .find(|ag| ag.id == group_ref)
                        .is_some_and(|ag| {
                            ag.attributes.iter().any(|attr| {
                                matches!(attr, Attribute::StringAttr(s) if s.contains("qdk_noise"))
                            })
                        })
                })
        })
    }

    fn emit_noise_intrinsic_call(
        &mut self,
        callee_name: &str,
        args: &[(Type, Operand)],
    ) -> PyResult<()> {
        if let Some(noise_map) = &self.noise_intrinsics {
            if let Some(&table_id) = noise_map.get(callee_name) {
                let qubit_count = args.len() as u32;
                let arg_offset = self.call_args.len() as u32;
                for (_, operand) in args {
                    let op = self.resolve_operand(operand)?;
                    if let OpVal::Reg(r) = op {
                        self.call_args.push(r);
                    } else {
                        let reg = self.alloc_reg(None, REG_TYPE_PTR);
                        self.emit(
                            OP_MOV | FLAG_SRC0_IMM,
                            OpVal::Reg(reg),
                            OpVal::IntImm(op.raw()),
                            OpVal::Reg(0),
                            OpVal::Reg(0),
                            OpVal::Reg(0),
                            OpVal::Reg(0),
                            OpVal::Reg(0),
                        );
                        self.call_args.push(reg);
                    }
                }
                let qop_idx =
                    self.emit_quantum_op(CORRELATED_NOISE_OP_ID, table_id, qubit_count, 0, 0.0);
                let z = OpVal::Reg(0);
                self.emit(
                    OP_QUANTUM_GATE,
                    z,
                    z,
                    z,
                    OpVal::IntImm(qop_idx),
                    OpVal::IntImm(qubit_count),
                    OpVal::IntImm(arg_offset),
                    z,
                );
            } else {
                return Err(PyValueError::new_err(format!(
                    "Missing noise intrinsic: {callee_name}"
                )));
            }
        }
        // No noise config → no-op
        Ok(())
    }

    // ── Control flow ────────────────────────────────────────────────────

    fn emit_jump(&mut self, dest: &str) -> PyResult<()> {
        let qname = Self::qualified_block_name(&self.current_func_name, dest);
        let target = self
            .block_to_id
            .get(&qname)
            .copied()
            .ok_or_else(|| PyValueError::new_err(format!("Unknown block: {dest}")))?;
        let z = OpVal::Reg(0);
        self.emit(OP_JUMP, OpVal::IntImm(target), z, z, z, z, z, z);
        Ok(())
    }

    fn emit_cond_branch(
        &mut self,
        cond: &Operand,
        true_dest: &str,
        false_dest: &str,
    ) -> PyResult<()> {
        let cond_reg = self.resolve_operand(cond)?;
        let true_block =
            self.block_to_id[&Self::qualified_block_name(&self.current_func_name, true_dest)];
        let false_block =
            self.block_to_id[&Self::qualified_block_name(&self.current_func_name, false_dest)];
        let z = OpVal::Reg(0);
        self.emit(
            OP_BRANCH,
            z,
            cond_reg,
            z,
            OpVal::IntImm(true_block),
            OpVal::IntImm(false_block),
            z,
            z,
        );
        Ok(())
    }

    fn emit_phi(
        &mut self,
        ty: &Type,
        incoming: &[(Operand, String)],
        result: &str,
    ) -> PyResult<()> {
        let dst_reg = self.alloc_reg(Some(result), Self::type_tag(ty));
        let phi_offset = self.phi_entries.len() as u32;
        for (value, block_name) in incoming {
            let operand = self.resolve_operand(value)?;
            let val_reg = match operand {
                OpVal::Reg(r) => r,
                _ => {
                    // Immediate → materialize into register
                    let reg = self.alloc_reg(None, Self::type_tag(ty));
                    self.emit(
                        OP_MOV | FLAG_SRC0_IMM,
                        OpVal::Reg(reg),
                        OpVal::IntImm(operand.raw()),
                        OpVal::Reg(0),
                        OpVal::Reg(0),
                        OpVal::Reg(0),
                        OpVal::Reg(0),
                        OpVal::Reg(0),
                    );
                    reg
                }
            };
            let qname = Self::qualified_block_name(&self.current_func_name, block_name);
            let block_id = self.block_to_id.get(&qname).copied().ok_or_else(|| {
                PyValueError::new_err(format!("Unknown phi source block: {block_name}"))
            })?;
            self.phi_entries.push(BcPhiEntry { block_id, val_reg });
        }
        let count = incoming.len() as u32;
        let z = OpVal::Reg(0);
        self.emit(
            OP_PHI,
            OpVal::Reg(dst_reg),
            z,
            z,
            OpVal::IntImm(phi_offset),
            OpVal::IntImm(count),
            z,
            z,
        );
        Ok(())
    }

    fn emit_select(
        &mut self,
        cond: &Operand,
        true_val: &Operand,
        false_val: &Operand,
        ty: &Type,
        result: &str,
    ) -> PyResult<()> {
        let dst = self.alloc_reg(Some(result), Self::type_tag(ty));
        let c = self.resolve_operand(cond)?;
        let t = self.resolve_operand(true_val)?;
        let f = self.resolve_operand(false_val)?;
        let z = OpVal::Reg(0);
        self.emit(OP_SELECT, OpVal::Reg(dst), c, z, t, f, z, z);
        Ok(())
    }

    fn emit_switch(
        &mut self,
        value: &Operand,
        default_dest: &str,
        cases: &[(i64, String)],
    ) -> PyResult<()> {
        let cond_reg = self.resolve_operand(value)?;
        let default_block =
            self.block_to_id[&Self::qualified_block_name(&self.current_func_name, default_dest)];
        let case_offset = self.switch_cases.len() as u32;
        for (case_val, block_name) in cases {
            let qname = Self::qualified_block_name(&self.current_func_name, block_name);
            let target_block = self.block_to_id[&qname];
            self.switch_cases.push(BcSwitchCase {
                case_val: i64_to_u32_masked(*case_val),
                target_block,
            });
        }
        let case_count = cases.len() as u32;
        let z = OpVal::Reg(0);
        self.emit(
            OP_SWITCH,
            z,
            cond_reg,
            z,
            OpVal::IntImm(default_block),
            OpVal::IntImm(case_offset),
            OpVal::IntImm(case_count),
            z,
        );
        Ok(())
    }

    fn emit_ret(&mut self, operand: Option<&Operand>) -> PyResult<()> {
        if !self.current_func_is_entry {
            // Return from IR-defined function
            if let Some(op) = operand {
                let ret_reg = self.resolve_operand(op)?;
                let z = OpVal::Reg(0);
                self.emit(OP_CALL_RETURN, z, ret_reg, z, z, z, z, z);
            } else {
                let z = OpVal::Reg(0);
                self.emit(OP_CALL_RETURN, z, z, z, z, z, z, z);
            }
        } else if let Some(op) = operand {
            let ret_reg = self.resolve_operand(op)?;
            let z = OpVal::Reg(0);
            self.emit(OP_RET, ret_reg, z, z, z, z, z, z);
        } else {
            let z = OpVal::Reg(0);
            self.emit(OP_RET, OpVal::IntImm(0), z, z, z, z, z, z);
        }
        Ok(())
    }

    // ── Comparison ──────────────────────────────────────────────────────

    fn emit_icmp(
        &mut self,
        pred: &IntPredicate,
        lhs: &Operand,
        rhs: &Operand,
        result: &str,
    ) -> PyResult<()> {
        let cond_code = match pred {
            IntPredicate::Eq => ICMP_EQ,
            IntPredicate::Ne => ICMP_NE,
            IntPredicate::Slt => ICMP_SLT,
            IntPredicate::Sle => ICMP_SLE,
            IntPredicate::Sgt => ICMP_SGT,
            IntPredicate::Sge => ICMP_SGE,
            IntPredicate::Ult => ICMP_ULT,
            IntPredicate::Ule => ICMP_ULE,
            IntPredicate::Ugt => ICMP_UGT,
            IntPredicate::Uge => ICMP_UGE,
        };
        let dst = self.alloc_reg(Some(result), REG_TYPE_BOOL);
        let s0 = self.resolve_operand(lhs)?;
        let s1 = self.resolve_operand(rhs)?;
        self.emit_simple(OP_ICMP | (cond_code << 8), OpVal::Reg(dst), s0, s1);
        Ok(())
    }

    fn emit_fcmp(
        &mut self,
        pred: &FloatPredicate,
        lhs: &Operand,
        rhs: &Operand,
        result: &str,
    ) -> PyResult<()> {
        let cond_code = match pred {
            FloatPredicate::Oeq => FCMP_OEQ,
            FloatPredicate::Ogt => FCMP_OGT,
            FloatPredicate::Oge => FCMP_OGE,
            FloatPredicate::Olt => FCMP_OLT,
            FloatPredicate::Ole => FCMP_OLE,
            FloatPredicate::One => FCMP_ONE,
            FloatPredicate::Ord => FCMP_ORD,
            FloatPredicate::Uno => FCMP_UNO,
            FloatPredicate::Ueq => FCMP_UEQ,
            FloatPredicate::Ugt => FCMP_UGT,
            FloatPredicate::Uge => FCMP_UGE,
            FloatPredicate::Ult => FCMP_ULT,
            FloatPredicate::Ule => FCMP_ULE,
            FloatPredicate::Une => FCMP_UNE,
        };
        let dst = self.alloc_reg(Some(result), REG_TYPE_BOOL);
        let s0 = self.resolve_operand(lhs)?;
        let s1 = self.resolve_operand(rhs)?;
        self.emit_simple(
            OP_FCMP | (cond_code << 8) | FLAG_FLOAT,
            OpVal::Reg(dst),
            s0,
            s1,
        );
        Ok(())
    }

    // ── IR-defined function call/return ─────────────────────────────────

    fn emit_ir_function_call(
        &mut self,
        func_name: &str,
        args: &[(Type, Operand)],
        result: Option<&str>,
        return_ty: Option<&Type>,
    ) -> PyResult<()> {
        let func_id = self.func_to_id[func_name];
        let arg_offset = self.call_args.len() as u32;
        for (_, operand) in args {
            let op = self.resolve_operand(operand)?;
            if let OpVal::Reg(r) = op {
                self.call_args.push(r);
            } else {
                let reg = self.alloc_reg(None, REG_TYPE_PTR);
                self.emit(
                    OP_MOV | FLAG_SRC0_IMM,
                    OpVal::Reg(reg),
                    OpVal::IntImm(op.raw()),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                    OpVal::Reg(0),
                );
                self.call_args.push(reg);
            }
        }
        let is_void = return_ty.is_none() || matches!(return_ty, Some(Type::Void));
        let return_reg = if is_void {
            VOID_RETURN
        } else {
            self.alloc_reg(result, REG_TYPE_I32)
        };
        let z = OpVal::Reg(0);
        self.emit(
            OP_CALL,
            OpVal::IntImm(return_reg),
            z,
            z,
            OpVal::IntImm(func_id),
            OpVal::IntImm(args.len() as u32),
            OpVal::IntImm(arg_offset),
            z,
        );
        Ok(())
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn extract_label(&self, operand: &Operand) -> String {
        match operand {
            Operand::GlobalRef(name) => {
                // Look up global's string initializer
                for global in &self.module.globals {
                    if global.name == *name {
                        if let Some(Constant::CString(s)) = &global.initializer {
                            return s.clone();
                        }
                    }
                }
                String::new()
            }
            _ => String::new(),
        }
    }
}

// ── Python-facing function ──────────────────────────────────────────────────

/// Compile adaptive-profile QIR text IR into the bytecode dict consumed by
/// `run_adaptive_parallel_shots`.
///
/// Returns a Python dict with the same keys as `AdaptiveProgram.as_dict()`.
#[pyfunction]
#[pyo3(signature = (ir, noise_intrinsics=None))]
pub fn compile_adaptive_program<'py>(
    py: Python<'py>,
    ir: &str,
    noise_intrinsics: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyDict>> {
    let module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("failed to parse IR: {e}")))?;

    let entry_idx = find_entry_point(&module)
        .ok_or_else(|| PyValueError::new_err("no entry point function found in IR"))?;

    let num_qubits: u32 = get_function_attribute(&module, entry_idx, "required_num_qubits")
        .ok_or_else(|| PyValueError::new_err("missing required_num_qubits attribute"))?
        .parse()
        .map_err(|e| PyValueError::new_err(format!("invalid required_num_qubits: {e}")))?;

    let num_results: u32 = get_function_attribute(&module, entry_idx, "required_num_results")
        .ok_or_else(|| PyValueError::new_err("missing required_num_results attribute"))?
        .parse()
        .map_err(|e| PyValueError::new_err(format!("invalid required_num_results: {e}")))?;

    // Build noise intrinsics lookup from Python dict
    let noise_map: Option<FxHashMap<String, u32>> = noise_intrinsics.map(|dict| {
        let mut map = FxHashMap::default();
        for (key, value) in dict.iter() {
            if let (Ok(k), Ok(v)) = (key.extract::<String>(), value.extract::<u32>()) {
                map.insert(k, v);
            }
        }
        map
    });

    let mut pass = AdaptivePass::new(&module, noise_map);
    pass.run(entry_idx)?;

    let entry_func = &module.functions[entry_idx];
    let entry_block_name =
        AdaptivePass::qualified_block_name(&entry_func.name, &entry_func.basic_blocks[0].name);
    let entry_block = pass.block_to_id[&entry_block_name];

    // Build the Python dict
    let dict = PyDict::new(py);

    dict.set_item("num_qubits", num_qubits)?;
    dict.set_item("num_results", num_results)?;
    dict.set_item("num_registers", pass.next_reg)?;
    dict.set_item("entry_block", entry_block)?;

    // blocks: list of (block_id, instr_offset, instr_count)
    let blocks = PyList::empty(py);
    for b in &pass.blocks {
        let t = PyTuple::new(py, [b.block_id, b.instr_offset, b.instr_count])?;
        blocks.append(t)?;
    }
    dict.set_item("blocks", blocks)?;

    // instructions: list of (opcode, dst, src0, src1, aux0, aux1, aux2, aux3)
    let instrs = PyList::empty(py);
    for i in &pass.instructions {
        let t = PyTuple::new(
            py,
            [
                i.opcode, i.dst, i.src0, i.src1, i.aux0, i.aux1, i.aux2, i.aux3,
            ],
        )?;
        instrs.append(t)?;
    }
    dict.set_item("instructions", instrs)?;

    // quantum_ops: list of (op_id, q1, q2, q3, angle)
    let qops = PyList::empty(py);
    for q in &pass.quantum_ops {
        let t = PyTuple::new(
            py,
            &[
                q.op_id.into_py_any(py)?,
                q.q1.into_py_any(py)?,
                q.q2.into_py_any(py)?,
                q.q3.into_py_any(py)?,
                q.angle.into_py_any(py)?,
            ],
        )?;
        qops.append(t)?;
    }
    dict.set_item("quantum_ops", qops)?;

    // functions: list of (entry_block, num_params, param_base)
    let funcs = PyList::empty(py);
    for f in &pass.functions {
        let t = PyTuple::new(py, [f.entry_block, f.num_params, f.param_base])?;
        funcs.append(t)?;
    }
    dict.set_item("functions", funcs)?;

    // phi_entries: list of (block_id, val_reg)
    let phis = PyList::empty(py);
    for p in &pass.phi_entries {
        let t = PyTuple::new(py, [p.block_id, p.val_reg])?;
        phis.append(t)?;
    }
    dict.set_item("phi_entries", phis)?;

    // switch_cases: list of (case_val, target_block)
    let cases = PyList::empty(py);
    for s in &pass.switch_cases {
        let t = PyTuple::new(py, [s.case_val, s.target_block])?;
        cases.append(t)?;
    }
    dict.set_item("switch_cases", cases)?;

    // call_args: list of u32
    let cargs = PyList::new(py, &pass.call_args)?;
    dict.set_item("call_args", cargs)?;

    // labels: list of str
    let lbls = PyList::new(py, &pass.labels)?;
    dict.set_item("labels", lbls)?;

    // register_types: list of u32
    let rtypes = PyList::new(py, &pass.register_types)?;
    dict.set_item("register_types", rtypes)?;

    Ok(dict)
}
