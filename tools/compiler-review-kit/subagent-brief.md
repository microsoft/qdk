# Bounded reviewer brief template

The lead fills every placeholder before delegating. Do not launch another agent
for the same objective or make agents share mutable case/output directories.

## Assignment

- Reference SHA: `<full SHA>`
- Candidate SHA: `<full SHA>`
- Historical baseline, if relevant: `<full SHA or none>`
- Assigned mechanism/files: `<specific bounded area>`
- Objective and stopping condition: `<concrete questions and finite test budget>`
- Known findings and issue candidates: `<context; not conclusions to assume>`
- Owned case/output directory: `<relative kit-owned directory>`
- Pinned run config and build locations: `<paths supplied by lead>`
- Profiles/build modes and exclusions: `<release; Base/Adaptive_RIF/Adaptive unless user requested otherwise; exclusions with reasons>`
- Attribution scope: `<responsible PR for stacks; individual commits only if user requested>`
- Permissions: `<normally no production edits, commits, GitHub writes, or nesting>`

## Procedure

Read the relevant sections of [reviewer.md](reviewer.md) for review policy and
[agent-reference.md](agent-reference.md) for commands, case manifests, build
integrity, and storage requirements. Use the pinned configuration supplied by the lead; do not
create a replacement configuration or change the overall review scope. Use
the [sandbox paths](agent-reference.md#sandbox-paths)
and your assigned directories; do not write working files into the repository
or execute repros outside the sandbox. Use
`review.py run` and shared immutable build overlays. Build fixtures in your context;
do not reuse Q# callable objects across contexts. Start analysis with
`review.py summarize`, run standalone repros with `review.py exec`, and reduce
Python callable-API cases with `review.py reduce-adapter`.

Read the assigned production changes and adjacent tests/callers. Derive runnable
cases and bounded variants from the changed invariant. Inspect issue repros before
execution. Do not assume an old corpus exists or silently guess entry expressions.
For resource, loop, callable, aggregate, or capture changes, use the
[Result/Qubit handle scenarios](reviewer.md#result-and-qubit-handle-aliasing) to
derive bounded exploratory cases within your assignment, not just known repros.

Justify deterministic oracles; keep stochastic observations inconclusive until
validated independently. Pure Unit outputs and QIR text differences are not
semantic evidence. Check simulator/compiler/runner stages separately. Distinguish
native panics, signals, runner/parser failures, diagnostics, and timeouts.

Minimize findings, preserve reference behavior, and test the assigned profiles in
release mode. Only build debug or test Adaptive_RI when the lead's brief records
the user's request. Narrow stack findings to PRs, not individual commits unless
requested. Matching panic sites do not prove duplicate
root causes. Report new triggers even if the underlying issue is known.
Do not investigate lost fixes unless the brief records the user's request.
Do not add string-literal, signed-zero, NaN, or infinity probe families.

Communicate scope changes/blockers to the lead rather than expanding to unrelated
subsystems. Do not re-investigate another reviewer's assigned area. Do not modify
the lead's config/report or another reviewer's worktree.

## Return

For each finding provide:

- Suggested classification and confidence, with evidence rather than labels alone.
- Minimal Q# and standalone Python; explicit expected and observed behavior.
- SHA/profile/build-mode matrix and raw stage results.
- Issue relationship: original repro, new trigger, suspected duplicate, or new bug.
- Suspected root cause and supporting source locations.
- PR attribution evidence for stacks; intermediate PR heads still needing checks.
  Include individual-commit attribution only when requested.
- Configured-client impact: demonstrated, likely with rationale, or untested.

Also return tested hypotheses that did not yield findings, exact coverage and
exclusions, generators/seeds, and limitations. “No finding” means no finding in
this bounded investigation, not proof of correctness.
