# 2D Ising Quench Demo: MPS Accuracy Against Cost

This demo simulates the Trotterized 2D Ising quench from the `qdk-chemistry`
[resource-estimation notebook](https://github.com/microsoft/qdk-chemistry/blob/1eb14a9d73685d4e57ee7ee7ca6f4d2ef845a6dd/examples/estimation_ising_2d.ipynb)
with cuTensorNet's matrix-product-state (MPS) backend, and measures its error
against an exact reference. The question it answers:

> How close to correct does MPS get, how much quicker is it, and how much bond
> dimension does a 2D lattice need, away from and near the critical point?

It is a preview for feedback, not a shipped feature. The API is the preview
`qdk.simulation.tensornetwork_qir`, whose names may change. A companion QEC
demo uses the same API for exact general contraction.

> **Preview status.** Items marked ⏳ are being prepared for the demo and
> their names may change. No accuracy-against-cost result has been measured
> yet; table and plot shapes below show what will be reported.

| | |
| --- | --- |
| Circuit source | `qdk-chemistry==2.2.1` builders, via [`build_measured_circuit.py`](build_measured_circuit.py) |
| Entry point | `tensornetwork_qir(qir, queries, method="mps" \| "contraction")` |
| Approximate method | MPS with bond dimension cap χ (default 128) |
| Reference | Exact, from cuTensorNet's state API without MPS truncation, where it fits |
| Reference host | NVIDIA A100 80GB PCIe, Linux x86_64 |
| Validation so far | 4x4 exact contraction agrees with an independent CPU state to `6e-10` ([Appendix A](#appendix-a--validation-history)) |

---

## TL;DR

```text
build_measured_circuit.py --field h ─► QIR ─┬─► tensornetwork_qir(method="mps", χ)  ─► m_z(χ), C_ZZ(χ), Cost, time
                                            └─► tensornetwork_qir(method="contraction") ─► m_z, C_ZZ exact, time
                                                          error(χ) = |MPS(χ) − exact|
```

**What you get.** Two tables and plots: the error of the lattice-averaged
magnetization and nearest-neighbour correlation against χ, next to time and
memory; and the χ each lattice size needs at `h=0.5` and at the critical
point `h=3.03`. Both are computed as expectation values, not from shots, so
shot noise does not hide the MPS error.

**What we want from you.** Which of the two ways of plugging this into
`qdk-chemistry` fits your algorithms ([§5.4](#54-how-its-done-two-qdk-chemistry-seams)),
and which models or observables matter to you ([§8](#8-feedback-wanted)).

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
to `t=1` with fourth-order Trotter–Suzuki and two subdivisions. Only the
lattice size and the field change.

| Scenario | J | h | What it is |
| --- | --- | --- | --- |
| Baseline | 1 | 0.5 | The notebook's default |
| Near-critical | 1 | 3.03 | Close to the square-lattice critical point $h/J \approx 3.044$ ([Blöte and Deng, Phys. Rev. E 66, 066110 (2002)](https://doi.org/10.1103/PhysRevE.66.066110)) |

**Observables**, on the state $|\psi\rangle$ before the terminal measurements,
with $n=N^2$ sites and $|E|=2N(N-1)$ bonds:

$$
m_z = \frac{1}{n}\sum_i \langle Z_i\rangle,
\qquad
C_{ZZ} = \frac{1}{|E|}\sum_{\langle i,j\rangle} \langle Z_i Z_j\rangle .
$$

**Error and "χ needed".** For MPS with bond-dimension cap χ,

$$
\varepsilon_m(\chi) = \bigl|m_z(\chi) - m_z^{\text{ref}}\bigr|,
\qquad
\varepsilon_C(\chi) = \bigl|C_{ZZ}(\chi) - C_{ZZ}^{\text{ref}}\bigr|,
\qquad
\chi_{\text{needed}} = \min\{\chi : \varepsilon_m,\ \varepsilon_C \le 10^{-3}\}.
$$

The threshold only sets the summary table; the full error-against-χ curve is
always shown.

## 2. How to use it

### 2.1 Prerequisites

Linux x86_64, an NVIDIA GPU, cuQuantum `libcutensornet.so.2`, and the preview
`qdk` build. `qdk-chemistry==2.2.1` is needed only to generate new circuits;
the circuits the demo uses ship with the sample. ⏳

### 2.2 Step 1: generate a circuit

```bash
python build_measured_circuit.py --nx 4 --ny 4 --field 3.03 --output ising-4x4-h3.03.ll   # ⏳ --field
```

The output is an ordinary Base-profile QIR program. `tensornetwork_qir`
knows nothing about lattices or fields; any program works as long as the
method supports its gates.

### 2.3 Step 2: query it

```python
from qdk.simulation import Cost, Expectation, MpsOptions, tensornetwork_qir

N = 4
n = N * N
bonds = [(y*N + x, y*N + x + 1) for y in range(N) for x in range(N - 1)] + \
        [(y*N + x, (y+1)*N + x) for y in range(N - 1) for x in range(N)]
m_z  = Expectation([("Z",  [q],    1 / n)          for q in range(n)])
c_zz = Expectation([("ZZ", [i, j], 1 / len(bonds)) for i, j in bonds])

qir = open("ising-4x4-h3.03.ll").read()

# ⏳ Exact reference: cuTensorNet's state API, no truncation.
exact = tensornetwork_qir(qir, [m_z, c_zz], method="contraction")

# ⏳ MPS with bond-dimension cap χ = 16.
approx = tensornetwork_qir(qir, [m_z, c_zz, Cost()], method="mps",
                           options=MpsOptions(max_bond_dimension=16))
# approx[2] == {"max_bond_dimension": ..., "state_bytes": ..., "workspace_bytes": ...}
```

Each `Expectation` result is complex, real up to rounding for these
observables. `Cost()` reports the largest bond the MPS actually reached, its
size and the workspace it needed.

### 2.4 Step 3: the χ sweep ⏳

`run.py` runs Step 2 for a list of χ values and sizes, writes one results
file, and renders tables and plots from it. Measuring needs the GPU;
rendering does not. Proposed usage:

```bash
python run.py measure --size 4 --field 3.03 --chi 2 4 8 16 32 64 128 --output results.json
python run.py render results.json
```

## 3. What the demo shows

### 3.1 Close to correct, but much quicker

Error against χ, with time and memory, at 4×4 and at larger sizes while the
exact reference fits. At 4×4 the reference is also checked against an
independent CPU state: for `h=0.5`, **$m_z = 0.9508364$, $C_{ZZ} = 0.9326830$**.

```text
ising2d | size=4 J=1 h=3.03 | reference: exact (… s)
χ     ε_m      ε_C      max bond   time     state     workspace
2     …        …        …          … s      … KiB     … MiB
…
128   …        …        …          … s      … KiB     … MiB
```

### 3.2 2D is hard for MPS, hardest near h = 3.03

χ needed against N, at `h=0.5` and `h=3.03`. Where the exact reference no
longer fits, the reference is the largest-χ MPS run, labelled as such and
never called exact.

```text
N    qubits   reference         χ needed (h=0.5)   χ needed (h=3.03)
4    16       exact             …                  …
…
```

### 3.3 Your own circuit or observable

Any Base-profile QIR with the supported gates works, and any Pauli sum is a
valid `Expectation`: for example single-site $\langle Z_i\rangle$, longer-range
correlations or the energy $\langle H\rangle$, with its `X` terms.

## 4. Reading the results

- **Errors are truncation only.** No shots are involved, so the difference
  from the reference is MPS truncation error.
- **Agreement is with the circuit, not with the physics.** Both sides
  simulate the same Trotterized circuit; neither measures Trotter error,
  which can be larger at `h=3.03`.
- **Beyond the exact reference, errors are convergence estimates.** The
  difference from the largest-χ run shows whether results have settled, not
  how far they are from exact.
- **The chain order matters.** MPS orders qubits by index, so vertical bonds
  span `N` sites ([§5.2](#52-why-mps-finds-2d-hard)). A better site order could
  lower the χ needed; it is not part of this demo.
- **Near-critical is a hypothesis.** The critical point is a ground-state
  property. For this short quench it is expected, not guaranteed, to be the
  hardest field.
- **Sign conventions do not matter.** Flipping the sign of `J`, of `h`, or
  both gives the same $m_z$ and $C_{ZZ}$ from $|0\cdots0\rangle$.

## 5. How it works

### 5.1 One API, two methods

```text
QIR ─► prepared program ─► circuit (gates + terminal measurements)
                                   │
          ┌────────────────────────┴────────────────────────┐
   method="mps", χ                                    method="contraction"
   cuTensorNet MPS state (truncates to χ)             cuTensorNet state, no truncation
          │                                                  │
          └──────► Expectation(Σ cₖ Pₖ), Cost ◄───────────────┘
```

Both methods share QDK's program execution; only the state representation
differs. The exact reference contracts the circuit's network for each
expectation value with an optimized contraction path. Its cost is set by the
circuit's structure rather than by entanglement, which is why it stops
fitting at some lattice size.

### 5.2 Why MPS finds 2D hard

```text
lattice (N=3)              MPS chain (site = qubit index)
 0 ─ 1 ─ 2                 0 ─ 1 ─ 2   3 ─ 4 ─ 5   6 ─ 7 ─ 8
 │   │   │                 └─────N─────┘
 3 ─ 4 ─ 5                 every vertical bond spans N chain sites
 │   │   │
 6 ─ 7 ─ 8
```

Horizontal bonds stay short, but every vertical bond becomes a long-range gate
across `N` chain sites. The entanglement across a cut of the chain can grow
with the lattice width, so the χ needed grows with `N`. How fast depends on
the state, which is what [§3.2](#32-2d-is-hard-for-mps-hardest-near-h--303)
measures.

### 5.3 What is and is not claimed

- **General contraction is exact:** no truncation, with an optimized
  contraction path. The QEC demo uses it for outcome probabilities.
- **This demo studies MPS against an exact reference.** It does not evaluate
  `Expectation` through the QEC demo's general-contraction builder; that
  needs a double-layer network ($\langle\psi|P|\psi\rangle$), which is not
  built.
- **PEPS is not planned.** It suits 2D lattices, but nothing in this preview
  implements it.

### 5.4 How it's done: two qdk-chemistry seams

`qdk-chemistry`'s QDK executor already calls `circuit.get_qir()` and
`run_qir(...)`, so QIR is its internal plumbing. A tensor-network backend can
sit behind either of two existing seams, whose contracts differ:

```text
CircuitExecutor:      (Circuit, shots, QuantumErrorProfile?) ─► bitstring counts
ExpectationEstimator: (Circuit, QubitOperator, shots, noise?) ─► ⟨H⟩, variance
this demo:            (QIR, [Expectation(P), …])             ─► values, no shot noise
```

The two sketches below are illustrative only: nothing in `qdk-chemistry` is
built or changed, and registration and settings are omitted.

**Seam 1: a drop-in `CircuitExecutor` (shots).** Wraps `run_qir(type="mps")`
like the full-state executor. The MPS backend is noiseless and runs
Base-profile programs, so it must reject a noise profile.

```python
from collections import Counter
from qdk import Result
from qdk.simulation import run_qir
from qdk_chemistry.algorithms.circuit_executor.base import CircuitExecutor
from qdk_chemistry.data import CircuitExecutorData

class QdkMpsSimulator(CircuitExecutor):
    def _run_impl(self, circuit, shots, noise=None):
        if noise is not None:
            raise ValueError("the MPS executor is noiseless; remove the noise profile")
        runs = run_qir(circuit.get_qir(), shots=shots, type="mps")
        counts = Counter("".join("1" if r == Result.One else "0" for r in reversed(run))
                         for run in runs)                 # little-endian, like the QDK executor
        return CircuitExecutorData(bitstring_counts=dict(counts), total_shots=shots,
                                   executor=self.name(), executor_metadata=runs)

    def name(self):
        return "qdk_mps_simulator"
```

**Seam 2: a tensor-network `ExpectationEstimator` (no shots).** Computes
$\langle H\rangle$ directly from `Expectation` queries, which is this demo's
value: no shot noise and no measurement-basis circuits.

```python
from qdk.simulation import Expectation, MpsOptions, tensornetwork_qir
from qdk_chemistry.algorithms.expectation_estimator.expectation_estimator import ExpectationEstimator
from qdk_chemistry.data import EnergyExpectationResult, MeasurementData

def _term(label):                                         # "IXZ" -> ("XZ", [1, 0])
    ops = [(p, len(label) - 1 - i) for i, p in enumerate(label) if p != "I"]
    return "".join(p for p, _ in ops), [q for _, q in ops]  # rightmost label is qubit 0

class TensorNetworkEstimator(ExpectationEstimator):
    def _run_impl(self, circuit, qubit_hamiltonian, total_shots, noise_model=None):
        if noise_model is not None:
            raise ValueError("tensor-network expectation values are noiseless")
        labels, coeffs = qubit_hamiltonian.pauli_strings, qubit_hamiltonian.coefficients
        queries = [Expectation([(*_term(label), 1.0)]) for label in labels]  # identity terms omitted for brevity
        values = [v.real for v in tensornetwork_qir(circuit.get_qir(), queries, method="mps",
                                                     options=MpsOptions(max_bond_dimension=64))]
        energy = float(sum(c * v for c, v in zip(coeffs, values)))
        return (EnergyExpectationResult(energy_expectation_value=energy, energy_variance=0.0,
                                        expvals_each_term=values, variances_each_term=[0.0] * len(values)),
                MeasurementData(hamiltonians=[qubit_hamiltonian], bitstring_counts=[], shots_list=[]))
```

The same estimator with `method="contraction"` gives the exact value where it
fits. **Which seam fits your algorithms better?** The answer sets the priority
for production integration.

### 5.5 Why not the other QDK simulators

Dense CPU/GPU statevectors stop at about 25 qubits
([measured](../mps_trotter_quench_demo/DEMO.md#22-the-dense-wall-measured)); the
wgpu `type="gpu"` path has a fixed 27-qubit single-precision limit
([`shader_types.rs`](../../../source/simulators/src/gpu_full_state_simulator/shader_types.rs)).
The sparse simulator densifies quickly under the transverse field, and the
Clifford simulator cannot run generic rotations. They remain useful references
at small sizes, as in [Appendix A](#appendix-a--validation-history).

## 6. Status

| Piece | Status |
| --- | --- |
| Circuit generator from `qdk-chemistry` (`--nx/--ny`, `h=0.5`) | Available |
| 4x4 exact contraction, validated against an independent CPU state | Done ([Appendix A](#appendix-a--validation-history)) |
| `tensornetwork_qir` API; contraction `Probability` and `Cost` (QEC demo) | Available (preview) |
| MPS and exact `Expectation` over Pauli sums; MPS `Cost` | ⏳ Demo |
| `--field`, `run.py` χ sweep, generated circuits, 4x4 `h=3.03` CPU reference | ⏳ Demo |
| Measured error-against-χ and χ-needed results | ⏳ Demo |

## 7. What could come next

None of these is planned; your feedback decides which, if any, come first.

| Possible addition | What it would enable |
| --- | --- |
| **A `qdk-chemistry` integration** through the seam you prefer ([§5.4](#54-how-its-done-two-qdk-chemistry-seams)) | Tensor-network backends behind your algorithms, without explicit QIR calls |
| **An optimized MPS site order** | Lower χ for 2D lattices ([§5.2](#52-why-mps-finds-2d-hard)) |
| **Accuracy controls in `run_qir(type="mps")`** | Bond dimension and cutoffs for shots, as for `tensornetwork_qir` |
| **More gates** | Circuits beyond `Rx`/`Rzz`, for other chemistry models |
| **Noise** | Noisy dynamics; both methods are noiseless today |

The walk-through of other chemistry examples that suit MPS is planned for
after the demo.

## 8. Feedback wanted

- **Which seam fits your algorithms better**: a drop-in `CircuitExecutor`, or
  a tensor-network `ExpectationEstimator` ([§5.4](#54-how-its-done-two-qdk-chemistry-seams))?
- Are `m_z` and `C_ZZ` the right observables, or do you need others, for
  example the energy or longer-range correlations?
- Is the `10⁻³` threshold for "χ needed" meaningful for your use, or is
  another accuracy target more useful?
- Which models, sizes or times would you run next?

**Feedback that shaped this demo.** On the 1D demo, a `qdk-chemistry` team
member pointed out that 2D is the canonical hard case for MPS, that `J=1,
h=3.03` is the square lattice's critical point, and that their builders
already provide the circuits. That is why the demo uses those circuits and
compares the two fields.

After we proposed this accuracy-against-cost study, they found it worth
seeing, while expecting MPS to fail at the critical point. They asked whether
we had extended this to general tensor networks and how well that is tested,
and wanted to see how it is done: they would rather add a proper
`CircuitExecutor` to `qdk-chemistry` than call QIR and cuQuantum explicitly.
Arbitrary contraction is a good-quality prototype, well tested in a few
scenarios ([Appendix A](#appendix-a--validation-history)); the QEC demo is
where it is used. [§5.4](#54-how-its-done-two-qdk-chemistry-seams) shows how
it is done and asks which seam fits.

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
