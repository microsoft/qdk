# QDK compiler review kit

Instructions and executable helpers for an agent reviewing QDK compiler changes
for panics, crashes, miscompiles, logical errors, and fixes to existing bugs. The
agent investigates the changes and produces an evidence-backed report; the kit
is not an automatic correctness proof or a substitute for reading code.

Run it inside the QDK Copilot sandbox, not directly on the host. A review writes
and executes arbitrary Python repros, adapters, and instrumentation. The kit's
worker processes isolate crashes, not untrusted code; they do not replace the
sandbox's filesystem, credential, and network protections.

## Prerequisites

- A running QDK Copilot sandbox with the kit and the revisions to compare in its
  working checkout at `/work/qdk`.
- Git, the revisions' Rust toolchain, and the sandbox's preinstalled Python
  environment at `/opt/qdk-venv`, including the QDK Python dependencies and
  `qirrunner` package's `qir-runner` executable.
- Authenticated `gh` access if the review includes GitHub issue snapshots.
- Sufficient space in the sandbox's `/work` volume for native builds. Missing
  dependencies must be supplied in the sandbox image; package feeds are blocked
  during sandbox sessions.

Inspect repros and adapters before executing them, even inside the sandbox. The
kit does not install dependencies, change the existing QDK installation, edit
production sources, or post to GitHub.

## Sample prompt

> Read `tools/compiler-review-kit/reviewer.md` and review `my-feature` against `origin/main`.
> Write the final report to `/exchange/from_copilot/review.md`.

Replace the revisions and report path with your own. You can also specify PRs,
client projects, or other scope constraints in the prompt. The agent creates and
pins its review configuration; you do not need to fill out a JSON file.
