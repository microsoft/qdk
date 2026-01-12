# TODO

- Need to add more correlated noise tests:
  - Write some simple tests and tables with 3 qubit correlated noise for bit-flip and phase-flip.
  - Figure out how to validate correcteness for phase-flips (change basis to `|+>` and measure?)
  - Test various orders of qubit arguments
  - Test scaling the qubit args up to 12 qubit noise tables.
  - Tests with multiple noise tables in the same program.
- Simplify the Python API/usage for correlated noise runs.
  - Dedicated Python GpuContext class the performs the QIR pass and manages the GPUContext
  - If new noise table is provided, re-run the QIR pass to insert the correlated noise ops
  - Should it error if unknown instruction is present?
- Finish one of Aarthi's noise table use-cases
  - Copy the Beryllium circuits to Q# with the custom noise intrinsic that matches the gadget tables
- Prep demo showing
  - Noise config usage (existing)
  - Noise table usage (loading from dir)
  - Noise table usage (programmatic string generation)
  - Speed of running consequtive programs with same noise tables
- Allow for setting/replacing one noise table between runs (efficiently)

## MISC

- Make SWAP just exchange qubit probabilities, not track/update them.
- Add unit tests for SWAP and other gates to verify correctness.

## General TODO list

- Add amplitude damping and dephasing noise
- Create a 'NoiseModel' widget for notebooks
- Add the ability to sample results if no noise or qubit reuse
- Add duration tracking and 'idle noise' to the GPU simulator
- Add a way to 'trace' execution of a specific shot (e.g. noise application, measurements, etc.)
- Update movement/circuit widgets to show above events

## Open questions

- 2Q gates that use the Rydberg interaction (CZ) have much higher damping and dephasing rates than single qubit gates.
- Also application is not symmetrical on the qubits if using "blockade" (only the excited qubit experiences Rydberg decay).
- So maybe apply CZ specific p_amp and p_phase on the control qubit only for CZ gates?

## How to simplify?

- Should 2q gates be able to handle their own loss, damping, measurement, scaling, etc.?
  - How would idle time dependent noise work on each qubit?
  - Could be useful if we ever add Mzz gates in the future?
- Should have a dedicated 1q/2q gate for 'non_unitary' to make it clear when all qubit probabilities need to be updated.
  - Could we get rid of the 'scale' field if we did this?
- Should have a dedicated 'is_pure_phase' flag for ops that only change the phase of the state vector?
  - This would allow us to skip the probability updates in the GPU simulator.

## Use-case and solution

- I want to model state preparation noise.
  - Model this as noise on a Reset operations (e.g. bitflip) in the NoiseConfig object
  - On circuit start, or after an MResetZ, append an Id with the noise after.

## How to implement sampling

- Add a 'sample' op and GPU can repeatedly sample?
- Or could return the state vector and let the CPU-side sample it?

## How to build for Metal (or CUDA)

- Have the Rust crate the processes QIR and inserts noise ops expose a C API and build an .so file
- Use an Xcode Object-C/Metal project that uses the above shared object.
- Mirror the GPU controller WebGPU code with similar Metal code.
- Mirror the WGSL shader code with in Metal
- Would it be better to expose the Objective-C/Metal project as an .so for Rust?
  - This would make it easier to use from Python on MacOS

## Work plan

- Merge Mz back into single qubit op
  - Use renormalization != 1.0 to handle any non-unitary ops
  - Add support for M/Mz, not just MResetZ
  - All single-qubit ops with renormalization != 1.0 should update all (non-definite) probs, as matrix may be non-unitary
- Eventually, will need to track duration to handle branching. Maybe just to that now?
  - No, just add a dedicated `damping_noise(p_amp, p_phase, qubit)` op for now.
- Ops building will need to track duration and qubit usage and insert the damping_noise ops as needed.
  - Does clifford sim already do this?
- NoiseConfig will need a duration on each op.
- NoiseConfig will need T1/T2 values for amplitude damping and dephasing.
- Will need to honor 'parallel' blocks to avoid adding duration multiple times.

## Detailed TODO for fast 'no-noise' runs

- [ ] Add a pass in the gpu simulator to set a flag if no_noise (no noise table and no noise ops)
- [ ] In the same pass flag if there is no qubit reuse (i.e. every qubit is not measurement or entangled after any first measurement op)
- [ ] If both flags above are true, add a 'sample' option to the GPU simulator

  - [ ] Elide all measurements
  - [ ] Generate and sort a stream of random numbers (one for each shot) and put in a buffer
    - Maybe add a final sequence of 'sample' ops to the end of the QIR stream
  - [ ] Run only one shot
  - [ ] Use the random numbers to sample the final state vector
  - [ ] Should have an option to force NOT using sampling

- [ ] Implement non_pauli_noise (including loss) op in GPU simulator and write some Rust tests
- [ ] Add duration to ops in NoiseConfig and expose in Python
- [ ] Add pass to add 'add_duration' op to the QIR stream based on op durations in NoiseConfig and begin/end parallel blocks
  - [ ] Ensure the above doesn't break existing simulations
- [ ] Add T1/T2 to NoiseConfig and expose in Python
- [ ] Add damping_noise op to QIR stream based on accumulated durations and T1/T2 values
- [ ] Add damping noise to CPU based noisy simulator and compare

## Use cases

### Basic T1/T2 noise model

```python
noise = NoiseConfig()

noise.sx.duration = 50  # ns
noise.cz.duration = 150  # ns
noise.mov.duration = 100  # ns

noise.amplitude_damping.set_t1(50e3)  # 50 us
noise.dephasing.set_t2(70e3)           # 70 us (must be <= 2*T1)

results = run_qir_gpu(qir, shots=1000, noise=noise)
```

Resulting QIR stream

```txt
# First ops have no duration yet, so no damping noise
mov (1,3) 5
loss_noise(p_loss = 0.0005) 5
pauli_noise(p_x = 0.0, p_y = 0.0, p_z = 0.0) 5

mov ..

# Next set of ops, now have duration from prior mov batch, so apply damping noise to each gate
# gates on time since last op on that qubit
sx 5
damping_noise(p_amp = 0.0001, p_phase = 0.00015) 5
pauli_noise(p_x = 0.0, p_y = 0.0, p_z = 0.0) 5

sx ...
```

Inside the simulator, when a qubit is used in a gate, we check its accumulated duration, and insert an damping op as needed. (And reset the accumulated duration to zero).

The does mean that, similar to loss, we need to add a single qubit Id gate prior to any 2q gates to do
the single qubit damping noise.

Simulator 'noise insertion' workflow is:

- Initialize a struct to track each qubit duration since last damping noise
- For each op in the QIR stream:
  - If it is 'begin parallel', push a new duration tracking context with 0 duration. Continue.
  - If it is 'end parallel', pop the duration tracking context, and add duration to each qubit. Continue.
  - If a two qubit op and loss or damping noise is enabled:
    - Apply a preceding identity gate for each qubit for the loss/damping noise
  - If an Mz and loss, damping, or SPAM noise is enabled:
    - Apply a preceding identity gate for that qubit for the loss/damping/SPAM noise
  - If accumulated duration on a qubit is > 0:
    - Compute p_amp and p_phase from T1/T2 and duration
    - Apply damping_noise(p_amp, p_phase, qubit)
    - Reset accumulated duration for that qubit
  - If in a duration tracking context, add the op's duration if > current context duration.
  - Else add the op's duration to each qubit.

## Other TODOs

--- SPIKE CODE ---

// Get the duration-based noise operation for the given qubit in the shot. Note that this returns a
// matrix of real values (not complex values) stored in a vec4f.
// TOOD: This should be called and applied when T1 or T2 are != 0.0 (and != +inf) and the qubit isn't already lost
fn get_duration_noise(shot_idx: u32, qubit: u32) -> vec4f {
let shot = &shots[shot_idx];
let qstate = &shot.qubit_state[qubit];

```wgsl
    // TODO: Check the math when T1 = +inf (no amplitude damping) or T2 = 2 * T1 (no 'pure' dephasing)
    // NOTE: IEEE 754 specifies that division by +inf results in 0.0, and that +inf == +inf
    // NOTE: The WGSL spec states: "Implementations may assume that overflow, infinities, and NaNs are not present during shader execution."
    // NOTE: T1 or T2 should never be zero, as that would be instantaneous damping/dephasing

    // TODO: Need to store & retrieve T1 and T2 from the shot state or uniforms
    // t1 = relaxation time, t2 = dephasing time, t_theta = 'pure' dephasing time
    // t2 must be <= 2 * t1
    let t1 = 0.000003;
    let t2 = 0.000001; // t2 should be <= 2 * t1

    // TODO: Should duration be a u32 in some user-defined unit to avoid float precision issues over long durations?
    let duration = shot.duration;
    let time_idle = duration - qstate.idle_since;

    if (t1 == 0.0 && t2 == 0.0 || qstate.heat == -1.0 || time_idle <= 0.0 || duration <= 0.0) {
        // No noise to apply, or qubit is lost, or no time has passed
        return vec4f(1.0, 0.0, 0.0, 1.0);
    }

    // No amplitude damping noise (t1 == 0.0) means treat t1 as +inf, and 1/+inf = 0.0

    // We need to avoid infinities here, so handle the t2 == 2 * t1 case separately. We treat a value of 0.0 as +inf.
    let t_theta = select(1.0 / ((1.0 / t2) - (1 / (2 * t1))), 0.0, t2 >= 2 * t1);


    // TODO: Remember to reset the idle_since for the qubit when acted upon

    // Work through some concrete examples here, as the values can be so small we want to check it doesn't underflow a float (10**-38)
    // - Let idle time be in ns and is 100ns.
    // - Let T1 be (10_000ns) and T2 be (10_000ns), so T_theta = 1 / (1/10_000 - 1/(20_000)) = 1 / (0.0001 - 0.00005) = 1 / 0.00005 = 20_000ns
    // - Then p_damp = 1 - exp(-100 / 10_000) = 1 - exp(-0.01) = 1 - 0.99005 = 0.00995
    // - Then p_dephase = 1 - exp(-100 / 20_000) = 1 - exp(-0.005) = 1 - 0.99501 = 0.00499
    // - If T1 = 10 seconds or 10_000_000_000ns, then p_damp = 1 - exp(-100 / 10_000_000_000) = 1 - exp(-0.00000001) = 1 - 0.99999999 = 0.00000001
    //
    // Note: For very small x, exp(-x) ~= 1 - x, so if x is below some threshold, we may just want to return x * -1 instead of 1 - exp(x), else
    // the intermediate result may round to 1.0 - 1.0 in float precision.

    // Amplitude damping probability (0% if no T1 value specified)
    var p_damp: f32 = 0.0;
    if (t1 > 0.0) {
        let x = time_idle / t1;
        if (x < 0.00001) {
            p_damp = x; // Use linear approximation for very small x
        } else {
            p_damp = 1.0 - exp(-x);
        }
    }

    var p_dephase: f32 = 0.0;
    if (t_theta > 0.0) {
        let x = time_idle / t_theta;
        if (x < 0.00001) {
            p_dephase = x; // Use linear approximation for very small x
        } else {
            p_dephase = 1.0 - exp(-x);
        }
    }

    let rand = shot.rand_damping; // TODO: Guess we don't need shot.rand_dephase?

    // The combined 'thermal relaxation' noise matricies are:
    //   - AP0: [[1, 0], [0, sqrt(1 - p_damp - p_dephase)]]
    //   - AP1: [[0, sqrt(p_damp)], [0, 0]]
    //   - AP2: [[0, 0], [0, sqrt(p_dephase)]]
    // Work backwards, defaulting to AP0 if AP2 or AP1 aren't selected
    // AP0 will just be the identity matrix if there's no damping or dephasing

    let p_ap2 = p_dephase * qstate.one_probability;
    let p_ap1 = p_damp * qstate.one_probability;

    if (rand < p_ap2) {
        // Return AP2 with renormalization to bring state vector back to norm 1.0
        return vec4f(0.0, 0.0, 0.0, 1.0 / sqrt(qstate.one_probability));
    } else if (rand < (p_ap2 + p_ap1)) {
        // Return AP1 with renormalization to bring state vector back to norm 1.0
        return vec4f(0.0, 1.0 / sqrt(qstate.one_probability), 0.0, 0.0);
    } else {
        // Return AP0
        // Entry (1,1) needs to scale down, then renormalize both back up so total probability is norm 1.0
        let new_1_1_scale = sqrt(1.0 - p_damp - p_dephase);
        let renorm = 1.0 / sqrt(qstate.zero_probability + qstate.one_probability * new_1_1_scale * new_1_1_scale);
        return vec4f(renorm, 0.0, 0.0, new_1_1_scale * renorm);
    }

## }
```
