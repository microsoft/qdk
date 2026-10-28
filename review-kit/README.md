# QDK compiler review kit

Instructions and executable helpers for an agent reviewing QDK compiler changes
for panics, crashes, miscompiles, logical errors, and fixes to existing bugs. The
agent investigates the changes and produces an evidence-backed report; the kit
is not an automatic correctness proof or a substitute for reading code.

## Prerequisites

- Linux and a local QDK Git checkout containing the revisions to compare.
- Git and the revisions' Rust toolchain.
- A repository-local `.venv` containing Python 3.10+, the QDK Python dependencies,
  and the `qirrunner` package's `qir-runner` executable.
- Authenticated `gh` access if the review includes GitHub issue snapshots.
- Disk space and dependency access for native builds.

Workers isolate crashes, not untrusted code: only inspected, trusted repros and
adapters should be executed. The kit does not install dependencies, change the
existing QDK installation, edit production sources, or post to GitHub.

## Sample prompt

> Read `review-kit/reviewer.md` and review `my-feature` against `upstream/main`.
> Write the final report to `/exchange/outbox/review.md`.

Replace the revisions and report path with your own. You can also specify PRs,
client projects, or other scope constraints in the prompt. The agent creates and
pins its review configuration; you do not need to fill out a JSON file.
