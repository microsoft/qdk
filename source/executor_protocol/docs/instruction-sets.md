# Instruction sets and their fit

For those who declare the instructions a part sends or accepts, and who
need to know when two parts' sets fit: what a set declares, how a set and
its instructions are built, how each `>>` checks the fit, and how a set is
compared and sent across a process boundary. The other terms are those of
the [README](../README.md#terms).

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

A part states the instructions it sends or accepts as an
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

Each `>>` checks that every instruction the part above sends is accepted
below, with the same blocks, parameters and readouts: matched by what the
two instructions declare they do, or, where neither declares it, by
mnemonic between sets of one name, and refused otherwise; the rules, and
their exceptions, are in [`fit.rs`](../src/instruction_set/fit.rs). When
the sets fit, the sender gets, through its `bind`, the receiver's opcode
for each instruction it sends ([`OpcodeMap`](../src/instruction_set/opcode_map.rs)),
with the matches made by mnemonic alone listed, since what those
instructions do is not checked; it sends its instructions in the
receiver's opcodes. Otherwise the boundary's faults
([`FitFaults`](../src/instruction_set/fit.rs)) name both sets and list
every mismatch.

## Equality and the stable form

The fit asks whether one set can serve another; equality asks whether
two sets are the same one, as a part that receives a set across a process
boundary does to check it runs the set the sender runs. Two sets are
equal (`==`) when their declarations are: their names, their block types
(each with its qubits) in order, and their instructions in order, every
field of each, so that equal sets give each opcode the same instruction.
A meaning compares in its canonical form, so one written otherwise (a
Clifford's images in another order) is the same. What a set derives when
it is built is not compared.

With the crate's `serde` feature (off by default), a set has a stable
form: its declaration alone, written in a self-describing format, one
that names fields and variants, such as JSON, and read back through
`InstructionSet::new`, so a decoded set is checked as a built one is and
a faulty one is refused, naming the set and its faults. A struct's field
the form does not have, as a newer build's form may hold, is refused,
naming it. A format that writes fields and
variants by position (a binary one) reads back what the same build wrote,
but its form is not pinned across builds. In JSON, the set `Gates` of one
instruction, `rz`, rotating its qubit by its parameter `theta`:

```json
{
  "name": "Gates",
  "block_types": [["qubit", 1]],
  "instructions": [
    {
      "mnemonic": "rz",
      "inputs": [{ "block_type": "qubit", "variadic": false }],
      "outputs": [{ "block_type": "qubit", "variadic": false }],
      "parameters": [{ "name": "theta", "kind": "number" }],
      "outcomes": 0,
      "flags": [],
      "meaning": [
        {
          "operation": {
            "rotate": { "pauli": "Z_0", "angle": { "parameter": 0 } }
          },
          "guard": null
        }
      ]
    }
  ]
}
```

Steps, guards and parameter kinds are named as Qodec names them, a
guard's terms by position, a Pauli as Qodec's text, and every other part
by its Rust name; a meaning is `null` when undeclared. The example is
copied from the form the [tests](../src/tests/instruction_set/stable_form.rs)
pin, which is its source: keep the two the same. A literal angle is a
64-bit float, read back equal only from a format that keeps it bit for bit
([`Angle`](../src/instruction_set/meaning/angle.rs)).
