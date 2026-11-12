# The MPS simulator: design notes

The [README](README.md) gives the phases of a run, what each does and
why. Each file's header gives the rules of its part. This page adds what
neither holds: the methods of `QirMachine` and `MpsEngine` that each phase
calls, in order.

## The calls of each phase

Each band of the diagram is one phase, with its calls in order; lower and
fuse call no `MpsEngine` method:

```mermaid
sequenceDiagram
  participant M as QirMachine
  participant R as MpsResumer
  participant E as MpsEngine
  Note over M,E: Phase: start
  R->>M: program(), start_shot()
  Note over M,E: Phase: resume
  R->>M: resume(context, outcomes)
  M-->>R: a batch of calls, or the end with the records
  Note over M,E: Phase: lower, then fuse
  Note over M,E: Phase: prepare, at the group's first operator or draw
  R->>E: prepare(capacity)
  Note over M,E: Phase: apply
  R->>E: apply(state, sites, matrix)
  Note over M,E: Phase: read bond, where a run of operators ends
  R->>E: shape(state)
  Note over M,E: Phase: draw
  R->>E: probability_of_one(state, site)
  Note over M,E: Phase: split, when the shots drew both values
  R->>E: fork(state)
  R->>E: project(fork, site, the other value)
  R->>E: apply(fork, site, X), for a reset whose fork keeps One
  Note over M,E: Phase: settle
  R->>E: project(state, site, value)
  R->>E: apply(state, site, X), for a reset that drew One
  Note over M,E: Phase: end
  R->>E: retained_weight(state)
```
