// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// See https://webgpufundamentals.org/webgpu/lessons/webgpu-wgsl.html for an overview
// See https://www.w3.org/TR/WGSL/ for the details

// NOTE: WGSL doesn't have the ternary operator, but does have a built-in function `select` that can be used to achieve similar functionality.
// See https://www.w3.org/TR/WGSL/#select-builtin

// ***** IMPORTANT: Keep this first section in sync with the shader_types module in shader_types.rs *****

const MAX_QUBITS_PER_THREAD: u32 = 10u;
const MAX_QUBITS_PER_WORKGROUP: u32 = 12u;

const ID: u32      = 0;
const RESET: u32   = 1;
const X: u32       = 2;
const Y: u32       = 3;
const Z: u32       = 4;
const H: u32       = 5;
const S: u32       = 6;
const S_ADJ: u32   = 7;
const T: u32       = 8;
const T_ADJ: u32   = 9;
const SX: u32      = 10;
const SX_ADJ: u32  = 11;
const RX: u32      = 12;
const RY: u32      = 13;
const RZ: u32      = 14;
const CX: u32      = 15;
const CZ: u32      = 16;
const RXX: u32     = 17;
const RYY: u32     = 18;
const RZZ: u32     = 19;
const CCX: u32     = 20;
const MZ: u32      = 21;
const MRESETZ: u32 = 22;
const MEVERYZ: u32 = 23;
const SWAP: u32    = 24;
const MATRIX: u32   = 25;
const MATRIX_2Q: u32 = 26;

struct Op {
    op_id: u32,
    q1: u32,
    q2: u32,
    q3: u32,
    _00r: f32,
    _00i: f32,
    _01r: f32,
    _01i: f32,
    _02r: f32,
    _02i: f32,
    _03r: f32,
    _03i: f32,
    _10r: f32,
    _10i: f32,
    _11r: f32,
    _11i: f32,
    _12r: f32,
    _12i: f32,
    _13r: f32,
    _13i: f32,
    _20r: f32,
    _20i: f32,
    _21r: f32,
    _21i: f32,
    _22r: f32,
    _22i: f32,
    _23r: f32,
    _23i: f32,
    _30r: f32,
    _30i: f32,
    _31r: f32,
    _31i: f32,
    _32r: f32,
    _32i: f32,
    _33r: f32,
    _33i: f32,
    rzr: f32,
    rzi: f32,
}

struct Result {
    entry_idx: u32,
    probability: f32,
}

// ***** END IMPORTANT SECTION *****

const M_SQRT1_2 = 0.70710678118654752440084436210484903; /* 1/sqrt(2) */
const t_coeff: vec2f = vec2f(M_SQRT1_2, M_SQRT1_2); // 1/sqrt(2) + i*1/sqrt(2)
const t_adj_coeff = vec2f(M_SQRT1_2, -M_SQRT1_2);

const PROB_THRESHOLD = 0.01;

// Input to the shader. The length of the array is determined by what buffer is bound.
//
// StateVector entries
@group(0) @binding(0)
var<storage, read_write> stateVec: array<vec2f>;
// Circuit ops.
@group(0) @binding(1)
var<storage, read> op: Op;

// Results
@group(0) @binding(2)
var<storage, read_write> results: array<Result>;

@group(0) @binding(3)
var<storage, read_write> result_idx: atomic<u32>;

// The below should all be overridden by the Rust code when creating the pipeline based on the circuit
override WORKGROUP_SIZE_X: u32;
override QUBIT_COUNT: u32;

@compute @workgroup_size(WORKGROUP_SIZE_X)
fn run_statevector_ops(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // This will end up being a linear id of all the threads run total (including across workgroups).
    let thread_id = global_id.x + global_id.y * WORKGROUP_SIZE_X;

    // For the last op, the first thread should scan the probabilities and write the results.
    if (op.op_id == MEVERYZ) {
        scan_probabilities(thread_id);
        return;
    }
    // TODO: MZ and MRESETZ (assume base profile with all measurements at the end of the circuit for now)

    switch op.op_id {
        case ID {
            // No operation, just return.
            return;
        }
        case X {
            apply_x_op(thread_id);
            return;
        }
        case Y {
            apply_y_op(thread_id);
            return;
        }
        case Z {
            apply_z_op(thread_id);
            return;
        }
        case H, SX, SX_ADJ, RX, RY {
            apply_unitary_1q_op(thread_id);
            return;
        }
        case RZ {
            apply_rz_op(thread_id);
            return;
        }
        case S {
            apply_s_op(thread_id);
            return;
        }
        case S_ADJ {
            apply_s_adj_op(thread_id);
            return;
        }
        case T {
            apply_t_op(thread_id);
            return;
        }
        case T_ADJ {
            apply_t_adj_op(thread_id);
            return;
        }
        case CX {
            apply_cx_op(thread_id);
            return;
        }
        case CZ {
            apply_cz_op(thread_id);
            return;
        }
        case RZZ {
            apply_rzz_op(thread_id);
            return;
        }
        case SWAP {
            apply_swap_op(thread_id);
            return;
        }
        case RXX, RYY {
            apply_rxx_ryy_op(thread_id);
            return;
        }
        case CCX {
            apply_3q_op(thread_id);
            return;
        }
        case MATRIX {
            apply_1q_matrix_op(thread_id);
            return;
        }
        case MATRIX_2Q {
            apply_2q_matrix_op(thread_id);
            return;
        }
        default {
            // TODO: Report error for unsupported op
        }
    }
}

fn cplxmul(a: vec2f, b: vec2f) -> vec2f {
    return vec2f(
        a.x * b.x - a.y * b.y,
        a.x * b.y + a.y * b.x
    );
}

fn apply_x_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp0 = stateVec[offset];
        let amp1 = stateVec[offset + stride];

        stateVec[offset] = amp1;
        stateVec[offset + stride] = amp0;

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

fn apply_y_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp0 = stateVec[offset];
        let amp1 = stateVec[offset + stride];

        stateVec[offset] = vec2f(-amp1.y, amp1.x);
        stateVec[offset + stride] = vec2f(amp0.y, -amp0.x);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

fn apply_z_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let index = offset + stride;
        let amp = stateVec[index];
        stateVec[index] = -amp;

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

fn apply_s_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);
    const coeff1: vec2f = vec2f(0.0, 1.0);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp1 = stateVec[offset + stride];

        stateVec[offset + stride] = cplxmul(amp1, coeff1);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

fn apply_s_adj_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);
    const coeff1 = vec2f(0.0, -1.0);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp1 = stateVec[offset + stride];

        stateVec[offset + stride] = cplxmul(amp1, coeff1);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

fn apply_t_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp1 = stateVec[offset + stride];

        stateVec[offset + stride] = cplxmul(amp1, t_coeff);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

fn apply_t_adj_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp1 = stateVec[offset + stride];

        stateVec[offset + stride] = cplxmul(amp1, t_adj_coeff);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

/// Applies a general 1-qubit operation using matrix multiplication.
///
/// This function parallelizes 1-qubit gate operations across GPU threads by having each thread
/// process a chunk of the state vector. The key insight is that for a 1-qubit gate on qubit `q`,
/// we need to process pairs of amplitudes that differ only in the q-th bit.
///
/// Variables explained:
/// - ITERATIONS: Maximum number of amplitude pairs each thread processes (512 for MAX_QUBITS_PER_THREAD=10)
/// - stride: Distance between |0⟩ and |1⟩ states for the target qubit (2^q1)
/// - thread_start_iteration: Starting iteration index for this thread (thread_id * ITERATIONS)
/// - offset: Actual state vector index where this thread starts processing
/// - iterations: Actual number of pairs this thread will process (may be less than ITERATIONS)
///
/// Example: 3-qubit system, applying X gate to qubit 1 (q1=1), thread_id=0
/// - State vector: [|000⟩, |001⟩, |010⟩, |011⟩, |100⟩, |101⟩, |110⟩, |111⟩] (indices 0-7)
/// - ITERATIONS = 512 (but only 2^(3-1) = 4 pairs exist)
/// - stride = 2^1 = 2 (distance between |0_0⟩ and |1_0⟩ states for qubit 1)
/// - thread_start_iteration = 0 * 512 = 0
/// - offset = 0 % 2 + (0 / 2) * 4 = 0 (start at state vector index 0)
/// - iterations = min(512, 4) = 4
///
/// Thread 0 processes pairs: (0,2), (1,3), (4,6), (5,7)
/// - offset=0: process states |000⟩ ↔ |010⟩ (indices 0,2)
/// - offset=1: process states |001⟩ ↔ |011⟩ (indices 1,3)
/// - offset=4: process states |100⟩ ↔ |110⟩ (indices 4,6)
/// - offset=5: process states |101⟩ ↔ |111⟩ (indices 5,7)
fn apply_unitary_1q_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    let coeff00: vec2f = vec2f(op._00r, op._00i);
    let coeff01: vec2f = vec2f(op._01r, op._01i);
    let coeff10: vec2f = vec2f(op._10r, op._10i);
    let coeff11: vec2f = vec2f(op._11r, op._11i);

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp0 = stateVec[offset];
        let amp1 = stateVec[offset + stride];

        stateVec[offset] = cplxmul(amp0, coeff00) + cplxmul(amp1, coeff01);
        stateVec[offset + stride] = cplxmul(amp0, coeff10) + cplxmul(amp1, coeff11);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }
}

/// Applies a single-qubit matrix operator using the matrix elements stored in the Op struct.
///
/// Unlike unitary operations, matrix operators may not preserve norm, so this operation
/// should typically be followed by renormalization when used in quantum error models.
fn apply_1q_matrix_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let coeff00: vec2f = vec2f(op._00r, op._00i);
    let coeff01: vec2f = vec2f(op._01r, op._01i);
    let coeff10: vec2f = vec2f(op._10r, op._10i);
    let coeff11: vec2f = vec2f(op._11r, op._11i);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp0 = stateVec[offset];
        let amp1 = stateVec[offset + stride];

        stateVec[offset] = cplxmul(amp0, coeff00) + cplxmul(amp1, coeff01);
        stateVec[offset + stride] = cplxmul(amp0, coeff10) + cplxmul(amp1, coeff11);

        offset += 1;
        // If we walked past the end of the block, jump to the next stride
        // The target qubit flips to 1 when we walk past the 0 entries, and
        // a target qubit value is also the stride size
        offset += (offset & stride);
    }

    // renormalize...
}

/// Applies a two-qubit matrix operator using the 4x4 matrix elements stored in the Op struct.
///
/// Unlike unitary operations, matrix operators may not preserve norm, so this operation
/// should typically be followed by renormalization when used in quantum error models.
fn apply_2q_matrix_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);

    // Matrix coefficients for the 4x4 matrix operator using standard layout
    // Row 0: |00⟩ output (standard row0 -> _00, _01, _02, _03)
    let coeff00: vec2f = vec2f(op._00r, op._00i); // |00⟩⟨00| = row0[0]
    let coeff01: vec2f = vec2f(op._01r, op._01i); // |00⟩⟨01| = row0[1]
    let coeff02: vec2f = vec2f(op._02r, op._02i); // |00⟩⟨10| = row0[2]
    let coeff03: vec2f = vec2f(op._03r, op._03i); // |00⟩⟨11| = row0[3]

    // Row 1: |01⟩ output (standard row1 -> _10, _11, _12, _13)
    let coeff10: vec2f = vec2f(op._10r, op._10i); // |01⟩⟨00| = row1[0]
    let coeff11: vec2f = vec2f(op._11r, op._11i); // |01⟩⟨01| = row1[1]
    let coeff12: vec2f = vec2f(op._12r, op._12i); // |01⟩⟨10| = row1[2]
    let coeff13: vec2f = vec2f(op._13r, op._13i); // |01⟩⟨11| = row1[3]

    // Row 2: |10⟩ output (standard row2 -> _20, _21, _22, _23)
    let coeff20: vec2f = vec2f(op._20r, op._20i); // |10⟩⟨00| = row2[0]
    let coeff21: vec2f = vec2f(op._21r, op._21i); // |10⟩⟨01| = row2[1]
    let coeff22: vec2f = vec2f(op._22r, op._22i); // |10⟩⟨10| = row2[2]
    let coeff23: vec2f = vec2f(op._23r, op._23i); // |10⟩⟨11| = row2[3]

    // Row 3: |11⟩ output (standard row3 -> _30, _31, _32, _33)
    let coeff30: vec2f = vec2f(op._30r, op._30i); // |11⟩⟨00| = row3[0]
    let coeff31: vec2f = vec2f(op._31r, op._31i); // |11⟩⟨01| = row3[1]
    let coeff32: vec2f = vec2f(op._32r, op._32i); // |11⟩⟨10| = row3[2]
    let coeff33: vec2f = vec2f(op._33r, op._33i); // |11⟩⟨11| = row3[3]

    let iterations: i32 = select(1 << (QUBIT_COUNT - 2), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let start_count: i32 = i32(thread_id) * ITERATIONS;
    let end_count: i32 = start_count + iterations;

    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = QUBIT_COUNT - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    for (var i: i32 = start_count; i < end_count; i++) {
        let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
        let offset01: i32 = offset00 | (1 << hiQubit);
        let offset10: i32 = offset00 | (1 << lowQubit);
        let offset11: i32 = offset01 | offset10;

        let amp00 = stateVec[offset00];
        let amp01 = stateVec[offset01];
        let amp10 = stateVec[offset10];
        let amp11 = stateVec[offset11];

        // Apply the 4x4 matrix transformation
        // New |00⟩ = coeff00*|00⟩ + coeff01*|01⟩ + coeff02*|10⟩ + coeff03*|11⟩
        stateVec[offset00] = cplxmul(amp00, coeff00) + cplxmul(amp01, coeff01) + cplxmul(amp10, coeff02) + cplxmul(amp11, coeff03);
        // New |01⟩ = coeff10*|00⟩ + coeff11*|01⟩ + coeff12*|10⟩ + coeff13*|11⟩
        stateVec[offset01] = cplxmul(amp00, coeff10) + cplxmul(amp01, coeff11) + cplxmul(amp10, coeff12) + cplxmul(amp11, coeff13);
        // New |10⟩ = coeff20*|00⟩ + coeff21*|01⟩ + coeff22*|10⟩ + coeff23*|11⟩
        stateVec[offset10] = cplxmul(amp00, coeff20) + cplxmul(amp01, coeff21) + cplxmul(amp10, coeff22) + cplxmul(amp11, coeff23);
        // New |11⟩ = coeff30*|00⟩ + coeff31*|01⟩ + coeff32*|10⟩ + coeff33*|11⟩
        stateVec[offset11] = cplxmul(amp00, coeff30) + cplxmul(amp01, coeff31) + cplxmul(amp10, coeff32) + cplxmul(amp11, coeff33);
    }

    // renormalize...
}

fn apply_cx_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);

    let iterations: i32 = select(1 << (QUBIT_COUNT - 2), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let start_count: i32 = i32(thread_id) * ITERATIONS;
    let end_count: i32 = start_count + iterations;

    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = QUBIT_COUNT - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    for (var i: i32 = start_count; i < end_count; i++) {
        // q1 is the control, q2 is the target
        let offset10: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2) | (1 << op.q1);
        let offset11: i32 = offset10 | (1 << op.q2);

        let old10 = stateVec[offset10];
        stateVec[offset10] = stateVec[offset11];
        stateVec[offset11] = old10;
    }
}

fn apply_cz_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);

    let iterations: i32 = select(1 << (QUBIT_COUNT - 2), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let start_count: i32 = i32(thread_id) * ITERATIONS;
    let end_count: i32 = start_count + iterations;

    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = QUBIT_COUNT - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    let qubit_mask = (1 << lowQubit) | (1 << hiQubit);
    for (var i: i32 = start_count; i < end_count; i++) {
        let offset: i32 = qubit_mask | (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
        stateVec[offset] *= -1;
    }
}

fn apply_rz_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);

    let coeff2 = vec2f(op.rzr, op.rzi);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    for (var i: i32 = 0; i < iterations; i++) {
        let amp1 = stateVec[offset + stride];
        stateVec[offset + stride] = cplxmul(amp1, coeff2);
    }
}

fn apply_rxx_ryy_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);

    let coeff00 = vec2f(op._00r, op._00i);
    let coeff01 = vec2f(op._01r, op._01i);
    let coeff02 = vec2f(op._02r, op._02i);
    let coeff03 = vec2f(op._03r, op._03i);
    let coeff10 = vec2f(op._10r, op._10i);
    let coeff11 = vec2f(op._11r, op._11i);
    let coeff12 = vec2f(op._12r, op._12i);
    let coeff13 = vec2f(op._13r, op._13i);
    let coeff20 = vec2f(op._20r, op._20i);
    let coeff21 = vec2f(op._21r, op._21i);
    let coeff22 = vec2f(op._22r, op._22i);
    let coeff23 = vec2f(op._23r, op._23i);
    let coeff30 = vec2f(op._30r, op._30i);
    let coeff31 = vec2f(op._31r, op._31i);
    let coeff32 = vec2f(op._32r, op._32i);
    let coeff33 = vec2f(op._33r, op._33i);

    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = QUBIT_COUNT - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    let start_count: i32 = i32(thread_id) * ITERATIONS;

    let iterations: i32 = select(1 << (QUBIT_COUNT - 2), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);

    let end_count: i32 = start_count + iterations;


    for (var i: i32 = start_count; i < end_count; i++) {
        let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
        let offset01: i32 = offset00 | (1 << hiQubit);
        let offset10: i32 = offset00 | (1 << lowQubit);
        let offset11: i32 = offset01 | offset10;

        let amp00 = stateVec[offset00];
        let amp01 = stateVec[offset01];
        let amp10 = stateVec[offset10];
        let amp11 = stateVec[offset11];

        // Apply the full 4x4 matrix transformation using precomputed coefficients
        // New |00⟩ = coeff00*|00⟩ + coeff01*|01⟩ + coeff02*|10⟩ + coeff03*|11⟩
        stateVec[offset00] = cplxmul(amp00, coeff00) + cplxmul(amp01, coeff01) + cplxmul(amp10, coeff02) + cplxmul(amp11, coeff03);
        // New |01⟩ = coeff10*|00⟩ + coeff11*|01⟩ + coeff12*|10⟩ + coeff13*|11⟩
        stateVec[offset01] = cplxmul(amp00, coeff10) + cplxmul(amp01, coeff11) + cplxmul(amp10, coeff12) + cplxmul(amp11, coeff13);
        // New |10⟩ = coeff20*|00⟩ + coeff21*|01⟩ + coeff22*|10⟩ + coeff23*|11⟩
        stateVec[offset10] = cplxmul(amp00, coeff20) + cplxmul(amp01, coeff21) + cplxmul(amp10, coeff22) + cplxmul(amp11, coeff23);
        // New |11⟩ = coeff30*|00⟩ + coeff31*|01⟩ + coeff32*|10⟩ + coeff33*|11⟩
        stateVec[offset11] = cplxmul(amp00, coeff30) + cplxmul(amp01, coeff31) + cplxmul(amp10, coeff32) + cplxmul(amp11, coeff33);
    }
}

fn apply_rzz_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);
    // Use precomputed matrix coefficients for diagonal RZZ matrix
    let coeff00: vec2f = vec2f(op._00r, op._00i); // |00⟩⟨00| = e^(-i*θ/2)
    let coeff11: vec2f = vec2f(op._11r, op._11i); // |01⟩⟨01| = e^(i*θ/2)
    let coeff22: vec2f = vec2f(op._22r, op._22i); // |10⟩⟨10| = e^(i*θ/2)
    let coeff33: vec2f = vec2f(op._33r, op._33i); // |11⟩⟨11| = e^(-i*θ/2)

    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = QUBIT_COUNT - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    let start_count: i32 = i32(thread_id) * ITERATIONS;
    let iterations: i32 = select(1 << (QUBIT_COUNT - 2), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let end_count: i32 = start_count + iterations;

    for (var i: i32 = start_count; i < end_count; i++) {
        let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
        let offset01: i32 = offset00 | (1 << hiQubit);
        let offset10: i32 = offset00 | (1 << lowQubit) ;
        let offset11: i32 = offset01 | offset10;

        let amp00 = stateVec[offset00];
        let amp01 = stateVec[offset01];
        let amp10 = stateVec[offset10];
        let amp11 = stateVec[offset11];

        // Apply diagonal matrix elements using precomputed coefficients
        stateVec[offset00] = cplxmul(amp00, coeff00);
        stateVec[offset01] = cplxmul(amp01, coeff11);
        stateVec[offset10] = cplxmul(amp10, coeff22);
        stateVec[offset11] = cplxmul(amp11, coeff33);
    }
}

fn apply_swap_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);

    let iterations: i32 = select(1 << (QUBIT_COUNT - 2), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let start_count: i32 = i32(thread_id) * ITERATIONS;
    let end_count: i32 = start_count + iterations;

    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = QUBIT_COUNT - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    for (var i: i32 = start_count; i < end_count; i++) {
        let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
        let offset01: i32 = offset00 | (1 << hiQubit);
        let offset10: i32 = offset00 | (1 << lowQubit);

        let temp = stateVec[offset01];
        stateVec[offset01] = stateVec[offset10];
        stateVec[offset10] = temp;
    }
}

fn apply_3q_op(thread_id: u32) {

}

fn scan_probabilities(thread_id: u32) {
    // Scan the chunk of the state vector assigned to this thread and for any probabilities above 1%,
    // write the result to the results buffer and update the atomic index.
    const ITERATIONS: u32 = 1u << (MAX_QUBITS_PER_THREAD);

    let start_idx: u32 = thread_id * ITERATIONS;
    let iterations: u32 = select(1u << (QUBIT_COUNT), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let end_idx: u32 = start_idx + iterations;

    for (var i: u32 = start_idx; i < end_idx; i++) {
        // Calculate the probability of this entry
        let entry = stateVec[i];
        let prob = entry.x * entry.x + entry.y * entry.y;
        if prob > PROB_THRESHOLD {
            // Use atomic operations to safely write to the results buffer
            let curr_idx = atomicAdd(&result_idx, 1);
            if curr_idx >= arrayLength(&results) {
                // Shouldn't happen, but just for safety
                continue;
            }
            results[curr_idx].entry_idx = i;
            results[curr_idx].probability = prob;
        }
    }
}
