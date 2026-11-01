# Instruction sets: the detailed rules

[Protocol overview](../../README.md).

This page is for maintainers of the crate, and for those who need a rule
at its edge: calls and replies that allocate nothing; when two actions
are the same; compatibility in full, with each way an instruction is
matched or refused, how a boundary renumbers calls, and what a driver of
each instruction type gets; and the three checks in full, with their
errors, the conformance kit's check of calls, and what checking costs. A
user describing or matching sets needs none of it to start, and starts
at [Instruction sets](../instruction-sets.md).

## Calls and replies that allocate nothing

A call holds up to `Operands::INLINE` operands (three, the most any of
QIR's instructions takes) in itself, and more on the heap
(`a_call_keeps_any_number_of_operands`), so building or copying a call of
up to three allocates no memory
(`a_call_of_up_to_inline_operands_allocates_nothing`). An `ImmediateReply`
built event by event (`ImmediateReply::default`, then `push`) holds one
event in itself, such as the outcome of the measurement that ends a batch,
so a reply of at most one event allocates no memory
(`an_immediate_reply_of_at_most_one_event_allocates_nothing`); it hands
over the events in the order pushed
(`an_immediate_reply_hands_over_the_events_pushed_in_order`).
`benches/instruction_calls.rs` measures both against a `Vec`.

## When two actions are the same

An instruction's `Action`
([What an instruction does](../instruction-sets.md#what-an-instruction-does))
is compared so:

Two actions are the same when they are written the same, up to the order
of a Pauli's factors and of a Clifford's images
(`a_paulis_factors_compare_whatever_their_order`,
`a_cliffords_images_compare_whatever_their_order`), and a Clifford's
generators that it maps to themselves
(`a_clifford_drops_the_generators_it_fixes`). Steps in another order, or
blocks of other sizes, differ
(`actions_with_their_steps_in_another_order_differ`,
`actions_on_other_blocks_differ`). Reading Qodec's syntax is a loader's
job; the crate takes the data, and writes a Pauli in Qodec's syntax, for a
log (`a_pauli_is_written_in_qodecs_syntax`). An instruction may declare no
action (`InstructionSet::with`), as a vendor's instruction may not; it is
then matched only in a set of its name, and refused in any other
([compatibility](#compatibility-the-rules)).

## Compatibility: the rules

One set's instructions are matched in another's by
`InstructionSet::opcodes_in`, which gives each its opcode there, an
`OpcodeMap`
(`the_map_gives_each_instructions_opcode_in_the_other_set`). An
instruction is matched to one taking operands of the same kinds in the
same order:

- **By mnemonic, in a set of the same name.** A set's name is its
  identity: whatever `x` means, it means the same at both ends, so no
  action is needed. One set at both ends, as QEP's driver and executor
  share theirs, is matched with nothing renumbered
  (`a_set_without_actions_is_matched_by_mnemonic_in_a_set_of_its_name`),
  so such a driver and executor are composed and run
  (`a_driver_and_an_executor_sharing_a_set_without_actions_are_composed`),
  with an adapter sending that set between them
  (`an_adapter_sending_the_set_downstream_without_actions_is_composed`) or
  one passing it through, which offers the set under its name
  (`a_passing_adapter_offers_the_set_downstream_under_its_name`);
  and a part of it in any order
  (`a_part_of_a_set_without_actions_is_matched_by_mnemonic_in_a_set_of_its_name`);
  an instruction taking other operands is refused
  (`an_instruction_taking_other_operands_in_a_set_of_its_name_is_refused`).
- **By action, whatever its mnemonic, in a set of another name**, when
  both sets declare what it does: Stim's `R` is the toy's `reset`
  (`an_instruction_is_matched_by_action_under_another_mnemonic`). Among
  instructions doing the same, the one of the same mnemonic is chosen
  (`the_same_mnemonic_is_preferred_among_instructions_doing_the_same`).
- **Undeclared otherwise**: between sets of other names, when either
  leaves the action undeclared, what the instruction does cannot be
  checked, so it is refused, as `UndeclaredAction` when the other set has
  an instruction of its mnemonic taking operands of the same kinds, and as
  below otherwise
  (`an_instruction_whose_action_either_set_leaves_undeclared_is_refused`,
  `sets_of_other_names_still_need_actions`), wherever sets are matched:
  when an adapter is composed with the executor downstream of it
  (`an_instruction_whose_action_the_downstream_set_leaves_undeclared_is_refused`),
  and when a driver is
  (`an_instruction_whose_action_the_drivers_set_leaves_undeclared_is_refused_when_composed`;
  see
  [The rules of composition](composition.md#the-rules-of-composition)).
  It is not looked for under another mnemonic
  (`an_instruction_without_an_action_is_not_matched_under_another_mnemonic`).

Whatever the names, a mnemonic that both sets declare to do different
things is refused as `DifferentAction`, though another instruction does
the same: one mnemonic meaning two things at one boundary is taken for a
collision, such as two unrelated sets given one name
(`an_instruction_a_set_of_its_name_declares_doing_something_else_is_refused`,
`a_mnemonic_both_sets_declare_doing_different_things_is_refused_whatever_their_names`),
and whatever the operands it takes there
(`a_mnemonic_both_sets_declare_doing_different_things_is_refused_whatever_its_operands`).
So a collision of names is caught wherever both sets declare what an
instruction does, and not where either leaves it undeclared.

An instruction not matched is `InstructionMismatch::Missing` when the
other set has no instruction of its mnemonic, `DifferentAction` when that
instruction does something else
(`an_instruction_doing_something_else_under_its_mnemonic_differs`), and
`DifferentOperands` when it takes operands of other kinds
(`an_instruction_doing_the_same_with_other_operands_differs_by_its_operands`).
Every instruction not matched is reported, in the order of the set
matched, so that all can be fixed at once
(`every_instruction_not_matched_is_reported`).

One set fits another (`InstructionSet::fits`) when each of its
instructions is matched there: `fits` is `opcodes_in` without the map
(`a_set_doing_the_same_under_other_mnemonics_fits`). So:

- **A part fits the whole.** A set fits itself, and any part of a set
  fits the set, under its name or, when each instruction declares what it
  does, under another (`a_set_fits_itself`, `a_part_of_a_set_fits_the_set`;
  declaring no action, under its name,
  `a_set_without_actions_is_matched_by_mnemonic_in_a_set_of_its_name`);
  the empty set fits any set (`an_empty_set_fits_any_set`). Between sets
  of other names, an instruction whose action either set leaves
  undeclared does not fit, though both sets have it under its mnemonic
  (`an_instruction_whose_action_either_set_leaves_undeclared_is_refused`).
- **Fitting goes one way.** A set does not fit a part of itself, which
  lacks some of its instructions (`a_set_does_not_fit_a_part_of_itself`).
- **Opcodes do not matter.** Instructions match by mnemonic or by action, so
  two sets that declare the same instructions in another order fit each
  other, though their opcodes differ
  (`sets_declaring_the_same_instructions_in_another_order_fit_each_other`).
- **Operands match by position.** A `measure` taking a tag then a block does not
  fit one taking a block then a tag
  (`an_instruction_taking_its_operands_in_another_order_downstream_does_not_fit`).

The rule is the same when a chain is built (what an adapter sends, against
the set downstream) and when a driver is composed (what it needs, against the
set the chain accepts: a needed instruction not matched is one not
offered). Both checks call `opcodes_in`, so they cannot drift apart.
`Capabilities::resets` stays a field of its own, not derived from the set,
since a component need not describe a set. An adapter, even one that
changes the set, passes on by default the `resets` of the executor
downstream (it may change them, through
`AdapterBoundary::offered_upstream`), and that is safe:
where sets are described, the instructions matched are what guarantee a
reset. A caller that resets has a reset instruction in its set, and a
chain whose set lacks it is refused, though downstream resets
(`a_driver_resetting_upstream_of_a_code_without_a_reset_is_refused_when_composed`,
`tests/runtime_isa.rs`). `resets` informs only where no set is described.

Since a part fits the whole, a component states only the instructions it
uses, and composes with any component downstream that accepts them, in any
order:

- **An adapter** states the instructions it sends (`Adapter::sends`), and
  `compose` checks them against the downstream set. Downstream of an adapter of
  `InstructionCall`s, `compose` puts an executor that renumbers each batch:
  calls in the downstream numbering pass as they are
  (`calls_in_the_downstream_numbering_reach_the_shot_as_they_are`), and calls in
  another are copied into it
  (`calls_in_another_numbering_are_renumbered_on_their_way_down`), unless the
  adapter sends in the numbering downstream, taken when it is built
  (`AdapterBoundary::adopt_downstream_numbering`,
  `an_adapter_that_can_send_in_the_downstream_numbering_does`). The run-time
  guide's `Encoding` does: it maps its gadgets into the downstream set then,
  through the map `compose` matched the sets with, so each gadget goes down with
  no name looked up, in the downstream numbering, not copied again to renumber
  it: about 2 ns per call saved, on batches of 10 000
  (`benches/instruction_calls.rs`, its `emitting` table). Each boundary is
  matched once. A typed adapter gets the executor downstream of it as it is, at
  no cost.
- **A driver** states the instructions its program sends
  (`Needs::with_instructions`) and builds its calls with its own set. It is
  composed with a chain into a run (`compose`, or `>>`), which checks the two
  sets' compatibility when they are composed. A set the chain lacks is
  refused with every instruction that does not fit
  (`a_set_the_executor_lacks_is_refused_with_every_instruction_that_does_not_fit`),
  as are the driver's other needs the chain lacks, whatever its set
  (`a_driver_needing_what_the_executor_lacks_is_refused_whatever_its_set`).
  Otherwise, by the driver's instruction type:
  - **`InstructionCall`s**, which name their instruction by opcode, are
    matched once, when the driver is composed. When each instruction has
    the same opcode in both sets, a batch reaches the chain as it is
    (`a_batch_in_the_executors_numbering_reaches_it_as_it_is`). When the
    numbering differs, the run holds the chain behind an executor that
    copies each batch with its opcodes replaced, into a buffer it keeps,
    and the driver runs its own `x`, not the chain's instruction at that
    opcode
    (`a_driver_whose_set_is_in_another_order_runs_its_own_instructions`,
    `each_call_runs_only_its_own_batch`); a call at an opcode the
    driver's set lacks is refused, though the chain has one there, or an
    earlier call of the batch that the executor's set refuses, if there is
    one, as the executor alone would refuse it
    (`a_call_outside_the_drivers_set_is_refused`,
    `a_renumbered_batch_is_refused_at_its_first_call_outside`, and on the
    guide's code,
    `the_code_in_a_drivers_numbering_refuses_calls_outside_the_drivers_set`).
    A driver that states no set is refused, since its opcodes cannot be
    checked
    (`a_driver_of_calls_by_opcode_that_states_no_set_is_refused_when_composed`).
    The renumbering costs at most about 1 ns per batch when calls reach
    the chain as they are, which any executor that does work hides, and
    when it copies, about 2 ns per call and 4.6 ns per batch (measured on
    an executor that only reads its calls; on the toy, about 55 ns per
    batch of one call). A driver that cannot afford the copy builds its
    calls by name with the set the chain describes, read before the driver
    is made (`Capabilities::instruction_set`, then `InstructionSet::call`),
    and states that set as its needs, so its calls are in the chain's
    numbering and reach it as they are, as above; on the guide's code,
    they run as the driver means them
    (`a_driver_builds_its_calls_by_name_with_the_codes_set`).
  - **A typed driver**, whose instructions name themselves (an enum's
    variants), states its set in any order, and its calls reach the
    executor as they are, at no cost
    (`a_typed_driver_in_another_order_runs_its_instructions_as_they_are`):
    its instruction type implements `TypedInstruction`, in one line. With
    no set stated on either side, the compiler checks that both run the
    same type
    (`a_typed_driver_and_an_executor_that_describe_no_set_compose`).
  - **No other instruction type** composes (a compile error, which no
    test pins): the trait both implement, `Instruction`, is sealed, since
    another crate's type whose calls carry an opcode of its own could not
    be renumbered, and its calls would run the executor's instructions at
    those opcodes with no error.

  A caller that calls an executor directly, without a run, sends calls in
  the executor's numbering: the set the executor describes
  (`Capabilities::instruction_set`), not the one its needs state.
  `Capabilities::admit` checks that the executor accepts the caller's
  instructions, in any order, not that they have the same opcodes, so a
  call numbered by another set may reach another instruction, and when
  the two take the same operands (`x` and `reset`), no check sees it.

## The checks: the rules

A description is checked in three places, each before anything runs:

1. **A lowering, when the chain is built.** An adapter that accepts one set
   and sends another states both (`Adapter::accepts`, `Adapter::sends`);
   one that states only the set it accepts sends what it accepts
   (`an_adapter_that_states_no_sends_sends_what_it_accepts`), unless it
   makes calls by opcode from instructions of another type: the set it
   accepts does not number those calls, so it states the set it sends. One
   that decodes calls by opcode into typed instructions and states only the
   set it accepts sends no set: the calls upstream of it are checked against
   the set it accepts
   (`a_driver_whose_calls_a_decoder_does_not_accept_is_refused_when_composed`),
   and nothing is checked downstream
   (`a_decoder_stating_only_the_set_it_accepts_describes_it_whatever_the_set_downstream`).
   Otherwise `compose` checks that what it sends fits the downstream set
   ([compatibility](#compatibility-the-rules)), and renumbers its calls
   into it where it numbers them otherwise, as it checks the ids that adapters
   draw with. It reports every fault of the parts built together at once
   (`BuildFaults`, [Composing a run](../composition.md#composing-a-run)),
   the faults that adapters report when they are built
   (`BuildError::Adapter`) among them; a part composed with the executor
   first is built on its own, before what is upstream of it
   ([The rules of composition](composition.md#the-rules-of-composition)).

   The instructions missing downstream, doing something else there, taking
   other operands there, or, between sets of other names, whose action
   either set leaves undeclared, are `BuildError::Instructions`
   (`an_adapter_accepting_what_the_downstream_set_lacks_is_refused_when_built`;
   for a code's gadgets,
   `a_code_sending_what_the_device_lacks_is_refused_when_built`, and with
   the code's own faults, `every_build_fault_is_reported_at_once`); a
   downstream executor that describes no set, where what an adapter sends
   must fit one, is `BuildError::NoInstructionSetDownstream`
   (`an_adapter_upstream_of_a_component_with_no_instruction_set_is_refused_when_built`);
   an adapter that makes calls by opcode from instructions of another type
   and states no set it sends, whatever it accepts, is
   `BuildError::UnstatedSends`
   (`an_adapter_that_makes_calls_by_opcode_and_states_no_set_is_refused`,
   `an_adapter_that_makes_calls_by_opcode_and_states_only_the_set_it_accepts_is_refused`;
   stating its set, it composes:
   `an_adapter_that_makes_calls_by_opcode_and_states_the_set_it_sends_is_composed`).
   When the downstream executor still refuses a call, the adapter reports
   it at the call it received, with `Rejected::at_origin`
   (`a_downstream_refusal_names_the_logical_call`).

2. **A caller's needs, when a driver is composed, or by hand
   (`Capabilities::admit`).** A caller that states the instructions it will send
   (`Needs::with_instructions`) is refused by `Capabilities::admit` with
   `Rejected::Unmet` for `Need::Instructions` when the chain accepts fewer of
   them, and that is its first unmet need, after qubits, feedback, resets,
   replayable and probabilities
   (`several_unmet_needs_are_reported_one_at_a_time_in_a_fixed_order`);
   `needed` and `offered` count instructions, and `mismatches` lists each
   needed instruction that does not fit
   (`every_needed_instruction_not_offered_is_reported`;
   `a_driver_is_composed_only_with_a_code_that_has_the_instructions_it_needs`,
   in both guides). A driver composed with an executor is refused then, before
   anything runs, each need it lacks a `BuildError::Unmet` among the chain's
   faults (`a_driver_whose_needs_the_chain_lacks_is_refused_when_composed`).

3. **A call**, by an executor of `InstructionCall`s, since the compiler
   cannot: `InstructionSet::check_batch(batch)` refuses the batch with
   `Rejected::Instruction` at its first call whose opcode the set lacks
   (`InstructionMismatch::UnknownOpcode`) or whose operands are of other
   kinds (`InstructionMismatch::DifferentOperands`), the first in the
   batch's order, as `Rejected::Instruction` promises, since a check made
   at each call stops at its first fault; `InstructionSet::call`
   refuses to build such a call by its mnemonic. The conformance kit checks
   an implementation keeps the rule
   (`conformance::refuses_calls_outside_its_set`, run on the guide's device
   and code in `the_device_and_the_code_refuse_calls_outside_their_sets`):
   from the call it is given, it makes one call outside for each way
   `InstructionSet::check` can refuse a call: at an opcode the set lacks;
   with an operand too many; with one too few (the last left out), when
   the call takes an operand; and, at each of the call's operands in turn,
   an operand of each other kind (a block for a number, one number for
   another, a tag for an integer). It sends each first, in the middle and
   last of a batch, and each must be refused at its index; and every
   ordered pair of them, one twice included, in
   `[outside, accepted, outside]`, which must be refused at the first, 0,
   so that an executor that looks for each reason in its own pass over the
   batch shows. It first asks that a batch of three of the call it is
   given be run, so that a refusal can only be the call outside's. A kit
   self-test
   (`refuses_calls_outside_its_set_tries_each_reason_a_set_has_to_refuse_a_call_at_each_place`)
   classifies each call the check sends by `check`'s own answer, and asks
   for each reason at each place and each pair. A variant that `check`'s
   error gains fails to compile in that classification until it is told
   apart; once it is listed among the test's reasons, the test fails until
   the check tries it. A new reason under a variant already told apart (a
   check of an operand's value, say) needs a new call outside in the
   check, and no test asks for it.

Every executor of calls checks each call it receives: an index in range
and the operands' kinds. A chain could instead check once, where calls
enter it from a caller it does not trust, and let the components
downstream trust each other; that is left until the check is measured to
cost too much. On a counting profiler, which does little work of its own,
an executor of calls costs about 10 ns per instruction more than one of a
Rust type, the check included ([Performance](../performance.md)): too little
to drop the check, so every executor keeps it.
`benches/instruction_calls.rs` measures it
(`cargo bench -p executor_protocol --bench instruction_calls`):

- three executors doing the same work: of a Rust type, of calls by
  opcode, and of calls by mnemonic;
- the representation of calls and replies;
- a driver's calls against an executor called directly, in the
  executor's numbering (as a driver's calls built by name with the
  executor's set are) and in another, copied by the renumbering, in
  batches of many calls and of one;
- an adapter composed with the toy's shots, its calls passed or copied at
  the boundary downstream of it;
- an adapter that sends a call of its own for each call, its batches
  copied at the boundary or, adopting the downstream numbering, passed.
