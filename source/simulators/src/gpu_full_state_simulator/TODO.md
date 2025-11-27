# Add amplitude damping and dephasing

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

## TODO

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

- Add support for SWAP (like CZ, should not change probabilities)
