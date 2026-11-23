# Instruction sets and their fit

For those who write an instruction set, or a part that runs one set's
instructions on another: what the protocol asks of a set, when it asks
it, and how one set's instructions run on another. The other terms are those
of the [README](../README.md#terms).

## Terms

The protocol carries any set's instructions without reading them; it asks
the set the few things it checks:

- **Instruction set** ([`InstructionSet`](../src/instruction_set.rs)): a
  type whose value states the instructions a part sends or accepts, and
  which says what the protocol asks of them. Each part names the set as an
  associated type (`Sends`, `Accepts`).
- **Instruction**: one instruction of the set, as a batch holds it (the
  set's `Instruction` type). The protocol never builds or reads one: it
  hands it to the set.
- **Binding** ([`Binding`](../src/instruction_set.rs)): what a sender gets
  when its set fits the receiver's, such as the receiver's encoding of
  each instruction it sends; `()` where both share one encoding.
- **The set's own capabilities**: what a part requires, or provides, of the
  set beside the resources and features every chain states, such as the
  qubits a shot holds (the set's `Capabilities` type); they flow down a
  chain with them, as [Capabilities and their fit](capabilities.md) says.
- **Requirement** and **fault**: what a checked batch's instructions are
  checked against on a run, prepared once per run from the set's
  capabilities that arrive at the receiver (and once at each join, from
  those that state nothing); and why the set refuses an instruction.

## What a set states, and when the protocol reads it

| Item                                | What it says                                                                     | Read                                                                                        |
| ----------------------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| The set's type                      | which parts can be joined: a sender's set type must be its receiver's            | by the compiler, at `through` and `on`                                                      |
| `fits` (from `BoundaryCompatible`)  | whether a sender's set value fits a receiver's, giving the binding or a mismatch | by each join; inside a part of the user's, at each run's `ready`                            |
| `Capabilities`, with its own `fits` | the set's own capabilities, required above and provided below                    | by the `on` that makes a runner, and at each run's start                                    |
| `unconstrained`                     | the capabilities that state nothing                                              | by a part's default `provides`, and by a driver's default `requires` as its set's part      |
| `readout_slots`                     | the readout slots one instruction writes                                         | by a sender, which sums them into a batch's stated slots                                    |
| `requirement`                       | what a checked batch's instructions are checked against                          | by each join, from `unconstrained`'s; then once per run, at each boundary accepting the set |
| `check`                             | an instruction's readout slots, or the set's fault                               | for each instruction of a checked batch, before the batch runs                              |

A set's mismatch and fault are its own typed errors; the protocol's shared
errors ([`BoundaryMismatch`](../src/composition/composition_faults.rs),
[`CapabilitiesMismatch`](../src/capabilities.rs) and
[`BatchFault`](../src/capabilities/boundary/batch_limits.rs)) hold them
boxed, so a caller reads them with `downcast_ref`. The small set
`CoinGates` of [`examples/compose_and_run.rs`](../examples/compose_and_run.rs)
implements the trait in full.

## The fit

Two parts join only where the sender's set type is the receiver's, which
the compiler checks. The join then calls `fits` on the two set values:
where they fit, the sender gets the binding through its `bind`, before any
run, and sends its instructions as the binding says. Otherwise the `on`
that makes a runner reports the set's mismatch, with those of the
capabilities, as [Capabilities and their fit](capabilities.md) says; at a
boundary inside a part of the user's, which the `on` cannot see into,
every run is refused with it instead.

## A checked batch's instructions

An unchecked batch runs as sent, with only its count of instructions
checked. A checked batch is checked in full at each boundary before it
runs: each instruction by the receiver's `check`, against the requirement
prepared at the run's start; then the readout slots `check` returned,
summed, against those its sender stated. The first fault refuses the
batch, at its position, as the
[`BatchFault`](../src/capabilities/boundary/batch_limits.rs) says.

## Running one set on another

Where a sender's set is not its receiver's, a
[`TranslatingAdapter`](../src/roles/translating_adapter.rs) between them
writes the instructions of one as instructions of the other, and reads
their readouts back: a lowering is one. A facility to write one
instruction by instruction, as a decomposition of each gate into native
ones would be, is planned.
