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
    clippy::cast_sign_loss
)]

use crate::{
    MeasurementResult, OutputRecord, QubitID, ResultID, Simulator,
    bytecode::{AdaptiveProgram, Instruction},
};

use super::encoding::{
    FCMP_OEQ, FCMP_OGE, FCMP_OGT, FCMP_OLE, FCMP_OLT, FCMP_ONE, FLAG_AUX0_IMM, FLAG_AUX1_IMM,
    FLAG_AUX2_IMM, FLAG_AUX3_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_EQ, ICMP_NE,
    ICMP_SGE, ICMP_SGT, ICMP_SLE, ICMP_SLT, ICMP_UGE, ICMP_UGT, ICMP_ULE, ICMP_ULT, OP_ADD,
    OP_ALLOCA, OP_AND, OP_ASHR, OP_BRANCH, OP_CALL, OP_CALL_RETURN, OP_CONST, OP_FADD, OP_FCMP,
    OP_FDIV, OP_FMUL, OP_FPEXT, OP_FPTOSI, OP_FPTOUI, OP_FPTRUNC, OP_FREM, OP_FSUB, OP_GEP,
    OP_ICMP, OP_INTTOPTR, OP_JUMP, OP_LOAD, OP_LSHR, OP_MEASURE, OP_MOV, OP_MUL, OP_NOP, OP_OR,
    OP_PEEK_LOSS, OP_PHI, OP_QUANTUM_GATE, OP_READ_LOSS, OP_READ_RESULT, OP_READOUT_NOISE,
    OP_RECORD_OUTPUT, OP_RESET, OP_RET, OP_SDIV, OP_SELECT, OP_SEXT, OP_SHL, OP_SITOFP, OP_SREM,
    OP_STORE, OP_SUB, OP_SWITCH, OP_TRUNC, OP_UDIV, OP_UITOFP, OP_UREM, OP_WRITE_RESULT, OP_XOR,
    OP_ZEXT, OPID_CORRELATED_NOISE, OPID_CX, OPID_CY, OPID_CZ, OPID_H, OPID_MOVE, OPID_MRESETZ,
    OPID_MZ, OPID_RESETZ, OPID_RX, OPID_RXX, OPID_RY, OPID_RYY, OPID_RZ, OPID_RZZ, OPID_S,
    OPID_S_ADJ, OPID_SWAP, OPID_SX, OPID_SX_ADJ, OPID_T, OPID_T_ADJ, OPID_X, OPID_Y, OPID_Z,
    VOID_RETURN,
};

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

struct CallStackFrame {
    block_id: u64,
    previous_block_id: u64,
    return_pc: u64,
    return_reg: u64,
    registers: Vec<u64>,
    memory_size: usize,
}

struct Runtime {
    pc: u64,
    current_block_id: u64,
    previous_block_id: u64,
    exit_code: u64,
    registers: Vec<u64>,
    call_stack: Vec<CallStackFrame>,
    memory: Vec<u64>,
}

impl Runtime {
    fn new(num_registers: u32, entry_block: u64, entry_pc: u64, constant_data: &[u64]) -> Self {
        let mut memory = Vec::with_capacity(constant_data.len() + 256);
        memory.extend_from_slice(constant_data);
        Self {
            pc: entry_pc,
            current_block_id: entry_block,
            previous_block_id: 0,
            exit_code: 0,
            registers: vec![0; num_registers as usize],
            call_stack: Vec::with_capacity(128),
            memory,
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

    fn read_memory(&self, addr: u64) -> u64 {
        self.memory[addr as usize]
    }

    fn write_memory(&mut self, addr: u64, val: u64) {
        let idx = addr as usize;
        if idx >= self.memory.len() {
            self.memory.resize(idx + 1, 0);
        }
        self.memory[idx] = val;
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

    if op_id == OPID_CORRELATED_NOISE {
        let qubit_count = rt.resolve_u64(instr.aux1, instr.opcode, 4) as usize;
        let arg_offset = rt.resolve_u64(instr.aux2, instr.opcode, 5) as usize;
        let table_id = op.q1 as u32;
        let targets: Vec<QubitID> = (0..qubit_count)
            .map(|i| rt.read_reg(program.call_args[arg_offset + i]) as QubitID)
            .collect();
        sim.correlated_noise_intrinsic(table_id, &targets);
    } else {
        let angle = rt.resolve_f64(instr.src0, instr.opcode, 0);
        let q1 = rt.resolve_u64(instr.aux1, instr.opcode, 4) as QubitID;
        let q2 = rt.resolve_u64(instr.aux2, instr.opcode, 5) as QubitID;
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

fn dispatch_measure<S: Simulator>(
    program: &AdaptiveProgram<u64>,
    sim: &mut S,
    instr: &Instruction<u64>,
    rt: &Runtime,
) {
    let op_idx = instr.aux0 as usize;
    let op = &program.quantum_ops[op_idx];
    let qubit = rt.resolve_u64(instr.aux1, instr.opcode, 4) as QubitID;
    let result_id = rt.resolve_u64(instr.aux2, instr.opcode, 5) as ResultID;

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
    let qubit = rt.resolve_u64(instr.aux1, instr.opcode, 4) as QubitID;

    match op.op_id {
        OPID_RESETZ => sim.resetz(qubit),
        _ => panic!("unsupported reset op_id={}", op.op_id),
    }
}

// ---------------------------------------------------------------------------
// Main interpreter entry point
// ---------------------------------------------------------------------------

/// Runs a single shot of `program` on `sim` and returns the recorded outputs.
///
/// The interpreter runs until the program returns; a program that never
/// returns will not terminate.
#[allow(clippy::too_many_lines)]
pub fn run_shot<S: Simulator>(program: &AdaptiveProgram<u64>, sim: &mut S) -> Vec<OutputRecord> {
    let entry_pc = program.block_table[program.entry_block as usize].instr_offset;
    let mut rt = Runtime::new(
        program.num_registers,
        program.entry_block,
        entry_pc,
        &program.constant_data,
    );

    // Output records captured during this shot, in execution order.
    let mut records: Vec<OutputRecord> = Vec::new();

    loop {
        let instr = program.instructions[rt.pc as usize];
        let op = instr.primary_opcode();
        let subcode = instr.sub_opcode();
        let flags = instr.opcode;

        match op {
            OP_NOP => rt.pc += 1,

            OP_RET => {
                rt.exit_code = rt.resolve_u64(instr.dst, flags, 2);
                return records;
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
                    previous_block_id: rt.previous_block_id,
                    return_pc: rt.pc + 1,
                    return_reg: instr.dst,
                    registers: rt.registers.clone(),
                    memory_size: rt.memory.len(),
                });

                let param_base = func.param_base_reg;
                for i in 0..arg_count {
                    let arg_reg = program.call_args[arg_offset + i];
                    // Read the saved caller state so overlapping argument registers are safe.
                    let val = rt.call_stack.last().expect("caller frame exists").registers
                        [arg_reg as usize];
                    rt.write_reg(param_base + i as u64, val);
                }

                rt.current_block_id = func.entry_block_id;
                rt.pc = block_pc(program, rt.current_block_id);
            }

            OP_CALL_RETURN => {
                let frame = rt.call_stack.pop().expect("call stack underflow");
                let return_reg = frame.return_reg;
                let return_value =
                    (return_reg != VOID_RETURN).then(|| rt.resolve_u64(instr.src0, flags, 0));
                rt.current_block_id = frame.block_id;
                rt.previous_block_id = frame.previous_block_id;
                rt.pc = frame.return_pc;
                rt.registers = frame.registers;
                // Retain writes through caller-owned pointers; discard only callee allocations.
                rt.memory.truncate(frame.memory_size);
                if let Some(value) = return_value {
                    rt.write_reg(return_reg, value);
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
                let result_id = rt.resolve_u64(instr.src0, flags, 0) as ResultID;
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

            OP_WRITE_RESULT => {
                let value = match rt.resolve_u64(instr.src0, flags, 0) {
                    0 => false,
                    1 => true,
                    invalid => panic!("Invalid value for write_result: {invalid}"),
                };
                let result_id = rt.resolve_u64(instr.src1, flags, 1) as ResultID;
                sim.write_result(value, result_id);
                rt.pc += 1;
            }

            OP_READ_LOSS => {
                // Reports whether the measurement that produced this result
                // observed a lost qubit. The simulator records ``Loss`` in
                // its measurement buffer when the qubit was lost prior to
                // the measurement; here we simply project that to a 1/0 bool
                // for the program to branch on.
                let result_id = rt.resolve_u64(instr.src0, flags, 0) as ResultID;
                let measurements = sim.measurements();
                let val = u64::from(
                    result_id < measurements.len()
                        && matches!(measurements[result_id], MeasurementResult::Loss),
                );
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_PEEK_LOSS => {
                let target = rt.resolve_u64(instr.aux0, flags, 3) as QubitID;
                let result_id = rt.resolve_u64(instr.aux1, flags, 4) as ResultID;
                sim.peek_loss(target, result_id);
                rt.pc += 1;
            }

            OP_READOUT_NOISE => {
                let p_zero_as_one = rt.resolve_f64(instr.aux0, flags, 3);
                let p_one_as_zero = rt.resolve_f64(instr.aux1, flags, 4);
                let result_id = rt.resolve_u64(instr.aux2, flags, 5) as ResultID;
                sim.apply_readout_noise(p_zero_as_one, p_one_as_zero, result_id);
                rt.pc += 1;
            }

            OP_RECORD_OUTPUT => {
                // The record kind is encoded as an immediate in `aux1`. Every
                // leaf record is captured into the unified `records` stream in
                // execution order so results and classical values are handled
                // identically:
                //   0 = result, the value comes from the measurement buffer.
                //   3 = bool, 4 = int, 5 = double; classical values computed
                //       at runtime.
                //   1 = array, 2 = tuple; structural markers reconstructed by
                //       the host from the static QIR, so they are a no-op here.
                match instr.aux1 {
                    0 => {
                        let result_id = rt.resolve_u64(instr.src0, flags, 0) as ResultID;
                        let measurements = sim.measurements();
                        let value = measurements
                            .get(result_id)
                            .copied()
                            .unwrap_or(MeasurementResult::Zero);
                        records.push(OutputRecord::Result(value));
                    }
                    3 => records.push(OutputRecord::Bool(
                        rt.resolve_u64(instr.src0, flags, 0) != 0,
                    )),
                    4 => records.push(OutputRecord::Int(rt.resolve_i64(instr.src0, flags, 0))),
                    5 => records.push(OutputRecord::Double(rt.resolve_f64(instr.src0, flags, 0))),
                    _ => {}
                }
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
                #[allow(clippy::float_cmp)]
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

            // Rust documentation says f64::rem is implemented as
            // `x - (x / y).trunc() * y`
            // which has the same semantics as C's `fmod`,
            // satisfying LLVM's semantics for `FREM`.
            OP_FREM => {
                let a = rt.resolve_f64(instr.src0, flags, 0);
                let b = rt.resolve_f64(instr.src1, flags, 1);
                rt.write_f64(instr.dst, a % b);
                rt.pc += 1;
            }

            // ----- Type conversion -----
            OP_ZEXT | OP_TRUNC | OP_INTTOPTR => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_SEXT => {
                // Sign-extend a narrower integer (src_bits wide) to a full i64.
                //
                // Uses the shift-left-then-arithmetic-shift-right trick:
                //   1. Shift left by (64 - src_bits) to move the narrow sign bit
                //      into bit 63 (the i64 sign position).
                //   2. Arithmetic shift right by the same amount to replicate the
                //      sign bit across all upper bits.
                //
                // Example: sext i1 true (value 1, src_bits=1)
                //   shift = 63
                //   1 << 63 = 0x8000..0  (sign bit set)
                //   >> 63   = 0xFFFF..F  (-1 as i64)
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

            OP_FPEXT | OP_FPTRUNC => {
                let val = rt.resolve_f64(instr.src0, flags, 0);
                rt.write_f64(instr.dst, val);
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

            OP_FPTOUI => {
                let val = rt.resolve_f64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val as u64);
                rt.pc += 1;
            }

            OP_UITOFP => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
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
            #[allow(clippy::match_same_arms)]
            OP_MOV => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_SELECT => {
                let cond = rt.resolve_u64(instr.src0, flags, 0) != 0;
                let true_val = rt.resolve_u64(instr.aux0, flags, 3);
                let false_val = rt.resolve_u64(instr.aux1, flags, 4);
                rt.write_reg(instr.dst, if cond { true_val } else { false_val });
                rt.pc += 1;
            }

            OP_CONST => {
                rt.write_reg(instr.dst, instr.src0);
                rt.pc += 1;
            }

            // ----- Memory operations -----
            OP_ALLOCA => {
                let num_words = rt.resolve_u64(instr.src0, flags, 0);
                let addr = rt.memory.len() as u64;
                let end = (addr + num_words) as usize;
                if end > rt.memory.len() {
                    rt.memory.resize(end, 0);
                }
                rt.write_reg(instr.dst, addr);
                rt.pc += 1;
            }

            OP_LOAD => {
                let addr = rt.resolve_u64(instr.src0, flags, 0);
                let val = rt.read_memory(addr);
                rt.write_reg(instr.dst, val);
                rt.pc += 1;
            }

            OP_STORE => {
                let val = rt.resolve_u64(instr.src0, flags, 0);
                let addr = rt.resolve_u64(instr.src1, flags, 1);
                rt.write_memory(addr, val);
                rt.pc += 1;
            }

            OP_GEP => {
                let base = rt.resolve_u64(instr.src0, flags, 0);
                let index = rt.resolve_u64(instr.src1, flags, 1);
                let elem_size = rt.resolve_u64(instr.aux0, flags, 3);
                let addr = base.wrapping_add(index.wrapping_mul(elem_size));
                rt.write_reg(instr.dst, addr);
                rt.pc += 1;
            }

            _ => panic!("unsupported opcode 0x{op:02X} at pc={}", rt.pc),
        }
    }
}

fn block_pc(program: &AdaptiveProgram<u64>, block_id: u64) -> u64 {
    program.block_table[block_id as usize].instr_offset
}
