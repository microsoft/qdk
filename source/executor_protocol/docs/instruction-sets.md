# Instruction sets

[Protocol overview](../README.md).

## Instruction sets: types and descriptions

An executor accepts the instructions of its message, `InstructionBatch<I>`. The
type `I` fixes the shape of an instruction; a description, an
`InstructionSet`, says which instructions the executor runs. The two are
independent, and a chain may mix them:

| `I`               | The shape of a call is checked           | Guide                  |
| ----------------- | ---------------------------------------- | ---------------------- |
| a Rust type       | by the compiler (`Op`, `Logical`)        | `tests/static_isa.rs`  |
| `InstructionCall` | by the executor, against its description | `tests/runtime_isa.rs` |

Either kind of executor may describe its set in
`Capabilities::instruction_set`, and then a chain checks it when it is
built and when a driver is composed with it (see [The checks](#the-checks)).

### A static set: a type

An `Executor<InstructionBatch<Logical>>` (the repetition code of
`tests/static_isa.rs`) accepts logical instructions and nothing else, and
composing upstream of it an adapter that sends instructions of another
type is a compile error, which no test pins. The type does not say which
instructions a caller several components upstream will need, nor whether
the downstream executor runs every instruction the gadgets send, so the
code describes both sets too, by hand, as the toy describes its own.
`RepetitionCode`, the code as an adapter, states them; `compose` checks
what the gadgets send against the set downstream, and the executor
composed states the logical set, not the downstream one:

```rust
fn sends(&self) -> Option<Arc<InstructionSet>> {
    Some(Arc::new(gadget_instructions()))
}

fn accepts(&self) -> Option<Arc<InstructionSet>> {
    Some(Arc::new(logical_set()))
}
```

A toy that lacks one of them is refused when the code is composed with it
(`the_code_is_built_only_against_an_executor_that_runs_its_gadgets`).

An adapter that changes the instruction set states the set it accepts
(or `None`), never the one downstream, which its caller cannot send. A driver
that needs an instruction the code lacks is refused when composed
(`a_driver_is_composed_only_with_a_code_that_has_the_instructions_it_needs`,
in `tests/static_isa.rs`).
A way for a type to give its own description, so that the two cannot
drift, is left until an executor needs one; until then a test matches
every variant to a call of its description
(`each_description_names_every_instruction_it_stands_for`).

### A run-time set: a description

When the set is known only at run time (read from a file, for instance a
Qodec instruction set, or sent by a remote device), the instructions are
`InstructionCall`s, and the executor describes the set it accepts. A
description follows Qodec's instruction sets: each instruction is named
by its mnemonic and takes, in order, block operands, then parameters, then
one tag per readout. The toy (`tests/toy/simulator.rs`) describes itself so:

```rust
pub fn toy_set() -> InstructionSet {
    use OperandKind::{Block, Tag};
    InstructionSet::new("toy")
        .with_action("coin", [Block], action("coin"))
        .with_action("x", [Block], action("x"))
        .with_action("cx", [Block, Block], action("cx"))
        .with_action("measure", [Block, Tag], action("measure"))
        .with_action("reset", [Block], action("reset"))
}
```

Each instruction declares what it does, its `Action` (below), so that a
set of another name, which names or numbers it otherwise, is matched to it.
A set's name is its identity: sets of one name are matched by mnemonic
([compatibility](#compatibility)), so name a set for its source (`qep`,
`c4c6`), not for its role in a chain (`device`, `sent`).

Each instruction has an opcode, its position in the order the set
declares it: here `coin` is 0 and `reset` 4. A call carries the opcode,
not the mnemonic, so an executor checks and dispatches it by an index; a
name is resolved once, when a program or an adapter is built. A log or a
debugger gets the names back with `set.display(&call)`, from
`a_call_is_displayed_with_its_mnemonic`, on the tests' `device()`, a set
whose `rz` takes a block and a real:

```rust
let set = device();
let rz = set
    .call("rz", [Operand::Block(2), Operand::Real(0.5)])
    .expect("`rz` takes a block and a real");
assert_eq!(set.display(&rz).to_string(), "rz(block 2, real 0.5)");
```

An opcode means something only in its set. A part of a set, or the same
instructions in another order, numbers them differently, so an adapter
resolves the names it sends against the set downstream, never against its own
(`a_part_of_a_set_numbers_its_instructions_in_its_own_order`). The run-time
guide's code (`Encoding`) does so when the chain is built, and each call
then goes down as its gadget with no name looked up.

An executor dispatches a call through an `OpcodeTable`, which it builds
from its set once: `set.table(|mnemonic| …)` holds the entry the closure
gives for each instruction at the instruction's opcode
(`a_table_holds_each_instructions_entry_at_its_opcode`), and is refused
with every mnemonic it gives none for, so that all are seen at once
(`a_table_reports_every_instruction_without_an_entry`).

Building or copying a call of up to three operands allocates no
memory, nor does an `ImmediateReply` of at most one event built with
`push`
(`a_call_of_up_to_inline_operands_allocates_nothing`,
`an_immediate_reply_of_at_most_one_event_allocates_nothing`;
[Calls and replies that allocate nothing](internals/instruction-sets.md#calls-and-replies-that-allocate-nothing)).

### What an instruction does

A description may also declare what each instruction does, its `Action`,
as Qodec declares it in an instruction's `in`, `out` and `action`: the
sizes of the blocks it takes and gives (a block of Stim's `qubit` type
encodes one qubit), then its steps in order, among `stabilize` (prepare
the state that Paulis stabilize), `clifford` (map Paulis to their images),
`pauli`, `observe` (measure Paulis, a readout each) and `rotate` (about a
Pauli, by a literal angle or one of the call's parameters). Stim's set in
Qodec, as the tests write it (`src/tests/instruction_set.rs`):

```rust
fn cx() -> Action {
    Action::new([1, 1], [1, 1]).clifford([
        (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
        (Pauli::z(1), Pauli::z(0) * Pauli::z(1)),
    ])
}
```

```rust
fn stim() -> InstructionSet {
    InstructionSet::new("stim")
        .with_action("R", [Block], reset())
        .with_action("M", [Block, Tag], measure())
        .with_action("CX", [Block, Block], cx())
        .with_action("X", [Block], x())
}
```

Two actions are the same when they are written the same, up to the order
of a Pauli's factors and of a Clifford's images
(`a_paulis_factors_compare_whatever_their_order`); steps in another order
differ (`actions_with_their_steps_in_another_order_differ`). The full
rule is in
[When two actions are the same](internals/instruction-sets.md#when-two-actions-are-the-same).
An instruction may declare no action (`InstructionSet::with`), as a
vendor's instruction may not; it is then matched only in a set of its
name ([compatibility](#compatibility)).

Known issue, open: nothing checks an action's blocks against the `Block`
operands of its instruction, so a description that disagrees with itself
is matched as written; a loader that reads both from one Qodec instruction
keeps them together
(`an_instruction_whose_action_and_operands_disagree_on_its_blocks_is_not_matched`,
ignored, in `src/tests/instruction_set.rs`).

### Compatibility

One set's instructions are matched in another's by
`InstructionSet::opcodes_in`, which gives each its opcode there
(`the_map_gives_each_instructions_opcode_in_the_other_set`). An
instruction is matched to one taking operands of the same kinds in the
same order:

- **By mnemonic, in a set of the same name**, with no action needed: a
  set's name is its identity
  (`a_set_without_actions_is_matched_by_mnemonic_in_a_set_of_its_name`).
- **By action, whatever its mnemonic, in a set of another name**, when
  both sets declare what it does: Stim's `R` is the toy's `reset`
  (`an_instruction_is_matched_by_action_under_another_mnemonic`).
- **Refused otherwise**: between sets of other names, an instruction
  whose action either set leaves undeclared
  (`an_instruction_whose_action_either_set_leaves_undeclared_is_refused`).

A mnemonic that both sets declare to do different things is refused as
`DifferentAction`, even where another instruction does the same
(`a_mnemonic_both_sets_declare_doing_different_things_is_refused_whatever_their_names`).
Every instruction not matched is reported, so that all can be fixed at
once (`every_instruction_not_matched_is_reported`).

One set fits another (`InstructionSet::fits`) when each of its
instructions is matched there. So:

- a part of a set fits the set (`a_part_of_a_set_fits_the_set`), and a
  set does not fit a part of itself
  (`a_set_does_not_fit_a_part_of_itself`);
- opcodes do not matter: two sets that declare the same instructions in
  another order fit each other
  (`sets_declaring_the_same_instructions_in_another_order_fit_each_other`);
- operands match by position: a `measure` taking a tag then a block does
  not fit one taking a block then a tag
  (`an_instruction_taking_its_operands_in_another_order_downstream_does_not_fit`).

Since a part fits the whole, a component states only the instructions it
uses, and composes with any component downstream that accepts them, in
any order:

- **An adapter** states the instructions it sends (`Adapter::sends`).
  `compose` checks them against the set downstream, and copies its calls
  into the downstream numbering where it numbers them otherwise
  (`calls_in_another_numbering_are_renumbered_on_their_way_down`).
- **A driver** states the instructions its program sends
  (`Needs::with_instructions`). A set the chain lacks is refused when
  composed, with every instruction that does not fit
  (`a_set_the_executor_lacks_is_refused_with_every_instruction_that_does_not_fit`).
  A driver of `InstructionCall`s whose set numbers its instructions
  otherwise runs its own instructions, each batch copied into the chain's
  numbering
  (`a_driver_whose_set_is_in_another_order_runs_its_own_instructions`). A
  typed driver's calls reach the executor as they are
  (`a_typed_driver_in_another_order_runs_its_instructions_as_they_are`).

A caller that calls an executor directly, without a run, sends calls in
the executor's numbering: the set the executor describes
(`Capabilities::instruction_set`), not the one its needs state.
`Capabilities::admit` checks that the executor accepts the caller's
instructions, in any order, not that they have the same opcodes, so a
call numbered by another set may reach another instruction, and when the
two take the same operands (`x` and `reset`), no check sees it.

Each way an instruction is matched or refused, what a boundary costs, and
what a driver of each instruction type gets are in
[Compatibility: the rules](internals/instruction-sets.md#compatibility-the-rules).

### The checks

A description is checked in three places, each before anything runs:

1. **When the chain is built**: what an adapter sends must fit the set
   downstream, and every fault of the parts built together is reported at
   once
   (`an_adapter_accepting_what_the_downstream_set_lacks_is_refused_when_built`,
   `every_build_fault_is_reported_at_once`); a part composed with the
   executor first is built on its own, before what is upstream of it
   ([Composing a run](composition.md#composing-a-run)).
2. **When a driver is composed, or by hand with `Capabilities::admit`**:
   the instructions a caller needs must fit the set the chain accepts,
   and their unmet need lists each that does not
   (`every_needed_instruction_not_offered_is_reported`,
   `a_driver_whose_needs_the_chain_lacks_is_refused_when_composed`).
   `compose` reports every unmet need of the driver
   (`every_need_of_a_driver_that_the_chain_lacks_is_reported`); `admit`
   returns only the first, in a fixed order where the instructions come
   after qubits, feedback, resets, replayable and probabilities, so an
   earlier unmet need hides the instructions'
   (`several_unmet_needs_are_reported_one_at_a_time_in_a_fixed_order`).
3. **At each call**, by an executor of `InstructionCall`s, since the
   compiler cannot: `InstructionSet::check_batch` refuses the batch with
   `Rejected::Instruction` at its first call whose opcode the set lacks or
   whose operands are of other kinds. The conformance check
   `conformance::refuses_calls_outside_its_set` checks that an executor
   keeps the rule
   (`the_device_and_the_code_refuse_calls_outside_their_sets`).

The errors of each check, its tests, and what checking each call costs
are in
[The checks: the rules](internals/instruction-sets.md#the-checks-the-rules).

## How applications use it

**A code, as Qodec gives it.** `tests/runtime_isa.rs` runs part of the
repetition code of the Python runtime's fixtures
([`repetition3.qodec.yaml`](../../qdk_package/tests/ec_tests/runtime/fixtures/repetition3.qodec.yaml))
as example code: evidence that the protocol meets the needs of a code's
adapter, not an adapter for use. What a code provides is plain data, a
`Code`, and the guide's adapter (`Encoding`, which makes an `Encoded`
on each shot, whose replies are `Decoded`) runs any `Code` upstream
of any executor that describes its set and its qubits (see
[the guide](guides.md#a-run-time-isa-a-code-given-as-data)). A Qodec
loader would fill it from a Qodec layer (Qodec's term; in a chain, an
adapter); it is planned, with no milestone yet, for the codes the Python
runtime loads from Qodec files, such as the repetition code below. Each
field would come from the layer so:

| `Code`       | From the Qodec layer                                                                                              |
| ------------ | ----------------------------------------------------------------------------------------------------------------- |
| `accepts`    | its instruction set: block operands, `number` and `integer` parameters, readouts (`observe` outcomes, then flags) |
| `sends`      | the instructions its gadget circuits use, in the downstream set, with their actions                               |
| `block_size` | the qubits of its block                                                                                           |
| `gadgets`    | each instruction's gadget: its circuit's calls, operands as slots of the call, and its readouts                   |

**What a Qodec loader needs.** The fixture's gadgets, as Qodec writes
them:

```yaml
idle.gadget.yaml:
  circuit:
    format: stim
    source: |-
      R 3 4
      CX 0 3 1 3
      CX 1 4 2 4
      M 3 4
  checks:
    - ["circuit.readouts[0]", "in[0].stabilizers[0]"]
    # …
m.gadget.yaml:
  circuit: { format: stim, source: "M 0 1 2" }
  # checks: …
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
rz.gadget.yaml:
  circuit:
    source: [rotate_z: [0, theta: theta]]
  parameter_bindings: { theta: circuit.source.theta }
```

1. **Reading the files.** Qodec's own crate (`qodec`, in
   [microsoft/qdk-ec](https://github.com/microsoft/qdk-ec)) reads a Qodec,
   as files or as one bundle; a loader would use it, and parse the gadget
   circuits, which are Stim text or lists of calls. The guide transcribes
   three of the fixture's instructions by hand, their Stim circuits
   written as calls.
2. **Matching the downstream set by action.** The physical set of this fixture
   names its instructions after Stim (`R`, `M`, `X`), not after the
   executor downstream (`reset`, `measure`, `x`). A loader would read each
   instruction's `in`, `out` and `action` into an `Action`
   (`InstructionSet::with_action`), so that the sets are matched by action;
   an instruction without one is refused ([compatibility](#compatibility);
   `an_instruction_the_code_sends_without_an_action_is_refused_when_built`).
   An instruction with `in` and `out` but no `action` (the fixture's
   `idle`) is the identity, as Qodec checks it, so a loader would declare
   `Action::new([1], [1])` for it. Parsing Qodec's Paulis and steps stays
   with the loader.
3. **Ancillas.** `idle` uses qubits 3 and 4, outside the block. A loader
   would name them as ancillas, placed after every block and shared by all
   gadgets, so that the qubits a code needs are one block and the most
   ancillas a gadget uses. A loader could also build only the gadgets of
   the instructions a user's program needs, so that the ancillas of those
   gadgets alone size the code (for example, a Steane code needs 19 qubits
   for all its gadgets, as its fault-tolerant idle uses 12 ancillas beside
   the block of 7, but 8 for a program that only prepares, applies `h` and
   measures). The guide's code has no ancillas: it transcribes no gadget
   that uses them.
4. **Parameters and several blocks.** `rz` passes its `theta` down to
   `rotate_z`, and the gadget of a logical gate on two blocks acts on
   both: a gadget's call would name a parameter of the logical call, or a
   qubit of any of its blocks, which the guide's `Slot` cannot (it names a
   qubit of the call's one block, or a circuit readout). Also missing: a
   constant argument of a gadget's call (as `M(0.01) 0`), an `OperandKind`
   for a `bit` parameter (a run-time input) and for a variadic block
   operand.
5. **Readouts and decoding.** A logical readout is the parity of the
   circuit readouts its gadget names, and one of them without a value
   leaves it without one
   (`a_circuit_readout_with_no_value_leaves_its_logical_readout_without_one`).
   Still missing: a logical readout that is a parity plus a constant bit,
   and a decoder, which reads the checks' readouts and tracks the Pauli
   frame (`in[0].z[0]` reads the frame, and counts as 0 in the guide)
   through the shot, seeing every readout of the shot.
6. **What would be refused when built.** Until each is supported, a
   loader would refuse it when the chain is built, naming the gadget, so
   that a code never runs wrongly: Pauli frame updates, feed-forward (steps
   guarded by a readout), gadgets that move a block, `REPEAT`, and the
   operands above. The guide's `Code` has no way to write them, and
   trusts its gadgets: a loader would check each against its instruction
   (the operands of its calls, its readouts) when it reads it.

The guide's example shows items 2 and 5, except what item 5 lists as
still missing; the decoder of item 5 would run with the code, not in it.

**Region execution of adaptive programs (planned),** for programs whose
measurements decide what runs next, such as the phase estimation of H₂.
None of it is built yet. A runtime (planned) would be an adapter that runs
the program's classical part itself and sends its quantum regions
downstream. The last executors of its chains would be
`Executor<InstructionBatch<RegionInstruction>>` (a planned type): the
regions' instructions would be a type, which the compiler would check the
runtime against, and an executor would refuse only qubits and results
beyond its own, so nothing would need describing. A description is for when
the gates a program uses (QIR's) and those an executor accepts can differ:
an executor of native gates, or one that simulates only Clifford gates,
would describe them, and the runtime would state the gates it sends
(`Adapter::sends`), which `compose` matches in the set described downstream
when the chain is built, so a missing gate would be refused then instead of
in the middle of the program. The executor downstream of the runtime would
keep its type: what it describes would be the gates inside its regions, not
the variants of `RegionInstruction`. What exists is `Rejected::at_origin`:
it renumbers a refusal from downstream so that it names the instruction the
adapter received, as the repetition code does
(`a_refusal_names_the_logical_instruction`).

**A device over a link (L1 and L0, planned, with no milestone yet).** A
remote device, such as one behind a cloud service, is reached over a link.
Its set is known only when it answers, so it describes it. The description
is a value (names and operand kinds) that a link frame would carry, as it
would carry `UnmetNeed`'s `mismatches`; `Need::Instructions` already has a
wire code (6).

The detailed rules, for maintainers of the crate, are in
[Instruction sets: the detailed rules](internals/instruction-sets.md).
