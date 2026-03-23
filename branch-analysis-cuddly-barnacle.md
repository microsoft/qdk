# Local Branch Analysis — 2026-03-18

**User:** Mine Starks (16928427+minestarks@users.noreply.github.com)
**Branches analyzed:** 85 `minestarks/` branches (3 skipped with 0 commits ahead)
**Branches with unique work:** ~20
**Already landed in main:** 53 (deleted)
**Remaining branches:** ~32

## Legend

| Symbol             | Meaning                                                |
| ------------------ | ------------------------------------------------------ |
| **Status**         |                                                        |
| LOCAL ONLY         | Branch exists only locally — no remote backup anywhere |
| Pushed             | Branch exists on `origin` (may or may not match local) |
| **Merge w/ main**  |                                                        |
| ✅ Clean           | Merges with current `origin/main` without conflicts    |
| ⚠️ N conflicts     | Merge produces N conflicted files                      |
| ↑ Up to date       | Branch already includes latest `origin/main`           |
| **Recommendation** |                                                        |
| Toss               | Work fully landed in main; zero unique code remains    |
| Finish             | Has unique value; could become a PR with effort        |
| Keep (exploration) | WIP/experimental work worth preserving                 |
| Evaluate           | Needs human decision                                   |
| Push ASAP          | LOCAL ONLY branch with significant unreplicated work   |

---

## Branches With Unique Remaining Value

These branches contain code that has **not** fully landed in `origin/main`. Sorted by priority.

| #   | Branch                               | Commits | Last Date      | Local Only?      | Merge w/ main   | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Recommendation                     |
| --- | ------------------------------------ | ------- | -------------- | ---------------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- |
| 1   | `minestarks/restore-command`         | 2       | **2026-03-18** | Yes              | ↑ Up to date    | Restores the `qsharp-vscode.updateCopilotInstructions` command registration and context menu entry in `package.json` that was accidentally dropped in a recent refactor. Single file, +8 lines.                                                                                                                                                                                                                                                                | **Finish**                         |
| 2   | `minestarks/circuit-folding`         | 227     | 2025-11-14     | No (pushed)      | ⚠️ 76 conflicts | **Master circuit feature branch.** RIR→circuit transformation with conditional rendering, scope grouping, loop/repetition collapsing, variable-argument gates, PyQIR circuit builder, expand/collapse UI, circuit snapshot tests. Contains `NOTES.md` inventory. Several sub-features extracted and merged (#2761, #2942, #2943, #2944, #2993) but top-level scope grouping, loop collapsing, and unified builder remain unmerged. Active WIP with known bugs. | **Keep (exploration)**             |
| 3   | `minestarks/circuit-source-links-v2` | 13      | 2025-11-14     | No (diverged:10) | Not tested      | Second iteration of source links with CR feedback fixes and logic bug corrections. Core feature (#2761) landed; incremental fixes may still be relevant.                                                                                                                                                                                                                                                                                                       | **Evaluate**                       |
| 4   | `minestarks/circuit-snapshot-tests`  | 9       | 2025-10-27     | **Yes**          | ⚠️ 25 conflicts | HTML-based snapshot testing infrastructure for circuit visualization. Test runner, `.snapshot.html` expected outputs, renderer fixes. 42 files, +1574/−189. Evolved approach may have landed via #2743 but this specific framework is unique.                                                                                                                                                                                                                  | **Push ASAP / Evaluate**           |
| 5   | `minestarks/sized-array`             | 85      | 2025-10-07     | No (pushed)      | ⚠️ 49 conflicts | Despite the branch name, this is primarily an advanced circuit visualization branch. Adds RIR debug metadata, loop detection, conditional rendering, source code links, symbolic gate arguments, PyQIR-based circuit building, and extensive configuration. Contains `NOTES.md` tracking work items. Significant overlap with `circuit-folding` — appears to be a parallel/earlier iteration of the same work.                                                 | **Keep (exploration)**             |
| 6   | `minestarks/deep-references`         | 5       | 2025-08-15     | **Yes**          | ⚠️ 3 conflicts  | Extends Definition/References/Hover/Rename to resolve through import/export re-exports ("deep references"). Refactors `name_locator.rs` with `Ident` parameter for alias tracking. 670+ lines of new reference tests. Base PR #2641 landed; this branch has additional commits with deeper reference resolution.                                                                                                                                               | **Push ASAP / Finish**             |
| 7   | `minestarks/imports`                 | 3       | 2025-08-06     | **Yes**          | ⚠️ 17 conflicts | Major import/export resolution revamp: introduces `ImportKind` enum (`Wildcard`/`Direct`), extracts `resolve/imports.rs` module, renames `Kind::Term` → `Kind::Callable`, removes redundant legacy exports. 42 files, +3457/−2024.                                                                                                                                                                                                                             | **Push ASAP / Evaluate**           |
| 8   | `minestarks/completions-reexports`   | 2       | 2025-08-01     | No (diverged:3)  | ⚠️ 23 conflicts | Large feature for re-exported item completions. Modifies parser import/export handling, refactors completions engine, adds extensive tests. 48 files, +4621/−2116. Goes beyond what PRs #2638 and #2640 merged.                                                                                                                                                                                                                                                | **Keep (exploration)**             |
| 9   | `minestarks/mine-profiles`           | 14      | 2025-07-30     | No (diverged:1)  | ⚠️ 36 conflicts | QIR profile selection via `@EntryPoint` attribute and manifest. Plumbs profile detection through WASM/npm/VS Code. Original landed (#2591) but was reverted (#2623), redesigned (#2636). This branch diverges from what landed.                                                                                                                                                                                                                                | **Evaluate**                       |
| 10  | `minestarks/test-cases-only`         | 1       | 2025-07-14     | **Yes**          | ⚠️ 5 conflicts  | Documents known re-export bugs via failing test cases. Includes `NOTES.md` and `new_test_cases_analysis.md`. 15 files, +1162/−2. No implementation fixes — pure test scaffolding.                                                                                                                                                                                                                                                                              | **Push ASAP / Keep (exploration)** |
| 11  | `minestarks/vscode-openqasm-qshar`   | 24      | 2025-05-09     | No (pushed)      | ⚠️ 67 conflicts | Unifies Q# and OpenQASM language support in VS Code. Adds `ProjectType` enum, OpenQASM TextMate grammar, OpenQASM debugger tests, refactors project system to handle both languages. 73 files changed, +3530/−1689 lines. Significant refactoring of WASM, project system, and extension layers.                                                                                                                                                               | **Keep (exploration)**             |
| 12  | `minestarks/github-errors`           | 2       | 2025-02-06     | **Yes**          | ⚠️ 2 conflicts  | Refines diagnostic suppression for `qsharp-github-source:` URI scheme at compilation level (vs document level). 2 files, +61/−6. Distinct approach from related work in main.                                                                                                                                                                                                                                                                                  | **Push ASAP / Evaluate**           |
| 13  | `minestarks/doc-not-in-files`        | 2       | 2025-01-24     | **Yes**          | ⚠️ 6 conflicts  | Fixes LS panic when Q# file not in `qsharp.json` `files` list. Adds `DocumentNotInProject` error variant. Refactors `FileSystemAsync` trait. 10 files, +659/−1119. No evidence of landing in main.                                                                                                                                                                                                                                                             | **Push ASAP / Finish**             |
| 14  | `minestarks/symbolic-circuit-args`   | 2       | 2025-01-09     | **Yes**          | ⚠️ 10 conflicts | Displays symbolic parameter names (θ, θ/2) in circuit diagrams instead of concrete values. Extends FIR to track symbolic expressions. 17 files, +667/−153. Not in main.                                                                                                                                                                                                                                                                                        | **Push ASAP / Evaluate**           |
| 15  | `minestarks/dont-unwrap-value`       | 16      | 2024-12-12     | No (pushed)      | ⚠️ 9 conflicts  | Exposes user-defined Q# callables to Python. Adds `package_globals()`, Python wrapper classes, 200+ test lines. Feature concept landed via #2940 but this specific implementation may differ.                                                                                                                                                                                                                                                                  | **Evaluate**                       |
| 16  | `minestarks/whitespace`              | 7       | 2024-03-21     | **Yes**          | ⚠️ 4 conflicts  | Enhances Q# formatter with proper indentation logic: delimiter newline context, type parameter state machine, `FormatterState` struct. 4 files, +552/−95. Contains debug artifacts.                                                                                                                                                                                                                                                                            | **Push ASAP / Keep (exploration)** |

---

## Branches to Evaluate (mixed status)

These branches have partial overlap with main, exploratory nature, or are personal tooling.

| #   | Branch                                  | Commits | Last Date  | Description                                                                                                                                   | Recommendation                 |
| --- | --------------------------------------- | ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| 1   | `minestarks/package-lock-npm-11`        | 1       | 2025-11-18 | npm 11 lockfile regeneration. Not merged. Pushed + up-to-date.                                                                                | Evaluate                       |
| 2   | `minestarks/maybe-allow-bigger-cycles`  | 3       | 2025-08-12 | Extends RCA for multi-operation call cycles. Base landed (#2654); extra cycle work is unique. LOCAL ONLY.                                     | Keep (exploration)             |
| 3   | `minestarks/imports-revamp`             | 2       | 2025-08-02 | Earlier exploratory version of `imports` branch. Superseded by `imports`. LOCAL ONLY.                                                         | Evaluate (may be superseded)   |
| 4   | `minestarks/ui-tests-2`                 | 1       | 2025-05-01 | Circuit UI tests (Playwright approach). Prototype that informed #2743. Pushed.                                                                | Keep (exploration)             |
| 5   | `minestarks/ui-tests`                   | 1       | 2025-04-24 | Circuit UI tests (jsdom approach). Superseded by `ui-tests-2` (Playwright). LOCAL ONLY.                                                       | Toss                           |
| 6   | `minestarks/playground-attempt-2`       | 1       | 2025-02-11 | Playground layout rework (attempt 2 after revert of attempt 1). Not landed. Pushed.                                                           | Evaluate                       |
| 7   | `minestarks/reexport-investigate`       | 6       | 2025-01-31 | Investigation of re-export resolution bugs. `#[ignore]` tests, exposed `GlobalScope`. Pushed + up-to-date.                                    | Keep (exploration)             |
| 8   | `minestarks/jupyterlab-faster-build`    | 3       | 2025-01-28 | WIP build optimization for JupyterLab. Not landed. LOCAL ONLY.                                                                                | Keep (exploration)             |
| 9   | `minestarks/playground-flex`            | 1       | 2025-01-09 | Playground layout (attempt 1). Landed then reverted (#2098 → #2117). LOCAL ONLY.                                                              | Toss (superseded by attempt-2) |
| 10  | `minestarks/upgrade-eslint`             | 2       | 2025-01-07 | ESLint ecosystem upgrade. Unclear if landed. LOCAL ONLY.                                                                                      | Evaluate                       |
| 11  | `minestarks/python-interop-refactor`    | 10      | 2024-12-10 | Q# callable → Python prototype. Feature landed via #2940 (different implementation). Pushed.                                                  | Toss (superseded)              |
| 12  | `minestarks/python-fun`                 | 1       | 2024-10-31 | Pyodide POC for playground. Quick spike, no follow-up. LOCAL ONLY.                                                                            | Toss                           |
| 13  | `minestarks/main`                       | 13      | 2024-10-18 | Personal working branch — circuit viz, LS improvements, TODO.md. 86 files. Mostly landed. LOCAL ONLY.                                         | Toss                           |
| 14  | `minestarks/reexport-completions`       | 9       | 2024-10-11 | Major completions overhaul — modular architecture, PathKind, extensive tests. Conceptually landed but codebase diverged. Pushed + up-to-date. | Keep (exploration)             |
| 15  | `minestarks/memtest-baseline`           | 21      | 2024-10-08 | Detailed memory profiling of compiler pipeline. `AstCounter` visitor, allocation tracking per compilation stage. Not in main. LOCAL ONLY.     | Keep (exploration)             |
| 16  | `minestarks/memtest-branch`             | 26      | 2024-10-08 | Combined memtest + member-completions for A/B memory impact measurement. LOCAL ONLY.                                                          | Keep (exploration)             |
| 17  | `minestarks/ls-batch-updates-attempt`   | 1       | 2024-09-26 | Explicitly labeled "promising but failing attempt" at LS update batching. LOCAL ONLY.                                                         | Keep (exploration)             |
| 18  | `minestarks/project-deps`               | 4       | 2024-06-06 | Early dependency support iteration. Partially landed via #1663. LOCAL ONLY.                                                                   | Toss                           |
| 19  | `minestarks/pending-background-work`    | 1       | 2024-05-30 | LS waiting mechanism for pending compilations. Approach may be superseded. LOCAL ONLY.                                                        | Keep (exploration)             |
| 20  | `minestarks/extension-publish-personal` | 11      | 2024-03-13 | Personal tooling — modifies extension metadata for personal publishing. LOCAL ONLY.                                                           | Evaluate (personal utility)    |

---

## Branch Relationship Map

### Circuit Visualization Family

```
minestarks/circuit-viz (prototype, toss)
├── minestarks/circuit-viz-build (internalize lib, toss)
├── minestarks/circuit-source-links (landed #2761, toss)
│   ├── minestarks/circuit-source-links-v2 (CR feedback, evaluate)
│   └── minestarks/circuit-collapse (superseded, toss)
│       ├── minestarks/circuit-folding ★ (master WIP branch, KEEP)
│       └── minestarks/circuit-disable-tracing-vscode-run (landed, toss)
├── minestarks/circuit-disable-tracing (landed, toss)
│   └── minestarks/circuit-disable-tracing-python (landed, toss)
├── minestarks/circuit-snapshot-tests (unique test infra, evaluate)
├── minestarks/sized-array (parallel circuit WIP, keep)
├── minestarks/symbolic-circuit-args (unique feature, push ASAP)
├── minestarks/zoom-circuit (landed, toss)
└── minestarks/resizing-fix (landed, toss)
```

### Import/Export Family

```
minestarks/importer-exporter (landed, toss)
├── minestarks/importer-exporter-2 (landed, toss)
├── minestarks/merge-scopes (landed, toss)
├── minestarks/imports-revamp (superseded, evaluate)
├── minestarks/imports ★ (active WIP, PUSH ASAP)
├── minestarks/completions-reexports ★ (WIP, keep)
│   └── minestarks/completions-reexports-first-draft (superseded, toss)
└── minestarks/reexport-completions (exploration, keep)
    └── minestarks/reexport-investigate (exploration, keep)
```

### Copilot/AI Family

```
minestarks/copilot (shipped+removed, toss)
├── minestarks/copilot-vscode (shipped+removed, toss)
│   └── minestarks/copilot-vscode-bells (shipped+removed, toss)
├── minestarks/copilot-instructions-prompt (landed, toss)
└── minestarks/remove-quantum-copilot (landed, toss)
```

---

## Priority Actions

### 🔴 1. Push to Remote IMMEDIATELY (LOCAL ONLY with significant unique work)

These branches have substantial, irreplaceable work that exists ONLY on the local machine:

| Priority | Branch                              | Commits | Risk Level                                           |
| -------- | ----------------------------------- | ------- | ---------------------------------------------------- |
| CRITICAL | `minestarks/imports`                | 3       | Active import/export resolution revamp — recent work |
| HIGH     | `minestarks/deep-references`        | 5       | Extended reference resolution, 670+ test lines       |
| HIGH     | `minestarks/doc-not-in-files`       | 2       | LS crash fix for files not in project                |
| HIGH     | `minestarks/circuit-snapshot-tests` | 9       | Test infrastructure, 42 files                        |
| MEDIUM   | `minestarks/symbolic-circuit-args`  | 2       | Symbolic parameter display in circuits               |
| MEDIUM   | `minestarks/whitespace`             | 7       | Formatter indentation logic                          |
| MEDIUM   | `minestarks/test-cases-only`        | 1       | Documented re-export test cases                      |
| MEDIUM   | `minestarks/github-errors`          | 2       | GitHub URI error suppression                         |
| LOW      | `minestarks/memtest-baseline`       | 21      | Memory profiling instrumentation                     |
| LOW      | `minestarks/memtest-branch`         | 26      | Memory profiling + completions A/B                   |

### 🟡 2. Branches Ready to Become PRs (with rebase effort)

| Branch                        | Effort | Notes                                                      |
| ----------------------------- | ------ | ---------------------------------------------------------- |
| `minestarks/restore-command`  | Low    | Already up-to-date with main. 1 file, +8 lines. Ready now. |
| `minestarks/github-errors`    | Medium | 2 conflicts, small focused fix.                            |
| `minestarks/doc-not-in-files` | Medium | 6 conflicts, includes file relocations.                    |
| `minestarks/deep-references`  | Medium | 3 conflicts in LS, well-tested.                            |

### 🟢 3. Branches Needing Decisions

| Branch                             | Question                                                                                                           |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `minestarks/circuit-folding`       | Is this still the primary circuit development branch? Should sub-features continue to be extracted as smaller PRs? |
| `minestarks/sized-array`           | Same circuit work as `circuit-folding` — which is canonical? Should one be archived?                               |
| `minestarks/imports`               | Recent import revamp work — is this the next direction for the compiler?                                           |
| `minestarks/completions-reexports` | Overlaps with `imports` — which branch is the canonical path forward?                                              |
| `minestarks/mine-profiles`         | Diverges from landed profile work (#2636) — is the alternative approach still relevant?                            |
| `minestarks/playground-attempt-2`  | Playground layout was reverted — should attempt 2 be revisited?                                                    |
| `minestarks/package-lock-npm-11`   | Still needed for npm 11 compatibility?                                                                             |

### 🧹 4. Cleanup Summary

- **53 branches** deleted (work fully in main) ✅
- **~16 branches** have unique value (keep/push/evaluate)
- **~20 branches** in mixed/evaluate status
- **1 branch pair** is a direct ancestor
  - `imports-revamp` → `imports` (evolution)
