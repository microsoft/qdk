# Agent reference

This file is for reviewing agents. Follow [reviewer.md](reviewer.md) for the review
procedure; consult this reference for configuration, commands, case manifests,
build integrity, and evidence storage. Delegated reviewers follow their completed
[subagent brief](subagent-brief.md).

## Sandbox environment

Run inside the QDK Copilot sandbox. Its working checkout is `/work/qdk`; Git,
the Rust toolchain, and Python dependencies are supplied by the sandbox image.
Use `/opt/qdk-venv/bin/python` and `/opt/qdk-venv/bin/qir-runner`, not an assumed
repository-local `.venv`. Issue snapshots additionally require authenticated `gh`.
Package feeds are blocked during sessions; report missing dependencies rather
than trying to install them.

The controller uses only the Python standard library. It does not install tools,
alter the existing QDK installation, post to GitHub, commit changes, or modify
production sources. Workers use matching native/Python overlays in disposable
processes. This is crash isolation, **not a sandbox**: execute only inspected,
trusted adapters and Q# programs inside the surrounding sandbox. Reviews write
and execute arbitrary Python; do not run the kit directly on the host.

A skill/custom-agent wrapper is optional. The kit works as ordinary files/scripts.
Do not add this entire procedure to global instructions for unrelated coding work.

### Sandbox paths

The layout below is based on `upstream/amcasey/sandbox` at
`2eecd9260d848728061777e4616adf9b8de013b2`, specifically
`tools/copilot-sandbox/compose.yaml` and the agent image's Dockerfile/entrypoint.
`/work` is an instance-scoped Docker volume that survives container sessions;
deleting the sandbox instance removes it. `/tmp` is an ephemeral, RAM-backed
**noexec** tmpfs. The exchange directories are host bind mounts, not caches.

The harness uses these sandbox-specific defaults. Choose a unique review `name`;
`REVIEW` below stands for that name. Concurrent reviews must not share mutable
working directories. Temporary paths use Python's temporary directory (`/tmp`
unless overridden by `TMPDIR`).

| Artifact                                                                                                                                                                   | Default sandbox location                                                    | Lifetime                                                               |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Kit instructions, scripts, schemas, examples, curated tests                                                                                                                | `/work/qdk/tools/compiler-review-kit/`                                      | Version-controlled source                                              |
| Compiler checkout (`repository`), `source/`, `library/`, `samples/`                                                                                                        | `/work/qdk/`                                                                | Existing `/work` volume                                                |
| Python environment (`venv`), including `bin/python` and `bin/qir-runner`                                                                                                   | `/opt/qdk-venv/`                                                            | Preinstalled in the sandbox image                                      |
| Configuration, generated cases, adapters, repro/variant scripts, generator output/seeds                                                                                    | `/tmp/compiler-review-kit/REVIEW/`                                          | Current session; the agent writes its config and fixtures here         |
| Runs, project snapshots, QIR, stage logs, oracle checks, exec/timing/variant results, reductions, attribution, draft scripts, extraction dumps/logs, rechecks (`work_dir`) | `/tmp/compiler-review-kit/REVIEW/`                                          | Disposable; save compact evidence before cleanup                       |
| Compiler worktrees, extraction worktrees, Cargo targets, native overlays, build logs and build lock (`build_dir`)                                                          | `/work/compiler-review-kit/build/`                                          | Reusable native build state in the `/work` volume                      |
| `evidence.json`, compact summaries, bundled findings/repros/seeds (`cache_dir` plus review name)                                                                           | `/work/cache/compiler-review-kit/REVIEW/`                                   | Budgeted cross-session evidence in the `/work` volume                  |
| Final report (`report`); issue drafts when requested                                                                                                                       | `/exchange/from_copilot/REVIEW.md`; use `REVIEW-issues.md` for issue drafts | User-facing deliverables                                               |
| User-provided context or a previous report                                                                                                                                 | `/exchange/to_copilot/`                                                     | Read-only input; copy inspected executable fixtures into owned scratch |
| Worker working directories, temporary projects and `.ll` files (`tempfile`)                                                                                                | `/tmp/` or `TMPDIR`                                                         | Current process/session                                                |
| `clients[].path`, case `source_file`, `project_root`, `adapter`                                                                                                            | Actual sandbox checkout/input path                                          | User-specified inputs; never assume host paths                         |
| Copilot session/history files                                                                                                                                              | `/home/agent/.copilot/`                                                     | Separate Copilot Docker volume; not review evidence storage            |

`build_dir` contains `worktrees/`, `extract/`, `cargo-target/`, `extract-target/`,
and `builds/<SHA>-<mode>/`. Do not put it on `/tmp`: Cargo executables and native
`.so` overlays need executable mappings. Only data and Python source go in
`work_dir`; the interpreter reads Python source even on a noexec mount.
Image caches such as `/opt/cargo` belong to sandbox provisioning, not the
review's evidence-pruning budget.

The sandbox launcher owns `/seed.git` (read-only host Git input) and `/outbox.git`
(Git output), not the review kit. Use ordinary checkout/Git operations rather
than storing review artifacts in those mounts. `/exchange/from_copilot` is for
deliverables only. Never put build caches, bulk cases, or reusable evidence there.

For example, the agent can write `/tmp/compiler-review-kit/REVIEW/review.json`
and then pin its chosen revisions; the path fields need no overrides:

```json
{
  "reference": "origin/main",
  "candidate": "HEAD",
  "name": "REVIEW"
}
```

Evidence bundles may be pruned or lost with the volume: keep standalone repros
embedded in the delivered report as well.

## Configuration

The lead reviewer creates the configuration from the user's request; a
human-authored configuration is optional. Use `review.schema.json` for supported
fields and defaults. `review.example.json` illustrates optional settings, not
filters or exclusions to inherit for every review.

Store the configuration in an owned review working directory. Command examples
below use `/tmp/compiler-review-kit/REVIEW/review.json` as a placeholder:
substitute the actual owned configuration path. Apply the
[sandbox paths](#sandbox-paths)
instead of writing working files into the repository. Keep the final report
outside disposable work/cache directories and resolve supplied paths relative to
the configuration file.

Only two fields are required:

```json
{
  "reference": "origin/main",
  "candidate": "HEAD"
}
```

The repository defaults to `/work/qdk`, even when the configuration is outside
the checkout. All supplied relative paths resolve from the configuration file.
Use path overrides only when the review requires a different owned location.

| Optional field                 | Meaning/default                                                                                 |
| ------------------------------ | ----------------------------------------------------------------------------------------------- |
| `previously_reviewed`          | Extra previous-candidate column, only for user-requested lost-fix tracking; omitted by default  |
| `historical_reference`         | Additional explicitly labeled baseline                                                          |
| `layers`                       | Intermediate heads of a PR stack, oldest first; run as columns `layer1`, `layer2`, ...          |
| `issues`                       | Author/minimum-number sweep plus explicit issue includes; both open and closed issues           |
| `clients`                      | Named local Git checkouts to investigate; no chemistry checkout is assumed                      |
| `exclusions`                   | Case ID/origin/path globs, each with a reason                                                   |
| `report`                       | Output path; defaults to `/exchange/from_copilot/<name>.md`                                     |
| `profiles`                     | Base, Adaptive_RIF, Adaptive by default; Adaptive_RI only when requested                        |
| `debug_validation`             | `none` by default; only when requested, `targeted` replays findings or `all` runs both modes    |
| `keep_qir`                     | `false`: retain failure/uncertain QIR, discard uncomplicated successful QIR; `true` retains all |
| `limits`                       | Stage timeouts, memory, concurrency, shots, seed                                                |
| `name`                         | Review name for scratch/evidence; defaults to the explicit report's stem, or `review`           |
| `cache_dir`, `cache_budget_mb` | Evidence cache root; `$REVIEW_KIT_CACHE` or `/work/cache/compiler-review-kit`; 50 MiB           |
| `venv`                         | Python environment; `/opt/qdk-venv`                                                             |
| `work_dir`                     | Temporary review data; `/tmp/compiler-review-kit/<name>` (honors `TMPDIR`)                      |
| `build_dir`                    | Executable build state; `/work/compiler-review-kit/build`                                       |
| `repository`                   | Compiler checkout; `/work/qdk`                                                                  |

Defaults: two concurrent cases; compile 300 seconds; simulator and runner 120
seconds each; 16 GiB address-space limit per worker; eight shots; seed 1.
Timeouts are evidence of incomplete execution, not proof of a compiler bug.
Memory limits also affect subprocesses spawned by the worker.

`review.schema.json` is suitable for editor validation. The CLI additionally
rejects unknown/duplicate fields. It resolves branch names once per command;
subsequent builds within that command use full SHAs. For a multi-command review,
**replace moving refs with the SHAs printed by `check`** before proceeding.

`check` records dirty main/client checkouts. Compiler builds exclude main-worktree
edits. Client cases snapshot local project files, including deliberate edits:
review their dirty state, or use a clean client worktree.

## Commands

Run from `/work/qdk`, using the sandbox's preinstalled interpreter:

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py check /tmp/compiler-review-kit/REVIEW/review.json
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py build /tmp/compiler-review-kit/REVIEW/review.json
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py run /tmp/compiler-review-kit/REVIEW/review.json tools/compiler-review-kit/examples/cases.json
```

`run` prints its new run directory. It builds missing revisions automatically.
Every configured role is a column: `reference` and `candidate`, plus
`historical_reference`, `previously_reviewed` and each `layerN` when configured.
`run --roles` narrows the extra columns (`--roles` alone runs only the two endpoints).
Results never resume from an old JSONL file: every run is fresh. A run contains
resolved configuration/tools/build identities, case snapshots, stage results,
raw output, and retained unmodified QIR. The main checkout is never switched.

`cases.json` and its bundled inputs are the authority for what ran. Sources are
inlined; adapters and complete local dependency graphs are snapshotted with
relative paths, sharing each project once per bundle without merging distinct
packages. Content identities exclude temporary paths and provenance notes.
Exports, saved evidence, reports, and comparisons use these frozen inputs, not
the original manifest or configuration. Changed frozen inputs are an error.

Release is the only default build mode. Do not build debug unless the user asks,
including for assertion-sensitive findings. If the user requests targeted debug
validation, set `debug_validation=targeted`, assemble a findings manifest, and
explicitly replay it:

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py run /tmp/compiler-review-kit/REVIEW/review.json /tmp/compiler-review-kit/REVIEW/findings.json --mode debug
```

Only when the user requests debug validation of every case,
`debug_validation=all` runs release and debug for every selected case.
Adaptive_RI is also opt-in: add it to the configuration's `profiles` only when
requested. Existing support for all four profiles remains available.

Generate an **evidence draft** from a run, then complete it using the report
template. The generated draft does not establish root causes or PR attribution.
Individual-commit attribution is not required unless requested.
Self-checking scripts are generated only for cases with an explicit non-Unit
deterministic expectation; other cases retain their evidence and need an oracle
before publication. Report and issue repros use one renderer, embedding the same
stage execution and output parsing code as the harness. They need no kit files
when run, and include local projects and adapters.

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py report /tmp/compiler-review-kit/REVIEW/runs/RUN_DIRECTORY
```

Existing reports are protected; `--overwrite` is only for an owned generated draft.
Helper outputs explicitly named with `--output` can be replaced; choose owned
artifact paths, never an unrelated existing file.

`run` prints `[completed/total]` as cases finish, so a slow case never makes a run
look stalled; `results.jsonl` is rewritten in manifest order when the run ends.

### Summaries and run-to-run comparison

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py summarize /tmp/compiler-review-kit/REVIEW/runs/RUN [--json summary.json]
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py compare /tmp/compiler-review-kit/REVIEW/runs/OLD /tmp/compiler-review-kit/REVIEW/runs/NEW
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py subset A.json B.json --match 'i38*' --ids extra-case \
  --output /tmp/compiler-review-kit/REVIEW/subset.json
```

`summarize` cross-tabulates reference→candidate outcomes per profile, groups
candidate failures by normalized signature (numbers replaced by `N`) with their
reference outcomes, lists flagged regressions, and checks the **profile hierarchy**:
a program validated on a lower tested profile should not fail on a higher one.
The full supported hierarchy is Base ⊂ Adaptive_RI ⊂ Adaptive_RIF ⊂ Adaptive;
Adaptive_RI is not tested by default. Violations new at the candidate are listed, and all
candidate violations are grouped by the higher profile's failure signature. It also lists:

- **loud → silent**: the reference panicked, crashed or rejected the program and the
  candidate returns a wrong result. Not a regression verdict, but a bug that stopped
  announcing itself; report it explicitly.
- **intermediate-layer hazards**: a
  `layerN` that breaks a program both endpoints get right (or silences a loud
  reference failure). Stacked PRs merge one at a time, so these ship if a layer lands alone.
- **lost fixes** against `previously_reviewed`, only when that optional column was
  configured in response to a user request. Do not investigate them by default.
- **the candidate's own extracted test programs that fail end to end**: their cargo
  tests pass, so the test helpers skip part of the user pipeline.
- **identical QIR** at both endpoints, which is the only evidence for observe-only cases.
- **compile-time outliers**: per case and profile, candidate/reference compile-stage
  time of at least 2x and +0.25 s. Sweeps run concurrently, so confirm with `timing`.

Compile-time medians come from concurrent sweeps and are only a hint.

`compare` diffs two runs case by case (candidate outcomes by default; `--side
reference` compares reference columns). Use it after every PR update: rerun the
same manifests and compare reference-relative regressions and fixes. Investigating
**lost fixes** is opt-in. `--select` terms (`a=STATUS`, `b!=STATUS`, ...) must all
hold for one shared profile; `--output` exports the selected frozen cases from run
B into a portable bundle. Comparisons reject changed source, dependencies,
adapters, or execution settings under the same ID; different profile coverage
can still be compared on shared columns. Build modes must match.
An empty selection writes `[]`, replacing any previous output; it is not a
runnable case manifest.
A change from a loud failure to a wrong result is labelled `worse (silent)`.
`subset` uses the same bundling and content-conflict checks. Duplicate IDs with
identical inputs are merged; conflicting definitions are an error, including
during cached rechecks.

Only when the user requests lost-fix analysis, configure `previously_reviewed`
or select differences between earlier and current candidate runs:

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py compare OLD NEW --select 'a=validated,b!=validated' \
  --output /tmp/compiler-review-kit/REVIEW/lost-fixes.json
```

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py table RUN [RUN ...] [--profiles Adaptive_RIF Adaptive] \
  [--cases 'i38*'] [--only-diff]
```

`table` prints, for each case, one line per revision with the simulator values and
each profile's values (wrong ones in `**bold**`), or the failure kind and message.

### Timing

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py timing /tmp/compiler-review-kit/REVIEW/review.json CASES.json \
  [--roles reference candidate] [--profiles Adaptive_RIF] [--repeats 5] [--cases-match 'unary*']
```

Each revision's overlay times context creation, loading (`eval`/adapter) and
`compile` separately, in one process per sweep. Sweeps are interleaved and rotated
across repeats so no revision always runs first or under a load spike. The output
keeps every repeat, including failed case/profile records, in `timing.json`.
Medians are not rounded before arithmetic; each series includes its range and
sample standard deviation (unknown for a single sample). Ratios are medians of
same-repeat candidate/baseline ratios, not ratios of medians. Outliers
(`--outlier`, default 1.5x) show pair counts and timing ranges. Use it for client entry points and for every
`summarize` outlier before reporting a performance change.
Samples are saved after each sweep; a later worker failure leaves a `failed`
timing artifact with partial evidence, not a complete performance claim.
Older timing summaries without raw repeats must be rerun to obtain paired ratios.

### Variant matrices

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py variants /tmp/compiler-review-kit/REVIEW/review.json VARIANTS.py [--roles ...|--refs ...]
```

A trusted variants file defines `VARIANTS`: a list of dictionaries with `name`,
`expected` (as printed, for example `1`, `[1, 0]`, `true`) and either `source`, or
`body` appended to an optional `PRELUDE`, with an `entry` string or a `build(ctx)`
callable returning `(entry, args)` for the callable API. Optional `PROFILES` and
`PROJECT` (files of a temporary Q# project; sources are evaluated afterwards, as a
separate compilation unit). Every variant runs in the simulator and on each profile
(compile + qir-runner) at each revision using the ordinary case runner, with the
same independent stages, memory caps, timeouts, import checks, and retained shots.
The command returns a standard run directory, not a separate worker-result format.
The matrix marks wrong results and lists
the variants whose outcome differs between revisions. Use it to narrow a finding to
the one ingredient that matters (see `tools/compiler-review-kit/examples/variants.py`).
Profile selection uses `--profiles`, then an explicit configuration `profiles`,
then the variants module's `PROFILES`, then Base/Adaptive_RIF/Adaptive. Include
Adaptive_RI only when the user asks.
Keep definitions self-contained and construct QDK objects inside `build(ctx)`,
not at module import time. Printed `expected` values are display checks; curate a
typed deterministic case before treating a variant as an adjudicated finding.
Difference filters include simulator observations, not just compiled profiles.

### Executing standalone repros on each revision

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py exec /tmp/compiler-review-kit/REVIEW/review.json repro.py \
  [--roles reference candidate | --refs SHA1 SHA2] [--mode debug] [--args ...]
```

`exec` runs a self-contained Python script with each revision's pinned overlay
(no `.venv` package swapping), keeps full stdout/stderr per revision under
`<work_dir>/exec`, and reports the exit status, final Python exception, and every
native panic with its `file:line`, message, and the enclosing Rust `fn` resolved
at that revision. Use it for the final inline repros and for variant tables.
Scripts and worker stages share process-group supervision: a timeout requests
termination, allows bounded cleanup, then kills the group if necessary.

### Extracting test programs

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py extract-tests /tmp/compiler-review-kit/REVIEW/review.json \
  --crate qsc_fir_transforms --revision candidate --output /tmp/compiler-review-kit/REVIEW/fir-tests.json
```

In an owned worktree, this inserts a dump call at the start of the crate's test
helpers that take an explicit expected value (bundled hooks:
`qsc_fir_transforms` `check_semantic_equivalence_impl` and `check_qir_int_result`),
runs `cargo test -p CRATE --lib` with an isolated target directory, and converts the
dump into deterministic cases. IDs hash the source, so manifests from several
revisions can be merged without duplicates (removed tests keep their programs).
When a helper is renamed, extraction fails rather than producing an empty corpus;
supply `--hooks FILE` (same JSON shape as `DEFAULT_HOOKS` in `harness/extraction.py`).
Failing tests still contribute their programs; inspect the recorded log.

### Issue overlap

`issues --all` snapshots every repository issue (titles and bodies, no comments).
`match-issues SNAPSHOT --run RUN` (or `--text 'signature'`) lists issues whose text
contains each candidate failure signature's stable prefix. A match is a lead, not a
duplicate: the same invariant panic has many causes. GitHub's search (`gh search`,
`gh issue list --search`) can silently return nothing in restricted environments; use
the snapshot and `match-issues --text 'phrase'` as the search instead.

### New diagnostics and panic sites

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py new-diagnostics /tmp/compiler-review-kit/REVIEW/review.json [--json found.json]
```

This lexically scans added/removed lines outside test-named Rust files, including
diagnostics removed by deleting whole files. It lists codes, `#[error]` messages,
and added panic/assert/expect sites with candidate locations. Inline test modules
may appear and multiline Rust syntax may be missed; use the results as leads,
not an exhaustive inventory. Every new rejection is a place to write valid
programs at its boundary. For example, `UnsupportedRecursiveInlining` once also
rejected valid static-depth recursion. Every removed code needs a check that the
condition is now handled.

### Drafting issues

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py draft-issues /tmp/compiler-review-kit/REVIEW/review.json ISSUES.py \
  --output /tmp/compiler-review-kit/REVIEW/issues.md [--revisions reference candidate] [--only SLUG] [--cached]
```

`ISSUES.py` (trusted Python) defines `ISSUES`: dictionaries with `slug`, `title`,
`kind` (may contain `{panic:REVISION}`, replaced by the panic's linked location),
`affects`, `regression`, `found_in`, `related`, `description`, `source`, `expected`
(as printed), `affected` (rows that show the bug), `correct` and `cause`. Optional
fields:

- `build`: code defining `FORMS`, for callable-API forms;
- `project`: files of a temporary project, with `source` evaluated afterwards;
- `how`, `doc` and `revisions`.

Optional module-level `HEADER`, `COMMENTS` (Markdown for existing issues), `REVISIONS`
and `COLUMNS` (column labels). Each repro is rendered from
`templates/issue_repro.py.template`, executed with `exec` at every revision, and the
issue's Outputs table, messages and panic links come from those executions. The
scripts exit 0 when the bug no longer reproduces, and 1 when it reproduces.
Generated scripts use the configuration's profiles, omitting Adaptive_RI by
default. If no tested row reproduces but an affected row was omitted, the script
exits 2 with an inconclusive result, not a claim that the bug is fixed.
See `issue-template.md` for the format.
`--cached` and reuse during `--only` require matching script contents, resolved
targets, build mode, timeout, and verified native/Python/library/tool provenance.
Builds are prepared once per distinct SHA, not once per issue. A mismatched cache
entry is rerun, never relabeled with the current revision.

### Generators and oracle tools

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/generators/callable_chains.py /tmp/compiler-review-kit/REVIEW/gen \
  --inline 1600 --layout 800 --python 400 --seed 2 --profiles Adaptive_RIF Adaptive
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py oracle-check /tmp/compiler-review-kit/REVIEW/review.json /tmp/compiler-review-kit/REVIEW/gen/cases.json
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py run /tmp/compiler-review-kit/REVIEW/review.json /tmp/compiler-review-kit/REVIEW/gen/cases.json
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py stages /tmp/compiler-review-kit/REVIEW/fir-tests.json /tmp/compiler-review-kit/REVIEW/gen/cases.json
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py statecheck RUN/CASE/candidate/Base.ll
```

`generators/callable_chains.py` composes factories, closures, records holding
callables, static/mutable/measurement-dependent selection, callable arrays (including
nested closures from one factory capturing arrays of different lengths, arrays held in
structs, and arrays of closures over captured loop counts), gate side effects,
`Adjoint`/`Controlled`, `within`, and library higher-order operations;
a layout family uses `(Int, Qubit[]) => Unit` callables with holes in different
positions, and a Python family composes factories through the callable API. Its
oracle is by construction: identity qubits see only X/CNOT/CCX and one probe qubit
sees Rz multiples of π/2 inside a single H…H pair, so every correct program ends in
a computational-basis state and the generator models the expected result. Wrong
selection/environment/order changes identity bits; a dropped or extra `Adjoint` or a
wrong captured angle flips or randomizes the probe. The Python family also builds
structs holding callables, closures capturing such structs, and entry operations taking
a struct argument. Record the command and seed.

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/generators/dynamic_recursion.py /tmp/compiler-review-kit/REVIEW/rec \
  --inner 300 --outer 150 --classical 50 --seed 1
```

`generators/dynamic_recursion.py` generates recursion under measurement-dependent
control flow, with static depths:

- recursive operations whose recursive call, gate and continuation sit in the taken
  branch, the untaken branch, or after it; with array or scalar parameters, and
  through mutual recursion;
- static recursions called from a caller's dynamic branch as body, `Adjoint` or
  `Controlled`;
- classical recursion in a dynamic branch.

Results are modelled; ancillas are measured in known basis states.

`oracle-check` simulates every case with an explicit expectation at one revision
and exits non-zero on disagreements: run it before trusting generated or curated
expectations. `stages` reports how many chain stages programs combine, e.g. to see
whether deep compositions come only from a PR author's own tests. `statecheck`
replays a restricted static-resource QIR subset in a small statevector: one
marked entry function, one straight-line block, constant handles/angles, modeled
unitary gates, and terminal measurements. Hexadecimal LLVM doubles are decoded
as IEEE values. Unsupported instructions/calls, helper functions, branches,
mid-circuit measurements, and explicit resets are rejected, not approximated.
`state_point` explicitly identifies the state before terminal measurements (or
the end of a measurement-free unitary sequence). `P(|1>)` and dirty unmeasured
qubits describe that state, not the state after measurement/reset. This can catch
uncompute errors, but is not a full QIR verifier or a Result-slot aliasing oracle.

### Discovery

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py samples /tmp/compiler-review-kit/REVIEW/review.json \
  --output /tmp/compiler-review-kit/REVIEW/sample-candidates.json
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py issues /tmp/compiler-review-kit/REVIEW/review.json \
  --output /tmp/compiler-review-kit/REVIEW/issues.json
```

Sample discovery uses the pinned candidate checkout (`--revision reference` selects
the other endpoint). It finds Q# files/projects and conservatively infers
zero-argument entries. Inspect the candidates before execution. Ambiguous/skipped
entries appear in the adjacent `.discovery.json`, not as successes. Entries using
implicit file namespaces may need correction. Notebook/OpenQASM/Python scenarios
are selected manually.

Issue discovery snapshots issue bodies and paginated comments, never evaluates or
executes Markdown. Author sweeps include closed issues. Explicit includes override
author/number filters. Commit issue references are added; PR references are recorded
separately. Related issues mentioned only in PR discussions require reviewer
discovery. No author selector means no author sweep; use `issues --all` (below) for
whole-repository overlap searches.

### Attribution and reduction

For a PR stack, put intermediate PR heads in `layers` and compare those columns
to identify introducing/fixing PRs. Record which PR each layer represents. Do not
search individual commits by default.

Only if the user requests individual-commit attribution:

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py attribute /tmp/compiler-review-kit/REVIEW/review.json \
  /tmp/compiler-review-kit/REVIEW/findings.json --case CASE_ID \
  --output /tmp/compiler-review-kit/REVIEW/attribution.json
```

Attribution evaluates every commit in the range, not a presumed-monotonic binary
search. Results include parents. Neighboring topologically ordered rows are not
necessarily parent/child: confirm the responsible transition on actual parents.
For rebased/non-ancestral comparisons use an explicit ordered `--refs REF ...`.
Failures abort with partial progress saved; an unbuilt commit is not a good commit.

Source reduction is independent of attribution and remains part of finding
minimization by default:

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py reduce /tmp/compiler-review-kit/REVIEW/review.json \
  /tmp/compiler-review-kit/REVIEW/findings.json --case CASE_ID --profile Adaptive \
  --kind panic --needle 'failure signature' \
  --output /tmp/compiler-review-kit/REVIEW/reduced.json
```

Reduction deletes lines while preserving a specified candidate failure and, by
default, a semantically validated reference. `--allow-preexisting` removes that
reference requirement. Panic/rejection reduction requires a signature; wrong-result
reduction requires a deterministic oracle. It is deliberately simple, bounded by
`--attempts` (default 100), and records exhaustion. It does not establish that the
root cause survived minimization. Projects must first be reduced manually to
standalone Q#.
Both reducers share a compile-first predicate for panic/rejection/crash cases.
Nonmatching variants do not run the simulator or build the reference. Only a
matching failure triggers the simulator/reference checks needed to preserve a
validated regression. Wrong-result reduction still runs the full semantic path.

`reduce-adapter` (same options) reduces Python-adapter cases: it alternates
simplifying `prepare()` (replace a call by one of its call/list arguments, drop a
list element) with line reduction of module-level Q# source strings, and exports
the reduced adapter and tested project/dependency snapshot as a portable bundle
beside the output manifest. When line deletion stalls because every
remaining line matters, probe targeted variants instead (literal versus constant
expression, functor sets, one versus two captures) with `exec`, and report the
variant table: it often explains the trigger better than a smaller program.

## Case manifests

See `cases.schema.json` and `examples/cases.json`. Every case needs a unique ID and
an explicit oracle. Paths are relative to the case manifest.

```json
[
  {
    "id": "measure-one",
    "source_file": "repro.qs",
    "entry": "Main()",
    "oracle": {
      "kind": "deterministic",
      "expected": { "type": "result", "value": 1 }
    },
    "issues": [1234]
  }
]
```

Cases can supply inline `source`, `source_file`, `project_root`, or a trusted
self-contained Python `adapter`; project/source/adapter may be combined where
appropriate. Without an adapter, `entry` is required. An adapter defines
`prepare(ctx) -> (entry_or_callable, args)` and is invoked independently for each
context. Build all Q# callables/UDTs in that context; do not return objects from
another context. Keep local helper code in the adapter itself. Installed client
dependencies must be present in the configured Python environment; missing dependencies are infrastructure
failures, not compiler diagnostics.

Project snapshots preserve separate local dependency packages. Supported project
layout is `qsharp.json` plus `src/`, with local dependencies declared using `path`.
Source symlinks are rejected explicitly. Other layouts need an adapter/manual
bundle. GitHub dependencies remain remote; pin them to immutable revisions and
document them. QDK GitHub library dependencies use the build's own checkout through
`QDK_LIB_OVERRIDE_MICROSOFT_QDK`; that intentional override is recorded in each build.
Do not describe a manually filtered client project as full-project compatibility.

Oracle policies:

- `deterministic`: the reviewer asserts deterministic _observable return values_.
  An optional hand-derived typed `expected` is strongest. Otherwise both endpoint
  simulators must agree; a sampled singleton is not how determinism is established.
- `observe`: collect complete shots and frequencies without declaring equivalence
  or a miscompile. Use statistical/state checks manually where required.
- `compile_only`: capability/compilation coverage, not a correctness oracle.

Typed values distinguish `result`, `bool`, `int`, `double`, `array`, `tuple`,
`unit`, and (simulator-only) `string`. Doubles use Python's canonical `float.hex()`
spelling. These are serialization details, not a request to investigate Q# string
literals, signed zero, NaN, or infinities. Pure `Unit` return values
never establish semantic equivalence. The kit does not compare messages, quantum
states, or side effects as an oracle.

## Build integrity and storage

Linux builds use `cargo build --locked -p qdk --lib`, with `--release` where selected,
and the configured Python interpreter as `PYO3_PYTHON`. Each overlay combines that
revision's Python package with its resulting `libqdk.so`. This fast path targets the
current QDK layout/ABI; unsupported historical layouts fail explicitly.

Build cache reuse verifies tool/dependency metadata, package/library digests,
worktree SHA and non-ignored modifications; workers verify actual import locations and
the native digest. Compilation failures stop the build, never reuse a stale library.
Ordinary runs never reuse result rows or patch QIR. Only explicit issue-draft
cache reuse uses recorded, matching execution identities.

All revisions share one Cargo target directory so third-party crates compile once.
Cargo names workspace-crate artifacts by workspace-relative path and judges freshness
by source mtimes, so a worktree checked out before another revision was built could
silently link that revision's crates (observed as `E0061` argument-count errors).
Every build therefore refreshes its worktree's timestamps first and disables
incremental compilation (`CARGO_INCREMENTAL=0`).

The configured `build_dir` includes Git-registered worktrees. Do not
recursively delete it while those registrations remain. After inspecting ownership,
remove only specific kit worktrees with `git worktree remove PATH`, then their
specific build/run artifacts. Never remove another review's scratch/worktrees.

Keep configuration examples, prompts, small curated repros, generators/seeds, and
compact final evidence. Do not check in native binaries, bulk generated cases, or
all QIR. Cold builds need dependencies and disk space, but no saved corpus.

### Evidence cache and rechecks

Scratch directories disappear regularly, and with them a review's manifests,
adapters and scripts. The evidence cache keeps a small copy outside them. It defaults to
`/work/cache/compiler-review-kit/<name>/`,
or `$REVIEW_KIT_CACHE`, or `cache_dir`. **The cache may also be wiped at any time:**
everything in it is either regenerable or recoverable from a report's embedded repro
scripts. It only makes the next review's recheck quick and exact. Never put it in a
deliverables directory.

```bash
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py save-evidence /tmp/compiler-review-kit/REVIEW/review.json \
  --findings findings.json client-cases.json --runs RUN [RUN ...] \
  --files repros/ variants.py gen/generator.json fir-tests.json
/opt/qdk-venv/bin/python tools/compiler-review-kit/review.py recheck /tmp/compiler-review-kit/REVIEW/review.json [--name PREVIOUS] [--report REPORT.md]
```

`save-evidence` bundles case manifests with their adapters and projects (one copy per
shared project), the non-quiet cases of each run with a compact summary, and extra
files. A saved run is exported from its frozen snapshot even if the original
inputs/configuration are gone. JSON over 1 MiB in extra files is gzipped.
`report` saves the run's non-quiet cases automatically using that run's pinned
configuration. `cache_budget_mb` is a best-effort target: recognized review
directories are pruned oldest first, but the current review is never deleted.
If that review alone exceeds the budget, other reviews are retained rather than
evicted pointlessly. Every over-budget save emits a warning to compact the review
or raise the budget. Unrecognized entries are not deleted and can also prevent
the cache from reaching its target.

`recheck` reruns a cached review: its cases at the configured roles, printed as a
`table`, and its repro scripts at each revision. The last printed `REPRODUCED` or
`NOT REPRODUCED` verdict takes precedence over the exit code, including for older
scripts using the opposite exit convention. Without a verdict, exit 0 means
not reproduced; other exits are reported without claiming reproduction or a fix.
If the cached review is gone, `recheck` extracts and runs the
repro scripts embedded in `--report` (default: the configured report), which is how
earlier findings should be rechecked at the start of an updated review.

## Validate the kit

```bash
/opt/qdk-venv/bin/python -m unittest discover -s tools/compiler-review-kit/tests -v
/opt/qdk-venv/bin/python tools/compiler-review-kit/tests/smoke.py --config /tmp/compiler-review-kit/REVIEW/review.json
```

The bundled smoke config compares `HEAD^` with `HEAD`; this tests plumbing, not
arbitrary branch correctness. Supply an owned config with the revisions you
intend to validate if those defaults are unsuitable.
Fixtures cover the default profiles, typed/nested finite numeric outputs, stochastic
observations, callable arguments, and separate local project dependencies.
Only when debug is requested, `tests/smoke.py --mode debug` exercises debug builds.
The smoke test asserts the expected
profile/status matrix and exact shot counts, rather than accepting exit zero alone.
`statecheck` covers only its explicitly supported premeasurement subset; no
chemistry-specific trace interpreter is included.

The repository's `.prettierignore` excludes the legacy
`tools/compiler-review-kit/.work/`. With the sandbox mapping, generated reports
and case manifests are outside the repository instead.
