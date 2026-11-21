# Instruction sets and their fit

For those who declare the instructions a part sends or accepts, and who
need to know when two parts' sets fit: what a set declares, how a set and
its instructions are built, and how each join checks the fit. The other
terms are those of the [README](../README.md#terms).

## Terms

A set's instructions act on blocks, and a batch holds them by opcode:

- **Block** ([`BlockType`](../src/instruction_set/block_type.rs)): a unit
  of a shot's quantum state that an instruction acts on, of a type: a
  qubit, for a program in QIR (the form the QDK compiles programs to), or
  a block of an error-correcting code, for that code's logical
  instructions.
- **Instruction** ([`Instruction`](../src/instruction_set/instruction.rs)):
  one instruction of a set, as a batch holds it: its opcode (its place in
  the set), the shot's blocks it acts on, its arguments and, rarely, the
  values it expects of its flags without noise (`select`; flags are
  readouts, as [Readout slots](batches-and-readouts.md#readout-slots)
  says).

## Declaring a set

A part states the instructions it sends (`sends`, for a driver or an
adapter) or accepts (`accepts`, for an adapter or an executor) as an
[`InstructionSet`](../src/instruction_set.rs), in the model of Qodec, a
format that describes each level of a quantum error-correcting code as
data. Its model describes instruction sets in general, at any level of a
chain, from a simulator's gates to a code's operations on its blocks: for
each instruction, as data, its blocks, parameters and readouts, and what
it does. So two parts' sets can be checked against each other before any
shot runs, matching instructions by what they do rather than by name, and
a part written against Qodec's sets needs no translation. A set written in
code is declared with
[`instruction_set!`](../src/instruction_set.rs), which names its opcodes
as constants; a set known only at run time is built by
`InstructionSet::new`, and its opcodes resolved once with
`InstructionSet::opcode`. `InstructionSet::new` refuses every fault of the
set's declaration at once
([`DeclarationFault`](../src/instruction_set.rs)). What a set declares is
in [`instruction_set.rs`](../src/instruction_set.rs). An instruction is
built by name with `InstructionSet::instruction`, checked at once, or, on
a part's hot path, as a struct literal with those opcodes
([`Instruction`](../src/instruction_set/instruction.rs)).

## The fit

Each join checks that every instruction the part above sends is accepted
by the part below it, with the same blocks, parameters and readouts:
matched by what the two instructions declare they do, or, where neither
declares it, by mnemonic between sets of one name, and refused otherwise;
the rules, and their exceptions, are in
[`fit.rs`](../src/instruction_set/fit.rs). When the sets fit, the sender
gets, through its `bind`, the receiver's opcode for each instruction it
sends ([`OpcodeMap`](../src/instruction_set/opcode_map.rs)), with the
matches made by mnemonic alone listed, since what those instructions do is
not checked; it sends its instructions in the receiver's opcodes.
Otherwise the `on` that makes a runner reports each mismatch
([`InstructionMismatch`](../src/instruction_set/fit.rs)), with those of the
resources and features, as [Capabilities and their fit](capabilities.md)
says; at a boundary inside a part of the user's, which the `on` cannot
see into, every run is refused with them instead.
