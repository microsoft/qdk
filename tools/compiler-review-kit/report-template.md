# Compiler review: `<candidate>` versus `<reference>`

## Summary

Reference: `<full SHA>`  
Candidate: `<full SHA>`  
Historical baseline: `<full SHA or none>`  
Previously reviewed / rebase mapping: `<details or none>`

Follow the [defaults and opt-ins](reviewer.md#defaults-and-opt-ins): include
opt-in results, columns, and sections only when the user requested them, and
attribute stack findings to individual PRs.

| Finding      | Classification                      | Profiles     | Responsible PR / requested commit        | Related issue              | Client impact           |
| ------------ | ----------------------------------- | ------------ | ---------------------------------------- | -------------------------- | ----------------------- |
| `<ID/title>` | `<regression/fixed/non-regression>` | `<profiles>` | `<PR / requested commit / not narrowed>` | `<issue/new trigger/none>` | `<client and evidence>` |

## Changes since the previous review

Only for updated reviews; omit otherwise. Name the candidates (first, previous,
current) and keep that terminology throughout.

- New or changed commits (from `git range-diff`); stacks with identical trees.
- Rerun results versus the previous candidate (`review.py compare`): unchanged
  suites, new fixes, new regressions against the reference.
- Status of earlier unresolved findings: resolved, unchanged, or changed shape, from
  `review.py recheck` (cached cases and scripts, or the previous report's repros).

For a follow-up PR reviewed on top of the candidate, put a dated update section
here instead: inputs and merge commit, what it restores/fixes/regresses, and the
evidence. Leave the earlier findings below intact.

## 1. Regressions

### R1. `<specific behavior>`

**Expected / observed:** `<precise results or failure>`  
**Affected profiles/builds:** `<matrix>`  
**Introduced by:** `<responsible PR for stacks; commit only if requested; otherwise not narrowed>`

**Related issues:** `<links and relationship>`  
**Client impact:** `<demonstrated effect or explicitly untested>`  
**Cause/confidence:** `<mechanism, locations, uncertainty>`

```qsharp
// Replace with a minimal runnable program.
```

```python
# Replace with a standalone executed repro using the selected QDK build overlay.
# Derive qir-runner from the interpreter directory; retain raw exit/error output.
```

| Revision | Mode        | Profile     | Simulator    | Compilation  | Runner       | Outcome            |
| -------- | ----------- | ----------- | ------------ | ------------ | ------------ | ------------------ |
| `<SHA>`  | `<release>` | `<profile>` | `<observed>` | `<observed>` | `<observed>` | `<interpretation>` |

Observed output:

```text
<actual output, including failures and exit statuses>
```

Repeat this full structure for every unresolved issue, including performance
regressions confirmed with `review.py timing` (per-revision medians and ratios, and
the client entry points affected). Do not substitute a local absolute project path
for a self-contained repro.

### Merge-order hazards

Only for stacked PRs reviewed with `layers`: programs an intermediate layer breaks or
silently miscompiles that the full stack gets right (`summarize`'s intermediate-layer
hazards), the PR represented by that layer, the repairing PR, and the client impact
if that PR merges alone. Use the same PR-boundary evidence to attribute other
stack findings.

## 2. Fixed issues

For each: original repro, reference failure, candidate's validated correct
behavior, profiles/modes, fixing PR for stacks (commit only if requested), tested variants, remaining limitations,
and whether commits/PRs reference the issue. Separate partial fixes and acceptance
without semantic validation.

## 3. Non-regression issues

List loud → silent cases (`summarize`) first: a reference panic or rejection that
became a wrong result is a non-regression by verdict but needs the full structure.

Use the full unresolved-issue structure above. Show the reference failure; explain
whether this is the original trigger, a new trigger, or a demonstrated distinct
root cause. Include configured-client impact.

## 4. Lost fixes

Opt-in; omit this section otherwise.
This covers behavior an earlier candidate fixed that the current
candidate no longer fixes. These are not regressions against the reference. For
each: the earlier and current behavior, the affected programs/issues, and the
responsible PR established on the current stack (not inferred from which commit
was dropped).

## Issue status table

| Issue      | State at snapshot | Tested repro/variant | Profiles     | Reference  | Candidate  | Assessment                                                                      |
| ---------- | ----------------- | -------------------- | ------------ | ---------- | ---------- | ------------------------------------------------------------------------------- |
| `<number>` | `<open/closed>`   | `<case>`             | `<profiles>` | `<result>` | `<result>` | `<repro/fixed/partial/newly accepted/unsupported/no runnable repro/not tested>` |

## Coverage and limitations

- Run manifests, exact tool/dependency versions, build/import provenance.
- Native suites and end-to-end cases, source/API surfaces and profiles.
- Generated families, seeds, budgets and reduction results; how many chain stages
  programs combine (`review.py stages`) and whether deep compositions came only from
  the change's own tests.
- Profile-hierarchy violations (lower profile validates, higher profile fails) at
  both endpoints, new ones called out.
- Release coverage, plus any opt-in debug or Adaptive_RI coverage.
  Stochastic/Unit/state checks and oracle limitations.
- Full client projects versus explicitly named subsets; client revisions/edits.
- qdk-chemistry source revision and scenarios exercised on Adaptive_RIF/Adaptive,
  or unavailable coverage; distinguish source-derived repros from full-client validation.
- Exclusions with reasons, pending discovery, unsupported profiles, failed builds,
  infrastructure errors, timeouts, and incomplete attribution at the requested granularity.
- Final inline repro execution evidence.

Do not interpret “no observed regression” as exhaustive correctness.
