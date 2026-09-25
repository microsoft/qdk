# 2D Ising Quench Demo: MPS and General Tensor-Network Contraction

This demo simulates the Trotterized 2D Ising quench from the `qdk-chemistry`
[resource-estimation notebook](https://github.com/microsoft/qdk-chemistry/blob/1eb14a9d73685d4e57ee7ee7ca6f4d2ef845a6dd/examples/estimation_ising_2d.ipynb)
with two tensor-network methods, through the public `qdk.simulation.run_qir` API:

- **MPS**, the existing cuTensorNet matrix-product-state backend, which approximates the state by truncation;
- **general contraction**, which contracts the circuit's 2D tensor network without truncation.

Both methods run the **same** QIR program and return ordered measurement shots.
The question the demo lets you explore is:

> For a given Ising quench and accuracy, how far does each method reach, and at what cost?

It is a demonstration for feedback, not a shipped feature. It sits next to the
1D [MPS Trotter quench demo](../mps_trotter_quench_demo/DEMO.md) and uses the
same execution layer.

> **Preview status.** Circuit generation and `type="mps"` work today.
> General contraction through `run_qir`, the `--field` option and the demo
> script are **planned for this demo's first version**; commands for them are
> marked ⏳ and their names may change. Nothing in this document has been
> measured as an MPS versus general-contraction comparison yet.

| | |
| --- | --- |
| Circuit source | `qdk-chemistry==2.2.1` builders, via [`build_measured_circuit.py`](build_measured_circuit.py) |
| Public entry point | `run_qir(qir, shots=..., type=...)` |
| Methods | MPS (today) · general contraction (⏳ first version) |
| Reference host | NVIDIA A100 80GB PCIe, Linux x86_64 |
| Validation so far | 4x4 exact contraction agrees with an independent CPU state to `6e-10` ([Appendix A](#appendix-a--validation-history)) |

---

## TL;DR

**What you do.** Generate an Ising circuit for an `N×N` lattice, run the same
QIR with MPS and with general contraction, and compare two observables and
their cost.

```text
generator(N, h) ──► QIR ──► run_qir(type="mps")        ──► shots ──┐
                        └─► run_qir(general contraction) ──► shots ──┴─► m_z, C_ZZ ± error, time, memory
```

**Why it is interesting.** 2D lattices are known to be hard for MPS, and a
field near the critical point is expected to make them harder. General
contraction has no truncation, but its cost grows with the circuit's
contraction structure. Which one wins, and where, is what we want to measure
with you, not something we assume.

**What we want from you.** Run the examples, try your own sizes and fields,
and tell us what is useful and what is missing ([§8](#8-feedback-wanted)).
[§7](#7-what-could-come-next) lists what could come next, including noise.

---

## 1. The problem

The notebook's Hamiltonian on an open `N×N` square lattice, qubit `q = y*N + x`:

$$
H = J \sum_{\langle i,j \rangle} Z_i Z_j + h \sum_i X_i,
\qquad
|\psi(t)\rangle \approx U_{\text{Trotter}}(t)\,|0\rangle^{\otimes N^2}.
$$

```text
      o───o───o───o     o = qubit, q = y*N + x
      │   │   │   │     ─ / │ = ZZ bond (J), open boundaries
      o───o───o───o     every o also feels a transverse field h·X
      │   │   │   │
      o───o───o───o     notebook: 10×10 = 100 qubits (resource estimation only)
      │   │   │   │     this demo: from 4×4 = 16 qubits upwards
      o───o───o───o
```

Every circuit uses the notebook's settings: start in $|0\cdots0\rangle$, evolve
to `t=1` with fourth-order Trotter–Suzuki and two subdivisions, then measure
every qubit in Z. Only the lattice size and the field change.

**Two fields to compare.**

| Scenario | J | h | What it is |
| --- | --- | --- | --- |
| Baseline | 1 | 0.5 | The notebook's default |
| Near-critical | 1 | 3.03 | Close to the square-lattice critical point |

The ground-state critical point is $h/J \approx 3.044$
([Blöte and Deng, Phys. Rev. E 66, 066110 (2002)](https://doi.org/10.1103/PhysRevE.66.066110),
same Pauli normalization). That is a zero-temperature, infinite-lattice
property. Whether it makes *this* finite-time quench on a small lattice harder
is exactly the kind of thing the demo lets you check.

**Two observables, from the shots.** For each shot, with bits $b_i$ and spins
$z_i = 1 - 2b_i$, on $n=N^2$ sites and $|E|=2N(N-1)$ bonds:

$$
m_z = \frac{1}{n}\sum_i z_i
\quad\text{(how much of the initial polarization survives)},
\qquad
C_{ZZ} = \frac{1}{|E|}\sum_{\langle i,j\rangle} z_i z_j
\quad\text{(how aligned neighbours are)}.
$$

The demo averages both over shots and reports a standard error.

## 2. How to use it

### 2.1 Prerequisites

Same host requirements as the [MPS demo](../mps_trotter_quench_demo/DEMO.md#41-prerequisites):
Linux x86_64, an NVIDIA GPU, cuQuantum `libcutensornet.so.2`, and QDK built with
`--qdk --editable`. Circuit generation also needs `qdk-chemistry`:

```bash
./source/qdk_package/.venv/bin/python -m pip install qdk-chemistry==2.2.1
```

### 2.2 Step 1: generate the circuit

```bash
./source/qdk_package/.venv/bin/python \
  samples/python_interop/ising2d_tensor_network_demo/build_measured_circuit.py \
  --nx 4 --ny 4 --output ising-4x4-h0.5.ll
# wrote ising-4x4-h0.5.ll: 16 qubits, 16 measured results
```

The output is an ordinary Base-profile QIR program. `run_qir` knows nothing
about lattices or fields; any program works as long as the chosen method
supports its gates.

⏳ First version adds `--field h` (today the field is fixed at `h=0.5`).

### 2.3 Step 2: run it

```python
from qdk.simulation import MpsOptions, run_qir

qir = open("ising-4x4-h0.5.ll").read()

# MPS: works today
mps_shots = run_qir(qir, shots=1000, seed=42, type="mps",
                    mps_options=MpsOptions(device="nvidia"))

# ⏳ General contraction: first version; selector name to be decided
tn_shots = run_qir(qir, shots=1000, seed=42, type="tensor_network")
```

Each result is a list of shots, and each shot is a list of `Result` values
ordered `q0, q1, ...`. A general-contraction request never falls back to MPS
silently: if it cannot run, it fails with an error.

### 2.4 Step 3: summarize

```python
import numpy as np
from qdk import Result

def summarize(shots, N):
    b = np.array([[r == Result.One for r in shot] for shot in shots], dtype=float)
    z = 1.0 - 2.0 * b                                   # (shots, N*N), q = y*N + x
    bonds = [(y*N + x, y*N + x + 1) for y in range(N) for x in range(N - 1)] + \
            [(y*N + x, (y+1)*N + x) for y in range(N - 1) for x in range(N)]
    m = z.mean(axis=1)                                  # one value per shot
    c = np.mean([z[:, i] * z[:, j] for i, j in bonds], axis=0)
    se = lambda v: v.std(ddof=1) / np.sqrt(len(v))
    return {"m_z": (m.mean(), se(m)), "C_ZZ": (c.mean(), se(c))}

print(summarize(mps_shots, 4))
```

The error bars come from the spread across shots. Sites and bonds in the same
shot are correlated, so they are averaged per shot first rather than treated
as independent samples.

### 2.5 ⏳ The demo script

The planned `run.py` does steps 1–3 for both methods and prints one table.
Proposed usage:

```bash
./source/qdk_package/.venv/bin/python \
  samples/python_interop/ising2d_tensor_network_demo/run.py \
  --size 4 --field 0.5 --shots 1000 --seed 42 \
  --methods mps tensor_network --output ~/ising2d-4x4-h0.5.json
```

`--size N` sets up an `N×N` circuit and `--field` sets `h`. Nothing else changes
between runs. Expected output shape (values are placeholders until measured):

```text
ising2d | size=4 qubits=16 J=1 h=0.5 shots=1000 seed=42
method           m_z               C_ZZ              time     peak memory
exact            0.9508            0.9327            —        —
mps              …  ± …            …  ± …            … s      … MiB
tensor_network   …  ± …            …  ± …            … s      … MiB
```

At sizes where an exact reference exists, the script prints it and whether each
method agrees within its error bars. Timings split setup (planning,
preparation) from sampling, because repeated shots reuse the setup.

## 3. Examples

### Example 1: baseline 4x4, both methods against exact (h=0.5)

The first thing to run. At 16 qubits the exact answer is known: from the
independent CPU state for this exact circuit,
**$m_z = 0.9508$ and $C_{ZZ} = 0.9327$**. With 1000 shots expect error bars of
about `0.003` and `0.004`. Both methods should agree with these values; that is
the sanity check before scaling up.

```bash
run.py --size 4 --field 0.5 --shots 1000 --methods mps tensor_network   # ⏳
```

### Example 2: near-critical 4x4 (h=3.03)

Same size and settings, field near the critical point. The first version ships
an independent exact reference for this 4x4 circuit too, so both methods are
checked here as well.

```bash
run.py --size 4 --field 3.03 --shots 1000 --methods mps tensor_network  # ⏳
```

### Example 3: grow the lattice

Keep the field and increase `--size` until a method runs out of time or
memory. The script reports, per method, time and peak memory, and for MPS
whether it had to truncate.

```bash
for N in 4 5 6 7 8; do
  run.py --size $N --field 3.03 --shots 1000 --methods mps tensor_network \
         --output ~/ising2d-${N}x${N}-h3.03.json                          # ⏳
done
```

Beyond the exact-reference sizes, agreement between the two methods is evidence,
not proof: MPS may be truncating, and general contraction may simply be too
expensive. Both outcomes are useful results.

### Example 4: your own circuit

`run_qir` accepts any Base-profile QIR with terminal measurements. Today MPS
supports `X`, `H`, `Rx`, `Rz`, `CNOT` and `Rzz` on at least two qubits. The first
version of general contraction supports the gates this demo needs (`Rx`,
`Rzz`); more gates can be added if you need them.

## 4. Reading the results

- **Error bars cover shot noise only.** More shots shrink them, but cannot
  remove MPS truncation error or Trotter error.
- **Agreement is with the circuit, not with the physics.** Both methods simulate
  the same Trotterized circuit. Matching the exact reference means the method is
  right for that circuit; it does not measure how far the circuit is from exact
  time evolution. That gap can be larger at `h=3.03`, with the same Trotter
  settings.
- **Near-critical is a hypothesis, not a promise.** The critical point is a
  ground-state property. For this short quench it may or may not be the
  hardest field for either method.
- **The two methods scale differently.** For MPS the cost depends on how
  entangled the state becomes, so it changes with `h`. For general contraction
  the cost is set mostly by the circuit's structure, which is the same for
  every `h` at a given size.
- **Sign conventions do not matter here.** Flipping the sign of `J`, of `h`, or
  both gives the same shot distribution from $|0\cdots0\rangle$, so the demo's
  results also describe the ferromagnetic convention.

## 5. How it works

### 5.1 One program, pluggable samplers

```text
run_qir(qir, shots, type=...)
        │
shared execution: runs the program, owns shots and measurement order
        │   asks for "S ordered samples of the final state"
        ▼
Sampling interface (backend-neutral)
 ├─ cuTensorNet MPS sampler                      available today (type="mps")
 ├─ cuTensorNet general-network sampler          ⏳ first version
 └─ further implementations                      on request, for example:
      · generic contraction sampler on the shared contraction interfaces
      · other devices or libraries
```

The sampling interface is the extension point. The first version provides one
general-contraction implementation, cuTensorNet's native sampler. According to
NVIDIA's documentation, it samples groups of qubits from their reduced density
matrices and reuses cached intermediate tensors between those contractions. Other implementations plug in
behind the same interface and are checked by the same tests. For example, a
generic sampler built on the existing
[shared contraction interfaces](../../../source/simulators/src/execution/README.md#shared-contraction-contracts-i3)
would work with any contraction backend and expose a finer cost breakdown.
They are developed if there is interest.

### 5.2 Why MPS finds 2D hard

```text
lattice (N=3)              MPS chain (mode = qubit index)
 0 ─ 1 ─ 2                 0 ─ 1 ─ 2   3 ─ 4 ─ 5   6 ─ 7 ─ 8
 │   │   │                 └─────N─────┘
 3 ─ 4 ─ 5                 every vertical bond spans N chain sites
 │   │   │
 6 ─ 7 ─ 8
```

MPS arranges the qubits in a line, in qubit order. Horizontal bonds stay short,
but every vertical bond becomes a long-range gate across `N` chain sites, which
can grow the MPS bond dimension. How much it grows depends on the state, so
MPS may still do well at small `h` or short times. It is a motivation for the
comparison, not a verdict.

### 5.3 Why general contraction needs a sampler

General contraction computes numbers from the whole 2D network at once, without
truncation. Asking it for the full state is only practical up to about 4x4:

| Lattice | Qubits | Full state (complex f64) |
| --- | --- | --- |
| 4×4 | 16 | 1 MiB |
| 5×5 | 25 | 512 MiB |
| 6×6 | 36 | 1 TiB |

So shots come from the sampler, which contracts only small conditional
probabilities, never the full state. Full amplitudes are used only to validate
small cases. A small output does not make contraction cheap, though: its cost
is set by the intermediate tensors, which is what Example 3 measures.

### 5.4 Why not the other QDK simulators

Dense CPU/GPU statevectors stop at about 25 qubits
([measured](../mps_trotter_quench_demo/DEMO.md#22-the-dense-wall-measured)); the
wgpu `type="gpu"` path has a fixed 27-qubit single-precision limit
([`shader_types.rs`](../../../source/simulators/src/gpu_full_state_simulator/shader_types.rs)).
The sparse simulator densifies quickly under the transverse field, and the
Clifford simulator cannot run generic rotations. They remain useful references
at small sizes.

## 6. Status

| Piece | Status |
| --- | --- |
| Circuit generator from `qdk-chemistry` (`--nx/--ny`, `h=0.5`) | Available |
| `run_qir(type="mps")` | Available |
| 4x4 general contraction, validated against an independent CPU state (private, native) | Done ([Appendix A](#appendix-a--validation-history)) |
| Shared contraction interfaces and reusable inputs | Done |
| Sampling interface and cuTensorNet general-network sampler | ⏳ First version |
| General contraction through `run_qir` | ⏳ First version |
| `--field`, `run.py`, near-critical 4x4 reference | ⏳ First version |
| Measured MPS vs general-contraction results | ⏳ First version |
| Further samplers, public MPS truncation settings, more gates | On request |

## 7. What could come next

If the demo is useful to you, these are directions we could take. None is
planned yet; your feedback decides which come first.

| Possible addition | What it would enable |
| --- | --- |
| **Noise** through `run_qir(noise=...)` for the tensor-network methods | Noisy Ising dynamics and noisy QEC rounds; both methods are noiseless today |
| **Expectation values without shots** | Contract $\langle Z_i\rangle$, $\langle Z_i Z_j\rangle$ or energies directly, with no shot noise |
| **Probabilities of chosen outcomes** | Exact probability of a bitstring or of fixed measurement outcomes (for example postselection or acceptance probabilities) |
| **Mid-circuit measurement, reset and feedforward** | Repeated-round programs such as syndrome extraction; the methods accept terminal-measurement (Base-profile) programs today |
| **More gates** | Circuits beyond the current `Rx`/`Rzz` (contraction) and `X, H, Rx, Rz, CNOT, Rzz` (MPS) sets, for example `T` or general rotations |
| **Accuracy controls** | Choose MPS bond dimension, cutoffs and precision yourself |
| **Larger contractions** | Splitting one contraction into slices across time or several GPUs |
| **Other samplers and backends** | The alternatives listed in [§5.1](#51-one-program-pluggable-samplers) |

**A QEC demo is next.** It will use the same pattern (one QIR program,
several methods, current QDK simulators as references for small cases) on
error-correction circuits. Its questions are different: does contraction cost
stay bounded as rounds are added, what are the exact event and acceptance
probabilities, and what happens with non-Clifford content such as coherent
over-rotation. Programs come from Stim circuits through the existing
`qdk.stim` compiler, so QEC tools that export Stim can feed it directly. It
needs several of the additions above, starting with outcome probabilities and
mid-circuit measurement.

## 8. Feedback wanted

- Are the Ising examples the right ones? Which sizes, fields or times matter to you?
- Are `m_z` and `C_ZZ` useful, or do you need other observables (for example single-site $\langle Z_i\rangle$ or correlations at a distance)?
- Would you want to tune MPS accuracy (bond dimension, cutoffs) yourself?
- Which other circuits would you run: other chemistry models, QEC circuits, your own?
- Which of the additions in [§7](#7-what-could-come-next) would you need first?
- How should the method be selected in `run_qir`? (See the
  [`type=` naming discussion](../mps_trotter_quench_demo/DEMO.md#t17-in-detail--the-type-selector-conflates-two-axes).)

**Feedback that shaped this demo.** A `qdk-chemistry` team member wrote, on the
1D demo (2026-09-08):

> This is a good start - 1D has an analytical solution. 2D is canonically hard for MPS. Depending
> on the parameterization. If you take J=1 and h=3.03, that's the quantum critical point on a
> square lattice. Moving away from that will make the problem easier.
>
> Per the above - you don't need to stand up these circuits yourself. They're in QDK-chemistry. If
> you'd like a run through, let me know.

That is why the demo uses the chemistry circuits, includes MPS, and adds the
`h=3.03` field. A later review pointed out that approximate tensor-network
methods can trade a little accuracy for a lot of speed, which is why the demo
compares accuracy against cost rather than treating "no truncation" as the
goal.

---

## Appendix A — Validation history

These iterations built and validated the general-contraction path before any
public integration. They are retained as the evidence and provenance behind
the demo. Their frozen fixtures and numerical limits are unchanged; statements
about "not yet implemented" describe the state at the time of each iteration.

### Validation map

| Iteration | What it established |
| --- | --- |
| [I1](#i1-retained-input-and-cpu-reference) | Frozen 4x4 `h=0.5` circuit and an independent CPU state reference |
| [I2](#i2-neutral-network-and-shared-coefficient-buffers) | Circuit-to-tensor-network builder and shared coefficient buffers |
| [I3a](#i3a-bounded-native-numerical-experiment) | Native A100 contraction of diagnostic, 2x2 and 4x4 cases against the references |
| Shared interfaces | [Contraction interfaces and reusable inputs](../../../source/simulators/src/execution/README.md#shared-contraction-contracts-i3), tiny cases GPU-validated |

Two circuit shapes appear in this history. **Case A** is the notebook's
`order=4, num_divisions=2` construction used everywhere above (12 field layers
and 10 bond layers, 432 gates at 4x4). **Case B** would be a first-order
construction with more subdivisions, a later convergence check. They name
Trotter schedules, not the two field scenarios.

### I1 retained input and CPU reference

The fixed case is **4x4, J=1, h=0.5, time=1, Trotter order 4, two subdivisions,
identity preparation**. The existing generator is the chemistry decoupling boundary;
neither the reference replay nor the future TN runtime needs chemistry once the input
is frozen. The oracle uses the released `qdk==1.32.3` CPU sparse engine, not a native
extension built from this worktree.

```text
1. Existing chemistry recipe -> one ordered Case A gate body
                                      |
2.                                    +-> measured Q# -> Base QIR
                                      |                   |
                                      |             future TN input
3.                                    +-> same Q# with pre-measurement DumpMachine
                                                          |
4.                                                 SparseStateSim
                                                          |
5.                                            bit conversion -> amplitudes/probabilities
```

`qsharp_source` emits both forms from the same gates; the diagnostic is inserted
at construction, immediately before `MResetEachZ`. There is no second circuit
implementation, QIR state-observation API, TN builder, or NVIDIA dependency.
The existing QDK `AggregateGatesPass` independently checks **every gate, angle,
operand and sequence position** in both the original chemistry QIR and measured
QIR against that body. A separate structural check admits only a single straight-line
entry block, initialization, the expected gates, and the terminal output sequence.

The pinned generator produces **192 Rx + 240 Rzz = 432 gates**. Its actual lattice
is **open in both directions, row-major `q = y*nx + x`, with 24 undirected bonds
of weight 1 and no DFS reordering**. The full adjacency and coloring are retained,
not inferred from a drawing. The measured Base compiler emits 16 `m__body` calls
(terminal resets are eliminated) and one result array ordered `q0` through `q15`.

The coefficient check uses the hand-derived fourth-order Suzuki composition:

$$
p = \frac{1}{4-4^{1/3}},\quad
S_4(\Delta) = S_2(p\Delta)^2 S_2((1-4p)\Delta) S_2(p\Delta)^2,\quad
\Delta = \tfrac12.
$$

Each symmetric `S2(s)` has half-field `Rx(h*s)` layers around the commuting
bond layer `Rzz(2*J*s)`. Adjacent field layers within a subdivision combine.
Tests check this layer order, all sites/bonds and positive/negative coefficients
at 2x2 and 4x4, rather than merely summing angles.

#### Artifacts and numerical contract

All retained files are under [`fixtures/case_a_4x4/`](fixtures/case_a_4x4/):

| File                         | Meaning                                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `unmeasured.ll`              | Original chemistry QIR; generation provenance, not the runtime input                                                      |
| `measured.qs`, `measured.ll` | Frozen measured Q# and verified Base-QIR execution input                                                                  |
| `reference.qs`               | Same gate body, with exactly one dump before terminal measurement                                                         |
| `amplitudes.npy`             | NumPy array of 65,536 little-endian complex-f64 amplitudes (`<c16`), 1 MiB payload plus header                            |
| `probabilities.npy`          | NumPy array of 65,536 little-endian f64 probabilities (`<f8`), 512 KiB payload plus header                                |
| `provenance.json`            | Parameters, actual lattice, package/native-binary and script hashes, source base commit, formats and artifact hashes      |
| `validation.json`            | Conversion accounting, norm, two-seed state repeatability, measured generation/reference time, and explicit I1-only scope |

The source base commit plus script hashes identify the source used during generation;
they do not claim the scripts were unchanged from that commit. Regeneration records a
new time/provenance report and refuses to overwrite an existing artifact directory.
NumPy's `.npy` headers embed shape, dtype and byte order without changing the
floating-point values. The repository's scoped binary Git attributes prevent
line-ending normalization from changing these files; their staged bytes are
hash-checked too.

Q# state dumps put `q0` at the most significant bit. The adapter reverses basis bits
to agree with the planned first-axis-fastest TN output:

$$
k = \sum_{q=0}^{15} b_q 2^q,\qquad
Z = \sum_k |\psi_k|^2,\qquad
p_k = |\psi_k|^2/Z.
$$

All values must be finite and `abs(Z-1) <= 1e-8` **before** normalization.
Analytic signed-Rx, Rzz phase, nonadjacent interference and asymmetric-qubit checks
use absolute error `1e-12`. Repeated reference executions and frozen-state replay
must agree to maximum complex-amplitude error and probability total-variation
distance `<= 1e-12`, without global-phase alignment. The recorded candidate limits
for later TN comparison are `1e-8` for both metrics; no TN comparison had run at
I1 time (I3a later passed them).

The retained squared-norm error is **6.66e-15**; the two pre-measurement states
(seeds 42 and 17) agree exactly on the recorded host. These seeds exercise reference
repeatability, not the later public-shot sampling contract. Sparse simulation has
its existing floating-point/pruning policy; this is not an exact-arithmetic oracle.
No post-reset state or shot histogram is substituted for the numerical reference.

#### Reproduction

From the repository root, using Python 3.11 on the qualified aarch64 host:

```bash
python3.11 -m venv .venv-ising-i1
.venv-ising-i1/bin/python -m pip install -r samples/python_interop/ising2d_tensor_network_demo/requirements-reference.txt
.venv-ising-i1/bin/python samples/python_interop/ising2d_tensor_network_demo/reference.py verify \
  --input samples/python_interop/ising2d_tensor_network_demo/fixtures/case_a_4x4
.venv-ising-i1/bin/python -m pytest -q samples/python_interop/ising2d_tensor_network_demo/test_reference.py

# Optional regeneration into a NEW directory; never overwrite the frozen input.
.venv-ising-i1/bin/python samples/python_interop/ising2d_tensor_network_demo/reference.py generate \
  --output .venv-ising-i1/regenerated-case-a
```

To replay without chemistry, install only `qdk==1.32.3`, `pyqir==0.12.5` and
`numpy==2.3.5` in a separate environment, then run the same `verify` command.
This path rechecks hashes and conversion, recompiles the retained Q#, and recomputes
the state on CPU; it never imports chemistry. It is exercised in a chemistry-free
environment as part of I1. The full test suite additionally needs `pytest` and
chemistry for the two generator tests.

Read either array with `numpy.load(path, allow_pickle=False)`; its dtype and
shape are self-describing and checked by `verify`. The retained QIR is also admitted by the public CPU `run_qir` route;
that check establishes input/output shape only, not the amplitude oracle.

### I2 neutral network and shared coefficient buffers

I2 delivers `qdk_simulators::execution::CircuitTensorNetwork`, not a numerical
backend or new `run_qir` selector. The builder receives a resolved
`QuantumEvolutionRegion` plus its qubit count; it knows nothing about Ising
parameters or chemistry. `qdk_simulators` now depends on the unchanged,
shapes-only `tensornet` crate. MPS and vendor code are unchanged.

```text
1. Frozen measured.ll -> existing AdaptiveProfilePass
2. Existing native conversion -> PreparedAdaptiveProgram
3. AdaptiveExecution::ExecuteRegion -> neutral builder
4. TensorNetwork + immutable buffer bank + node-to-buffer bindings
5. ContractionQuery keeps every final qubit axis
   STOP: no contraction, measurements or samples
```

Initial zero states share `[1,0]`. Rx is a dense factor with axes
`[output,input]`; it creates a fresh wire index. Rzz is a diagonal factor with
axes `[current(q1),current(q2)]`, sharing those indices across the gate:

$$
R_x(\theta)=
\begin{pmatrix}
\cos(\theta/2)&-i\sin(\theta/2)\\
-i\sin(\theta/2)&\cos(\theta/2)
\end{pmatrix},
\qquad
D_{ZZ}(\theta)[a,b]=e^{-i\theta(-1)^{a+b}/2}.
$$

The owner exposes `network()`, `buffers()`, `node_buffer_ids()` and
`output_axes()`. Tensor node `v` uses
`buffers()[node_buffer_ids()[v]]`; the buffer length equals the node's
`Indices::element_count()`. All values follow `Indices::offset_of` and
`strides()`: column-major, first axis fastest. Final axes are ordered
`q0` through `q15`, with `k = sum(b[q] * 2^q)`, matching I1.
`query()` borrows the network; the owner contains no self-reference.

Repeated gates of the same kind and **exact f64 angle bits** share immutable
coefficient storage even when their wire identities differ. Different gate
kinds never alias merely because they have the same buffer length. There is
no approximate matching, mutable scratch-buffer reuse, gate fusion or
device-buffer allocation. The supported gates are I, Rx and Rzz; I is a
validated no-op. Invalid operands, repeated Rzz operands, nonfinite angles and
wire-count overflow return explicit errors. An empty region retains its
zero-state boundaries; zero qubits and no gates describe the scalar one.
Building a network does not allocate its dense amplitude output.

For the unchanged frozen input, qualification establishes:

| Surface | Result |
| --- | --- |
| Gate accounting | Every one of 192 Rx and 240 Rzz gates checked against the existing QDK QIR collector, including coefficients, operand order and wire versions |
| Network | 448 nodes: 16 initial boundaries and 432 gate factors; 208 distinct dimension-two indices |
| Buffer bank | 6 immutable buffers containing 22 complex-f64 values, **352 coefficient bytes**; excludes graph/binding storage and future device/workspace allocations |
| Query | 16 ordered final axes; 65,536 output elements; 160 legal hyperedges; no marginalized wires |
| Numerical qualification | Tiny built networks contracted by NumPy `einsum`, compared against signed/phase-sensitive analytic values with absolute error `1e-12`, no global-phase alignment |
| Coverage | 12 public Rust API tests and 20 Python qualification cases; buffer size, values, sharing, ownership, invalid inputs, nonadjacent/asymmetric cases and frozen-QIR construction |

The private `_tensor_network_build_probe` exports copies of actual builder
data for those tests. It uses the existing preparation/command APIs, admits
one leading region in one block, and stops before measurement without
fabricating outcomes. It is not a full-program validator; I1 separately
qualifies the frozen terminal suffix. Tiny numerical evaluation is bounded
to 12 distinct binary indices and uses NumPy, not a new CPU contractor.
At I2 time the **full 4x4 network had not been contracted or compared
numerically with the CPU reference**; I3a later did both.

#### I2 reproduction

From the repository root, using the existing development environment with
Maturin, PyQIR, NumPy and pytest (and `patchelf` for Linux wheel RPATH setup):

```bash
cargo fmt -p qdk_simulators -p qdk -- --check
cargo test -p qdk_simulators --no-default-features --test tensor_network
cargo test -p qdk_simulators --no-default-features --lib execution
cargo clippy -p qdk_simulators -p qdk --all-targets -- -D warnings

PATH="$PWD/source/qdk_package/.venv/bin:$PATH" \
VIRTUAL_ENV="$PWD/source/qdk_package/.venv" \
source/qdk_package/.venv/bin/python -m maturin develop --release \
  --manifest-path source/qdk_package/Cargo.toml
source/qdk_package/.venv/bin/python -m pytest -q \
  samples/python_interop/ising2d_tensor_network_demo/test_tensor_network.py

# Keep I1's pinned reference environment separate from the development build.
.venv-ising-i1/bin/python -m pytest -q \
  samples/python_interop/ising2d_tensor_network_demo/test_reference.py
```

No fixtures are regenerated or modified by I2. Host qualification needs no
GPU and does not establish an optimizer path, treewidth, contraction cost,
GPU buffer lifecycle, full-size numerical agreement or terminal-shot behavior.

### I3a bounded native numerical experiment

The approved order is **numerical end-to-end evidence before common
plan/optimizer/executor interfaces**. The reusable private cuTensorNet path is
implemented through the actual I2 builder and its immutable shared-buffer bank.
The diagnostic and 2x2 each passed two native A100 contractions, with byte-identical
repeated readbacks and amplitude/norm/probability errors below `1e-12`. The 4x4
case was initially rejected before contraction because its selected path required
about 2.04 GiB of scratch, exceeding the original 64 MiB ceiling. The source-built
retry at `511141aba105cbd5738380d67246a1f1e8f909a9`, with a 3 GiB ceiling,
passed both 4x4 contractions with byte-identical readbacks. Maximum amplitude
error was `5.983150429055106e-10`; probability TV and squared-norm error also
passed the `1e-8` limit. All explicit cleanup succeeded. Kernel tracing remains
deferred; this is private numerical execution, not public `run_qir` integration.

| Case | Circuit | Output | Numerical limit |
| --- | --- | --- | --- |
| Asymmetric diagnostic | 3 qubits, idle q1; Rx(0.7,q0), Rx(0.7,q2), Rzz(0.41,q0,q2), Rx(-0.3,q0), Rzz(0.41,q2,q0), Rx(0.29,q2) | 8 amplitudes | `1e-12` |
| 2x2 Case A | Open row-major grid; J=1, h=0.5, time=1, order 4, two subdivisions, identity preparation; 48 Rx + 40 Rzz | 16 amplitudes | `1e-12` |
| Frozen 4x4 Case A | Unchanged I1 input; 192 Rx + 240 Rzz | 65,536 amplitudes (1 MiB) | `1e-8` |

Every limit applies independently to maximum complex-amplitude absolute error,
probability total variation and squared-norm error. Values must be finite.
There is no global-phase alignment or amplitude normalization; probabilities
are normalized only after both vectors' squared norms pass.
Output order is q0 least significant/first tensor axis fastest.

`fixtures/i3a_numerical/` retains exact f64 gate-angle bits, the diagnostic and
2x2 CPU arrays, and hashes/provenance. Its 4x4 adapter references the existing I1
array without replacing it. `i3a_reference.py` reuses the released QDK sparse
engine and the existing recipe/conversion/reference machinery, independently of
the new contractor; the diagnostic also has the phase-sensitive I2 analytic
cross-check. Reproduce checks in the existing pinned reference environment:

```sh
.venv-ising-i1/bin/python samples/python_interop/ising2d_tensor_network_demo/i3a_reference.py verify \
  samples/python_interop/ising2d_tensor_network_demo/fixtures/i3a_numerical
.venv-ising-i1/bin/python -m pytest -q \
  samples/python_interop/ising2d_tensor_network_demo/test_i3a_reference.py
```

Each case optimizes once, exports owned metadata, closes source owners, imports
into a fresh network without search, prepares and contracts twice with overwrite
semantics and no intervening output clear. The separate
`--contraction-qualification` validator selector stops before larger cases on
failure. Search uses one sample/thread, seed 17, no reconfiguration, deferred
rank simplification or automatic slicing, and a 64 MiB optimizer constraint.
Separately, device-scratch limits are 64 MiB for diagnostic/2x2 and **3 GiB for
4x4**; the host-scratch limit remains 1 MiB for all cases,
allocating minima (256-byte device floor), disabling caches, and using no
autotuning or memory pool. Unique inputs/output are separately accounted;
there is **no total GPU-memory cap**. See the
[native numerical contract](../../../source/cutensornet/README.md#private-general-network-numerical-execution)
for ownership, cleanup and evidence requirements.

This includes one bounded frozen 4x4 run in I3a, not a broader I3b campaign.
Native source-build provenance, retained numerical readbacks, resource reports
and cleanup remain acceptance gates. The observed 4x4 budget rejection is retained
as a host regression through the production owner; the larger native ceiling
does not relax that guard. Nsight tracing and performance analysis are later work.
At the time, common interfaces and public `run_qir`/sampling were paused. The
shared contraction interfaces have since been implemented; public general
contraction is planned for this demo's first version.

The separate [overnight plan-quality suite](../../../source/cutensornet/README.md#overnight-contraction-plan-experiments)
keeps these qualification cases unchanged and sweeps only frozen 4x4 Case A:
eight optimizer configurations plus a supplied chronological control through the
same native owner. Review this nine-trial first stage before selecting seed
follow-ups or intermediate search settings; the original 55-case grid is opt-in.
It uses 32 GiB optimizer/device-scratch limits, no host-scratch
policy ceiling, and records actual allocations and sampled process memory.
First/repeated execution timings, independent numerical checks and failure
evidence are retained per trial. This new suite still needs source review and
native execution; it is not covered by the accepted fixed-case results above.

---

## Appendix B — cuTensorNet general-contraction API reference

**Historical onboarding notes below are not new work instructions.** The generic
Network/contraction bindings, loader symbols and lifecycle owners already exist.
What remains is topology/data binding, actual optimization/contraction and native
qualification, not re-porting these declarations. I1 does not change this surface.

The general-contraction consumer needs cuTensorNet's path optimization, slicing
and arbitrary-topology execution, not the MPS-specific `State` API. The reference
below was checked against NVIDIA's version-pinned docs (not the
"latest" docs, which as of this writing resolve to a much newer, unrelated cuQuantum release —
see the version-pinning note below).

**Version pinning.** [`version.rs`](../../../source/cutensornet/src/version.rs)'s `POLICY` pins
this crate to cuTensorNet **2.13.0** (`cutensornet_runtime: 21_300`) and CUDA runtime **12.9**
(`cuda_runtime: 12_090`) — a hard equality check, not a minimum. `DEMO.md`'s Appendix A.3 records
that the Rust bindings were generated from the `cuquantum-linux-x86_64-26.06.0.17_cuda12-archive`
archive, i.e. cuQuantum SDK release **26.06.0** is the one that bundles cuTensorNet 2.13.0. NVIDIA's
docs are versioned by the SDK release, not the individual library version, so the correct reference
is `docs.nvidia.com/cuda/cuquantum/26.06.0/cutensornet/...` — _not_
`docs.nvidia.com/cuda/cuquantum/latest/...`, whose version-switcher metadata resolves to `26.06.0`
plus several releases (this changes over time; check `nv-versions.json` if revisiting this later).

**Current source of truth.** See the
[`cutensornet-symbols.txt` manifest](../../../source/cutensornet/scripts/cutensornet-symbols.txt),
[`bindings/v2_13.rs`](../../../source/cutensornet/src/bindings/v2_13.rs) and
[`ContractionResources`](../../../source/cutensornet/src/library/simulation/contraction.rs).
Declarations and lifecycle tests alone do not establish numerical execution.

**Reference call set for a first working (non-autotuned) general contraction.**
Confirmed against NVIDIA's reference example for this exact version
([`tensornet_example.cu`](https://github.com/NVIDIA/cuQuantum/blob/v26.06.0/samples/cutensornet/tensornet_example.cu),
linked from the [26.06.0 contraction-serial doc](https://docs.nvidia.com/cuda/cuquantum/26.06.0/cutensornet/examples/contraction-serial.html)).
Note this version uses a simplified **"Network"**-centric naming (`cutensornetCreateNetwork`,
`cutensornetNetworkAppendTensor`, ...) — generic web search results for cuTensorNet turn up an
older, more verbose `NetworkDescriptor`-style naming from earlier SDK releases; that older naming
does not apply to our pinned 2.13.0/26.06.0 combination and should not be used as a reference here.

| #   | Symbol                                                             | Purpose                                                             |
| --- | ------------------------------------------------------------------ | ------------------------------------------------------------------- |
| 1   | `cutensornetCreateNetwork` / `cutensornetDestroyNetwork`           | Create/destroy the network topology descriptor                      |
| 2   | `cutensornetNetworkAppendTensor`                                   | Register each input tensor's modes/extents                          |
| 3   | `cutensornetNetworkSetOutputTensor`                                | Declare the output tensor's modes                                   |
| 4   | `cutensornetNetworkSetAttribute`                                   | Set compute type (`CUTENSORNET_NETWORK_COMPUTE_TYPE`)               |
| 5   | `cutensornetCreateContractionOptimizerConfig` / `Destroy...`       | Optimizer config object                                             |
| 6   | `cutensornetContractionOptimizerConfigSetAttribute`                | e.g. hyper-sample count                                             |
| 7   | `cutensornetCreateContractionOptimizerInfo` / `Destroy...`         | Holds the resulting path                                            |
| 8   | `cutensornetContractionOptimize`                                   | The actual path-finder call                                         |
| 9   | `cutensornetContractionOptimizerInfoGetAttribute`                  | Query num slices, FLOP count, etc.                                  |
| 10  | `cutensornetWorkspaceComputeContractionSizes`                      | Size the workspace (reuses our existing `WorkspaceDescriptor` type) |
| 11  | `cutensornetNetworkPrepareContraction`                             | Prepare for execution                                               |
| 12  | `cutensornetNetworkSetInputTensorMemory` / `SetOutputTensorMemory` | Bind device buffers                                                 |
| 13  | `cutensornetNetworkContract`                                       | Execute the contraction                                             |
| 14  | `cutensornetCreateSliceGroupFromIDRange` / `DestroySliceGroup`     | Needed even for "contract everything" in one call (or pass `NULL`)  |

These symbols are already declared/loaded; the list describes the native execution
work still to qualify, not a new-symbol count or an effort estimate.

**Exact signatures**, extracted directly from the version-pinned
[26.06.0 function reference](https://docs.nvidia.com/cuda/cuquantum/26.06.0/cutensornet/api/functions.html)
(each confirmed by exact mangled-name-length match, not substring search — the naming has several
near-collisions in this API, e.g. `cutensornetCreateNetwork` vs. the _different, older_
`cutensornetCreateNetworkDescriptor`, and `cutensornetContractionOptimize` vs.
`cutensornetContractionOptimizerConfigGetAttribute`; a plain substring search would silently pick
the wrong one for either pair):

```c
cutensornetStatus_t cutensornetCreateNetwork(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t *networkDesc);

cutensornetStatus_t cutensornetDestroyNetwork(
    cutensornetNetworkDescriptor_t networkDesc);

cutensornetStatus_t cutensornetNetworkAppendTensor(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t networkDesc,
    int32_t numModes,
    const int64_t extents[],
    const int32_t modeLabels[],
    const cutensornetTensorQualifiers_t *const qualifiers,
    cudaDataType_t dataType,
    int64_t *tensorId);

cutensornetStatus_t cutensornetNetworkSetOutputTensor(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t networkDesc,
    int32_t numModes,
    const int32_t modeLabels[],
    cudaDataType_t dataType);

cutensornetStatus_t cutensornetNetworkSetAttribute(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t networkDesc,
    cutensornetNetworkAttributes_t attr,
    const void *const buffer,
    size_t sizeInBytes);

cutensornetStatus_t cutensornetCreateContractionOptimizerConfig(
    const cutensornetHandle_t handle,
    cutensornetContractionOptimizerConfig_t *optimizerConfig);

cutensornetStatus_t cutensornetDestroyContractionOptimizerConfig(
    cutensornetContractionOptimizerConfig_t optimizerConfig);

cutensornetStatus_t cutensornetContractionOptimizerConfigSetAttribute(
    const cutensornetHandle_t handle,
    cutensornetContractionOptimizerConfig_t optimizerConfig,
    cutensornetContractionOptimizerConfigAttributes_t attr,
    const void *buffer,
    size_t sizeInBytes);

cutensornetStatus_t cutensornetCreateContractionOptimizerInfo(
    const cutensornetHandle_t handle,
    const cutensornetNetworkDescriptor_t networkDesc,
    cutensornetContractionOptimizerInfo_t *optimizerInfo);

cutensornetStatus_t cutensornetDestroyContractionOptimizerInfo(
    cutensornetContractionOptimizerInfo_t optimizerInfo);

cutensornetStatus_t cutensornetContractionOptimize(
    const cutensornetHandle_t handle,
    const cutensornetNetworkDescriptor_t networkDesc,
    const cutensornetContractionOptimizerConfig_t optimizerConfig,
    uint64_t workspaceSizeConstraint,
    cutensornetContractionOptimizerInfo_t optimizerInfo);

cutensornetStatus_t cutensornetContractionOptimizerInfoGetAttribute(
    const cutensornetHandle_t handle,
    const cutensornetContractionOptimizerInfo_t optimizerInfo,
    cutensornetContractionOptimizerInfoAttributes_t attr,
    void *buffer,
    size_t sizeInBytes);

cutensornetStatus_t cutensornetWorkspaceComputeContractionSizes(
    const cutensornetHandle_t handle,
    const cutensornetNetworkDescriptor_t networkDesc,
    const cutensornetContractionOptimizerInfo_t optimizerInfo,
    cutensornetWorkspaceDescriptor_t workDesc);

cutensornetStatus_t cutensornetNetworkPrepareContraction(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t networkDesc,
    const cutensornetWorkspaceDescriptor_t workDesc);

cutensornetStatus_t cutensornetNetworkSetInputTensorMemory(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t networkDesc,
    int64_t tensorId,
    const void *const buffer,
    const int64_t strides[]);

cutensornetStatus_t cutensornetNetworkSetOutputTensorMemory(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t networkDesc,
    void *const buffer,
    const int64_t strides[]);

cutensornetStatus_t cutensornetNetworkContract(
    const cutensornetHandle_t handle,
    cutensornetNetworkDescriptor_t networkDesc,
    int32_t accumulateOutput,
    const cutensornetWorkspaceDescriptor_t workDesc,
    const cutensornetSliceGroup_t sliceGroup,
    cudaStream_t stream);

cutensornetStatus_t cutensornetCreateSliceGroupFromIDRange(
    const cutensornetHandle_t handle,
    int64_t sliceIdStart,
    int64_t sliceIdStop,
    int64_t sliceIdStep,
    cutensornetSliceGroup_t *sliceGroup);

cutensornetStatus_t cutensornetDestroySliceGroup(
    cutensornetSliceGroup_t sliceGroup);
```

The originally required opaque handle types (already declared) follow the same
opaque-pointer ownership pattern as `cutensornetHandle_t` and
`cutensornetWorkspaceDescriptor_t`: `cutensornetNetworkDescriptor_t`,
`cutensornetContractionOptimizerConfig_t`, `cutensornetContractionOptimizerInfo_t`, plus
`cutensornetSliceGroup_t`. The three `*Attributes_t` enums (`cutensornetNetworkAttributes_t`,
`cutensornetContractionOptimizerConfigAttributes_t`, `cutensornetContractionOptimizerInfoAttributes_t`)
are plain C enums (`int`-sized), set/read via the generic `SetAttribute`/`GetAttribute` +
`void*`/`sizeInBytes` pattern already used by `cutensornetStateConfigure` today — no new marshaling
idiom needed. `cutensornetWorkspaceComputeContractionSizes` and `cutensornetNetworkPrepareContraction`
both reuse `cutensornetWorkspaceDescriptor_t`, already onboarded.

**Historical optional follow-ons, not I1 work or priced estimates:**
`cutensornetCreateNetworkAutotunePreference` / `NetworkAutotunePreferenceSetAttribute` /
`NetworkAutotuneContraction` / `DestroyNetworkAutotunePreference` (lets cuTENSOR pick the best
kernel per pairwise contraction — a real perf win once we're running the same topology repeatedly,
e.g. across shots or across an `h`-sweep); and `cutensornetContractionOptimizerInfoSetAttribute` +
`cutensornetNetworkSetOptimizerInfo` (import a precomputed path instead of re-optimizing every
call — useful once path caching matters).

**Explicitly out of scope for this effort**, recorded here only so it isn't rediscovered from
scratch later:

- **Gradient computation**: `cutensornetNetworkSetGradientTensorMemory` / `SetAdjointTensorMemory`,
  `cutensornetComputeGradientsBackward` (experimental), and
  `cutensornetExpectationComputeWithGradientsBackward` (a gradient-aware extension of the
  `Expectation` API we already have). Relevant only if/when we want variational (VQE-style)
  optimization rather than one-shot sampling — not needed for this demo.
- **Mixed-state / density-matrix simulation**: `CUTENSORNET_STATE_PURITY_MIXED`,
  `cutensornetCreateMarginalDiagonal` and related marginal-distribution APIs — for noise/channel
  modeling. Our objective is exact contraction of a pure-state circuit; not needed.
- **Two-site `ProjectionMPS` family** (`cutensornetStateProjectionMPS*`, new in 2.13.0) — local
  sweep-style MPS updates for DMRG-like workflows; orthogonal to exact contraction.
- **Direct Tensor SVD/decomposition control**: `cutensornetTensorSVDConfigSetAttribute` (algorithm
  choice: GESVD/GESVDJ/GESVDP/GESVDR) and `cutensornetTensorSVDInfoGetAttribute` — finer control
  over truncation than our current `StateFinalizeMPS` path exposes. Only relevant to the MPS
  backend, not the exact-contraction consumer.
- **`cutensornetWorkspacePurgeCache`** — minor cache-management utility.
- **Distributed/multi-GPU (MPI) execution** — relevant only once a single A100 is the bottleneck;
  no evidence yet that it is.

**Historical onboarding approach (delivered, retained for context):** The general
State/MPS bindings were originally onboarded through a separate, heavier exploratory worktree
(`cutensornet-rust-ffi`) whose job was to remove architectural uncertainty — dynamic loading, ABI
versioning, opaque-handle ownership/`Drop` safety — before the "execution" side (this worktree)
could integrate. That uncertainty is now resolved and proven (this worktree's `lib.rs`/
`bindings/v2_13.rs` are, if anything, ahead of that worktree's current state). Onboarding the
Network/contraction family reuses the exact same dynamic-loading, same ABI file
(`bindings/v2_13.rs`, unchanged header version), and the same `Create`/`Destroy`-handle lifecycle
pattern already established for `State`/`Sampler` — it is incremental work on a settled
foundation, the same shape as the `Rzz` gate addition, not a new architectural spike. GPU
validation is human-in-the-loop either way (neither worktree has direct A100 access); routing
through a second worktree's bundle/evidence-tarball handoff would add coordination overhead
without adding safety.

**Discipline to follow**, matched to what every prior addition to this crate
(`f1257acf1`..`791a64b5b`) actually did, not assumed:

- **Narrow, single-purpose commits** — bindings added in a separate commit from the Rust logic
  that consumes them (e.g. `991904397` "Add cuTensorNet sampler bindings" was bindings-only;
  `7a539a55c` "Port cuTensorNet sampler execution" was the logic, as its own commit).
- **Explicit validation in every commit message** — concrete test counts (e.g. "88 qdk_cutensornet
  tests, 127 qdk_simulators tests") plus workspace `clippy`/`rustfmt` clean, not just "tests pass."
- **Explicit non-goals per commit** (e.g. `c3d151b54`: "Non-goals: ... A100 execution") — bounds
  scope so partial progress isn't mistaken for done.
- **Frozen-surface count bumped in the same commit that adds symbols**, with the before/after
  count named in the message (e.g. "grows from 25 to 30").
- **Cross-platform compile, Linux/x86_64-gated native loading** —
  `#[cfg(all(target_os = "linux", target_arch = "x86_64"))]` around the actual dynamic-loading/FFI
  code so the crate still builds and unit-tests on every host; only the real native calls are
  host-restricted.
- **Hardware acceptance exists at two levels, and both matter.** Checked directly by running the
  suite: `source/cutensornet/tests/availability.rs:4` is a Rust integration test carrying
  `#[ignore = "requires the audited CUDA Runtime 12.9 and cuTensorNet 2.13 libraries"]`, which
  asserts the discovered runtime reports cuTensorNet `21_300` and CUDA `12_090`. It is skipped by
  default (`cargo test -p qdk_cutensornet` reports `1 ignored`) and only runs when explicitly
  requested via `--ignored` on a host with the audited libraries. Above that,
  `qdk_package/tests/test_cpu_simulator.py:160-161` gates real NVIDIA _execution_ behind the
  Python-level `QDK_NVIDIA_TESTS` environment variable, against the public `run_qir` entry point.
  So the crate's convention is: symbol resolution/version checks as an `#[ignore]`d Rust test,
  end-to-end GPU execution as a `QDK_NVIDIA_TESTS`-gated Python test. The new contraction-path
  hardware check should follow that split rather than inventing a third convention.
- **No prior-worktree port available this time.** Earlier additions (`991904397`, `7a539a55c`)
  explicitly ported already-written declarations from a specific commit in `cutensornet-rust-ffi`
  ("Port ... from eed6e1bbe"). Confirmed that worktree has no Network/contraction work at all
  (above), so these bindings are written fresh from the NVIDIA reference signatures, not ported.

The remaining I3a gate is the approved diagnostic/2x2/frozen-4x4 A100 numerical
sequence, with workspace/lifetime/readback/cleanup and kernel-activity evidence.
Numerical slicing and broader I3b work remain separate. Neither symbol resolution
nor a host fake replaces those numerical checks.
