# GPU simulator TODOs

Glossary:

- 'thread' is a single execution unit on the GPU that is part of a workgroup.
- 'workgroup' is a group of GPU threads that run in sync on one GPU core and can share workgroup memory.
- 'shot' is a single execution of the quantum circuit.
- 'state vector' is the array representing the quantum state of the system, perhaps for multiple shots.
- 'chunk' is a contiguous segment of the state vector that a thread updates.
- 'batch' is a collection of shots processed concurrently on the GPU.
- 'prepare' kernel updates the shot state in between 'execute' kernel invocations (single-threaded per shot)
- 'execute' kernel applies the quantum operations to the state vector and sums probabilties (multi-threaded per shot)

## Open questions/decisions

- How to do noise on a 2-qubit gate?
  - Have a noise call for both qubits after the gate (i.e. non-correlated noise).
- How to do SPAM noise?
  - Model by adding an 'Id' gate with noise after reset or before measurement.
- How to correlate measurement results back into the reported result?
  - Return all measurement results along with their result id, and correlate back on the CPU.
  - Support only Result[] for now.
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
