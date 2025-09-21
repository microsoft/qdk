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

struct Op {
    op_id: u32,
    q1: u32,
    q2: u32,
    q3: u32,
    angle: f32,
    _00r: f32,
    _00i: f32,
    _01r: f32,
    _01i: f32,
    _10r: f32,
    _10i: f32,
    _11r: f32,
    _11i: f32
}

struct Result {
    entry_idx: u32,
    probability: f32,
}

// ***** END IMPORTANT SECTION *****

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
        case X, Y, Z, H, S, S_ADJ, T, T_ADJ, SX, SX_ADJ, RX, RY, RZ {
            apply_1q_op(thread_id);
            return;
        }
        case CX, CZ, RXX, RYY, RZZ, SWAP {
            apply_2q_op(thread_id);
            return;
        }
        case CCX {
            apply_3q_op(thread_id);
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

fn apply_1q_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 1);

    let stride: i32 = 1 << op.q1;
    let thread_start_iteration: i32 = i32(thread_id) * ITERATIONS;

    // Find the start offset based on the thread and stride
    var offset: i32 = thread_start_iteration % stride + ((thread_start_iteration / stride) * 2 * stride);
    let iterations: i32 = select(ITERATIONS, (1 << (QUBIT_COUNT - 1)), QUBIT_COUNT < MAX_QUBITS_PER_THREAD);

    let coeff00: vec2f = vec2f(op._00r, op._00i);
    let coeff01: vec2f = vec2f(op._01r, op._01i);
    let coeff10: vec2f = vec2f(op._10r, op._10i);
    let coeff11: vec2f = vec2f(op._11r, op._11i);

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

// The first two lines subtracting 2 prevent the use of the simulator for 1 qubit
// We hit the same issue trying to implemented CCX which prevents simulation with
// only two qubits.
fn apply_2q_op(thread_id: u32) {
    const ITERATIONS: i32 = 1 << (MAX_QUBITS_PER_THREAD - 2);

    let iterations: i32 = select(1 << (QUBIT_COUNT - 2), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let start_count: i32 = i32(thread_id) * ITERATIONS;
    let end_count: i32 = start_count + iterations;

    // Coefficients for rotation gates
    var coeff0: vec2f = vec2f(0.0, 0.0);
    var coeff1: vec2f = vec2f(0.0, 0.0);

    switch op.op_id {
        case RXX {
            coeff0 = vec2f(cos(op.angle / 2.0), 0.0);
            coeff1 = vec2f(0.0, -sin(op.angle / 2.0));
        }
        case RYY {
            coeff0 = vec2f(cos(op.angle / 2.0), 0.0);
            coeff1 = vec2f(0.0, sin(op.angle / 2.0));
        }
        case RZZ {
            coeff0 = vec2f(cos(op.angle / 2.0), -sin(op.angle / 2.0));
            coeff1 = vec2f(cos(op.angle / 2.0), sin(op.angle / 2.0));
        }
        default {
            // No coefficients needed for CX, CZ, SWAP
        }
    }

    let lowQubit = select(op.q1, op.q2, op.q1 > op.q2);
    let hiQubit = select(op.q1, op.q2, op.q1 < op.q2);

    let lowBitCount = lowQubit;
    let midBitCount = hiQubit - lowQubit - 1;
    let hiBitCount = QUBIT_COUNT - hiQubit - 1;

    let lowMask = (1 << lowBitCount) - 1;
    let midMask = (1 << (lowBitCount + midBitCount)) - 1 - lowMask;
    let hiMask = (1 << (lowBitCount + midBitCount + hiBitCount)) - 1 - midMask - lowMask;

    for (var i: i32 = start_count; i < end_count; i++) {
        switch op.op_id {
            case CX {
                // q1 is the control, q2 is the target
                let offset10: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2) | (1 << op.q1);
                let offset11: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2) | (1 << op.q1) | (1 << op.q2);

                let old10 = stateVec[offset10];
                stateVec[offset10] = stateVec[offset11];
                stateVec[offset11] = old10;
            }
            case CZ {
                let offset: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);
                stateVec[offset] *= -1;
            }
            case RZZ {
                // old impl
                let coeff: vec2f = select(vec2f(0.0), vec2f(cos(op.angle), -sin(op.angle)), op.op_id == RZZ);
                let offset01: i32 = (i & lowMask) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);
                let offset10: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | ((i & hiMask) << 2);

                stateVec[offset01] = cplxmul(stateVec[offset01], coeff);
                stateVec[offset10] = cplxmul(stateVec[offset10], coeff);

                // impl like others
                // let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
                // let offset01: i32 = (i & lowMask) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);
                // let offset10: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | ((i & hiMask) << 2);
                // let offset11: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);

                // stateVec[offset00] = cplxmul(stateVec[offset00], coeff0);
                // stateVec[offset01] = cplxmul(stateVec[offset01], coeff1);
                // stateVec[offset10] = cplxmul(stateVec[offset10], coeff1);
                // stateVec[offset11] = cplxmul(stateVec[offset11], coeff0);
            }
            case RXX {
                let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
                let offset01: i32 = (i & lowMask) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);
                let offset10: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | ((i & hiMask) << 2);
                let offset11: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);

                let amp00 = stateVec[offset00];
                let amp01 = stateVec[offset01];
                let amp10 = stateVec[offset10];
                let amp11 = stateVec[offset11];

                stateVec[offset00] = cplxmul(amp00, coeff0) + cplxmul(amp11, coeff1);
                stateVec[offset01] = cplxmul(amp01, coeff0) + cplxmul(amp10, coeff1);
                stateVec[offset10] = cplxmul(amp10, coeff0) + cplxmul(amp01, coeff1);
                stateVec[offset11] = cplxmul(amp11, coeff0) + cplxmul(amp00, coeff1);
            }
            case RYY {
                let offset00: i32 = (i & lowMask) | ((i & midMask) << 1) | ((i & hiMask) << 2);
                let offset01: i32 = (i & lowMask) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);
                let offset10: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | ((i & hiMask) << 2);
                let offset11: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);

                let amp00 = stateVec[offset00];
                let amp01 = stateVec[offset01];
                let amp10 = stateVec[offset10];
                let amp11 = stateVec[offset11];

                stateVec[offset00] = cplxmul(amp00, coeff0) - cplxmul(amp11, coeff1);
                stateVec[offset01] = cplxmul(amp01, coeff0) + cplxmul(amp10, coeff1);
                stateVec[offset10] = cplxmul(amp10, coeff0) + cplxmul(amp01, coeff1);
                stateVec[offset11] = cplxmul(amp11, coeff0) - cplxmul(amp00, coeff1);
            }
            case SWAP {
                let offset01: i32 = (i & lowMask) | ((i & midMask) << 1) | (1 << hiQubit) | ((i & hiMask) << 2);
                let offset10: i32 = (i & lowMask) | (1 << lowQubit) | ((i & midMask) << 1) | ((i & hiMask) << 2);

                let temp = stateVec[offset01];
                stateVec[offset01] = stateVec[offset10];
                stateVec[offset10] = temp;
            }
            default {

            }
        }
    }
}

fn apply_3q_op(thread_id: u32) {

}

fn scan_probabilities(thread_id: u32) {
    // Scan the chunk of the state vector assigned to this thread and for any probabilities above 1%,
    // write the result to the results buffer and update the atomic index.
    const ITERATIONS: u32 = 1u << (MAX_QUBITS_PER_THREAD);

    let iterations: u32 = select(1u << (QUBIT_COUNT), ITERATIONS, QUBIT_COUNT >= MAX_QUBITS_PER_THREAD);
    let start_idx: u32 = thread_id * ITERATIONS;
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
