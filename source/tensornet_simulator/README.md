# `tensornet_simulator`

The tensor-network simulator: matrix product states over any
`tensornet::MpsEngine`, run through the executor protocol. It is M1's MPS
method (④′b): what M0's ④ (`qdk_cutensornet`'s `run_mps_shots` and
`evaluate_state_queries`) did, now over the engine trait and the protocol.

## What this crate does

The engine (`tensornet`; on the GPU `qdk_cutensornet`'s `CuTensorNetEngine`,
exactly `tensornet::reference::ExactReference`) knows matrices on sites. The
protocol (`executor_protocol`) knows shots, sessions and outcomes.
`qdk_simulators::execution` knows programs, their walk and the region ISA.
This crate joins them in two ways:

- **The batch strategy** (`MpsBatchSimulator`), which M1 samples Base
  programs with, as M0 did: one MPS for every shot, the program's region
  applied as often as the walk runs it, then one `MpsEngine::sample` call
  for all the shots. Each shot's row is replayed through the program's
  walk (`RuntimeAdapter`), which answers each measurement from its
  qubit's column.
- **The per-shot terminal** (`TensorNetTerminal`, opened per shot by
  `PerShot(TensorNetShots)`), the one `mps-a` will run Adaptive programs
  with. Each shot has its own state, so a shot's measurements steer only
  that shot. It therefore offers feedback, resets and replayable shots.

State queries (`Evaluate`: expectations of Pauli sums, and the chain's cost)
read ψ, the state before the terminal measurements. They do so outside the
shot protocol, as M0's `evaluate_state_queries` did. `MpsStack` is M1's
stack builder: an engine and a bond cap χ, then `sample`, `evaluate`,
`batch` or `per_shot`.

Region gates become matrices through `qdk_simulators`'
`execution::operator_matrix`, the gate tables ③ shares. The per-shot
terminal then turns each region's gates into the operators the engine
applies, by its `GateFusion`:

- `Unfused` (the default) gives one operator per gate, as M0 did.
- `Pairs` multiplies the region's gates into blocks of one or two qubits.
  A single-qubit gate joins the block of its qubit's next or previous
  two-qubit gate, and consecutive gates on one pair make one 4×4.

Fusion is exact, and no block crosses a measurement or a reset. On H₂ it
gives 4.5× fewer operators (87,598 for 392,338 gates). Whether it saves
time on the A100 is not yet measured, so `Unfused` stays the default (see
`src/fusion.rs`). The batch strategy and the state queries stay unfused.

## Types and their uses

Each use below is backed by the test named next to it (the paths are under
`src/tests/`).

| Type or function                                            | Use                                                                                                                                    | Test                                                                                                               |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `MpsStack::sample`                                          | M0's `run_mps_shots`: N shots of a Base program for a seed. The same program, χ and seed give M0's sample.                             | `parity.rs`: `the_stack_samples_what_m0_sampled`, `the_same_seed_gives_the_same_shots_and_another_seed_others`     |
| `MpsStack::evaluate` (`Evaluate`)                           | M0's `evaluate_state_queries`: ⟨Σ⟩ and the chain's cost on ψ, after M0's preflight.                                                    | `queries.rs`: `mps14_mps19_…`, `mps15_…`, `mps16_…`, `mps17_…`; `programs.rs`: `state_queries_share_the_preflight` |
| `MpsStack::with_max_bond_dimension`                         | M0's `max_bond_dimension` option; `None` gives `DEFAULT_MAX_BOND` (128).                                                               | `programs.rs`: `max_bond_dimension_reaches_the_capacity`                                                           |
| `MpsStack::batch`, `MpsBatchSimulator`                      | A `ShotStrategy` that samples every shot from one MPS. It refuses `feedback`, `resets`, `replayable` and `probabilities` with `Unmet`. | `batch.rs`: `mps7_mps12_mps20_…`, `mps8_…`, `mps9_…`                                                               |
| `sampler_seed`                                              | The engine's sample seed for a session seed, as M0's `derive_sampler_seed`.                                                            | `parity.rs`: `the_sampler_seed_is_m0s`                                                                             |
| `MpsStack::per_shot`, `TensorNetShots`, `TensorNetTerminal` | One state per shot, through `PerShot`, with feedback, resets, replayable shots and each shot's ln p.                                   | `per_shot.rs`: `mps1_…`, `feedback_and_resets_run_shot_by_shot`, `a_shot_draws_the_same_alone_or_with_others`      |
| `GateFusion`, `SiteOperator`, `with_gate_fusion`            | How the per-shot terminal turns a region's gates into engine operators: `Unfused` (one per gate) or `Pairs` (blocks of ≤ 2 qubits).    | `fusion.rs`: `every_fusion_applies_the_product_of_the_regions_gates`, `fused_and_unfused_shots_draw_the_same_…`    |
| `TensorNetReport`, `MpsBatchReport`                         | What a session ran: its batches, its instructions, its engine operators and its largest bond.                                          | `per_shot.rs`: `mps2_the_report_keeps_the_largest_bond`; `batch.rs`: `mps9_…`                                      |
| `mps_capacity`                                              | The capacity for a qubit count and χ: each bond capped by the smaller side and by χ.                                                   | `tests.rs`: `the_capacity_caps_each_bond_at_the_smaller_side_and_at_chi`                                           |
| `StackError`, `ProgramError`, `QueryError`                  | Why nothing ran, before any state is prepared (L3), or why the engine failed (`is_environment_error`, as M0's).                        | `programs.rs`: `preflight_rejects_…`, `plan_rejects_…`; `batch.rs`: `mps11_…`                                      |

## Files

| File                           | What it holds                                                        |
| ------------------------------ | -------------------------------------------------------------------- |
| `src/stack.rs`                 | `MpsStack`, `StackError`, `DEFAULT_MAX_BOND`                         |
| `src/program.rs`               | M0's preflight (`plan`) and each shot's replay through the walk      |
| `src/batch.rs`                 | the batch strategy, its sessions and report; `sampler_seed`          |
| `src/terminal.rs`              | `TensorNetTerminal`, `TensorNetShots`, `TensorNetReport`             |
| `src/queries.rs`               | `Evaluate`, `StateQuery`, `StateQueryValue`, `MpsCost`, `QueryError` |
| `src/lowering.rs`              | region gates to matrices, by ③'s gate tables                         |
| `src/fusion.rs`                | `GateFusion`, `SiteOperator`: a region's gates to engine operators   |
| `src/capacity.rs`              | `mps_capacity`                                                       |
| `src/tests.rs`, `src/tests/`   | the tests (the module table is in `src/tests.rs`)                    |
| `qualify-simulator-on-a100.sh` | the A100 qualification run (below)                                   |

## Tests

On CPU, every test runs on `ExactReference`, or on `TestDoubleEngine`, a
counting or refusing wrapper around it. The cases that hold for any engine
are generic over `MpsEngine` (`src/tests/cases.rs`). They run on CPU and, in
`src/tests/qualification.rs`, on cuTensorNet.

```sh
cargo test -p tensornet_simulator
```

## A100 qualification

`src/tests/qualification.rs` runs M1's stack over `CuTensorNetEngine` on
these cases:

| Test                                       | Case                                                                                         |
| ------------------------------------------ | -------------------------------------------------------------------------------------------- |
| `bell_on_the_a100`                         | 1000 Bell shots are correlated; the hand-computed expectations hold to 1e-10                 |
| `b0_expectations_agree_with_the_reference` | 8 Pauli strings on 16 scrambled 4-qubit circuits agree with `ExactReference` to 1e-10        |
| `b1_widths`                                | M0's B1: an X pattern on 2, 3, 63, 64 and 128 qubits reads back; the bond stays 1            |
| `b2_trotter`                               | M0's B2: ⟨Σ ZᵢZᵢ₊₁⟩ at widths 12, 16 and 20, 8 steps, M0's values and `ExactReference`, 1e-9 |
| `b3_matched_bond`                          | M0's B3: width 128, 16 steps, χ 128, M0's value to 1e-10                                     |
| `b4_convergence`                           | M0's B4: width 256, 16 steps, χ 32, 64, 128 and 256, M0's values to 1e-9                     |
| `per_shot_bell_pairs_on_the_a100`          | 200 per-shot Bell shots through the walk, each with ln p = ln ½                              |
| `per_shot_feedback_and_resets_on_the_a100` | 40 shots with feedback, a reset and `MResetZ`, through the walk                              |
| `per_shot_phase_estimation_on_the_a100`    | 200 shots of 4-bit iterative phase estimation (feedback, resets), near textbook QPE          |
| `…_in_pairs_on_the_a100`                   | the two cases above with `GateFusion::Pairs`                                                 |
| `h2_qpe_on_the_a100`                       | M1's acceptance test: H₂ phase estimation from `QDK_H2_BYTECODE`, a session per shot (below) |

The engine is `Availability::mps_engine`, with M0's base qualification
policy (absolute cutoff 1e-10). M0 ran B3 and B4 with an absolute cutoff of
1e-12, which the engine does not expose. Their values and tolerances are
still M0's, so if B3 or B4 misses its tolerance, check the cutoff first.

They are `#[ignore]`d. `qualify-simulator-on-a100.sh` runs them on the pinned
A100 VM. First it checks the host, the tools, the commit, the GPU, the
libraries and their versions, and the disk. Then it builds a detached
worktree at the commit, next to the checkout, and runs:

```sh
cargo test -p tensornet_simulator --release -- --ignored --nocapture --test-threads=1 tests::qualification::
```

The script writes its results to `$HOME/simulator-qualification-<date>/` and
packs them as a `.tar.gz`, also when a step fails. The results are
`summary.txt` (each test passed or failed, then the measured values),
`run.log` and `environment.txt`. On the VM, with the qdk checkout at
`/data/domingom/qdk` and `COMMIT` the commit to qualify:

```sh
cd /data/domingom/qdk
git fetch origin domingom/tensor-network-simulator
git show COMMIT:source/tensornet_simulator/qualify-simulator-on-a100.sh > /data/domingom/qualify-simulator-on-a100.sh
chmod +x /data/domingom/qualify-simulator-on-a100.sh
/data/domingom/qualify-simulator-on-a100.sh --check COMMIT /data/domingom/qdk
/data/domingom/qualify-simulator-on-a100.sh --quick COMMIT /data/domingom/qdk
/data/domingom/qualify-simulator-on-a100.sh COMMIT /data/domingom/qdk
```

The options are:

- `--check` only checks, and builds and changes nothing.
- `--quick` runs Bell, B1 and the per-shot cases, unfused and in pairs.
- With no option, the script runs every case but H₂.
- `--h2 FILE [--shots N] [--fusion F] [--stop-after M]` runs H₂ only
  (below).

### H₂

`h2_qpe_on_the_a100` runs H₂'s phase estimation (29 qubits, 1,404 results,
713 `ICMP`s, 10 resets) on the MPS through the protocol, one session per
shot. It reads the `AdaptiveProgram.as_dict()` JSON that
`qdk_chemistry` produces (as `qdk_simulators`' `h2_qpe` test does) from
`QDK_H2_BYTECODE`, and `QDK_H2_SHOTS` (1), `QDK_H2_SEED` (0) and
`QDK_H2_CHI` (32). It prints each shot's results, its time, its engine
operators and the device memory. Every 10 measurements it also prints a
progress line with the time and the operators so far. At 50 shots or more it
asserts that the two most frequent outcomes are `01010` and `10110`, written
as `qdk_chemistry` writes them (the last result first). Copy the JSON to the
VM, then run one shot, then more:

```sh
/data/domingom/qualify-simulator-on-a100.sh --check --h2 /data/domingom/h2.bytecode.json COMMIT /data/domingom/qdk
/data/domingom/qualify-simulator-on-a100.sh --h2 /data/domingom/h2.bytecode.json --shots 1 COMMIT /data/domingom/qdk
/data/domingom/qualify-simulator-on-a100.sh --h2 /data/domingom/h2.bytecode.json --shots 50 COMMIT /data/domingom/qdk
```

To measure gate fusion, `--fusion unfused|pairs` sets `QDK_H2_FUSION`.
`--stop-after M` sets `QDK_H2_STOP_AFTER`, which stops each shot after M
measurements; such a shot has no row. For example, the first 100
measurements of one shot, unfused and then in pairs:

```sh
/data/domingom/qualify-simulator-on-a100.sh --h2 /data/domingom/h2.bytecode.json --fusion unfused --stop-after 100 COMMIT /data/domingom/qdk
/data/domingom/qualify-simulator-on-a100.sh --h2 /data/domingom/h2.bytecode.json --fusion pairs --stop-after 100 COMMIT /data/domingom/qdk
```

The H₂ driver is `src/tests/h2.rs`. `tests::fusion::h2_driver_on_a_coin_engine`
runs it on CPU, on an engine that applies nothing, and
`tests::fusion::h2_operators_per_shot` counts H₂'s operators under each
fusion. Both need `QDK_H2_BYTECODE`.

The library paths come from `QDK_CUTENSORNET_LIBRARY` and
`QDK_CUDART_LIBRARY` if they are set. Otherwise the script uses the pinned
paths, and then `ldconfig`. Send back the `.tar.gz`.
