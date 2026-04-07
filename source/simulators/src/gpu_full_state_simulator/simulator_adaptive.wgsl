// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// common.wgsl is appended to the beginning of this file at runtime.

const ERR_CALL_STACK_OVERFLOW = 3u;
const ERR_CALL_STACK_UNDERFLOW = 4u;
const ERR_INVALID_INSTRUCTION = 5u;

@group(0) @binding(0)
var<storage, read_write> workgroup_collation: WorkgroupCollationBuffer;
// Around 128 max partitions times 27 qubits times 8 bytes = 27 KB max size

struct QubitState {
    zero_probability: f32,
    one_probability: f32,
    heat: f32, // -1.0 = lost
    idle_since: f32,
}

// Used to track state for the random number generator per shot. See `next_rand_f32` later for details.
struct xorwow_state {
    counter: u32,
    x: array<u32, 5>
}

// Buffer containing the state for each shot to execute per kernel dispatch
// An instance of this is tracked on the GPU for every active shot
struct ShotData {
    shot_id: u32,
    next_op_idx: u32,

    // The below random numbers will be initialized from the RNG per operation in the 'prepare_op' stage
    // Then the 'execute_op' stage will read these precomputed random numbers for noise modeling
    rng_state: xorwow_state, // 6 x u32
    rand_pauli: f32,
    rand_damping: f32,
    rand_dephase: f32,
    rand_measure: f32,
    rand_loss: f32,

    // The type of the next operation to execute. This will be OPID_SHOT_BUFF_* if it should use the unitary from the op buffer
    op_type: u32,
    op_idx: u32,

    duration: f32, // Total duration of the shot so far, used for time-dependent noise modeling and shot estimations
    renormalize: f32, // Value to renormalize the state vector by on next execute (1.0 = no renormalization needed)

    // For quick testing during execution to enable skipping blocks of entries
    // TODO: Actually use these masks during execution to skip unneeded work
    qubit_is_0_mask: u32, // Bitmask for which qubits are currently in |0> state
    qubit_is_1_mask: u32, // Bitmask for which qubits are currently in |1> state

    // Track which qubit probabilities were updated in the last operation (to collate on next prepare_op)
    qubits_updated_last_op_mask: u32,
    // 20 x 4 bytes to this point = 80 bytes

    // Track the per-qubit probabilities for optimization of measurement sampling and noise modeling
    qubit_state: array<QubitState, MAX_QUBIT_COUNT>, // 27 x 16 bytes = 432 bytes
    // 512 bytes to this point

    // Map this to the Op structure for ease of use
    unitary: array<vec2f, 16>, // For MAT1Q and MAT2Q ops.

    // Adaptive interpreter state (embedded to reduce storage buffer count).
    // This is initialized by the host after the GPU init kernel runs.
    interp: InterpreterState,
}
// See https://www.w3.org/TR/WGSL/#structure-member-layout for alignment rules

@group(0) @binding(1)
var<storage, read_write> shots: array<ShotData>;

// Buffer containing the list of operations (gates and noise) that make up the program to simulate
struct Op {
    id: u32,
    q1: u32,
    q2: u32,
    q3: u32,
    // Entries in the unitary are: 00, 01, 02, 03, 10, 11, 12, 13, 20, ..., 32, 33
    // 1q matrix elements are stored in: 00, 01, 10, 11 (i.e., indices 0, 1, 4, and 5)
    unitary: array<vec2f, 16>,
} // Struct size: 4 * 4 + 16 * 8 = 144 bytes (which is aligned to 16 bytes)

@group(0) @binding(2)
var<storage, read> ops: array<Op>;

// The one large buffer of state vector amplitudes. (Partitioned into multiple shots)
@group(0) @binding(3)
var<storage, read_write> stateVector: array<vec2f>;

// Buffer for storing measurement results per shot
@group(0) @binding(4)
var<storage, read_write> results: array<atomic<u32>>;

// When an error occurs, the below diagnostic data structure is used to store information about the error
struct DiagnosticData {
    error_code: atomic<u32>,
    termination_count: atomic<u32>,
    extra1: u32,
    extra2: f32,
    extra3: f32,
    _padding: u32,
    shot: ShotData, // 640 bytes
    op: Op,         // 144 bytes
    // Below is usually 6,912 bytes (size = THREADS_PER_WORKGROUP (32) * (8 * MAX_QUBIT_COUNT (27))
    workgroup_probabilities: array<QubitProbabilityPerThread, THREADS_PER_WORKGROUP>,
    // Below is usually 27,648 bytes (1 << u32(MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP)) * (8 * MAX_QUBIT_COUNT) bytes
    collation_buffer: WorkgroupCollationBuffer,
};

@group(0) @binding(5)
var<storage, read_write> diagnostics: DiagnosticData;

struct Uniforms {
    batch_start_shot_id: i32,
    rng_seed: u32,
}

@group(0) @binding(6)
var<uniform> uniforms: Uniforms;

struct NoiseTableMetadata {
    /// The total probability of any noise (i.e. sum of all noise entries) in `Q1.63` format
    noise_probability_lo: u32,
    noise_probability_hi: u32,
    /// The start offset of this table's entries in the global `NoiseTableEntry` array
    start_offset: u32,
    /// The number of entries in this noise table
    entry_count: u32,
}

struct NoiseTableEntry {
    /// The correlated pauli string as bits (2 bits per qubit). If bit 0 is set, then it has bit-flip
    /// noise, and if bit 1 is set then it has phase-flip noise. e.g., `110001 == "YIX"`
    paulis_lo: u32,
    paulis_hi: u32,
    /// The probability of the noise occurring in `Q1_63` format. This is a float format where the high
    /// order bit (bit 63) has the value 1.0 (`2^0 / 1`), bit 62 has the value 0.5 (`2^1 / 1`), etc.
    /// all the way to bit 63 with a value of approx 1.0842e-19 (`2^63 / 1`). This gives a range of
    /// values from [0..2) with equal spacing of 1.0842e-19 between values (unlike float or double),
    /// which makes it more suitable for random numbers used to select between a large number of small
    /// probability entries.
    probability_lo: u32,
    probability_hi: u32,
}

// Template constants for noise table sizes (must be ≥ 1; host uses max(count,1)).
const NOISE_TABLE_COUNT: u32 = {{NOISE_TABLE_COUNT}};
const NOISE_ENTRY_COUNT: u32 = {{NOISE_ENTRY_COUNT}};

// BatchData holds all the read-only data shared across all shots in a batch.
struct BatchData {
    correlated_noise_tables: array<NoiseTableMetadata, NOISE_TABLE_COUNT>,
    correlated_noise_entries: array<NoiseTableEntry, NOISE_ENTRY_COUNT>,
    program: Program,
}

@group(0) @binding(7)
var<storage, read> batch_data: BatchData;


/// GPU bytecode instruction.
///
/// Layout:
/// - `opcode`: packed word — bits\[7:0\]=primary, bits\[15:8\]=sub/condition, bits\[23:16\]=flags
/// - `dst`: destination register or branch target
/// - `src0`, `src1`: source registers or immediates
/// - `aux0`-`aux3`: auxiliary fields (gate index, block ids, side-table offsets, etc.)
struct Instruction {
    opcode: u32,
    dst: u32,
    src0: u32,
    src1: u32,
    aux0: u32,
    aux1: u32,
    aux2: u32,
    aux3: u32,
}

struct Block {
    instr_offset: u32,
    instr_count: u32,
}

struct Function {
    entry_block_id: u32,
    param_count: u32,
    param_base_reg: u32,
    reserved: u32,
}

struct PhiNodeEntry {
    block_id: u32,
    val_reg: u32,
}

struct SwitchCase {
    case_val: u32,
    target_block: u32,
}

const INSTRUCTIONS_SIZE: u32 = {{INSTRUCTIONS_SIZE}};
const BLOCK_TABLE_SIZE: u32 = {{BLOCK_TABLE_SIZE}};
const FUNCTION_TABLE_SIZE: u32 = {{FUNCTION_TABLE_SIZE}};
const PHI_TABLE_SIZE: u32 = {{PHI_TABLE_SIZE}};
const SWITCH_CASES_SIZE: u32 = {{SWITCH_CASES_SIZE}};
const CALL_ARGS_SIZE: u32 = {{CALL_ARGS_SIZE}};

struct Program {
    /// Bytecode instructions.
    instructions: array<Instruction, INSTRUCTIONS_SIZE>,
    /// Block table: indexed by block ID.
    block_table: array<Block, BLOCK_TABLE_SIZE>,
    /// Function table.
    function_table: array<Function, FUNCTION_TABLE_SIZE>,
    /// Phi entries table: `[predecessor_block_id, value_register]` entries.
    phi_table: array<PhiNodeEntry, PHI_TABLE_SIZE>,
    /// Switch cases table: `[match_value, target_block]` entries.
    switch_table: array<SwitchCase, SWITCH_CASES_SIZE>,
    /// Call argument register indices.
    call_arg_table: array<u32, CALL_ARGS_SIZE>,
}

struct CallStackFrame {
    /// Resume on this block on return.
    block_id: u32,
    /// Instruction after the call.
    return_pc: u32,
    /// Where to write the return value.
    return_reg: u32,
    /// This is for alignment.
    reserved: u32,
}

// MAX_REGISTERS must be declared before InterpreterState which uses it.
const MAX_REGISTERS: u32 = {{MAX_REGISTERS}};

/// Per-shot interpreter state.
struct InterpreterState {
    /// Instruction index (absolute), PC stands for Program Counter.
    pc: u32,
    /// Current block ID.
    current_block_id: u32,
    ///Previous block ID (for phi resolution).
    previous_block_id: u32,
    /// 0=running, 1=quantum_pending, 2=terminated, 3=error, 4=yield.
    status: u32,
    /// Quantum op table index.
    pending_op_idx: u32,
    /// 0=gate, 1=measure, 2=reset.
    pending_op_type: u32,
    /// From ret instruction
    exit_code: u32,
    /// Call stack pointer.
    call_sp: u32,
    /// Call stack frames (4 u32 per frame × 14 frames = 56).
    call_stack_frames: array<CallStackFrame, 14>,
    /// Per-shot register file.
    registers: array<u32, MAX_REGISTERS>,
}

// -----------------------------------------------------------------------------
// Adaptive interpreter buffer bindings
// Termination counting is done via diagnostics.termination_count (binding 5).
// Interpreter state and registers are embedded in ShotData (binding 1).
// The program, noise tables, and noise entries are in batch_data (binding 7).
// -----------------------------------------------------------------------------

// -----------------------------------------------------------------------------
// Adaptive interpreter constants
// -----------------------------------------------------------------------------

const MAX_CLASSICAL_STEPS: u32 = 4096u;

// Status codes
const STATUS_RUNNING:          u32 = 0u;
const STATUS_QUANTUM_PENDING:  u32 = 1u;
const STATUS_TERMINATED:       u32 = 2u;
const STATUS_ERROR:            u32 = 3u;
const STATUS_YIELD:            u32 = 4u;

// -----------------------------------------------------------------------------
// Adaptive interpreter — opcodes
// -----------------------------------------------------------------------------

// Shared opcode constants for the Adaptive Profile QIR bytecode interpreter.
//
// These constants define the bytecode encoding used by the Python AdaptiveProfilePass
// (emitter). Values must stay in sync with the Python ``_adaptive_opcodes.py`` file.
//
// Opcode word layout::
//
//     bits [7:0]   = primary opcode
//     bits [15:8]  = sub-opcode / condition code
//     bits [23:16] = flags
//
// Compose via bitwise OR: ``opcode | (sub << 8) | flag``
// Example: ``OP_ICMP | (ICMP_SLE << 8) | FLAG_SRC1_IMM``

// -- Flags (pre-shifted to bit 16+) ------------------------------------------
const FLAG_SRC0_IMM: u32 = 1 << 16;  // src0 field is an immediate value, not a register
const FLAG_SRC1_IMM: u32 = 1 << 17;  // src1 field is an immediate value, not a register
const FLAG_DST_IMM:  u32 = 1 << 18;  // dst  field is an immediate value, not a register
const FLAG_AUX0_IMM: u32 = 1 << 19;  // aux0 field is an immediate value, not a register
const FLAG_AUX1_IMM: u32 = 1 << 20;  // aux1 field is an immediate value, not a register
const FLAG_AUX2_IMM: u32 = 1 << 21;  // aux2 field is an immediate value, not a register
const FLAG_AUX3_IMM: u32 = 1 << 22;  // aux3 field is an immediate value, not a register

// -- Control Flow -------------------------------------------------------------
const OP_NOP:           u32 = 0x00;
const OP_RET:           u32 = 0x02;
const OP_JUMP:          u32 = 0x04;
const OP_BRANCH:        u32 = 0x05;
const OP_SWITCH:        u32 = 0x06;
const OP_CALL:          u32 = 0x07;
const OP_CALL_RETURN:   u32 = 0x08;

// -- Quantum ------------------------------------------------------------------
const OP_QUANTUM_GATE:  u32 = 0x10;
const OP_MEASURE:       u32 = 0x11;
const OP_RESET:         u32 = 0x12;
const OP_READ_RESULT:   u32 = 0x13;
const OP_RECORD_OUTPUT: u32 = 0x14;

// -- Integer Arithmetic -------------------------------------------------------
const OP_ADD:           u32 = 0x20;
const OP_SUB:           u32 = 0x21;
const OP_MUL:           u32 = 0x22;
const OP_UDIV:          u32 = 0x23;
const OP_SDIV:          u32 = 0x24;
const OP_UREM:          u32 = 0x25;
const OP_SREM:          u32 = 0x26;

// -- Bitwise / Shift ---------------------------------------------------------
const OP_AND:           u32 = 0x28;
const OP_OR:            u32 = 0x29;
const OP_XOR:           u32 = 0x2A;
const OP_SHL:           u32 = 0x2B;
const OP_LSHR:          u32 = 0x2C;
const OP_ASHR:          u32 = 0x2D;

// -- Comparison ---------------------------------------------------------------
const OP_ICMP:          u32 = 0x30;
const OP_FCMP:          u32 = 0x31;

// -- Float Arithmetic ---------------------------------------------------------
const OP_FADD:          u32 = 0x38;
const OP_FSUB:          u32 = 0x39;
const OP_FMUL:          u32 = 0x3A;
const OP_FDIV:          u32 = 0x3B;

// -- Type Conversion ----------------------------------------------------------
const OP_ZEXT:          u32 = 0x40;
const OP_SEXT:          u32 = 0x41;
const OP_TRUNC:         u32 = 0x42;
const OP_FPEXT:         u32 = 0x43;
const OP_FPTRUNC:       u32 = 0x44;
const OP_INTTOPTR:      u32 = 0x45;
const OP_FPTOSI:        u32 = 0x46;
const OP_SITOFP:        u32 = 0x47;

// -- SSA / Data Movement -----------------------------------------------------
const OP_PHI:           u32 = 0x50;
const OP_SELECT:        u32 = 0x51;
const OP_MOV:           u32 = 0x52;
const OP_CONST:         u32 = 0x53;

// -- ICmp condition codes (sub-opcode, placed in bits[15:8] via << 8) ---------
// Reference: https://llvm.org/docs/LangRef.html#icmp-instruction
const ICMP_EQ:          u32 = 0;
const ICMP_NE:          u32 = 1;
const ICMP_SLT:         u32 = 2;
const ICMP_SLE:         u32 = 3;
const ICMP_SGT:         u32 = 4;
const ICMP_SGE:         u32 = 5;
const ICMP_ULT:         u32 = 6;
const ICMP_ULE:         u32 = 7;
const ICMP_UGT:         u32 = 8;
const ICMP_UGE:         u32 = 9;

// -- FCmp condition codes -----------------------------------------------------
// Reference: https://llvm.org/docs/LangRef.html#fcmp-instruction
const FCMP_FALSE:       u32 = 0;
const FCMP_OEQ:         u32 = 1;
const FCMP_OGT:         u32 = 2;
const FCMP_OGE:         u32 = 3;
const FCMP_OLT:         u32 = 4;
const FCMP_OLE:         u32 = 5;
const FCMP_ONE:         u32 = 6;
const FCMP_ORD:         u32 = 7;
const FCMP_UNO:         u32 = 8;
const FCMP_UEQ:         u32 = 9;
const FCMP_UGT:         u32 = 10;
const FCMP_UGE:         u32 = 11;
const FCMP_ULT:         u32 = 12;
const FCMP_ULE:         u32 = 13;
const FCMP_UNE:         u32 = 14;
const FCMP_TRUE:        u32 = 15;

// -- Sentinel values ----------------------------------------------------------
const VOID_RETURN:        u32 = 0xFFFFFFFF;  // Function does not have a return value.

// -----------------------------------------------------------------------------
// Adaptive interpreter — register file access
// -----------------------------------------------------------------------------

fn read_reg(shot_idx: u32, reg: u32) -> u32 {
    return shots[shot_idx].interp.registers[reg];
}

fn write_reg(shot_idx: u32, reg: u32, val: u32) {
    shots[shot_idx].interp.registers[reg] = val;
}

fn read_reg_i32(shot_idx: u32, reg: u32) -> i32 {
    return bitcast<i32>(read_reg(shot_idx, reg));
}

fn write_reg_i32(shot_idx: u32, reg: u32, val: i32) {
    write_reg(shot_idx, reg, bitcast<u32>(val));
}

fn read_reg_f32(shot_idx: u32, reg: u32) -> f32 {
    return bitcast<f32>(read_reg(shot_idx, reg));
}

fn write_reg_f32(shot_idx: u32, reg: u32, val: f32) {
    write_reg(shot_idx, reg, bitcast<u32>(val));
}

// -----------------------------------------------------------------------------
// Adaptive interpreter — instruction fetch and opcode extraction
// -----------------------------------------------------------------------------

fn fetch_instr(pc: u32) -> Instruction {
    return batch_data.program.instructions[pc];
}

fn get_opcode(packed: u32) -> u32   { return packed & 0xFFu; }
fn get_subcond(packed: u32) -> u32  { return (packed >> 8u) & 0xFFu; }
fn get_flags(packed: u32) -> u32    { return (packed >> 16u) & 0xFFu; }
fn is_src0_imm(flags: u32) -> bool  { return (flags & 1u) != 0u; }
fn is_src1_imm(flags: u32) -> bool  { return (flags & 2u) != 0u; }

fn resolve_i32(shot_idx: u32, operand: u32, flags: u32, operand_idx: u32) -> i32 {
    if (flags & (1u << operand_idx)) != 0u {
        return bitcast<i32>(operand);  // immediate
    }
    return read_reg_i32(shot_idx, operand);  // register
}

fn resolve_u32(shot_idx: u32, operand: u32, flags: u32, operand_idx: u32) -> u32 {
    if (flags & (1u << operand_idx)) != 0u {
        return operand;
    }
    return read_reg(shot_idx, operand);
}

fn resolve_f32(shot_idx: u32, operand: u32, flags: u32, operand_idx: u32) -> f32 {
    if (flags & (1u << operand_idx)) != 0u {
        return bitcast<f32>(operand);  // immediate (IEEE 754 bit pattern)
    }
    return read_reg_f32(shot_idx, operand);
}

// Resolves q1 for the current quantum instruction.
fn resolve_q1(shot_idx: u32) -> u32 {
    let state = shots[shot_idx].interp;
    let instr = fetch_instr(state.pc - 1);
    if (instr.opcode & FLAG_AUX1_IMM) != 0 {
        return instr.aux1;
    }
    return read_reg(shot_idx, instr.aux1);
}

// Resolves q2 for the current quantum instruction.
fn resolve_q2(shot_idx: u32) -> u32 {
    let state = shots[shot_idx].interp;
    let instr = fetch_instr(state.pc - 1);
    if (instr.opcode & FLAG_AUX2_IMM) != 0 {
        return instr.aux2;
    }
    return read_reg(shot_idx, instr.aux2);
}

// Resolves the rotation angle for the current quantum instruction.
// The angle is stored in the instruction's src0 field (register or immediate).
fn resolve_gate_angle(shot_idx: u32) -> f32 {
    let state = shots[shot_idx].interp;
    let instr = fetch_instr(state.pc - 1);
    let flags = get_flags(instr.opcode);
    return resolve_f32(shot_idx, instr.src0, flags, 0u);
}

fn get_measure_qubit(shot_idx: u32, op_idx: u32) -> u32 {
    return resolve_q1(shot_idx);
}

fn get_measure_result(shot_idx: u32, op_idx: u32) -> u32 {
    return resolve_q2(shot_idx);
}

// Read a measurement result from the existing results buffer.
// Results are stored as atomic<u32> at shot_idx * RESULT_COUNT + result_id.
fn read_measurement_result(shot_idx: u32, result_id: u32) -> bool {
    return atomicLoad(&results[shot_idx * RESULT_COUNT + result_id]) == 1u;
}

// Return true if the id corresponds to a rotation gate.
fn is_rotation_gate(id: u32) -> bool {
    return (12 <= id && id <= 14) || (17 <= id && id <= 19);
}

// Return true if the angle for the current rotation gate is dynamic.
fn is_dynamic_angle(shot_idx: u32) -> bool {
    let state = shots[shot_idx].interp;
    let instr = fetch_instr(state.pc - 1);
    return (instr.opcode | FLAG_SRC0_IMM) != 0;
}

// For every qubit, each 'execute' kernel thread will update its own workgroup storage location for accumulating probabilities
// The final probabilities will be reduced and written back to the shot state after the parallel execution completes.
struct QubitProbabilityPerThread {
    zero: array<f32, MAX_QUBIT_COUNT>,
    one: array<f32, MAX_QUBIT_COUNT>,
}; // size: 216 bytes

var<workgroup> qubitProbabilities: array<QubitProbabilityPerThread, THREADS_PER_WORKGROUP>;
// Workgroup memory size: THREADS_PER_WORKGROUP (32) * 216 = 6,912 bytes.

// Prepare correlated noise for the adaptive path.
// Qubit IDs are read from call_arg_table (register indices), following the same
// pattern as OP_CALL argument passing.
fn prep_correlated_noise(shot_idx: u32, op_idx: u32, qubit_count: u32, arg_offset: u32) {
    let noise_table_idx = ops[op_idx].q1;

    let sample = sample_correlated_noise(shot_idx, op_idx, noise_table_idx);
    if (sample.should_apply == 0u) { return; }

    // Build bit-flip and phase-flip masks using qubit IDs from registers via call_arg_table
    var bit_flip_mask: u32 = 0u;
    var phase_flip_mask: u32 = 0u;
    for (var i: u32 = 0u; i < qubit_count; i++) {
        let pauli_bits = get_pauli_bits(sample.paulis_lo, sample.paulis_hi, qubit_count, i);
        let arg_reg = batch_data.program.call_arg_table[arg_offset + i];
        let qubit_mask = 1u << read_reg(shot_idx, arg_reg);
        if ((pauli_bits & 0x1u) != 0u) { bit_flip_mask |= qubit_mask; }
        if ((pauli_bits & 0x2u) != 0u) { phase_flip_mask |= qubit_mask; }
    }

    commit_correlated_noise(shot_idx, op_idx, bit_flip_mask, phase_flip_mask);
}

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn initialize(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    // Get the params
    let params = get_shot_params(workgroupId.x, tid, 0 /* qubits per op */);

    // We want every thread to zero out its portion of the state vector for the shot
    // We also want threads executing in lockstep to update adjacent entries for better memory access patterns
    for (var i = 0; i < params.op_iterations; i++) {
        let entry_index: i32 = params.thread_idx_in_shot + i * params.total_threads_per_shot;
        stateVector[params.shot_state_vector_start + entry_index] = vec2f(0.0, 0.0);
    }

    // NOTE: No need to synchronize here, as each thread is writing to unique locations
    if (params.thread_idx_in_shot == 0) {
        // Set the |0...0> amplitude to 1.0 from the first workgroup & thread for the shot
        stateVector[params.shot_state_vector_start] = vec2f(1.0, 0.0);
        reset_all(params.shot_idx);
    }
}

// -----------------------------------------------------------------------------
// Adaptive interpreter — interpret_classical entry point
// -----------------------------------------------------------------------------
//
// This is the main classical bytecode interpreter for the GPU-based adaptive
// quantum simulator. It implements a register-based virtual machine that
// executes classical (non-quantum) instructions on the GPU, one thread per
// shot. Each shot has its own independent interpreter state (program counter,
// registers, call stack) allowing many shots to run in parallel with
// potentially divergent control flow paths (e.g., after mid-circuit
// measurements).
//
// ## Execution Model
//
// The interpreter runs cooperatively with the quantum simulation pipeline:
//
//   1. The host dispatches `interpret_classical` for all shots.
//   2. Each shot executes classical instructions in a loop until one of:
//      (a) A quantum operation is encountered → status = QUANTUM_PENDING,
//          which tells the host to run the quantum simulation kernels
//          (prepare_op → execute) before re-entering this function.
//      (b) A `ret` instruction terminates the shot → status = TERMINATED.
//      (c) The step limit (MAX_CLASSICAL_STEPS) is hit → status = YIELD,
//          which prevents any single dispatch from running forever; the host
//          simply re-dispatches to continue.
//      (d) An unknown opcode is hit → status = ERROR.
//
// ## Instruction Encoding
//
// Each instruction occupies 2 × vec4<u32> (8 u32 words) in the `bytecode`
// buffer, fetched by `fetch_instr(pc)` into the `Instr` struct with fields:
//
//   opcode : packed opcode word (bits [7:0] = primary op, [15:8] = sub-
//            condition for comparisons, [23:16] = flags for immediates)
//   dst    : destination register index (or immediate for RET)
//   src0   : first source operand (register index or immediate)
//   src1   : second source operand (register index or immediate)
//   aux0–3 : auxiliary fields whose meaning varies per opcode (e.g., block
//            IDs, function IDs, qubit indices, phi-table offsets, etc.)
//
// The `resolve_u32` / `resolve_i32` helpers read an operand as either a
// register value or an inline immediate based on the FLAG_SRC0_IMM /
// FLAG_SRC1_IMM bits in the flags byte. This lets the compiler embed small
// constants directly in the instruction stream without extra CONST ops.


@compute @workgroup_size(1)
fn interpret_classical(@builtin(global_invocation_id) gid: vec3<u32>) {
    // Each GPU thread handles exactly one shot. The global invocation ID
    // maps directly to the shot index.
    let shot_idx = gid.x;
    let state = shots[shot_idx].interp;

    // -- Early-exit for shots that already finished or errored --
    let status = state.status;
    if status == STATUS_TERMINATED || status == STATUS_ERROR {
        return;
    }

    // If we were paused (QUANTUM_PENDING after a quantum op, or YIELD after
    // hitting the step limit), transition back to RUNNING so the main loop
    // resumes executing instructions from where it left off.
    if status != STATUS_RUNNING {
        shots[shot_idx].interp.status = STATUS_RUNNING;
    }

    // -- Load interpreter registers from GPU memory into local variables --
    // Using local vars for the hot-path state avoids repeated global memory
    // loads/stores on every instruction. They are written back at the end.
    var pc: u32 = state.pc;  // program counter
    var block_id: u32 = state.current_block_id;
    var prev_block: u32 = state.previous_block_id; // for PHI
    var steps: u32 = 0u;             // counts instructions executed this dispatch
    var should_break: bool = false;  // set to true to exit the main loop

    // -- Main interpreter loop --
    // Fetches and executes one instruction per iteration. Exits when the
    // shot terminates, yields for quantum work, hits the step limit, or
    // encounters an error.
    loop {
        // Guard against infinite loops in classical code: after executing
        // MAX_CLASSICAL_STEPS instructions, yield back to the host which
        // will re-dispatch this kernel to continue.
        if steps >= MAX_CLASSICAL_STEPS {
            // Only yield if the shot hasn't already errored (an error
            // status must not be overwritten by a yield).
            if state.status != STATUS_ERROR {
                shots[shot_idx].interp.status = STATUS_YIELD;
            }
            break;
        }

        // Fetch the instruction at the current PC. Each instruction is
        // 2 × vec4<u32> (8 words) in the bytecode buffer.
        let instr = fetch_instr(pc);

        // Unpack the opcode word into its three components:
        //   op      — primary opcode (bits 7:0), determines which case below runs
        //   subcond — sub-condition code (bits 15:8), used only by ICMP/FCMP to
        //             select the specific comparison predicate (eq, ne, slt, etc.)
        //   flags   — immediate-mode flags (bits 23:16), tells resolve_* whether
        //             src0/src1 are register indices or inline immediates
        let op = get_opcode(instr.opcode);
        let subcond = get_subcond(instr.opcode);
        let flags = get_flags(instr.opcode);

        // -- Opcode dispatch --
        // The switch below implements every bytecode instruction. Instructions
        // are grouped by category. Most follow a common pattern:
        //   1. Read operands via resolve_u32/i32 (register or immediate)
        //   2. Compute the result
        //   3. Write back to the destination register via write_reg*
        //   4. Advance pc++
        //
        // Control-flow ops (JUMP, BRANCH, SWITCH, CALL) modify pc and
        // block_id directly instead of incrementing pc.
        //
        // Quantum ops (QUANTUM_GATE, MEASURE, RESET) write pending-op
        // metadata to the interpreter state and set should_break=true to
        // pause execution and hand control back to the host for quantum
        // kernel dispatch.
        switch op {

            // -------------------------------------------------------------
            // CONTROL FLOW
            // -------------------------------------------------------------

            // NOP: No operation. Simply advances the program counter.
            case OP_NOP {
                pc++;
            }

            // RET: Terminates this shot's execution.
            // The exit code (from dst, which may be an immediate) is stored
            // both in the per-shot interpreter state and atomically into the
            // results buffer. The atomic-compare-exchange ensures only the
            // first non-zero exit code is recorded for this shot (useful for
            // error reporting). The termination count in the diagnostics
            // buffer is incremented so the host can detect when all shots
            // have finished.
            case OP_RET {
                let exit_code = resolve_u32(shot_idx, instr.dst, flags, 2u);
                shots[shot_idx].interp.exit_code = exit_code;
                // Atomically store exit code into the last slot of this shot's
                // result region, but only if it has not already been set.
                let err_index = (shot_idx + 1) * RESULT_COUNT - 1;
                atomicCompareExchangeWeak(&results[err_index], 0u, exit_code);
                shots[shot_idx].interp.status = STATUS_TERMINATED;
                atomicAdd(&diagnostics.termination_count, 1u);
                should_break = true;
            }

            // JUMP: Unconditional branch to a target block.
            // Encoding: dst = target block ID.
            // Updates prev_block (needed by subsequent PHI instructions in
            // the target block) and sets pc to the first instruction of the
            // target block via the block_table lookup.
            case OP_JUMP {
                prev_block = block_id;
                block_id = instr.dst;
                pc = batch_data.program.block_table[instr.dst].instr_offset;
            }

            // BRANCH: Conditional branch (if/else).
            // Encoding: src0 = condition (register or immediate),
            //           aux0 = true-branch block ID,
            //           aux1 = false-branch block ID.
            // Evaluates the condition: if non-zero, jumps to aux0; otherwise
            // jumps to aux1. Like JUMP, updates prev_block for PHI nodes.
            case OP_BRANCH {
                let cond = resolve_u32(shot_idx, instr.src0, flags, 0u) != 0u;
                prev_block = block_id;
                if cond {
                    block_id = instr.aux0;
                    pc = batch_data.program.block_table[instr.aux0].instr_offset;
                } else {
                    block_id = instr.aux1;
                    pc = batch_data.program.block_table[instr.aux1].instr_offset;
                }
            }

            // SWITCH: Multi-way branch (like a C switch statement).
            // Encoding: src0 = value to match,
            //           aux0 = default block ID,
            //           aux1 = offset into switch_table,
            //           aux2 = number of case entries.
            // Each switch_table entry is a vec2<u32>(match_value, target_block).
            // Linearly scans the case table; if a match is found, jumps to
            // that block. If no match, falls through to the default block.
            case OP_SWITCH {
                let val = resolve_u32(shot_idx, instr.src0, flags, 0u);
                let default_block = instr.aux0;
                let case_offset = instr.aux1;
                let case_count = instr.aux2;
                var target_block = default_block;
                for (var i = 0u; i < case_count; i++) {
                    let entry = batch_data.program.switch_table[case_offset + i];
                    if entry.case_val == val {
                        target_block = entry.target_block;
                        break;
                    }
                }
                prev_block = block_id;
                block_id = target_block;
                pc = batch_data.program.block_table[target_block].instr_offset;
            }

            // CALL: Invokes a function.
            // Encoding: dst = register to receive the return value,
            //           aux0 = function ID (index into function_table),
            //           aux1 = argument count,
            //           aux2 = offset into call_arg_table.
            //
            // The function_table entry is vec4(entry_block, param_count,
            // param_base_reg, reserved).
            //
            // Steps:
            //   1. Push a return frame onto the per-shot call stack. Each
            //      frame stores: (return_block, return_pc, return_reg,
            //      reserved) — 4 u32 words. The stack supports up to 8 frames.
            //   2. Copy each argument from caller registers (looked up via
            //      call_arg_table) into callee parameter registers starting
            //      at param_base_reg.
            //   3. Jump to the function's entry block.
            case OP_CALL {
                let func_id = instr.aux0;
                let arg_count = instr.aux1;
                let arg_offset = instr.aux2;
                let func = batch_data.program.function_table[func_id];
                // Push return info onto the call stack
                let sp = state.call_sp;
                // Guard: prevent call stack overflow (max 8 frames)
                if sp >= 8u {
                    shots[shot_idx].interp.exit_code = ERR_CALL_STACK_OVERFLOW;
                    let err_idx = (shot_idx + 1) * RESULT_COUNT - 1;
                    atomicCompareExchangeWeak(&results[err_idx], 0u, ERR_CALL_STACK_OVERFLOW);
                    shots[shot_idx].interp.status = STATUS_ERROR;
                    atomicAdd(&diagnostics.termination_count, 1u);
                    should_break = true;
                    break;
                }
                shots[shot_idx].interp.call_stack_frames[sp].block_id = block_id;    // return_block — resume here on return
                shots[shot_idx].interp.call_stack_frames[sp].return_pc = pc + 1u;    // return_pc — instruction after the CALL
                shots[shot_idx].interp.call_stack_frames[sp].return_reg = instr.dst; // return_reg — where to write result
                shots[shot_idx].interp.call_sp = sp + 1u;
                // Copy caller arguments into the callee's parameter registers
                let param_base = func.param_base_reg;
                for (var i = 0u; i < arg_count; i++) {
                    let arg_reg = batch_data.program.call_arg_table[arg_offset + i];
                    write_reg(shot_idx, param_base + i, read_reg(shot_idx, arg_reg));
                }
                // Transfer control to the function entry block
                block_id = func.entry_block_id;
                pc = batch_data.program.block_table[block_id].instr_offset;
            }

            // CALL_RETURN: Returns from a function call.
            // Encoding: src0 = register holding the return value.
            //
            // Pops the top frame from the call stack to restore block_id and
            // pc to the instruction after the CALL. If the caller specified a
            // return register (not 0xFFFFFFFF), copies the return value into
            // that register.
            case OP_CALL_RETURN {
                if state.call_sp == 0u {
                    shots[shot_idx].interp.exit_code = ERR_CALL_STACK_UNDERFLOW;
                    let err_idx = (shot_idx + 1) * RESULT_COUNT - 1;
                    atomicCompareExchangeWeak(&results[err_idx], 0u, ERR_CALL_STACK_UNDERFLOW);
                    shots[shot_idx].interp.status = STATUS_ERROR;
                    atomicAdd(&diagnostics.termination_count, 1u);
                    should_break = true;
                    break;
                }

                let sp = state.call_sp - 1;
                shots[shot_idx].interp.call_sp = sp;
                block_id = state.call_stack_frames[sp].block_id;  // go back to the callers block
                pc = state.call_stack_frames[sp].return_pc;       // restore pc
                let return_reg = state.call_stack_frames[sp].return_reg;
                if return_reg != VOID_RETURN {
                    write_reg(shot_idx, return_reg, read_reg(shot_idx, instr.src0));
                }
            }

            // -------------------------------------------------------------
            // QUANTUM OPERATIONS — pause the interpreter, yield to the host
            // -------------------------------------------------------------
            // When the interpreter hits a quantum instruction, it cannot
            // execute it directly (quantum simulation runs in separate GPU
            // kernels with parallel state-vector processing). Instead, it
            // writes the pending operation details into the interpreter
            // state for the host to read, sets status = QUANTUM_PENDING,
            // advances pc past the instruction, and breaks out of the loop.
            //
            // The host then dispatches prepare_op (which reads the
            // pending op metadata and configures the shot for the quantum
            // kernel) followed by the execute kernel (which applies the
            // gate/measurement/reset to the state vector). After that, the
            // host re-dispatches interpret_classical to continue.
            //
            // Qubit IDs may be static (embedded in aux1/aux2 by the
            // compiler) or dynamic (computed at runtime and stored in
            // registers).

            // QUANTUM_GATE: Request a 1- or 2-qubit gate.
            // Encoding: aux0 = quantum op table index,
            //           aux1 = qubit 1 (or register if not sentinel),
            //           aux2 = qubit 2 (or register if not sentinel).
            case OP_QUANTUM_GATE {
                shots[shot_idx].interp.pending_op_idx = instr.aux0;
                shots[shot_idx].interp.pending_op_type = 0u; // type 0 = gate
                // Qubit IDs are resolved in prepare_op via resolve_q1/resolve_q2,
                // which use the FLAG_AUX1_IMM / FLAG_AUX2_IMM bits to decide
                // between immediate values and register lookups.
                shots[shot_idx].interp.status = STATUS_QUANTUM_PENDING;
                pc++;
                should_break = true;
            }

            // MEASURE: Request a qubit measurement.
            // Encoding: aux0 = quantum op table index,
            //           aux1 = qubit to measure (or register).
            // Only q1 is used; q2 is set to sentinel (unused).
            case OP_MEASURE {
                shots[shot_idx].interp.pending_op_idx = instr.aux0;
                shots[shot_idx].interp.pending_op_type = 1u; // type 1 = gate
                // Qubit and result IDs are resolved in prepare_op via
                // resolve_q1 (aux1) and resolve_q2 (aux2).
                shots[shot_idx].interp.status = STATUS_QUANTUM_PENDING;
                pc++;
                should_break = true;
            }

            // RESET: Request a qubit reset (measure + conditional X).
            // Encoding: aux0 = quantum op table index,
            //           aux1 = qubit to reset (or register).
            case OP_RESET {
                shots[shot_idx].interp.pending_op_idx = instr.aux0;
                shots[shot_idx].interp.pending_op_type = 2u; // type 2 = reset
                // Qubit ID is resolved in prepare_op via resolve_q1 (aux1).
                shots[shot_idx].interp.status = STATUS_QUANTUM_PENDING;
                pc++;
                should_break = true;
            }

            // -------------------------------------------------------------
            // QUANTUM RESULT ACCESS
            // -------------------------------------------------------------

            // READ_RESULT: Load a prior measurement outcome into a register.
            // Encoding: src0 = result ID (index into the results buffer),
            //           dst  = destination register.
            // The measurement result (0 or 1) was written by an earlier
            // MEASURE quantum op. This reads it atomically from the shared
            // results buffer and stores 0u or 1u into the destination
            // register, allowing classical code to branch on measurement
            // outcomes.
            case OP_READ_RESULT {
                let result_id = instr.src0;
                let result_val = read_measurement_result(shot_idx, result_id);
                write_reg(shot_idx, instr.dst, select(0u, 1u, result_val));
                pc++;
            }

            // RECORD_OUTPUT: Marker for output recording.
            // On the GPU this is a no-op — the host reads the results buffer
            // directly after all shots terminate. The instruction exists to
            // maintain compatibility with the QIR adaptive profile bytecode.
            case OP_RECORD_OUTPUT {
                pc++;
            }

            // -------------------------------------------------------------
            // INTEGER ARITHMETIC
            // -------------------------------------------------------------
            // All integer arithmetic ops follow the pattern:
            //   dst = src0 <op> src1
            // Operands are resolved via resolve_i32/u32, which checks the
            // FLAG_SRC0_IMM / FLAG_SRC1_IMM bits to determine if the field
            // is a register index or an inline immediate constant.

            // ADD: Signed integer addition. dst = src0 + src1.
            case OP_ADD {
                let a = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_i32(shot_idx, instr.src1, flags, 1u);
                write_reg_i32(shot_idx, instr.dst, a + b);
                pc++;
            }

            // SUB: Signed integer subtraction. dst = src0 - src1.
            case OP_SUB {
                let a = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_i32(shot_idx, instr.src1, flags, 1u);
                write_reg_i32(shot_idx, instr.dst, a - b);
                pc++;
            }

            // MUL: Signed integer multiplication. dst = src0 * src1.
            case OP_MUL {
                let a = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_i32(shot_idx, instr.src1, flags, 1u);
                write_reg_i32(shot_idx, instr.dst, a * b);
                pc++;
            }

            // UDIV: Unsigned integer division. dst = src0 / src1.
            case OP_UDIV {
                let a = resolve_u32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_u32(shot_idx, instr.src1, flags, 1u);
                write_reg(shot_idx, instr.dst, a / b);
                pc++;
            }

            // SDIV: Signed integer division (truncates toward zero). dst = src0 / src1.
            case OP_SDIV {
                let a = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_i32(shot_idx, instr.src1, flags, 1u);
                write_reg_i32(shot_idx, instr.dst, a / b);
                pc++;
            }

            // UREM: Unsigned integer remainder. dst = src0 % src1.
            case OP_UREM {
                let a = resolve_u32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_u32(shot_idx, instr.src1, flags, 1u);
                write_reg(shot_idx, instr.dst, a % b);
                pc++;
            }

            // SREM: Signed integer remainder.
            // Computes a - b * trunc(a/b) manually rather than using the %
            // operator, because WGSL i32 division truncates toward zero but
            // the built-in % may not preserve the sign of the dividend on
            // all GPU backends. This matches LLVM's srem semantics.
            case OP_SREM {
                let a = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_i32(shot_idx, instr.src1, flags, 1u);
                write_reg_i32(shot_idx, instr.dst, a - b * (a / b));
                pc++;
            }

            // -------------------------------------------------------------
            // BITWISE / SHIFT OPERATIONS
            // -------------------------------------------------------------
            // Operate on the raw u32 bit pattern of the register values.

            // AND: Bitwise AND. dst = src0 & src1.
            case OP_AND {
                write_reg(shot_idx, instr.dst,
                    resolve_u32(shot_idx, instr.src0, flags, 0u) & resolve_u32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // OR: Bitwise OR. dst = src0 | src1.
            case OP_OR {
                write_reg(shot_idx, instr.dst,
                    resolve_u32(shot_idx, instr.src0, flags, 0u) | resolve_u32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // XOR: Bitwise exclusive OR. dst = src0 ^ src1.
            case OP_XOR {
                write_reg(shot_idx, instr.dst,
                    resolve_u32(shot_idx, instr.src0, flags, 0u) ^ resolve_u32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // SHL: Logical shift left. dst = src0 << src1.
            case OP_SHL {
                write_reg(shot_idx, instr.dst,
                    resolve_u32(shot_idx, instr.src0, flags, 0u) << resolve_u32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // LSHR: Logical shift right (zero-fill). dst = src0 >> src1.
            case OP_LSHR {
                write_reg(shot_idx, instr.dst,
                    resolve_u32(shot_idx, instr.src0, flags, 0u) >> resolve_u32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // ASHR: Arithmetic shift right (sign-extending). dst = src0 >> src1.
            // Uses i32 to preserve the sign bit during the shift.
            case OP_ASHR {
                let a = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_u32(shot_idx, instr.src1, flags, 1u);
                write_reg_i32(shot_idx, instr.dst, a >> b);
                pc++;
            }

            // -------------------------------------------------------------
            // INTEGER COMPARISON (ICMP)
            // -------------------------------------------------------------
            // Compares two integer operands using the sub-condition code
            // encoded in bits [15:8] of the opcode word. The result is
            // written as 0u (false) or 1u (true) to the destination register.
            // Signed comparisons (SLT, SLE, SGT, SGE) use i32 directly;
            // unsigned comparisons (ULT, ULE, UGT, UGE) bitcast to u32.
            // These mirror LLVM icmp predicates.
            case OP_ICMP {
                let a = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_i32(shot_idx, instr.src1, flags, 1u);
                var result: bool = false;
                switch subcond {
                    case ICMP_EQ  { result = (a == b); }
                    case ICMP_NE  { result = (a != b); }
                    case ICMP_SLT { result = (a < b); }
                    case ICMP_SLE { result = (a <= b); }
                    case ICMP_SGT { result = (a > b); }
                    case ICMP_SGE { result = (a >= b); }
                    case ICMP_ULT { result = (bitcast<u32>(a) < bitcast<u32>(b)); }
                    case ICMP_ULE { result = (bitcast<u32>(a) <= bitcast<u32>(b)); }
                    case ICMP_UGT { result = (bitcast<u32>(a) > bitcast<u32>(b)); }
                    case ICMP_UGE { result = (bitcast<u32>(a) >= bitcast<u32>(b)); }
                    default {
                        shots[shot_idx].interp.status = ERR_INVALID_INSTRUCTION;
                        shots[shot_idx].interp.exit_code = ERR_INVALID_INSTRUCTION;
                        let err_idx = (shot_idx + 1) * RESULT_COUNT - 1;
                        atomicCompareExchangeWeak(&results[err_idx], 0u, ERR_INVALID_INSTRUCTION);
                        shots[shot_idx].interp.status = STATUS_ERROR;
                        atomicAdd(&diagnostics.termination_count, 1u);
                        should_break = true;
                    }
                }
                write_reg(shot_idx, instr.dst, select(0u, 1u, result));
                pc++;
            }

            // -------------------------------------------------------------
            // FLOAT COMPARISON (FCMP)
            // -------------------------------------------------------------
            // Compares two f32 operands using the sub-condition code.
            // "O" prefix = ordered (both operands are not NaN). The result
            // is written as 0u/1u. Mirrors LLVM fcmp ordered predicates.
            case OP_FCMP {
                let a = resolve_f32(shot_idx, instr.src0, flags, 0u);
                let b = resolve_f32(shot_idx, instr.src1, flags, 1u);
                var result: bool = false;
                switch subcond {
                    case FCMP_OEQ { result = (a == b); }
                    case FCMP_ONE { result = (a != b); }
                    case FCMP_OLT { result = (a < b); }
                    case FCMP_OLE { result = (a <= b); }
                    case FCMP_OGT { result = (a > b); }
                    case FCMP_OGE { result = (a >= b); }
                    default {
                        shots[shot_idx].interp.exit_code = ERR_INVALID_INSTRUCTION;
                        let err_idx = (shot_idx + 1) * RESULT_COUNT - 1;
                        atomicCompareExchangeWeak(&results[err_idx], 0u, ERR_INVALID_INSTRUCTION);
                        shots[shot_idx].interp.status = STATUS_ERROR;
                        atomicAdd(&diagnostics.termination_count, 1u);
                        should_break = true;
                    }
                }
                write_reg(shot_idx, instr.dst, select(0u, 1u, result));
                pc++;
            }

            // -------------------------------------------------------------
            // FLOAT ARITHMETIC
            // -------------------------------------------------------------
            // These operate on f32 values stored in registers via bitcast.
            // Operands are always register-based (no immediate flags for
            // float ops).

            // FADD: Float addition. dst = src0 + src1.
            case OP_FADD {
                write_reg_f32(shot_idx, instr.dst,
                    resolve_f32(shot_idx, instr.src0, flags, 0u) + resolve_f32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // FSUB: Float subtraction. dst = src0 - src1.
            case OP_FSUB {
                write_reg_f32(shot_idx, instr.dst,
                    resolve_f32(shot_idx, instr.src0, flags, 0u) - resolve_f32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // FMUL: Float multiplication. dst = src0 * src1.
            case OP_FMUL {
                write_reg_f32(shot_idx, instr.dst,
                    resolve_f32(shot_idx, instr.src0, flags, 0u) * resolve_f32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // FDIV: Float division. dst = src0 / src1.
            case OP_FDIV {
                write_reg_f32(shot_idx, instr.dst,
                    resolve_f32(shot_idx, instr.src0, flags, 0u) / resolve_f32(shot_idx, instr.src1, flags, 1u));
                pc++;
            }

            // -------------------------------------------------------------
            // TYPE CONVERSIONS
            // -------------------------------------------------------------
            // Maps LLVM-style type conversion instructions. Many are
            // identity ops on the GPU since all integer registers are 32-bit
            // and all floats are f32. They exist to keep the bytecode in
            // 1:1 correspondence with the compiled QIR instructions.

            // ZEXT: Zero-extend — identity on 32-bit GPU (values already u32).
            case OP_ZEXT {
                write_reg(shot_idx, instr.dst, resolve_u32(shot_idx, instr.src0, flags, 0u));
                pc++;
            }

            // SEXT: Sign-extend from a narrower bit width to i32.
            // aux0 encodes the source bit width (e.g., 1 for i1→i32).
            // The shift-left then arithmetic-shift-right trick propagates
            // the sign bit from position (src_bits-1) into all higher bits.
            case OP_SEXT {
                let val = resolve_i32(shot_idx, instr.src0, flags, 0u);
                let src_bits = instr.aux0;  // source type bit width
                if src_bits > 0u && src_bits < 32u {
                    let shift = 32u - src_bits;
                    write_reg_i32(shot_idx, instr.dst, (val << shift) >> shift);
                } else {
                    write_reg_i32(shot_idx, instr.dst, val);
                }
                pc++;
            }

            // TRUNC: Truncate — identity on 32-bit GPU (already the target width).
            case OP_TRUNC {
                write_reg(shot_idx, instr.dst, resolve_u32(shot_idx, instr.src0, flags, 0u));
                pc++;
            }

            // FPEXT: Float widen (e.g., f32→f64) — identity since GPU only uses f32.
            case OP_FPEXT {
                write_reg_f32(shot_idx, instr.dst, resolve_f32(shot_idx, instr.src0, flags, 0u));
                pc++;
            }

            // FPTRUNC: Float narrow (e.g., f64→f32) — identity since GPU only uses f32.
            case OP_FPTRUNC {
                write_reg_f32(shot_idx, instr.dst, resolve_f32(shot_idx, instr.src0, flags, 0u));
                pc++;
            }

            // INTTOPTR: Integer to pointer cast — identity, pointers are u32 on GPU.
            case OP_INTTOPTR {
                write_reg(shot_idx, instr.dst, resolve_u32(shot_idx, instr.src0, flags, 0u));
                pc++;
            }

            // FPTOSI: Float to signed integer conversion. dst = i32(src0).
            case OP_FPTOSI {
                write_reg_i32(shot_idx, instr.dst, i32(resolve_f32(shot_idx, instr.src0, flags, 0u)));
                pc++;
            }

            // SITOFP: Signed integer to float conversion. dst = f32(src0).
            case OP_SITOFP {
                write_reg_f32(shot_idx, instr.dst, f32(resolve_i32(shot_idx, instr.src0, flags, 0u)));
                pc++;
            }

            // -------------------------------------------------------------
            // PHI NODE (SSA resolution at runtime)
            // -------------------------------------------------------------
            // In SSA form, PHI nodes select a value based on which
            // predecessor block the control flow came from. The compiler
            // emits a phi_table with (predecessor_block_id, value_register)
            // pairs for each PHI instruction.
            //
            // Encoding: dst  = destination register,
            //           aux0 = offset into phi_table,
            //           aux1 = number of predecessor entries.
            //
            // At runtime, we scan the entries to find the one whose block
            // ID matches prev_block, then copy that register's value into
            // the destination. This is how the interpreter handles SSA
            // control-flow merges without explicit move instructions on
            // every edge.
            case OP_PHI {
                let offset = instr.aux0;
                let count = instr.aux1;
                for (var i = 0u; i < count; i++) {
                    let entry = batch_data.program.phi_table[offset + i];
                    if entry.block_id == prev_block {
                        write_reg(shot_idx, instr.dst, read_reg(shot_idx, entry.val_reg));
                        break;
                    }
                }
                pc++;
            }

            // -------------------------------------------------------------
            // DATA MOVEMENT
            // -------------------------------------------------------------

            // SELECT: Conditional move (ternary operator).
            // Encoding: src0 = condition, aux0 = true-value,
            //           aux1 = false-value, dst = destination.
            // dst = cond ? aux0 : aux1
            case OP_SELECT {
                let cond = resolve_u32(shot_idx, instr.src0, flags, 0u) != 0u;
                let true_val = resolve_u32(shot_idx, instr.aux0, flags, 3u);
                let false_val = resolve_u32(shot_idx, instr.aux1, flags, 4u);
                write_reg(shot_idx, instr.dst, select(false_val, true_val, cond));
                pc++;
            }

            // MOV: Register-to-register move (or immediate-to-register if flagged).
            // dst = src0 (resolved through flags for possible immediate).
            case OP_MOV {
                write_reg(shot_idx, instr.dst, resolve_u32(shot_idx, instr.src0, flags, 0u));
                pc++;
            }

            // CONST: Load an immediate constant into a register.
            // dst = src0 (always treated as a literal value, not a register).
            case OP_CONST {
                write_reg(shot_idx, instr.dst, instr.src0);
                pc++;
            }

            // Unknown opcode — flag the shot as errored.
            default {
                shots[shot_idx].interp.status = STATUS_ERROR;
                atomicAdd(&diagnostics.termination_count, 1u);
                should_break = true;
            }
        }
        steps++;
        if should_break { break; }
    }

    // -- Persist interpreter state back to GPU memory --
    // Write the local variables back so the next dispatch (after quantum ops
    // or a yield) can resume exactly where this invocation left off.
    shots[shot_idx].interp.pc = pc;
    shots[shot_idx].interp.current_block_id = block_id;
    shots[shot_idx].interp.previous_block_id = prev_block;
}

// -----------------------------------------------------------------------------
// Adaptive interpreter — prepare_op entry point
// -----------------------------------------------------------------------------
// Prepares a quantum operation for shots that have STATUS_QUANTUM_PENDING.
// Shots not in that state are set to OPID_ID so execute is a no-op.

@compute @workgroup_size(1)
fn prepare_op(@builtin(global_invocation_id) globalId: vec3<u32>) {
    let shot_idx = globalId.x;
    let shot = &shots[shot_idx];
    let state = shots[shot_idx].interp;
    let status = state.status;

    // Only process shots that are quantum-pending
    if status != STATUS_QUANTUM_PENDING {
        // Set op_type to ID so execute is a no-op for this shot
        shot.op_type = OPID_ID;
        shot.renormalize = 1.0;
        shot.qubits_updated_last_op_mask = 0u;
        return;
    }

    // Update shot state from prior op execution
    if shot.qubits_updated_last_op_mask != 0 {
        update_qubit_state(shot_idx);
    }
    shot_init_per_op(shot_idx);

    let op_idx = state.pending_op_idx;
    let op_type = state.pending_op_type;
    let op = &ops[op_idx];

    // Correlated noise: qubit IDs are stored as register indices in
    // call_arg_table; read aux1 (qubit count) and aux2 (arg offset)
    // from the instruction that triggered this quantum op.
    if op_type == 0u && op.id == OPID_CORRELATED_NOISE {
        let pc = state.pc;
        let noise_instr = fetch_instr(pc - 1u);
        let qubit_count = noise_instr.aux1;
        let arg_offset = noise_instr.aux2;
        shot.op_idx = op_idx;
        shot.op_type = op.id;
        prep_correlated_noise(shot_idx, op_idx, qubit_count, arg_offset);
        shots[shot_idx].interp.status = STATUS_RUNNING;
        return;
    }

    let q1 = resolve_q1(shot_idx);
    let q2 = resolve_q2(shot_idx);

    shot.unitary = op.unitary;

    switch op_type {
        case 0u { // Gate
            // For rotation gates, recompute the unitary from the dynamic angle stored
            // in the instruction's src0 field if needed. The op pool unitary was built
            // at upload time and may not reflect a runtime-computed angle.
            if is_rotation_gate(op.id) && is_dynamic_angle(shot_idx) {
                if op.id == OPID_RX || op.id == OPID_RY || op.id == OPID_RZ {
                    let angle = resolve_gate_angle(shot_idx);
                    let half = angle * 0.5;
                    let c = cos(half);
                    let s = sin(half);
                    if op.id == OPID_RX {
                        // [[cos(θ/2), -i·sin(θ/2)], [-i·sin(θ/2), cos(θ/2)]]
                        shot.unitary[0] = vec2f(c, 0.0);
                        shot.unitary[1] = vec2f(0.0, -s);
                        shot.unitary[4] = vec2f(0.0, -s);
                        shot.unitary[5] = vec2f(c, 0.0);
                    } else if op.id == OPID_RY {
                        // [[cos(θ/2), -sin(θ/2)], [sin(θ/2), cos(θ/2)]]
                        shot.unitary[0] = vec2f(c, 0.0);
                        shot.unitary[1] = vec2f(-s, 0.0);
                        shot.unitary[4] = vec2f(s, 0.0);
                        shot.unitary[5] = vec2f(c, 0.0);
                    } else {
                        // RZ: [[1, 0], [0, e^(iθ)]]
                        shot.unitary[0] = vec2f(1.0, 0.0);
                        shot.unitary[1] = vec2f(0.0, 0.0);
                        shot.unitary[4] = vec2f(0.0, 0.0);
                        shot.unitary[5] = vec2f(cos(angle), sin(angle));
                    }
                } else if op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_RZZ {
                    let angle = resolve_gate_angle(shot_idx);
                    let half = angle * 0.5;
                    let c = cos(half);
                    let s = sin(half);
                    if op.id == OPID_RXX {
                        // exp(-i·θ/2·X⊗X)
                        shot.unitary[0]  = vec2f(c, 0.0);
                        shot.unitary[3]  = vec2f(0.0, -s);
                        shot.unitary[5]  = vec2f(c, 0.0);
                        shot.unitary[6]  = vec2f(0.0, -s);
                        shot.unitary[9]  = vec2f(0.0, -s);
                        shot.unitary[10] = vec2f(c, 0.0);
                        shot.unitary[12] = vec2f(0.0, -s);
                        shot.unitary[15] = vec2f(c, 0.0);
                    } else if op.id == OPID_RYY {
                        // exp(-i·θ/2·Y⊗Y)
                        shot.unitary[0]  = vec2f(c, 0.0);
                        shot.unitary[3]  = vec2f(0.0, s);
                        shot.unitary[5]  = vec2f(c, 0.0);
                        shot.unitary[6]  = vec2f(0.0, -s);
                        shot.unitary[9]  = vec2f(0.0, -s);
                        shot.unitary[10] = vec2f(c, 0.0);
                        shot.unitary[12] = vec2f(0.0, s);
                        shot.unitary[15] = vec2f(c, 0.0);
                    } else {
                        // RZZ: diag(1, e^(iθ), e^(iθ), 1)
                        shot.unitary[0]  = vec2f(1.0, 0.0);
                        shot.unitary[5]  = vec2f(cos(angle), sin(angle));
                        shot.unitary[10] = vec2f(cos(angle), sin(angle));
                        shot.unitary[15] = vec2f(1.0, 0.0);
                    }
                }
            }

            shot.op_idx = op_idx;
            shot.op_type = op.id;

            // Check for noise ops after this gate in the ops pool
            let pauli_op_idx = get_pauli_noise_idx(op_idx);
            let loss_op_idx = get_loss_idx(select(op_idx, pauli_op_idx, pauli_op_idx != 0u));

            // Handle loss noise first (if qubit is lost, gate doesn't matter)
            if loss_op_idx != 0u {
                let loss_op = &ops[loss_op_idx];
                let p_loss = loss_op.unitary[0].x;
                if shot.rand_loss < p_loss {
                    prep_measure_reset(shot_idx, op_idx, true, false, true);
                    shots[shot_idx].interp.status = STATUS_RUNNING;
                    return;
                }
            }

            // Handle Pauli noise
            if pauli_op_idx != 0u {
                if ops[pauli_op_idx].id == OPID_PAULI_NOISE_1Q {
                    apply_1q_pauli_noise(shot_idx, op_idx, pauli_op_idx);
                } else {
                    apply_2q_pauli_noise(shot_idx, op_idx, pauli_op_idx);
                }
                shots[shot_idx].interp.status = STATUS_RUNNING;
                return;
            }

            // No noise — set up the op for execution

            // Turn multi-qubit matrix ops into shot buffer ops
            if op.id == OPID_RXX || op.id == OPID_RYY || op.id == OPID_MAT2Q || op.id == OPID_SWAP {
                shot.op_type = OPID_SHOT_BUFF_2Q;
            }

            // Turn 1Q matrix ops into shot buffer ops
            if op.id >= OPID_X && op.id < OPID_CX {
                shot.op_type = OPID_SHOT_BUFF_1Q;
            }

            // Phase gates all execute as RZ
            if is_1q_phase_gate(op.id) {
                shot.op_type = OPID_RZ;
            }

            // Set qubits_updated mask so next round knows which probabilities to update
            switch shot.op_type {
                case OPID_ID, OPID_CZ, OPID_RZ, OPID_RZZ {
                    shot.qubits_updated_last_op_mask = 0u;
                }
                case OPID_SHOT_BUFF_1Q {
                    shot.qubits_updated_last_op_mask = 1u << q1;
                }
                case OPID_CX, OPID_CY, OPID_SHOT_BUFF_2Q {
                    shot.qubits_updated_last_op_mask = (1u << q1) | (1u << q2);
                }
                default {}
            }
        }
        case 1u { // Measure
            // Check for noise ops before the measure op
            // (noise is applied as Id+noise, then original measure, matching non-adaptive pattern)
            let pauli_op_idx = get_pauli_noise_idx(op_idx);
            let loss_op_idx = get_loss_idx(select(op_idx, pauli_op_idx, pauli_op_idx != 0u));

            if loss_op_idx != 0u {
                let loss_op = &ops[loss_op_idx];
                let p_loss = loss_op.unitary[0].x;
                if shot.rand_loss < p_loss {
                    prep_measure_reset(shot_idx, op_idx, true, false, true);
                    shots[shot_idx].interp.status = STATUS_RUNNING;
                    return;
                }
            }

            if pauli_op_idx != 0u {
                // Apply noise to the Id gate before measure, then the measure itself
                // The non-adaptive path inserts Id+noise before measure; here the Id
                // is at op_idx and the original measure op follows after noise ops
                if ops[pauli_op_idx].id == OPID_PAULI_NOISE_1Q {
                    apply_1q_pauli_noise(shot_idx, op_idx, pauli_op_idx);
                } else {
                    apply_2q_pauli_noise(shot_idx, op_idx, pauli_op_idx);
                }
                shots[shot_idx].interp.status = STATUS_RUNNING;
                return;
            }

            // No noise — standard measure
            let resets = op.id == OPID_MRESETZ;
            prep_measure_reset(shot_idx, op_idx, false, true, resets);
        }
        case 2u { // Reset
            prep_measure_reset(shot_idx, op_idx, false, false, true);
        }
        default {
            shot.op_type = OPID_ID;
        }
    }

    // Mark shot as running so interpret_classical resumes next round
    shots[shot_idx].interp.status = STATUS_RUNNING;
}

@compute @workgroup_size(THREADS_PER_WORKGROUP)
fn execute(
        @builtin(workgroup_id) workgroupId: vec3<u32>,
        @builtin(local_invocation_index) tid: u32) {
    let shot_idx: i32 = i32(workgroupId.x) / WORKGROUPS_PER_SHOT;
    let shot_idx_u32: u32 = u32(shot_idx);
    let shot = &shots[shot_idx];

    // If it's an ID gate, or a pure phase gate (including CZ) then probabilities don't need updating
    // Correlated noise also updates probabilities in prepare_op, so can skip doing that here
    let update_probs = shot.op_type != OPID_ID && shot.op_type != OPID_CORRELATED_NOISE &&
            shot.op_type != OPID_RZ && shot.op_type != OPID_CZ && shot.op_type != OPID_RZZ;

    if (shot.op_type == OPID_ID) {
        // IGNORE
    } else if (shot.op_type == OPID_CORRELATED_NOISE) {
        apply_correlated_noise(workgroupId.x, tid);
    } else if (is_1q_op(shot.op_type)) {
        let q1: u32 = resolve_q1(shot_idx_u32);
        apply_1q_op(workgroupId.x, tid, q1);
    } else /* 2 qubit op */ {
        let q1: u32 = resolve_q1(shot_idx_u32);
        let q2: u32 = resolve_q2(shot_idx_u32);
        apply_2q_op(workgroupId.x, tid, q1, q2);
    }

    // workgroupBarrier can't be conditional in DX12 backend, so we have to do an unconditional one here
    // outside of the skip_work conditional above.
    workgroupBarrier();

    // If the workgroup is done updating, have the first thread reduce the per-thread probabilities into the
    // totals for this workgroup. The subsequent 'prepare_op' will sum the workgroup entries into the shot state.
    // Skip for correlated noise since probabilities were already updated in prepare_op.
    if (tid == 0 && update_probs) {
        let shot_idx: i32 = i32(workgroupId.x) / WORKGROUPS_PER_SHOT;
        let workgroup_collation_idx: i32 = select(-1, i32(workgroupId.x), WORKGROUPS_PER_SHOT > 1);
        for (var q: u32 = 0u; q < u32(QUBIT_COUNT); q++) {
            if (shot.qubits_updated_last_op_mask & (1u << q)) != 0u {
                sum_thread_totals_to_shot(q, shot_idx, workgroup_collation_idx);
            }
        }
    }
}
