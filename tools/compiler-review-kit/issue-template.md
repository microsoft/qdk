# Issue drafting template

Use this when the user asks for issues to file. Draft only; never file issues or post
comments unless separately authorized. Generate the drafts with
`review.py draft-issues` (see
[agent-reference.md](agent-reference.md#drafting-issues)): it renders each repro from
`templates/issue_repro.py.template`, executes it at every revision, and fills in the
Outputs table, the messages and the panic links from those executions. Never type
outputs by hand.
Generated scripts embed frozen inputs and the harness's shared stage runtime;
they are self-contained and use the same strict output parser and stage limits.

Write one sub-issue per distinct trigger (a shared panic site is not enough to merge
two), separated by `---`, plus a prose summary for a meta-issue. For a trigger of an
issue that already exists, draft a comment for that issue instead of a new issue.
Draft non-regressions as well as regressions when asked, and say which is which.

## Header

State the revisions the tables were generated at (`main` and the candidate, with
SHAs and PR numbers), how each repro works (simulator, then compile per profile and
qir-runner), and that every Reproduce script exits 0 when the bug no longer
reproduces, and 1 when it reproduces.
If an affected row was not tested and no tested row reproduces, the script exits
2 and reports an inconclusive result.

## Meta-issue summary

A few paragraphs, no tables required:

- What was reviewed (PRs/commits, `main` SHA) and how the issues were found (suites,
  generators, client scenarios), including the client usage patterns that matter.
- Groups of issues with a likely shared cause, and the evidence for it (for example
  a common field in the panic messages).
- Which issues are regressions from the change under review and which also reproduce
  on `main`.
- Related issues already tracked elsewhere, and their status.

---

## [Miscompile|Panic|Bug|Regression] <area>: <short description of the trigger>

**Kind:** <panic in `crate` ([`path:line`](link), in `function`) / wrong result / internal error> on <profiles>; the simulator is <correct>.  
**Affects:** <profiles, and which variants do or don't reproduce>  
**Regression:** <no, or yes: correct revision; responsible PR for stacks; individual commit only if requested>

**Found in:** <suite, generator and seed, client scenario; how reduced>  
**Related:** #<n> (<how it overlaps; related is not duplicate>), ...

<What the program does and why the expected result is what it is. A small variant
table when the trigger was narrowed (`review.py variants`).>

## Q#

```qsharp
// the minimal program; project files and later `ctx.eval` fragments shown separately
```

<How the argument is built, when the repro goes through the Python callable API.>

## Outputs

10 shots each. The simulator runs the program directly; each profile compiles it to
QIR, which is run with qir-runner. Wrong outputs and panics are in bold.

| Run          | `main` (`<sha>`)                    | <candidate> (`<sha>`) |
| ------------ | ----------------------------------- | --------------------- |
| Simulator    | `<value>`                           | `<value>`             |
| Base         | `<value>` / **panics: `<message>`** | ...                   |
| Adaptive_RIF | ...                                 | ...                   |
| Adaptive     | ...                                 | ...                   |
| **Correct**  | `<expected>`                        | `<expected>`          |

Add an Adaptive_RI row only if the user requested that profile. Use release builds
unless the user requested debug.

Messages:

```text
<revision>: <full first line of each panic or relevant diagnostic>
```

## Correct output

<Hand-derived expected value and the reasoning.>

## Probable cause

<What the QIR or variants show; say "not investigated" where that is the case.>

## Reproduce

```python
"""<title>

<what the script does>
Exits 0 when the bug no longer reproduces.
Reproduces when <rows> return something other than <expected>.

Linux only. Uses qdk and qir-runner from the running interpreter's environment.
"""
# Generated from templates/issue_repro.py.template: builds the program (or callable-API
# argument), runs the simulator and each profile's QIR through qir-runner, prints the rows
# and "REPRODUCED"/"NOT REPRODUCED", and exits 0 when the bug no longer reproduces.
```

---

## Comment for #<n> (<existing issue title>)

**<What changed with the reviewed PRs, or the new trigger>:** a short table of the
issue's repro at `main` and the candidate, the responsible PR for a stack (commit
only if requested), newly affected
shapes or clients, and a minimal repro only when the trigger is new. Keep it to what
a maintainer of that issue needs.

---

For client-dependent repros, name the client repository and commit, keep every other
input user-defined, and prefer a client-independent reduction as the issue itself.
