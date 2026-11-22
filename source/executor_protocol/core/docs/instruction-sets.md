# Instruction sets, their fit and their lowering

For those who write an instruction set, or a lowering from one set to
another: what the protocol asks of a set, when it asks it, and how a
lowering runs one set's instructions on another. The other terms are those
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
  capabilities that arrive at the receiver; and why the set refuses an
  instruction.

## What a set states, and when the protocol reads it

| Item                                | What it says                                                                     | Read                                                                                          |
| ----------------------------------- | -------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| The set's type                      | which parts can be joined: a sender's set type must be its receiver's            | by the compiler, at `through` and `on`                                                        |
| `fits` (from `BoundaryCompatible`)  | whether a sender's set value fits a receiver's, giving the binding or a mismatch | by each join; inside a part of the user's, at each run's `ready`                              |
| `Capabilities`, with its own `fits` | the set's own capabilities, required above and provided below                    | by the `on` that makes a runner, and at each run's start                                      |
| `unconstrained`                     | the capabilities that state nothing                                              | by a part's default `provides`, and by the runner as the set's part of a run's requirement    |
| `readout_slots`                     | the readout slots one instruction writes                                         | by a sender, which sums them into a batch's stated slots; by a lowering, to find each readout |
| `requirement`                       | what a checked batch's instructions are checked against                          | once per run, at each boundary whose receiver accepts the set                                 |
| `check`                             | an instruction's readout slots, or the set's fault                               | for each instruction of a checked batch, before the batch runs                                |

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

## Lowering one set to another

Where a sender's set is not its receiver's, a
[`Lowering`](../src/lowering.rs) runs one on the other: it rewrites each
instruction of its `From` set as instructions of its `To` set, and reads
the instruction's readouts back from theirs.
[`Lowered`](../src/lowering/lowered.rs) is the adapter that runs it in a
chain, placed explicitly, as
`driver.through(Lowered::new(lowering)).on(executor)`. A lowering states
only what is particular to it:

| Method               | What the lowering says                                      | Why                                                                                                                                                    |
| -------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `accepts`, `sends`   | the set it lowers, and the set it lowers it to              | the joins fit each against the part beside it, as for any adapter                                                                                      |
| `requires`           | what it requires below, given what arrives from above       | a rewriting may need more of the set's capabilities below, such as ancilla qubits                                                                      |
| `bind`               | keeps the binding of its `To` set into the receiver's       | `lower` writes each instruction as the receiver encodes it                                                                                             |
| `lower`              | appends the instructions that run one instruction below     | one instruction at a time, so `Lowered` keeps each one's place among those it sends below                                                              |
| `read` (has default) | writes an instruction's readouts from those of its lowering | the default copies them one for one, and panics, naming the lowering, if they differ in number; a lowering that reads a majority of three overrides it |

`Lowered` keeps the protocol's rules for the lowering: each batch it sends
below is checked if the batch it lowers was, with its readout slots
stated; each shot's conditions pass up unchanged; and its memory is reused
from one `execute` to the next.
