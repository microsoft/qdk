# The MPS engine: native calls and numerics

The details behind [the MPS engine](../README.md#the-mps-engine) of the
crate's README: the native calls each of its [phases](../README.md#phases)
makes, in order, and the numerics of its reads.

## Native calls of each phase

Each band of the diagram is one phase, or the session's opening or closing,
with its native calls in order.

```mermaid
sequenceDiagram
  participant S as MPS simulation method
  participant E as NativeState
  participant D as DeviceSession
  participant R as CUDA Runtime
  participant N as cuTensorNet
  Note over S,N: Session: open, in CuTensorNetEngine::new
  D->>R: cudaGetDeviceCount, cudaSetDevice, cudaStreamCreateWithFlags
  D->>N: Create (the engine's handle)
  Note over S,N: Phase: create
  S->>E: prepare(capacity)
  E->>N: CreateState, CreateWorkspaceDescriptor
  Note over S,N: Phase: restart, when due, before the next operator
  E->>N: Create (a new own handle), CreateState, StateInitializeMPS (the last computed MPS)
  E->>N: StateFinalizeMPS, StateConfigure ×4, CreateWorkspaceDescriptor
  E->>N: DestroyState (old), DestroyWorkspaceDescriptor (old)
  E->>R: cudaFree (the old state's operator tensors)
  E->>N: Destroy (the previous own handle, never the engine's)
  Note over S,N: Phase: operator
  S->>E: apply or project (sites, matrix)
  E->>R: cudaMalloc, cudaMemcpy (kept as long as the native state)
  E->>N: StateApplyTensorOperator
  Note over S,N: Phase: compute, on the first read after a change
  opt first compute of this native state
    E->>N: StateFinalizeMPS, StateConfigure ×4
  end
  E->>R: cudaMalloc (outputs), cudaMemGetInfo
  E->>N: StatePrepare (limit: the budget, or the larger of the free memory and the held scratch)
  E->>N: WorkspaceGetMemorySize
  opt the state's scratch is smaller
    E->>R: cudaMemGetInfo, cudaMalloc (larger scratch)
    E->>N: WorkspaceSetMemory (the state's descriptor)
    E->>R: cudaFree (the smaller scratch)
  end
  E->>N: WorkspaceSetMemory
  E->>R: cudaStreamSynchronize (cudaStreamLegacy: the copies)
  E->>N: StateCompute
  E->>R: cudaStreamSynchronize
  E->>N: StateCaptureMPS
  E->>R: cudaFree (the previous compute's outputs)
  Note over S,N: Phase: query
  E->>N: CreateNetworkOperator
  E->>R: cudaMalloc, cudaMemcpy (each distinct factor)
  E->>N: NetworkOperatorAppendProduct, CreateExpectation, ExpectationConfigure, CreateWorkspaceDescriptor
  E->>R: cudaMemGetInfo
  E->>N: ExpectationPrepare, WorkspaceGetMemorySize
  E->>N: WorkspaceSetMemory (if the expectation asks for scratch: the state's, grown first if needed)
  E->>R: cudaStreamSynchronize (cudaStreamLegacy: the copies)
  E->>N: ExpectationCompute
  E->>R: cudaStreamSynchronize
  E->>N: the destroys
  E->>R: cudaStreamSynchronize, cudaFree (the factors)
  Note over S,N: Phase: fork
  E->>N: CreateState, CreateWorkspaceDescriptor
  E->>R: cudaMalloc, cudaMemcpy (device to device, one per site)
  E->>R: cudaStreamSynchronize (cudaStreamLegacy: the copies)
  E->>N: StateInitializeMPS (the copy), StateFinalizeMPS, StateConfigure ×4
  Note over S,N: Phase: close
  S->>E: drop
  E->>R: cudaSetDevice, cudaStreamSynchronize
  E->>N: the destroys, the descriptors before the scratch they hold
  E->>R: cudaFree (every retained buffer)
  E->>N: Destroy (the state's own handle, if restarted)
  Note over S,N: Session: close, once the engine and all its states are dropped
  D->>R: cudaSetDevice, cudaStreamSynchronize
  D->>N: Destroy (the engine's handle)
  D->>R: cudaStreamDestroy
```

cuTensorNet's calls are shown without their `cutensornet` prefix; NVIDIA
documents each on its
[functions page](https://docs.nvidia.com/cuda/cuquantum/latest/cutensornet/api/functions.html#tensor-network-state-api),
for example
[`cutensornetStateCompute`](https://docs.nvidia.com/cuda/cuquantum/latest/cutensornet/api/functions.html#cutensornetstatecompute).
`ExpectationCompute` writes to host memory, and NVIDIA documents that it
synchronizes the stream only "in the current release", a "restriction
[that] may be released in the future": admitting a version other than 2.13.0
([Versions](../README.md#versions)) means checking that again.

## Numerics

Every read divides by the squared norm `ExpectationCompute` returns beside its
value. With $P^{(b)}_q$ the projector of site $q$ on value $b$:

$$\langle O\rangle = \frac{\langle\psi\rvert O \lvert\psi\rangle}{\langle\psi\vert\psi\rangle}, \qquad p_q(1) = \frac{\langle\psi\rvert P^{(1)}_q \lvert\psi\rangle}{\langle\psi\vert\psi\rangle}.$$

A conditional probability, with $\Pi = \prod_{(k, b) \in \text{given}} P^{(b)}_k$,
is two queries on the state:

$$p_q(1 \mid \text{given}) = \frac{\langle \Pi\, P^{(1)}_q \rangle}{\langle \Pi \rangle}.$$

A projection on value $b$ keeps the norm,
$\lvert\psi\rangle \mapsto P^{(b)}_q \lvert\psi\rangle / \sqrt{p_q(b)}$. The
engine pins `SvdNormalizationNone`, so a truncation never rescales what it
keeps: the retained weight is $w = \langle\psi\vert\psi\rangle \le 1$. The
bond asked for at cut $c$, between sites $c$ and $c+1$ of $n$, is the
capacity's bond there, $\chi^{\text{capacity}}_c$, lowered to what the cut
can carry (`NativeCapacity::new`); a split also drops the smallest singular
values under the cutoff of [`src/engine/policy.rs`](../src/engine/policy.rs):

$$\chi_c = \min\left(\chi^{\text{capacity}}_c,\ 2^{c+1},\ 2^{\,n-c-1}\right).$$
