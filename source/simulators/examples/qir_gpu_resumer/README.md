# A GPU resumer for `QirMachine` (prototype)

> **Prototype, not for merging.** This example's code supports the review of
> `QirMachine`; its recommendation below is a proposal for the QDK's GPU
> simulator, not a change made here.

## Executive summary

[`QirMachine`](../../src/qir/machine.rs) and
[`QirResumer`](../../src/qir/resumer.rs) can be the core of every simulator
the QDK runs QIR programs on: the machine runs a program's classical part
(branches, arithmetic on measured values, records) once for every target, and
each simulator keeps only its quantum part, as a resumer that runs the
machine's batches of quantum calls. This prototype rebuilds the GPU simulator
that way.

- **Recommendation: rebuild the GPU simulator on `QirMachine`, in two
  steps.** First, measure both simulators again on a native backend (Vulkan,
  or DX12 on a GPU other than the Adreno X1), with the `compare` mode pinned
  to that backend ([How to run it](#how-to-run-it)); the batching policy and
  the kernels that close the repetition codes' gap are prototyped here. If
  the ratios hold, do the rest of the production work, about 5–7.5 weeks,
  plus other backends, not estimated
  ([What is missing for production](#what-is-missing-for-production)).
- **One implementation of the classical semantics instead of two:** the
  rebuild removes the 1,338 interpreter lines of the current simulator's WGSL
  shader ([The duplication removed](#the-duplication-removed)).
- **Faster, with the gap closed on this GPU:** 6.5–7.8× faster than the
  current GPU simulator on the bench program, noiseless; on repetition codes
  at d = 9 and 11, 1.3–1.9× faster with conditioned batches and the kernels
  that use them (opt-in), against 0.36–0.57× with the machine's default
  batching
  ([Closing the gap](#closing-the-gap-conditioned-batches-and-kernels-that-use-them)).
- **The CPU already runs on the machine:** today's test programs, with the
  records of today's interpreter, at its speed.
- **Measured on one GPU:** every number here was measured on a Qualcomm
  Adreno X1-85 under WSL, through wgpu's GL backend, which is not a native
  backend of the current simulator; on another GPU or backend the ratios may
  differ either way
  ([Platform](#platform-and-what-it-costs-the-current-simulator)).

## Each simulator on `QirMachine`

The evidence, for each simulator that `run_qir` offers and for the new MPS
one:

| Simulator                             | Its resumer                                                   | Same results as today                                                                                                                                                                                                                   | Speed against today's path                                                                                                                                           |
| ------------------------------------- | ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CPU full state (`cpu`)                | [`QirSimulatorResumer`](../qir_resumers/simulator_resumer.rs) | every shot of the 244 cases of `run_shot`'s Python tests records what `run_shot` records ([the compatibility test](../../tests/run_shot_compatibility/main.rs))                                                                         | the same: 0.98–1.01× `run_shot`'s time per shot on those cases ([the benchmark](../../benches/qir_machine.rs))                                                       |
| Stabilizer (`stabilizer`, `clifford`) | the same resumer                                              | the same, on the 238 of those cases that the tests run on it                                                                                                                                                                            | not measured; the classical part alone (on a simulator that does nothing) costs the resumer 0.83–0.87× what it costs `run_shot`                                      |
| GPU (`gpu`)                           | `QirGpuResumer`, this example                                 | agrees with the current simulator and with the CPU resumer ([Correctness evidence](#correctness-evidence))                                                                                                                              | faster on the bench program; on the repetition code at d = 9, slower by default and faster with the opt-in steps ([Performance on the GPU](#performance-on-the-gpu)) |
| MPS (new, not in the QDK yet)         | `MpsResumer`, on the branch `domingom/mps-method-m1b`         | on a development branch: passes the resumer conformance kit; meets those tests' expectations on their noiseless programs but one, which checks for loss and is refused; H₂'s phase estimation, its readout within 10⁻⁹ of the exact one | no current path to compare: written on the machine from the start                                                                                                    |

`QirGpuResumer` ([`gpu_resumer.rs`](gpu_resumer.rs),
[`kernels.wgsl`](kernels.wgsl)) is the resumer that keeps every shot's state
vector on a GPU through wgpu: it runs the machine's batches there and returns
the measurement outcomes. It stands in for the current GPU simulator
([`gpu_full_state_simulator`](../../src/gpu_full_state_simulator/)).

## The duplication removed

The current simulator interprets the Adaptive Profile bytecode inside its WGSL
shader: a second implementation of what `QirMachine` does, which must track
it. Measured on `main`'s files:

| Current simulator's file                                                                          | Lines | Serving the bytecode interpreter                                                                                        | Adaptive op preparation          |
| ------------------------------------------------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------------------- | -------------------------------- |
| [`gpu_statevector_shaders.wgsl`](../../src/gpu_full_state_simulator/gpu_statevector_shaders.wgsl) | 3,717 | 1,338: opcodes and constants 152, interpreter structs 102, utility functions 153, the `interpret_classical` kernel 931  | 204 (`prepare_op_adaptive_impl`) |
| [`gpu_context.rs`](../../src/gpu_full_state_simulator/gpu_context.rs)                             | 1,112 | 514: the adaptive `impl GpuContext` (program upload, the shot loop), `add_noise_to_adaptive_ops`, `program_region_size` |                                  |
| [`gpu_resources.rs`](../../src/gpu_full_state_simulator/gpu_resources.rs)                         | 975   | 202: the adaptive bind group layout and buffer uploads                                                                  |                                  |
| [`shader_types.rs`](../../src/gpu_full_state_simulator/shader_types.rs)                           | 1,181 | 101: `CallStackFrame`, `InterpreterState`, `build_op_pool`                                                              |                                  |

The 1,338 interpreter lines are the duplicate: a rebuild on `QirMachine`
removes them. The other bytecode lines (op preparation, and about 820 lines of
Rust that upload the program and drive the shot loop) are replaced by the
resumer's host code, which does the same jobs for `QirMachine`'s batches. The
shader's other 2,175 lines (gates, noise, sampling, and the paths other than
the adaptive one) hold the quantum kernels, which the resumer's own 839 lines
of WGSL stand in for here; a rebuild keeps one set.

This is not a reduction in lines. The resumer is 3,526 lines (Rust 2,687,
WGSL 839); about 510 of the Rust lines are measurement scaffolding (the
`Strategy` switches that turn each optimization off, `Stats`, the `profile`
and `state_after` modes), and the rest includes work the current simulator's
adaptive path does not do (it runs one operation per round): fusing gates
into passes and blocks, and grouping shots with equal batches. The resumer
also lacks features the current simulator has
([What is missing for production](#what-is-missing-for-production)). The
gain is one implementation of the classical semantics instead of two.

## Performance on the GPU

The resumer is faster on the bench program, and, by default, slower on larger
repetition codes. The bench program is n qubits, 8 rounds of Rx on every qubit, a CZ
chain, `MResetZ` on qubit 0 and a conditional X, then `ResetAll`. The
repetition codes have d data qubits and d−1 ancillas, and 8 rounds of
syndrome extraction with a decoder that applies X corrections. The slowdown
comes from how `QirMachine` builds batches by default; an opt-in batching
policy and kernels that use it close it on this GPU
([Closing the gap](#closing-the-gap-conditioned-batches-and-kernels-that-use-them)).

Both simulators run the same compiled program, with the same shots, seed and
noise, timed alike in one process by the `compare` mode
([How to run it](#how-to-run-it)):

- **Scope:** each time is one call that runs every shot and returns its
  results (the current simulator's own result arrays, the resumer's output
  records), after the program is loaded; it excludes compilation (Q# to QIR to
  bytecode) and each simulator's shader compilation (the current simulator
  compiles in its first run, which takes 11.5–14.4 s; the resumer compiles in
  about 3 s when it is created).
- **Runs:** a warm-up run each, then 5 runs each, interleaved, the order
  rotating. Each time is the best of the 5. On the bench rows the median is
  within 3% of the best; the repetition-code rows vary more, so their median
  ratios are given too.
- **Reset:** the final `ResetAll` is never observed, so the default resumer
  skips it; "kept" runs it, as the current simulator does, and is the
  conservative ratio.
- **Noise:** depolarizing p on every gate and move. Not on measurements: with
  noise on `MResetZ`, every shot of the current simulator fails (error
  code 2), so it cannot be compared there (findings, below).
- **Agreement:** in each configuration, both simulators' outcome
  distributions agree (z ≤ 5).

All rows were measured in one session:

| Qubits × shots       | Current (s) | Resumer, reset kept (s) | Ratio, kept / dropped | Current, p = 10⁻³ (s) | Resumer, kept (s) | Ratio, kept / dropped |
| -------------------- | ----------- | ----------------------- | --------------------- | --------------------- | ----------------- | --------------------- |
| 12 × 1024            | 0.518       | 0.0725                  | 7.1× / 8.8×           | 0.524                 | 0.102             | 5.1× / 6.0×           |
| 16 × 256             | 1.92        | 0.295                   | 6.5× / 8.6×           | 1.94                  | 0.436             | 4.4× / 5.4×           |
| 20 × 16              | 2.85        | 0.376                   | 7.6× / 9.9×           | 2.86                  | 0.544             | 5.2× / 6.3×           |
| 22 × 4               | 3.09        | 0.395                   | 7.8× / 10.6×          | 3.10                  | 0.570             | 5.4× / 6.6×           |
| rep. code d=7 × 1024 | 0.242       | 0.248                   | 0.98× / 0.98×         | 0.263                 | 0.295             | 0.89× / 0.89×         |
| rep. code d=9 × 256  | 0.549       | 1.22                    | 0.45× / 0.45×         | 0.532                 | 1.49              | 0.36× / 0.36×         |

On the bench program the resumer is 6.5–7.8× faster noiseless and 4.4–5.4×
under noise, keeping the reset (8.6–10.6× and 5.4–6.6× dropping it). At the
median, the repetition codes' kept ratios are 1.01× and 0.95× at d = 7, and
0.51× and 0.40× at d = 9 (the current simulator's d = 9 times vary most,
0.53–0.67 s). Against the CPU resumer
([`simulator_resumer.rs`](../qir_resumers/simulator_resumer.rs), one thread,
which runs the reset) the resumer is 73× faster at 12 × 1024 and 164× at
22 × 4 with the reset kept, and 111× and 229× with it dropped.

### Where it is slower: repetition codes

With the default batching it breaks even at d = 7 and is 2.2× (noiseless) to
2.8× (noisy) slower at d = 9. Smaller codes favour it: at p = 10⁻², 1024 shots, it is 6.1× faster
at d = 3 and 4.5× at d = 5 (reset kept). Each syndrome bit a branch reads ends a round of the resumer, so d = 9
takes 65 rounds per chunk of 128 shots, about 3.7 passes each, and the run
costs about 2.6 ms per pass over a 17-qubit chunk here; under noise, shots
with different syndromes also form more groups, each with its own passes.
The current simulator runs one host round per quantum operation (below), 274
per batch of 128 shots at d = 9, at about 1 ms each. Why the resumer's
passes cost more per round at this size is answered in
[Closing the gap](#closing-the-gap-conditioned-batches-and-kernels-that-use-them):
the current simulator's passes move no memory on a repetition code's basis
states.

### Few shots

With one shot the ratio is larger, and it measures fixed latency, not
simulation: 36× at 20 × 1 (25× under noise) and 26× at 12 × 1
(17×), reset kept, best of 5 (35× and 25×, 21× and 16× at the median). The
current simulator runs one host round per quantum operation: it submits its
classical, prepare and execute kernels, then reads back how many shots have
ended. The 20-qubit bench program takes 342 such rounds (about 3.3 ms each on
this platform, 1.15 s in all); the resumer takes one readback per round of
measurements, 8 here.

### Platform, and what it costs the current simulator

Every GPU number in this README was measured on one GPU, a Qualcomm Adreno
X1-85 in WSL (Windows Subsystem for Linux), through wgpu's GL backend on Mesa's d3d12 Gallium driver, with
f32 amplitudes, on cores shared with other work. That is not the current
simulator's native backend (DX12, Vulkan or Metal), and it pays there:

- its DX12-only shader sections are stripped (`QDK_GPU_STRIP_DX12`);
- a buffer is capped at 128 MiB instead of up to 1 GiB, so at d = 9
  (17 qubits) it runs 256 shots as two batches of 128 (the resumer, which
  holds at most 128 MiB per chunk on any backend, runs two chunks too);
- its host round per quantum operation pays GL's synchronization latency,
  which a native backend may cut.

None of these was measured on a native backend; on a discrete GPU, or another
backend, the ratios may differ either way.

On Windows, DX12 is refused for this GPU: the current simulator does not
support the Adreno X1 on DX12, whose driver corrupts data past 256 MiB
(#4243), and the resumer, which uses the same adapter selection, refuses it
too. A native run on this GPU needs a Vulkan adapter (`adapters` shows
whether wgpu sees one, [How to run it](#how-to-run-it)).

## Closing the gap: conditioned batches, and kernels that use them

The repetition codes' gap closes in two halves: send the GPU the decoder's
corrections inside the batch, and update the kernels to make what those
batches hold cheap. Both are opt-in and prototyped here; every number below
was measured on this machine's GL backend
([Platform](#platform-and-what-it-costs-the-current-simulator)).

**First half: conditioned batches.** By default `QirMachine` ends a batch at
the first branch on a measurement result. An opt-in policy,
[`QirMachine::resume_conditioned`](../../src/qir/machine.rs), also puts a
simple result-conditioned call in the batch (for instance "apply X to qubit 3
if results 5 and 6 read 1"), and the resumer evaluates the condition per shot
on the GPU without ending the round. At d = 9 a chunk then takes one round
instead of 65. Alone it gains 1.0–1.15×: the cost was not the rounds but the
passes. 450 of a chunk's 566 ms of GPU time were its 73 measurements, three
passes over the whole state each. The default policy stays today's; a resumer
that cannot evaluate conditions never sees one.

**Second half: kernels for those batches.** Three changes cut a d = 9 chunk
from 315 passes to 62, and make most of them one read and one write of the
state:

| Change                   | What it does                                                                                                                                                                             | Gain, measured, over the step before (the first over conditioned batches alone) |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Joint measurement        | measures up to 8 qubits with nothing between them as one: one reduction over their joint outcomes, one sample, one collapse                                                              | 1.5–1.95×                                                                       |
| Pauli frame              | applies a run of conditioned Paulis (the decoder's corrections) and their noise as one pass                                                                                              | 1.06–1.15×                                                                      |
| Composed monomial blocks | composes a block of permutations with phases (X, CX, CZ, SWAP, S, T, Rz, and Pauli faults) on the host into one index map with phases, so a block's pass does not grow with its op count | 1.46–2.3×                                                                       |

With both halves, and the final reset run, the resumer is 1.3–1.9× the
current simulator at d = 9 and 11, noiseless and at p = 10⁻² (1.4–2.3× at the
median), against 0.36–0.57× with today's policy; on a variant of d = 9 whose
data qubits get Rx(0.1), so that no qubit stays in a definite state, it is
5.3×. The strategies are `cond` (the first half), `mono` (both halves) and
`monokeep` (both, with the final reset run;
[`gpu_resumer.rs`](gpu_resumer.rs), `Strategy::named`). One of those passes,
as ratios to the current simulator, best of 5 / median (noise on gates and
moves only):

| Program × shots         | Noise    | Current, best (s) | `cond`        | `monokeep`    |
| ----------------------- | -------- | ----------------- | ------------- | ------------- |
| rep. code d=9 × 256     | none     | 0.629             | 0.55× / 0.57× | 1.83× / 1.85× |
| rep. code d=9 × 256     | p = 10⁻² | 0.568             | 0.38× / 0.42× | 1.51× / 1.67× |
| rep. code d=11 × 64     | none     | 3.25              | 0.57× / 0.48× | 1.71× / 1.66× |
| rep. code d=11 × 64     | p = 10⁻² | 3.16              | 0.39× / 0.46× | 1.30× / 1.51× |
| dense variant d=9 × 256 | none     | 2.41              | 1.95× / 2.10× | 5.36× / 5.76× |

The ranges above span four such passes; the current simulator's times vary
most between them (0.52–0.80 s at d = 9, 3.1–5.1 s at d = 11).

The current simulator's kernels skip the amplitudes that qubits in a definite
state make zero, and a repetition code's states are basis states, so on these
codes its cost is its host round per operation, about 1 ms each on this
platform ([Platform](#platform-and-what-it-costs-the-current-simulator)).
Where a round costs less, the ratio on the repetition codes may shift back;
the dense variant's should hold.

## How it works

```mermaid
flowchart LR
    P["Adaptive Profile bytecode"] --> M["QirMachine: the classical part, one context per shot"]
    M -- "each shot's batch of quantum calls" --> R["QirGpuResumer: groups shots with equal batches, translates each batch into fused passes (cached)"]
    R -- "one submission per round" --> G["GPU: gate, diagonal and block passes, Pauli noise, sample, collapse"]
    G -- "one readback per round: the outcomes" --> R
    R -- "outcomes" --> M
```

What gives the speed (measured with the `bench` mode's strategies, reset
dropped):

- **Shot lockstep:** every shot of a chunk lives in one buffer, and one pass
  serves all shots of a group (one shot per chunk, `single`, is 85× slower at
  12 × 1024 and 7.8× at 16 × 256).
- **Gate fusion:** diagonal gates into one pass, gates on the low 10 qubits
  into one workgroup-memory block, high-qubit gates into gather blocks (one
  pass per gate, `perpass`, is 2.8–4.4× slower at 12 to 22 qubits).
- **Sampling on the GPU:** measurements are drawn from a per-shot generator on
  the GPU, so a round costs one readback whatever its measurements.
- **Dropping unobservable final work:** a round that writes no results is held
  until the next resume, and dropped when every shot ends right after it (the
  closing `ResetAll`; 1.17–1.36× on the bench program at the table's sizes).

`QirMachine` takes about 2 ms of a 64 ms run at 12 × 1024 (3%).

## Findings on the current simulator

These are in `main`'s code, not in this branch's changes; neither is covered
by a test there.

- **Noise set before the program is dropped.** `set_program` switches the
  context to its adaptive mode by replacing it (`GpuContext::swith_to_adaptive`
  in `gpu_context.rs`), which keeps the noise tables but clears the noise
  configuration, so the shots run noiseless.
- **Noise on measurements fails every shot.** With depolarizing noise on
  `MResetZ`, every shot of the adaptive path ends with error code 2, from Rust
  and from Python; noise on gates alone runs.
  `qdk_simulators::run_adaptive_shots_sync`, which `run_qir(type="gpu")` uses,
  sets noise then program on an adaptive context, so it should fail alike (not
  run). Seen on the GL backend only.

**Changes to the current simulator on this branch** (marked `SPIKE:`), all
included in its numbers above:

- `gpu_context.rs`: the compiled shader was never marked clean, so every run
  recompiled it, which takes most of the first run's 11.5–14.4 s; it now is.
  On a GL adapter buffers are capped at 128 MiB.
- `gpu_resources.rs`: `QDK_GPU_ALLOW_GL` accepts a GL adapter, with GL's
  binding limits; `QDK_GPU_STRIP_DX12` strips the shader's DX12-only sections
  there; `QDK_GPU_BACKEND` restricts the adapter to one backend. The resumer
  selects its adapter with the same function, so both run on the same one.
  Both files and the shader also gained debug switches (`QDK_GPU_TRACE`,
  `QDK_GPU_TRACE_STOP`, `QDK_GPU_DUMP_WGSL`), off by default.

## What is missing for production

Estimates; the rows marked prototyped are built here as a spike only:

| Item                                                                                                                                   | Estimate             |
| -------------------------------------------------------------------------------------------------------------------------------------- | -------------------- |
| Qubit loss (loss noise, `PeekLoss`): refused today                                                                                     | 2–3 days             |
| Idle noise: refused today                                                                                                              | about 1 day          |
| Heavily divergent shots: each group of equal batches costs its passes                                                                  | 1–2 weeks            |
| States over 128 MiB (more than 24 qubits)                                                                                              | about 1 week         |
| Python integration (`run_qir` on the resumer)                                                                                          | 2–3 days             |
| Batching policy with conditional calls (opt-in), for the repetition codes: prototyped here (`resume_conditioned`), with tests and docs | 2–4 days, API change |
| Joint measurement, Pauli frame and composed monomial blocks: prototyped here, with tests and docs                                      | 2–3 weeks            |
| Other backends (Vulkan, DX12, Metal) and devices                                                                                       | not estimated        |

## Correctness evidence

- The `QirResumer` conformance kit passes on the default strategy, with
  failing shots, and with one pass per gate and one shot per chunk.
- The `run_shot` corpus: its 238 noiseless cases at 2,000 shots agree with
  what their tests expect and with the CPU resumer (largest z 2.62); with
  noise support, 241 of its 244 cases pass and the 3 with qubit loss are
  refused.
- The adaptive GPU tests' 9 programs agree with what their tests expect, and
  with the CPU, as do the bench programs at 4 and 12 qubits.
- The state vector after random batches of every gate kind, at 5 to 22 qubits
  and for 6 strategies, matches an f64 reference: 1 − fidelity about 10⁻¹²,
  largest amplitude difference about 5·10⁻⁷, threshold 10⁻⁵ (`state` mode).
- Under noise, every configuration agrees with the CPU resumer (largest z
  3.12): depolarizing p = 10⁻² on the bench at 12 qubits, repetition codes
  d = 5, 7, 9 and `TeleportedTNoiseReporting`; correlated noise; readout noise;
  and five kinds of fault forced with probability 1 (`tx`: X after every T;
  `hy`, `mzx`, `cxzz`, `cxxy` alike) on 8 programs each.
- Against the current simulator, under gate noise p = 10⁻²: the repetition
  codes d = 3 and 5 and the bench at 12 qubits agree (z ≤ 1.8, 1024 shots).

## How to run it

**Prerequisites:** a GPU that wgpu can use; Rust as for the QDK. The program
files need the `qdk` Python package built from this tree
(`python build.py --qdk --no-check --no-test`, then install the wheel from
`target/wheels`).

Build, from the repository root:

```sh
cargo build --release -p qdk_simulators --example qir_gpu_resumer --features conformance
BIN=target/release/examples/qir_gpu_resumer
export GALLIUM_DRIVER=d3d12  # only on WSL with Mesa's d3d12 driver (Platform, above)
export QDK_GPU_ALLOW_GL=1    # only to accept a GL adapter (WSL)
export QDK_GPU_STRIP_DX12=1  # only for the current simulator on a GL adapter
$BIN adapters
# ADAPTER llvmpipe (LLVM ...) | backend Vulkan | device Cpu | driver ...
# ADAPTER Qualcomm(R) Adreno(TM) X1-85 GPU (D3D12) | backend Gl | device Other | driver ...
# SELECTED (current simulator and resumer): Qualcomm(R) Adreno(TM) X1-85 GPU (D3D12) (Gl, Other)
```

Both simulators pick their GPU with the same rule, the current simulator's
([`gpu_resources.rs`](../../src/gpu_full_state_simulator/gpu_resources.rs),
`GpuResources::try_get_adapter`): a discrete GPU before an integrated one,
Vulkan or Metal before DX12; a GL adapter only with `QDK_GPU_ALLOW_GL`, after
both; never a CPU adapter, nor the Adreno X1 on DX12. `adapters` lists every
adapter wgpu sees, marks the Adreno X1 on DX12 as refused, and prints the one
both would use, or fails. `QDK_GPU_BACKEND` (`vulkan`, `dx12`, `metal` or
`gl`) restricts both to one backend, so that a comparison runs both on the
same one; `compare` prints each simulator's adapter in its first lines
(`ADAPTER current: …`, `ADAPTER resumer: …`).

**Correctness** (seconds each, except `tests`, whose CPU reference takes
about 6 minutes):

```sh
$BIN conformance            # ... PASS
$BIN corpus                 # corpus: 244 cases run, 241 passed, 0 failed, 3 skipped
mkdir -p target/programs
python3 source/simulators/examples/qir_gpu_resumer/dump_programs.py target/programs
$BIN tests,target/programs/programs.json   # 11 lines ending ": ok", then PASS
P=target/programs/programs.json+target/programs/programs_noise.json
$BIN noisy,$P,p1e-2,bench_12:256,rep_5:256     # NOISY bench_12 p1e-2 all: 256 shots, ...: ok (each line), PASS
$BIN noisy,$P,corr,corr_branch:4000            # correlated noise
$BIN noisy,$P,p1e-9,readout_branch:20000       # readout noise
$BIN noisy,$P,tx,rep_3:2000                    # forced faults: tx, hy, mzx, cxzz, cxxy
$BIN state,5:3:205,12:4:300                    # STATE ...: ok (each line), PASS
```

`corpus` runs the `run_shot` compatibility corpus at 2,000 shots against the
CPU resumer; the 3 skipped cases use qubit loss. `noisy` compares against the
CPU resumer under the same noise; a line ends `ok` when z ≤ 5. These sizes are
reduced; the [correctness evidence](#correctness-evidence) used, under
`p1e-2`, `bench_12:1024`, `rep_5:1024`, `rep_7:300`, `rep_9:64` and
`TeleportedTNoiseReporting:256`
(its CPU reference takes about 5 minutes), and `state` at 5 to 22 qubits.

**Comparison with the current simulator.** Each configuration is
`<qubits>:<shots>[:<p>[:gates]]`, or a program name (`rep_7`, `rep_9`) in
place of `<qubits>`; `gates` puts the noise on gates and moves only, and
without it the noise is also on measurements. The table of
[Performance on the GPU](#performance-on-the-gpu) is this command, about 7
minutes:

```sh
$BIN compare,$P,12:1024:0,12:1024:1e-3:gates,16:256:0,16:256:1e-3:gates,20:16:0,20:16:1e-3:gates,22:4:0,22:4:1e-3:gates,20:1:0,20:1:1e-3:gates,12:1:0,12:1:1e-3:gates,rep_7:1024:0,rep_7:1024:1e-3:gates,rep_9:256:0,rep_9:256:1e-3:gates
# COMPARE bench_12 shots=1024 p=0e0 on every gate, move and measurement: current best ... s, median ..., worst ... (warm-up ...); resumer ...; resumer+reset ...
# RATIO bench_12 shots=1024 p=0e0 ...: reset kept ...x best, ...x median; reset dropped ...x best, ...x median
# OUTCOMES bench_12 ...: current vs resumer: tvd ..., z ...: ok (and vs resumer+reset)
# ... then PASS
$BIN compare,$P,rep_3:1024:1e-2:gates,rep_5:1024:1e-2:gates,bench_12:1024:1e-2:gates
$BIN compare,$P,rep_3:256:1e-2   # FAILS rep_3 ...: current: failed shots by error code: {2: 256}
```

The bench program outputs one bit, so its `OUTCOMES` lines say little; the
second command checks agreement where the outcomes vary (z 1.80, 1.42 and
0.45 at 1024 shots).

The table of [Closing the gap](#closing-the-gap-conditioned-batches-and-kernels-that-use-them)
is `compare` with the resumer's strategies chosen by
`QDK_COMPARE_STRATEGIES`; `QDK_COMPARE_TOLERATE` times the current simulator
even when some of its shots fail, as 2–3 of 256 do on the dense variant
(error code 2), and reports them (`TOLERATED`). The dense variant `repx_9`
is not produced by `dump_programs.py`: it is `rep_9` with the Rx(0.0) that
each round applies to every data qubit (an identity that gives
single-qubit gates their noise) turned into Rx(0.1), patched into the
compiled program outside this tree. About 6 minutes for the four
configurations:

```sh
export QDK_COMPARE_STRATEGIES=cond,mono,monokeep QDK_COMPARE_TOLERATE=1
$BIN compare,target/programs/programs_noise.json,rep_9:256,rep_9:256:1e-2:gates,rep_11:64,rep_11:64:1e-2:gates
# RATIO rep_9 shots=256 p=0e0 ...: monokeep vs current ...x best, ...x median; outcomes tvd ..., z ...: ok
```

**Benchmark of the resumer alone.** Each configuration is
`<qubits>:<shots>:<strategy>[:<p>]`, or a program name in place of
`<qubits>`; strategy `all` is the default resumer, `notail` keeps the final
`ResetAll`, `cpu` runs the CPU resumer, `cond`, `joint`, `frame`, `mono` and
`monokeep` add the steps of
[Closing the gap](#closing-the-gap-conditioned-batches-and-kernels-that-use-them)
one by one, and the others turn off one optimization each
([`gpu_resumer.rs`](gpu_resumer.rs), `Strategy::named`).

```sh
$BIN bench,$P,12:1024:all,12:1024:cpu,22:4:all,22:4:cpu
$BIN bench,$P,12:1024:notail,12:1024:cpu,22:4:notail,22:4:cpu
$BIN bench,$P,12:1024:all,12:1024:single,12:1024:perpass,16:256:all,16:256:single,16:256:perpass,22:4:all,22:4:perpass
# BENCH n=12 shots=1024 strategy=all p=0e0: first ... s, best ... s, ...; best: machine ... ms, ...
```

**The current simulator from Python.**
[`bench_current.py`](bench_current.py) times `GpuSimulator.run_shots` as a
Python user calls it, so its time also holds the conversion of each shot's
results to Python. Its best times agree with the `compare` mode's within
0.03 s on the bench program at 12 × 1024 and 20 × 1, and are 0.15 s longer
on the d = 9 code at 256 shots. It takes
`bench|rep,<n>,<rounds>,<shots>,<runs>,<p>[,gates]`:

```sh
python3 source/simulators/examples/qir_gpu_resumer/bench_current.py bench,12,8,1024,6,1e-3,gates
# CURRENT bench n=12 rounds=8 shots=1024 p=0.001 gates times=... best_after_warmup=...s
python3 source/simulators/examples/qir_gpu_resumer/bench_current.py rep,3,8,256,2,1e-2
# FAILED shots by error code: {2: 256}
```

It sets the noise after the program: `GpuSimulator.set_program` forgets a
noise configuration set before it
([Findings](#findings-on-the-current-simulator)).
