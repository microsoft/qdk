# QDK compiler review kit

Reusable instructions and executable helpers for reviewing groups of QDK commits
for panics, crashes, miscompiles, logical errors, and untagged fixes. This is a
review kit, not an automatic correctness proof or a substitute for reading code.

## Supported environment

Linux; Git and the revision's Rust toolchain; a repository-local `.venv` containing
Python 3.10+, the QDK Python dependencies, and the `qirrunner` package's `qir-runner`
executable. Issue snapshots additionally require authenticated `gh`.

The controller uses only the Python standard library. It does not install tools,
alter the existing QDK installation, post to GitHub, commit changes, or modify
production sources. Workers use matching native/Python overlays in disposable
processes. This is crash isolation, **not a sandbox**: execute only inspected,
trusted adapters and Q# programs.

## Start a review

1. Copy `review.example.json` to `review.json` in this directory. Set `reference`
   and `candidate`. Remove optional selectors/exclusions you do not want.
2. Ask your reviewing assistant:

   > Read `review-kit/reviewer.md` and conduct the review configured in
   > `review-kit/review.json`. Use the kit's helpers and report template.

3. The reviewer validates the environment and scope, discovers and curates cases,
   runs the differential matrix, investigates findings, and writes the report.

A skill/custom-agent wrapper is optional. The kit works as ordinary files/scripts.
Do not add this entire procedure to global instructions for unrelated coding work.

## Configuration

Only two fields are required:

```json
{
  "reference": "upstream/main",
  "candidate": "HEAD"
}
```

The configuration's containing Git checkout is the repository. All supplied paths
are relative to the configuration file. `repository` is an optional escape hatch
for configurations stored outside that checkout. The default runtime is
`<repository>/.venv`; the default work directory is `review-kit/.work`.

| Optional field                   | Meaning/default                                                                                    |
| -------------------------------- | -------------------------------------------------------------------------------------------------- |
| `previously_reviewed`            | Previous review's candidate: built and run as an extra column to find lost fixes; not the baseline |
| `historical_reference`           | Additional explicitly labeled baseline                                                             |
| `layers`                         | Intermediate heads of a PR stack, oldest first; run as columns `layer1`, `layer2`, ...             |
| `issues`                         | Author/minimum-number sweep plus explicit issue includes; both open and closed issues              |
| `clients`                        | Named local Git checkouts to investigate; no chemistry checkout is assumed                         |
| `exclusions`                     | Case ID/origin/path globs, each with a reason                                                      |
| `report`                         | Output path; defaults to `review.md` beside the config                                             |
| `profiles`                       | Base, Adaptive_RI, Adaptive_RIF, Adaptive                                                          |
| `debug_validation`               | `targeted` by default; `all` runs both modes, `none` opts out                                      |
| `keep_qir`                       | `false`: retain failure/uncertain QIR, discard uncomplicated successful QIR; `true` retains all    |
| `limits`                         | Stage timeouts, memory, concurrency, shots, seed                                                   |
| `name`                           | Review name for the evidence cache; defaults to the report file name without extension             |
| `cache_dir`, `cache_budget_mb`   | Evidence cache (see below); `$REVIEW_KIT_CACHE` or `<repository>/../cache/review-kit`; 50 MiB      |
| `venv`, `work_dir`, `repository` | Optional directory overrides; no individual tool-path settings                                     |

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

Run from the repository root, using its `.venv`:

```bash
.venv/bin/python review-kit/review.py check review-kit/review.json
.venv/bin/python review-kit/review.py build review-kit/review.json
.venv/bin/python review-kit/review.py run review-kit/review.json review-kit/examples/cases.json
```

`run` prints its new run directory. It builds missing revisions automatically.
Every configured role is a column: `reference` and `candidate`, plus
`historical_reference`, `previously_reviewed` and each `layerN` when configured.
`run --roles` narrows the extra columns (`--roles` alone runs only the two endpoints).
Results never resume from an old JSONL file: every run is fresh. A run contains
resolved configuration/tools/build identities, case snapshots, stage results,
raw output, and retained unmodified QIR. The main checkout is never switched.

Release is the primary mode. With `debug_validation=targeted`, the reviewer must
assemble a findings manifest and explicitly replay it:

```bash
.venv/bin/python review-kit/review.py run review-kit/review.json review-kit/.work/findings.json --mode debug
```

`debug_validation=all` runs release and debug for every selected case.

Generate an **evidence draft** from a run, then complete it using the report
template. The generated draft does not establish root causes or commit attribution.

```bash
.venv/bin/python review-kit/review.py report review-kit/.work/runs/RUN_DIRECTORY
```

Existing reports are protected; `--overwrite` is only for an owned generated draft.
Helper outputs explicitly named with `--output` can be replaced; choose owned
artifact paths, never an unrelated existing file.

`run` prints `[completed/total]` as cases finish, so a slow case never makes a run
look stalled; `results.jsonl` is rewritten in manifest order when the run ends.

### Summaries and run-to-run comparison

```bash
.venv/bin/python review-kit/review.py summarize review-kit/.work/runs/RUN [--json summary.json]
.venv/bin/python review-kit/review.py compare review-kit/.work/runs/OLD review-kit/.work/runs/NEW
.venv/bin/python review-kit/review.py compare OLD NEW --select 'a=validated,b!=validated' \
  --output review-kit/.work/lost-fixes.json
.venv/bin/python review-kit/review.py subset A.json B.json --match 'i38*' --ids extra-case \
  --output review-kit/.work/subset.json
```

`summarize` cross-tabulates reference→candidate outcomes per profile, groups
candidate failures by normalized signature (numbers replaced by `N`) with their
reference outcomes, lists flagged regressions, and checks the **profile hierarchy**:
a program validated on a lower profile (Base ⊂ Adaptive_RI ⊂ Adaptive_RIF ⊂ Adaptive)
should not fail on a higher one. Violations new at the candidate are listed, and all
candidate violations are grouped by the higher profile's failure signature. It also lists:

- **loud → silent**: the reference panicked, crashed or rejected the program and the
  candidate returns a wrong result. Not a regression verdict, but a bug that stopped
  announcing itself; report it explicitly.
- **lost fixes** against `previously_reviewed`, and **intermediate-layer hazards**: a
  `layerN` that breaks a program both endpoints get right (or silences a loud
  reference failure). Stacked PRs merge one at a time, so these ship if a layer lands alone.
- **the candidate's own extracted test programs that fail end to end**: their cargo
  tests pass, so the test helpers skip part of the user pipeline.
- **identical QIR** at both endpoints, which is the only evidence for observe-only cases.
- **compile-time outliers**: per case and profile, candidate/reference compile-stage
  time of at least 2x and +0.25 s. Sweeps run concurrently, so confirm with `timing`.

Compile-time medians come from concurrent sweeps and are only a hint.

`compare` diffs two runs case by case (candidate outcomes by default; `--side
reference` compares reference columns). Use it after every PR update: rerun the
same manifests and compare, or compare a previous candidate's run with the new one
to find **lost fixes**. `--select` terms (`a=STATUS`, `b!=STATUS`, ...) must all
hold for one shared profile; `--output` copies the selected cases' original entries
from run B's manifest. A change from a loud failure to a wrong result is labelled
`worse (silent)`. `subset` merges manifests by ID or glob and rewrites
`source_file`, `project_root` and `adapter` paths for the new location. An ID that
appears again with the same content (provenance fields aside) is merged; the same ID
with different content is an error.

```bash
.venv/bin/python review-kit/review.py table RUN [RUN ...] [--profiles Adaptive_RIF Adaptive] \
  [--cases 'i38*'] [--only-diff]
```

`table` prints, for each case, one line per revision with the simulator values and
each profile's values (wrong ones in `**bold**`), or the failure kind and message.

### Timing

```bash
.venv/bin/python review-kit/review.py timing review-kit/review.json CASES.json \
  [--roles reference candidate] [--profiles Adaptive_RIF] [--repeats 5] [--cases-match 'unary*']
```

Each revision's overlay times context creation, loading (`eval`/adapter) and
`compile` separately, in one process per sweep. Sweeps are interleaved and rotated
across repeats so no revision always runs first or under a load spike. The output
gives per-case medians and paired ratios against the first revision, plus outliers
(`--outlier`, default 1.5x). Use it for client entry points and for every
`summarize` outlier before reporting a performance change.

### Variant matrices

```bash
.venv/bin/python review-kit/review.py variants review-kit/review.json VARIANTS.py [--roles ...|--refs ...]
```

A trusted variants file defines `VARIANTS`: a list of dictionaries with `name`,
`expected` (as printed, for example `1`, `[1, 0]`, `true`) and either `source`, or
`body` appended to an optional `PRELUDE`, with an `entry` string or a `build(ctx)`
callable returning `(entry, args)` for the callable API. Optional `PROFILES` and
`PROJECT` (files of a temporary Q# project; sources are evaluated afterwards, as a
separate compilation unit). Every variant runs in the simulator and on each profile
(compile + qir-runner) at each revision. The matrix marks wrong results and lists
the variants whose outcome differs between revisions. Use it to narrow a finding to
the one ingredient that matters (see `review-kit/examples/variants.py`).

### Executing standalone repros on each revision

```bash
.venv/bin/python review-kit/review.py exec review-kit/review.json repro.py \
  [--roles reference candidate | --refs SHA1 SHA2] [--mode debug] [--args ...]
```

`exec` runs a self-contained Python script with each revision's pinned overlay
(no `.venv` package swapping), keeps full stdout/stderr per revision under
`<work_dir>/exec`, and reports the exit status, final Python exception, and every
native panic with its `file:line`, message, and the enclosing Rust `fn` resolved
at that revision. Use it for the final inline repros and for variant tables.

### Extracting test programs

```bash
.venv/bin/python review-kit/review.py extract-tests review-kit/review.json \
  --crate qsc_fir_transforms --revision candidate --output review-kit/.work/fir-tests.json
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
.venv/bin/python review-kit/review.py new-diagnostics review-kit/review.json [--json found.json]
```

This lists diagnostic codes, `#[error]` messages and panic/assert/expect sites that
the reference→candidate diff adds to non-test Rust code, with their candidate
locations, plus removed codes. Every new rejection is a place to write valid programs
at its boundary: the reviewed PRs added `UnsupportedRecursiveInlining`, which also
rejected valid static-depth recursion. Every removed code needs a check that the
condition is now handled.

### Drafting issues

```bash
.venv/bin/python review-kit/review.py draft-issues review-kit/review.json ISSUES.py \
  --output review-kit/.work/issues.md [--revisions reference candidate] [--only SLUG] [--cached]
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
See `issue-template.md` for the format.

### Generators and oracle tools

```bash
.venv/bin/python review-kit/generators/callable_chains.py review-kit/.work/gen \
  --inline 1600 --layout 800 --python 400 --seed 2 --profiles Adaptive_RI Adaptive_RIF Adaptive
.venv/bin/python review-kit/review.py oracle-check review-kit/review.json review-kit/.work/gen/cases.json
.venv/bin/python review-kit/review.py run review-kit/review.json review-kit/.work/gen/cases.json
.venv/bin/python review-kit/review.py stages review-kit/.work/fir-tests.json review-kit/.work/gen/cases.json
.venv/bin/python review-kit/review.py statecheck RUN/CASE/candidate/Base.ll
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
.venv/bin/python review-kit/generators/dynamic_recursion.py review-kit/.work/rec \
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
replays straight-line (Base) QIR in a small statevector and reports per-qubit
`P(|1>)` and unmeasured qubits not returned to `|0>` — effects a returned value
cannot show (for example, a body applied where an uncompute needed an adjoint).

### Discovery

```bash
.venv/bin/python review-kit/review.py samples review-kit/review.json \
  --output review-kit/.work/sample-candidates.json
.venv/bin/python review-kit/review.py issues review-kit/review.json \
  --output review-kit/.work/issues.json
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

```bash
.venv/bin/python review-kit/review.py attribute review-kit/review.json \
  review-kit/.work/findings.json --case CASE_ID \
  --output review-kit/.work/attribution.json
.venv/bin/python review-kit/review.py reduce review-kit/review.json \
  review-kit/.work/findings.json --case CASE_ID --profile Adaptive \
  --kind panic --needle 'failure signature' \
  --output review-kit/.work/reduced.json
```

Attribution evaluates every commit in the range, not a presumed-monotonic binary
search. Results include parents. Neighboring topologically ordered rows are not
necessarily parent/child: confirm the responsible transition on actual parents.
For rebased/non-ancestral comparisons use an explicit ordered `--refs REF ...`.
Failures abort with partial progress saved; an unbuilt commit is not a good commit.

Reduction deletes lines while preserving a specified candidate failure and, by
default, a semantically validated reference. `--allow-preexisting` removes that
reference requirement. Panic/rejection reduction requires a signature; wrong-result
reduction requires a deterministic oracle. It is deliberately simple, bounded by
`--attempts` (default 100), and records exhaustion. It does not establish that the
root cause survived minimization. Projects must first be reduced manually to
standalone Q#.

`reduce-adapter` (same options) reduces Python-adapter cases: it alternates
simplifying `prepare()` (replace a call by one of its call/list arguments, drop a
list element) with line reduction of module-level Q# source strings, and writes the
reduced adapter beside the output manifest. When line deletion stalls because every
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
dependencies must be present in `.venv`; missing dependencies are infrastructure
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
`unit`, and (simulator-only) `string`. Doubles use Python's canonical `float.hex()` spelling (including signed
zero); NaN payload distinctions are not represented. Pure `Unit` return values
never establish semantic equivalence. The kit does not compare messages, quantum
states, or side effects as an oracle.

## Build integrity and storage

Linux builds use `cargo build --locked -p qdk --lib`, with `--release` where selected,
and the configured `.venv` interpreter as `PYO3_PYTHON`. Each overlay combines that
revision's Python package with its resulting `libqdk.so`. This fast path targets the
current QDK layout/ABI; unsupported historical layouts fail explicitly.

Build cache reuse verifies tool/dependency metadata, package/library digests,
worktree SHA and non-ignored modifications; workers verify actual import locations and
the native digest. Compilation failures stop the build, never reuse a stale library.
No result cache or silent QIR patch is used.

All revisions share one Cargo target directory so third-party crates compile once.
Cargo names workspace-crate artifacts by workspace-relative path and judges freshness
by source mtimes, so a worktree checked out before another revision was built could
silently link that revision's crates (observed as `E0061` argument-count errors).
Every build therefore refreshes its worktree's timestamps first and disables
incremental compilation (`CARGO_INCREMENTAL=0`).

`.work` is ignored and disposable, but includes Git-registered worktrees. Do not
recursively delete it while those registrations remain. After inspecting ownership,
remove only specific kit worktrees with `git worktree remove PATH`, then their
specific build/run artifacts. Never remove another review's scratch/worktrees.

Keep configuration examples, prompts, small curated repros, generators/seeds, and
compact final evidence. Do not check in native binaries, bulk generated cases, or
all QIR. Cold builds need dependencies and disk space, but no saved corpus.

### Evidence cache and rechecks

`.work` disappears regularly, and with it a review's manifests, adapters and scripts.
The evidence cache keeps a small copy outside it. It defaults to
`<repository>/../cache/review-kit/<name>/` (for example `/work/cache/review-kit/...`),
or `$REVIEW_KIT_CACHE`, or `cache_dir`. **The cache may also be wiped at any time:**
everything in it is either regenerable or recoverable from a report's embedded repro
scripts. It only makes the next review's recheck quick and exact. Never put it in a
deliverables directory.

```bash
.venv/bin/python review-kit/review.py save-evidence review-kit/review.json \
  --findings findings.json client-cases.json --runs RUN [RUN ...] \
  --files repros/ variants.py gen/generator.json fir-tests.json
.venv/bin/python review-kit/review.py recheck review-kit/review.json [--name PREVIOUS] [--report REPORT.md]
```

`save-evidence` bundles case manifests with their adapters and projects (one copy per
shared project), the non-quiet cases of each run with a compact summary, and extra
files. JSON over 1 MiB is gzipped. `report` saves the run's non-quiet cases
automatically. The cache's total size is kept under `cache_budget_mb` by deleting
whole reviews, oldest first; the current review is never deleted.

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
.venv/bin/python -m unittest discover -s review-kit/tests -v
.venv/bin/python review-kit/tests/smoke.py
```

The smoke config compares `HEAD^` with `HEAD`; this tests plumbing, not arbitrary
branch correctness. Override those refs for revisions you intend to validate.
Fixtures cover all profiles, typed/nested outputs, signed zero, stochastic
observations, callable arguments, and separate local project dependencies.
`tests/smoke.py --mode debug` exercises debug builds. It asserts the expected
profile/status matrix and exact shot counts, rather than accepting exit zero alone.
`statecheck` covers straight-line QIR only; no chemistry-specific trace interpreter
is included.

The repository's `.prettierignore` excludes `review-kit/.work/`: generated reports
and case manifests there are not formatted source.
