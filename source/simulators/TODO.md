# GPU simulator TODOs

Glossary to help understand the terminology used in the code and notes:

- 'thread' is a single execution unit on the GPU that is part of a workgroup.
- 'workgroup' is a group of GPU threads that run in sync on one GPU core and can share workgroup memory.
- 'shot' is a single execution of a quantum circuit.
- 'state vector' is the array representing the quantum state of the system, perhaps for multiple shots.
- 'entries' are the individual complex amplitudes in the state vector.
- 'chunk' is a contiguous segment of the state vector that a thread updates.
- 'batch' is a collection of shots processed concurrently on the GPU.
- 'prepare' kernel updates the shot state in between 'execute' kernel invocations (single-threaded per shot)
- 'execute' kernel applies the quantum operations to the state vector and sums probabilties (multi-threaded per shot)

## Python calling

- GPU driver lives in `source/simulators/src/gpu_full_state_simulator/gpu_controller.rs`
- Add to code in `source/simulators/src/gpu_full_state_simulator/per_gate_pauli_noise.rs` to insert noise ops.
- Add the GPU execution to `source/simulators/src/gpu_full_state_simulator.rs`
- Add a `#[pyfunction]` to `source/pip/src/qir_simulation/gpu_full_state.rs` to drive the above
  - Inject noise based on NoiseConfig if provided in here.
- Document it in 'source/pip/qsharp/\_native.pyi'
- Add user facing API in `source/pip/qsharp/_simulation.py`
- Bind it in `source/pip/src/interpreter.rs`
- QirInstruction is defined in `source/pip/src/qir_simulation.rs`

## TODO

- Wire up to Python with noise addition from NoiseConfig.
- Add shot_id tracing and a Trace buffer to record ShotState at various points.
- Add option for multiple noise ops per op.
- Add qubit loss for single and two qubit gates and test.
  - To simplify logic for 2-qubit gate loss, just do the 2-qubit gate, and add an ID op with loss on
    each qubit after it. Ensure the 'execute' kernel is optimized to do nothing on 'id' gate. (This assumes
    most 'loss' will occur on move and idle gates, thus loss will rarely be configured on CX or CZ gates.
    Revisit this assumption later if needed)
- Add the Python pass to insert noise and loss operations into circuits from a NoiseConfig object.
- Add flags to shot (noise was applied to q1/q2, loss occurred on q1/q2) etc. and 'trace' buffer
  - In the trace buffer, for a specific shot, record the ShotState at the end of each 'prepare' for the shot.
- Try out the chemistry circuits at this point.
- Add custom 2q unitaries (e.g. rzz) and test.
- Add 'dispatch chunking' for circuits that are too large for one command buffer
- Add batching for multiple rounds of shots.
- Test the giant Ising circuits (4x4 and 5x5) end-to-end.
- Add amplitude damping & dephasing noise
- Add 'gradual rotation' noise.
- Add chunk skipping logic for qubits of known zero or one state.
- Add pass to move initial ops on high order qubits as late as possible, and measurements on high order qubits to as early as possible

## Open questions/decisions

- How to do noise on a 2-qubit gate?
  - For now, have a noise call for both qubits after the gate (i.e. non-correlated noise).
- How to do SPAM noise?
  - Model by adding an 'Id' gate with noise after reset or before measurement.
- How to minimize shader logic?
  - Only gates in the shader should be: ID, RZ, RZZ, CZ, CX, SWAP, MAT1Q, MAT2Q, MRESETZ, SAMPLE, PROBS
    - ID can be elided, MZ,RESET,SAMPLE,PROBS have unique logic, and RZ,RZZ,CZ,CX,SWAP can be optimized nicely.
    - Other unitary gates can be done with MAT1Q and MAT2Q. (Even with thermal, rotation, or idle noise?)
- How to turn an op into a 'loss'
  - For single qubi unitary, set the loss flag for the qubit in the shot state.
    - Execute kernel will treat as a a 'MRESETZ' with a result id of -1 (i.e. ignore the result).
  - For two qubit unitary, set the loss flag on either/both in the shot state.
    - Construct a unitary with `|0><0|*[OP]` (or `|1><1|`), set renormalize value in shot state. Dispatch MAT2Q.

## Next steps

- Verify float accumulation precision issues (up to 27 qubits)
- Write tests for the above using different size and permutations of circuits.
- Write tests the permute qubits and verify results are the same.
- Verify output statistically against sparse simulator and Qiskit for various circuits.

Gates to add for various scenarios:

- Add h & cx to test Bell pair, qrgn, maximum probability spread, and maximum entanglement.
  - Test float fidelity up to 27 qubits with H on all.
  - Add 'precise' summing to check effect on perf and accuracy.
  - Test cross-workgroup processing.
- To support Atom: sx, rz, cz, mresetz, mov (id with loss noise), Pauli noise.
  - Get Teleport working end-to-end for 12 & 27 qubits (4 or 9 teleports using 3 qubits each).
  - Test with various state prep and qubit remappings to ensure results remain correct.
- To support Benzene: h, rz, x, cx, s, sdg, MResetEveryZ (sample), depolarizing noise.
  - Give a drop to Rushi to test
- To support Ising: rx, rzz, MResetEveryZ (sample).
  - Get a 4x4 and a 5x5 lattice working end-to-end. Compare perf to sparse and to Qiskit.

Features:

- Check prob sums and use metalsim/metalsim/neumaier.c also if necessary.
- Take a circuit as input, with optional noise config.
  - Only support 'canonical' one and two qubit gates plus 'mov' for now, i.e., NOT 'ccx'.
  - Only support Pauli noise (incl. depolarizing) and qubit loss for now.
  - If no qubit is used post-measurement (base profile), replace all measurements with a final 'SAMPLE' call.
  - Timing info and thermal relaxation, idle noise, gradual rotation can come later.
- Add the noise operations to the circuit based on the noise config if provided (NOTE: Could also already be presented in the circuit).
- Calculate the shots, shot-size, batches, workgroups, threads, chunks, chunk-size, etc. based on circuit and GPU properties.
- Create the GPU resources and copy the circuit to the GPU.
- Loop for the number of batches needed:
  - Initialize shot states on the GPU (rng_seed and first_shot_id will need to be passed).
  - Dispatch 'prepare' and 'execute' kernels for the circuit for each op.
    - Count is the number of 'non-noise' operations
  - Copy the shot states back to the CPU.
- Process the shot states and return the measurement results.

## Later steps

- Add timing info, thermal relaxation, idle noise, gradual rotation.
- Support correlated Pauli noise on multiple qubit gates.
- If base profile and no noise, just run one shot and sample the final state vector.
- Support 'ccx' and 'ccz' gates.
- Collapse consecutive ops on same qubits into a single op in the 'prepare' kernel.
  - Could treat subsequent ops - if already dispatched by the CPU - as an ID and have the 'execute' kernel skip them.
- Try reordering ops for better collapsing of consecutive ops on same qubits.
- Add support for custom unitaries for gates, and kraus matrices for noise.
- Test shots with more than 22 qubits (spaning workgroups, float fidelity, etc.)
- Try grouping ops by collapsing consecutive ops on same 2 qubits.
- Support Adaptive profile and non Result[] shot results.

## More details

### The 'prepare' kernel

- It should combine quantum operations and any following noise into one op/matrix.
- It should set shot state for qubit probabilities and 'loss', and whether to 'renormalize', etc. to be used by the 'execute' kernel.
- (later) It should normalize 'phase' only gates into Rz rotations with a top-left entry of 1.

### The 'execute' kernel

- It should apply the quantum operations to the shot states.
- It should 'renormalize' the state if the shot flag is set.
- It should track probabilities for updated qubits as it applies operations.
- It should skip updating entries where the probability is known to be zero.
