// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! CPU bytecode interpreter for the Adaptive Profile QIR.

// The interpreter intentionally uses u64 registers and must cast between u64, i64,
// usize, and u32 pervasively. These casts are correct by construction (values come
// from a well-formed bytecode program). Suppressing the pedantic clippy lints here
// keeps the opcode dispatch readable.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::match_same_arms,
    clippy::single_match_else,
    clippy::too_many_lines
)]

use crate::{
    MeasurementResult, Simulator,
    bytecode::{AdaptiveProgram, Instruction},
};

// ---------------------------------------------------------------------------
// Opcode constants — must stay in sync with the Python `_adaptive_bytecode.py`
// and the WGSL `simulator_adaptive.wgsl` shader.
// ---------------------------------------------------------------------------

// Flags (pre-shifted to bit 16+)
const FLAG_SRC0_IMM: u64 = 1 << 16;
const FLAG_SRC1_IMM: u64 = 1 << 17;
const FLAG_DST_IMM: u64 = 1 << 18;
const FLAG_AUX0_IMM: u64 = 1 << 19;
const FLAG_AUX1_IMM: u64 = 1 << 20;
const FLAG_AUX2_IMM: u64 = 1 << 21;
const FLAG_AUX3_IMM: u64 = 1 << 22;

// Control flow
const OP_NOP: u8 = 0x00;
const OP_RET: u8 = 0x02;
const OP_JUMP: u8 = 0x04;
const OP_BRANCH: u8 = 0x05;
const OP_SWITCH: u8 = 0x06;
const OP_CALL: u8 = 0x07;
const OP_CALL_RETURN: u8 = 0x08;

// Quantum
const OP_QUANTUM_GATE: u8 = 0x10;
const OP_MEASURE: u8 = 0x11;
const OP_RESET: u8 = 0x12;
const OP_READ_RESULT: u8 = 0x13;
const OP_RECORD_OUTPUT: u8 = 0x14;

// Integer arithmetic
const OP_ADD: u8 = 0x20;
const OP_SUB: u8 = 0x21;
const OP_MUL: u8 = 0x22;
const OP_UDIV: u8 = 0x23;
const OP_SDIV: u8 = 0x24;
const OP_UREM: u8 = 0x25;
const OP_SREM: u8 = 0x26;

// Bitwise / shift
const OP_AND: u8 = 0x28;
const OP_OR: u8 = 0x29;
const OP_XOR: u8 = 0x2A;
const OP_SHL: u8 = 0x2B;
const OP_LSHR: u8 = 0x2C;
const OP_ASHR: u8 = 0x2D;

// Comparison
const OP_ICMP: u8 = 0x30;
const OP_FCMP: u8 = 0x31;

// Float arithmetic
const OP_FADD: u8 = 0x38;
const OP_FSUB: u8 = 0x39;
const OP_FMUL: u8 = 0x3A;
const OP_FDIV: u8 = 0x3B;

// Type conversion
const OP_ZEXT: u8 = 0x40;
const OP_SEXT: u8 = 0x41;
const OP_TRUNC: u8 = 0x42;
const OP_FPEXT: u8 = 0x43;
const OP_FPTRUNC: u8 = 0x44;
const OP_INTTOPTR: u8 = 0x45;
const OP_FPTOSI: u8 = 0x46;
const OP_SITOFP: u8 = 0x47;

// SSA / data movement
const OP_PHI: u8 = 0x50;
const OP_SELECT: u8 = 0x51;
const OP_MOV: u8 = 0x52;
const OP_CONST: u8 = 0x53;

// ICmp condition codes (sub-opcode)
const ICMP_EQ: u8 = 0;
const ICMP_NE: u8 = 1;
const ICMP_SLT: u8 = 2;
const ICMP_SLE: u8 = 3;
const ICMP_SGT: u8 = 4;
const ICMP_SGE: u8 = 5;
const ICMP_ULT: u8 = 6;
const ICMP_ULE: u8 = 7;
const ICMP_UGT: u8 = 8;
const ICMP_UGE: u8 = 9;

// FCmp condition codes (sub-opcode)
const FCMP_OEQ: u8 = 1;
const FCMP_OGT: u8 = 2;
const FCMP_OGE: u8 = 3;
const FCMP_OLT: u8 = 4;
const FCMP_OLE: u8 = 5;
const FCMP_ONE: u8 = 6;

// Quantum op IDs — must match `shader_types.rs` `OpID` and `GATE_MAP` in `_adaptive_pass.py`.
const OPID_RESETZ: u64 = 1;
const OPID_X: u64 = 2;
const OPID_Y: u64 = 3;
const OPID_Z: u64 = 4;
const OPID_H: u64 = 5;
const OPID_S: u64 = 6;
const OPID_S_ADJ: u64 = 7;
const OPID_T: u64 = 8;
const OPID_T_ADJ: u64 = 9;
const OPID_SX: u64 = 10;
const OPID_SX_ADJ: u64 = 11;
const OPID_RX: u64 = 12;
const OPID_RY: u64 = 13;
const OPID_RZ: u64 = 14;
const OPID_CX: u64 = 15;
const OPID_CZ: u64 = 16;
const OPID_RXX: u64 = 17;
const OPID_RYY: u64 = 18;
const OPID_RZZ: u64 = 19;
const OPID_MZ: u64 = 21;
const OPID_MRESETZ: u64 = 22;
const OPID_SWAP: u64 = 24;
const OPID_MOVE: u64 = 28;
const OPID_CY: u64 = 29;
const OPID_CORRELATED_NOISE: u64 = 131;

// Sentinel
const VOID_RETURN: u64 = 0xFFFF_FFFF_FFFF_FFFF;

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

struct CallStackFrame {
    block_id: u64,
    return_pc: u64,
    return_reg: u64,
}

struct Runtime {
    pc: u64,
    current_block_id: u64,
    previous_block_id: u64,
    exit_code: u64,
    registers: Vec<u64>,
    call_stack: Vec<CallStackFrame>,
}

impl Runtime {
    fn new(num_registers: u32, entry_block: u64, entry_pc: u64) -> Self {
        Self {
            pc: entry_pc,
            current_block_id: entry_block,
            previous_block_id: 0,
            exit_code: 0,
            registers: vec![0; num_registers as usize],
            call_stack: Vec::with_capacity(128),
        }
    }

    fn read_reg(&self, reg: u64) -> u64 {
        self.registers[reg as usize]
    }

    fn write_reg(&mut self, reg: u64, val: u64) {
        self.registers[reg as usize] = val;
    }

    fn resolve_u64(&self, operand: u64, flags: u64, operand_idx: u64) -> u64 {
        let imm_flag = match operand_idx {
            0 => FLAG_SRC0_IMM,
            1 => FLAG_SRC1_IMM,
            2 => FLAG_DST_IMM,
            3 => FLAG_AUX0_IMM,
            4 => FLAG_AUX1_IMM,
            5 => FLAG_AUX2_IMM,
            6 => FLAG_AUX3_IMM,
            _ => panic!("invalid operand index {operand_idx}"),
        };
        if flags & imm_flag != 0 {
            operand
        } else {
            self.read_reg(operand)
        }
    }

    fn resolve_i64(&self, operand: u64, flags: u64, operand_idx: u64) -> i64 {
        self.resolve_u64(operand, flags, operand_idx) as i64
    }

    fn resolve_f64(&self, operand: u64, flags: u64, operand_idx: u64) -> f64 {
        f64::from_bits(self.resolve_u64(operand, flags, operand_idx))
    }

    fn write_f64(&mut self, reg: u64, val: f64) {
        self.write_reg(reg, val.to_bits());
    }
}

// ---------------------------------------------------------------------------
// Quantum op dispatch
// ---------------------------------------------------------------------------

fn dispatch_quantum_gate<S: Simulator>(
    program: &AdaptiveProgram<u64>,
    sim: &mut S,
    instr: &Instruction<u64>,
    rt: &Runtime,
) {
    let op_idx = instr.aux0 as usize;
    let op = &program.quantum_ops[op_idx];
    let op_id = op.op_id;

    match op_id {
        OPID_CORRELATED_NOISE => {
            let qubit_count = rt.resolve_u64(instr.aux1, instr.opcode, 4) as usize;
            let arg_offset = rt.resolve_u64(instr.aux2, instr.opcode, 5) as usize;
            let table_id = op.q1 as u32;
            let targets: Vec<usize> = (0..qubit_count)
                .map(|i| rt.read_reg(program.call_args[arg_offset + i]) as usize)
                .collect();
            sim.correlated_noise_intrinsic(table_id, &targets);
        }
        _ => {
            let q1 = rt.resolve_u64(instr.aux1, instr.opcode, 4) as usize;
            let q2 = rt.resolve_u64(instr.aux2, instr.opcode, 5) as usize;
            let angle = op.angle;
            match op_id {
                OPID_X => sim.x(q1),
                OPID_Y => sim.y(q1),
                OPID_Z => sim.z(q1),
                OPID_H => sim.h(q1),
                OPID_S => sim.s(q1),
                OPID_S_ADJ => sim.s_adj(q1),
                OPID_T => sim.t(q1),
                OPID_T_ADJ => sim.t_adj(q1),
                OPID_SX => sim.sx(q1),
                OPID_SX_ADJ => sim.sx_adj(q1),
                OPID_RX => sim.rx(angle, q1),
                OPID_RY => sim.ry(angle, q1),
                OPID_RZ => sim.rz(angle, q1),
                OPID_CX => sim.cx(q1, q2),
                OPID_CY => sim.cy(q1, q2),
                OPID_CZ => sim.cz(q1, q2),
                OPID_RXX => sim.rxx(angle, q1, q2),
                OPID_RYY => sim.ryy(angle, q1, q2),
                OPID_RZZ => sim.rzz(angle, q1, q2),
                OPID_SWAP => sim.swap(q1, q2),
                OPID_MOVE => sim.mov(q1),
                _ => panic!("unsupported quantum gate op_id={op_id}"),
            }
        }
    }
}

fn dispatch_measure<S: Simulator>(
    program: &AdaptiveProgram<u64>,
    sim: &mut S,
    instr: &Instruction<u64>,
    rt: &Runtime,
) {
    let op_idx = instr.aux0 as usize;
    let op = &program.quantum_ops[op_idx];
    let qubit = rt.resolve_u64(instr.aux1, instr.opcode, 4) as usize;
    let result_id = rt.resolve_u64(instr.aux2, instr.opcode, 5) as usize;

    match op.op_id {
        OPID_MZ => sim.mz(qubit, result_id),
        OPID_MRESETZ => sim.mresetz(qubit, result_id),
        _ => panic!("unsupported measure op_id={}", op.op_id),
    }
}

fn dispatch_reset<S: Simulator>(
    program: &AdaptiveProgram<u64>,
    sim: &mut S,
    instr: &Instruction<u64>,
    rt: &Runtime,
) {
    let op_idx = instr.aux0 as usize;
    let op = &program.quantum_ops[op_idx];
    let qubit = rt.resolve_u64(instr.aux1, instr.opcode, 4) as usize;

    match op.op_id {
        OPID_RESETZ => sim.resetz(qubit),
        _ => panic!("unsupported reset op_id={}", op.op_id),
    }
}

// ---------------------------------------------------------------------------
// Main interpreter entry point
// ---------------------------------------------------------------------------

pub fn run_shot<S: Simulator>(program: &AdaptiveProgram<u64>, sim: &mut S) {
    const MAX_STEPS: u64 = 10_000_000;

    let entry_pc = program.block_table[program.entry_block as usize].instr_offset;
    let mut rt = Runtime::new(program.num_registers, program.entry_block, entry_pc);

    for _ in 0..MAX_STEPS {
        let instr = program.instructions[rt.pc as usize];
        let op = instr.primary_opcode();
        let subcode = instr.sub_opcode();
        let flags = instr.opcode;

        match op {
            OP_NOP => rt.pc += 1,

            OP_RET => {
                rt.exit_code = rt.resolve_u64(instr.dst, flags, 2);
                break;
            }

            OP_JUMP => {
                rt.previous_block_id = rt.current_block_id;
                rt.current_block_id = instr.dst;
                rt.pc = block_pc(program, rt.current_block_id);
            }

            OP_BRANCH => {
                let cond = rt.resolve_u64(instr.src0, flags, 0) != 0;
                let next_block = if cond { instr.aux0 } else { instr.aux1 };
                rt.previous_block_id = rt.current_block_id;
                rt.current_block_id = next_block;
                rt.pc = block_pc(program, rt.current_block_id);
            }

            OP_SWITCH => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                let default_block = instr.aux0;
                let case_offset = instr.aux1 as usize;
                let case_count = instr.aux2 as usize;
                let mut target_block = default_block;
                for i in 0..case_count {
                    let entry = program.switch_cases[case_offset + i];
                    if entry.case_val == val {
                        target_block = entry.target_block;
                        break;
                    }
                }
                rt.previous_block_id = rt.current_block_id;
                rt.current_block_id = target_block;
                rt.pc = block_pc(program, rt.current_block_id);
            }

            OP_CALL => {
                let func_id = instr.aux0 as usize;
                let arg_count = instr.aux1 as usize;
                let arg_offset = instr.aux2 as usize;
                let func = program.function_table[func_id];

                rt.call_stack.push(CallStackFrame {
                    block_id: rt.current_block_id,
                    return_pc: rt.pc + 1,
                    return_reg: instr.dst,
                });

                let param_base = func.param_base_reg;
                for i in 0..arg_count {
                    let arg_reg = program.call_args[arg_offset + i];
                    let val = rt.read_reg(arg_reg);
                    rt.write_reg(param_base + i as u64, val);
                }

                rt.current_block_id = func.entry_block_id;
                rt.pc = block_pc(program, rt.current_block_id);
            }

            OP_CALL_RETURN => {
                let frame = rt.call_stack.pop().expect("call stack underflow");
                let return_block = frame.block_id;
                let return_pc = frame.return_pc;
                let return_reg = frame.return_reg;

                rt.current_block_id = return_block;
                rt.pc = return_pc;
                if return_reg != VOID_RETURN {
                    let ret_val = rt.resolve_u64(instr.src0, flags, 0);
                    rt.write_reg(return_reg, ret_val);
                }
            }

            // ----- Quantum operations -----
            OP_QUANTUM_GATE => {
                dispatch_quantum_gate(program, sim, &instr, &rt);
                rt.pc += 1;
            }

            OP_MEASURE => {
                dispatch_measure(program, sim, &instr, &rt);
                rt.pc += 1;
            }

            OP_RESET => {
                dispatch_reset(program, sim, &instr, &rt);
                rt.pc += 1;
            }

            OP_READ_RESULT => {
                let result_id = rt.resolve_u64(instr.src0, flags, 0) as usize;
                let measurements = sim.measurements();
                let val = if result_id < measurements.len() {
                    match measurements[result_id] {
                        MeasurementResult::One => 1u64,
                        _ => 0u64,
                    }
                } else {
                    0u64
                };
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_RECORD_OUTPUT => {
                // No-op on CPU — results are read from the simulator directly.
                rt.pc += 1;
            }

            // ----- Integer arithmetic -----
            OP_ADD => {
                let a = rt.resolve_i64(instr.src0, flags, 0);
                let b = rt.resolve_i64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a.wrapping_add(b) as u64);
                rt.pc += 1;
            }

            OP_SUB => {
                let a = rt.resolve_i64(instr.src0, flags, 0);
                let b = rt.resolve_i64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a.wrapping_sub(b) as u64);
                rt.pc += 1;
            }

            OP_MUL => {
                let a = rt.resolve_i64(instr.src0, flags, 0);
                let b = rt.resolve_i64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a.wrapping_mul(b) as u64);
                rt.pc += 1;
            }

            OP_UDIV => {
                let a = rt.resolve_u64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a / b);
                rt.pc += 1;
            }

            OP_SDIV => {
                let a = rt.resolve_i64(instr.src0, flags, 0);
                let b = rt.resolve_i64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a.wrapping_div(b) as u64);
                rt.pc += 1;
            }

            OP_UREM => {
                let a = rt.resolve_u64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a % b);
                rt.pc += 1;
            }

            OP_SREM => {
                let a = rt.resolve_i64(instr.src0, flags, 0);
                let b = rt.resolve_i64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a.wrapping_rem(b) as u64);
                rt.pc += 1;
            }

            // ----- Bitwise / shift -----
            OP_AND => {
                let a = rt.resolve_u64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a & b);
                rt.pc += 1;
            }

            OP_OR => {
                let a = rt.resolve_u64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a | b);
                rt.pc += 1;
            }

            OP_XOR => {
                let a = rt.resolve_u64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1);
                rt.write_reg(instr.dst, a ^ b);
                rt.pc += 1;
            }

            OP_SHL => {
                let a = rt.resolve_u64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1) as u32;
                rt.write_reg(instr.dst, a.wrapping_shl(b));
                rt.pc += 1;
            }

            OP_LSHR => {
                let a = rt.resolve_u64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1) as u32;
                rt.write_reg(instr.dst, a.wrapping_shr(b));
                rt.pc += 1;
            }

            OP_ASHR => {
                let a = rt.resolve_i64(instr.src0, flags, 0);
                let b = rt.resolve_u64(instr.src1, flags, 1) as u32;
                rt.write_reg(instr.dst, a.wrapping_shr(b) as u64);
                rt.pc += 1;
            }

            // ----- Integer comparison -----
            OP_ICMP => {
                let a = rt.resolve_i64(instr.src0, flags, 0);
                let b = rt.resolve_i64(instr.src1, flags, 1);
                let result = match subcode {
                    ICMP_EQ => a == b,
                    ICMP_NE => a != b,
                    ICMP_SLT => a < b,
                    ICMP_SLE => a <= b,
                    ICMP_SGT => a > b,
                    ICMP_SGE => a >= b,
                    ICMP_ULT => (a as u64) < (b as u64),
                    ICMP_ULE => (a as u64) <= (b as u64),
                    ICMP_UGT => (a as u64) > (b as u64),
                    ICMP_UGE => (a as u64) >= (b as u64),
                    _ => panic!("unsupported icmp condition code {subcode}"),
                };
                rt.write_reg(instr.dst, u64::from(result));
                rt.pc += 1;
            }

            // ----- Float comparison -----
            OP_FCMP => {
                let a = rt.resolve_f64(instr.src0, flags, 0);
                let b = rt.resolve_f64(instr.src1, flags, 1);
                let result = match subcode {
                    FCMP_OEQ => a == b,
                    FCMP_ONE => a != b,
                    FCMP_OLT => a < b,
                    FCMP_OLE => a <= b,
                    FCMP_OGT => a > b,
                    FCMP_OGE => a >= b,
                    _ => panic!("unsupported fcmp condition code {subcode}"),
                };
                rt.write_reg(instr.dst, u64::from(result));
                rt.pc += 1;
            }

            // ----- Float arithmetic -----
            OP_FADD => {
                let a = rt.resolve_f64(instr.src0, flags, 0);
                let b = rt.resolve_f64(instr.src1, flags, 1);
                rt.write_f64(instr.dst, a + b);
                rt.pc += 1;
            }

            OP_FSUB => {
                let a = rt.resolve_f64(instr.src0, flags, 0);
                let b = rt.resolve_f64(instr.src1, flags, 1);
                rt.write_f64(instr.dst, a - b);
                rt.pc += 1;
            }

            OP_FMUL => {
                let a = rt.resolve_f64(instr.src0, flags, 0);
                let b = rt.resolve_f64(instr.src1, flags, 1);
                rt.write_f64(instr.dst, a * b);
                rt.pc += 1;
            }

            OP_FDIV => {
                let a = rt.resolve_f64(instr.src0, flags, 0);
                let b = rt.resolve_f64(instr.src1, flags, 1);
                rt.write_f64(instr.dst, a / b);
                rt.pc += 1;
            }

            // ----- Type conversion -----
            OP_ZEXT => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_SEXT => {
                let val = rt.resolve_i64(instr.src0, flags, 0);
                let src_bits = instr.aux0 as u32;
                let result = if src_bits > 0 && src_bits < 64 {
                    let shift = 64 - src_bits;
                    (val.wrapping_shl(shift)).wrapping_shr(shift)
                } else {
                    val
                };
                rt.write_reg(instr.dst, result as u64);
                rt.pc += 1;
            }

            OP_TRUNC => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_FPEXT | OP_FPTRUNC => {
                let val = rt.resolve_f64(instr.src0, flags, 0);
                rt.write_f64(instr.dst, val);
                rt.pc += 1;
            }

            OP_INTTOPTR => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_FPTOSI => {
                let val = rt.resolve_f64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val as i64 as u64);
                rt.pc += 1;
            }

            OP_SITOFP => {
                let val = rt.resolve_i64(instr.src0, flags, 0);
                rt.write_f64(instr.dst, val as f64);
                rt.pc += 1;
            }

            // ----- PHI node -----
            OP_PHI => {
                let offset = instr.aux0 as usize;
                let count = instr.aux1 as usize;
                for i in 0..count {
                    let entry = program.phi_entries[offset + i];
                    if entry.block_id == rt.previous_block_id {
                        let val = rt.read_reg(entry.val_reg);
                        rt.write_reg(instr.dst, val);
                        break;
                    }
                }
                rt.pc += 1;
            }

            // ----- Data movement -----
            OP_SELECT => {
                let cond = rt.resolve_u64(instr.src0, flags, 0) != 0;
                let true_val = rt.resolve_u64(instr.aux0, flags, 3);
                let false_val = rt.resolve_u64(instr.aux1, flags, 4);
                rt.write_reg(instr.dst, if cond { true_val } else { false_val });
                rt.pc += 1;
            }

            OP_MOV => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_CONST => {
                rt.write_reg(instr.dst, instr.src0);
                rt.pc += 1;
            }

            _ => panic!("unsupported opcode 0x{op:02X} at pc={}", rt.pc),
        }
    }
}

fn block_pc(program: &AdaptiveProgram<u64>, block_id: u64) -> u64 {
    program.block_table[block_id as usize].instr_offset
}
