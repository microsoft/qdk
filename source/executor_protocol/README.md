# `executor_protocol`

How a caller hands quantum work to an executor and reads its reply, the same
way whatever runs the work.

## What this crate does

It defines the protocol every component of an execution stack speaks, and
nothing that runs work. A simulator, an adapter that runs a program's
classical part, or a link to a remote device each implement it; the crates
that hold them depend on this one.

- A **message kind** (`Message`) fixes what a call means and which events its
  reply carries. Level L3 is a `Batch`: a list of instructions run in order on
  one shot, whose state carries over from the batches before it. Its events
  are `Outcome`s, a value under the tag the batch gave the readout.
- An **executor** (`Executor<M>`) accepts one message kind. A _terminal_ runs
  the message; an _adapter_ keeps its meaning and passes the work on to the
  executor it wraps, possibly in another instruction set. A chain of adapters
  is again an executor of the same kind, so a driver written once runs on
  every chain.
- A **call** returns at once, with a reply (`Completion`) or a typed refusal
  (`Rejected`). A refusal means that nothing started, so the caller may
  correct the message and send it again; each refusal has a retry class.
  A run that starts and cannot finish ends its reply with `Status::Failed`.
- **Replies** are polled for their events. `Immediate` is the reply of an
  executor that ran the work inside the call; `wait` blocks the calling
  thread on any reply.
- An executor **describes** what it offers (`Describe`, `Capabilities`), so a
  stack can be checked before it runs, and **closes** (`Close`) with a report
  that folds with the reports of the components it wraps (`Fold`).
- **Codes** (`reason_code`, `need_code`, `value_code`, `retry_code`) give the
  fixed numbers by which a refusal, a need and a value cross a link.

## Example

The region ISA in `qdk_simulators::execution` is an L3 instruction set:

```text
driver ──Batch<PreparedAdaptiveProgram>──► RuntimeAdapter ──Batch<RegionInstruction>──► SimulatorTerminal
       ◄── Outcome per output record ─────                 ◄── Outcome per measurement ──
```

`RuntimeAdapter` runs a program's branches and loops itself, and sends the
terminal the regions, measurements and resets it reaches.
