# Compiler review: `<candidate>` versus `<reference>`

## Summary

Reference: `<full SHA>`  
Candidate: `<full SHA>`  
Historical baseline: `<full SHA or none>`  
Previously reviewed / rebase mapping: `<details or none>`

| Finding      | Classification                      | Profiles     | Introducing/fixing commit | Related issue              | Client impact           |
| ------------ | ----------------------------------- | ------------ | ------------------------- | -------------------------- | ----------------------- |
| `<ID/title>` | `<regression/fixed/non-regression>` | `<profiles>` | `<SHA or unestablished>`  | `<issue/new trigger/none>` | `<client and evidence>` |

## Changes since the previous review

Only for updated reviews; omit otherwise. Name the candidates (first, previous,
current) and keep that terminology throughout.

- New or changed commits (from `git range-diff`); stacks with identical trees.
- Rerun results versus the previous candidate (`review.py compare`): unchanged
  suites, new fixes, new failures.
- Status of every earlier finding: resolved, unchanged, or changed shape, from
  `review.py recheck` (cached cases and scripts, or the previous report's repros).

For a follow-up PR reviewed on top of the candidate, put a dated update section
here instead: inputs and merge commit, what it restores/fixes/regresses, and the
evidence. Leave the earlier findings below intact.

## 1. Regressions

### R1. `<specific behavior>`

**Expected / observed:** `<precise results or failure>`  
**Affected profiles/builds:** `<matrix>`  
**Introduced by:** `<commit and parent evidence, or incomplete attribution>`  
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

| Revision | Mode              | Profile     | Simulator    | Compilation  | Runner       | Outcome            |
| -------- | ----------------- | ----------- | ------------ | ------------ | ------------ | ------------------ |
| `<SHA>`  | `<release/debug>` | `<profile>` | `<observed>` | `<observed>` | `<observed>` | `<interpretation>` |

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
hazards), the layer, the repairing layer, and the client impact if that layer merges
alone.

## 2. Fixed issues

For each: original repro, reference failure, candidate's validated correct
behavior, profiles/modes, fixing commit, tested variants, remaining limitations,
and whether commits/PRs reference the issue. Separate partial fixes and acceptance
without semantic validation.

## 3. Non-regression issues

List loud → silent cases (`summarize`) first: a reference panic or rejection that
became a wrong result is a non-regression by verdict but needs the full structure.

Use the full unresolved-issue structure above. Show the reference failure; explain
whether this is the original trigger, a new trigger, or a demonstrated distinct
root cause. Include configured-client impact.

## 4. Lost fixes

Only for updated reviews: behavior an earlier candidate fixed that the current
candidate no longer fixes. These are not regressions against the reference. For
each: the earlier and current behavior, the affected programs/issues, and the
commit responsible established by bisecting the current stack (not inferred from
which commit was dropped).

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
- Release/debug coverage; stochastic/Unit/state checks and oracle limitations.
- Full client projects versus explicitly named subsets; client revisions/edits.
- Exclusions with reasons, pending discovery, unsupported profiles, failed builds,
  infrastructure errors, timeouts, and incomplete attribution.
- Final inline repro execution evidence.

Do not interpret “no observed regression” as exhaustive correctness.
