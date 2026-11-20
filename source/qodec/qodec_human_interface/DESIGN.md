---
title: qodec workbench design
description: Why qodec-hi is shaped the way it is, starting from what people do when they build qodecs and study how they lower programs.
---

## The problem

A qodec is built, not only read. Someone writes a code, declares an instruction set, writes a gadget circuit for each instruction, and states the gadget's checks, readouts and frames. They stack layers, then run a program through the stack to see whether it lowers and decodes as intended. They go around this loop many times.

The first version of `qodec-hi` served the second half of that loop: it took a finished qodec and showed how a program lowers through it. Building was left to a text editor, so the person had to imagine what an edit would do, then restart to find out. That is the situation Bret Victor's _Learnable Programming_ argues against. This design treats both halves as one loop and applies the essay's principles to each step of it.

## The loop

1. Change something: a code operator, an instruction, a gadget's circuit or one of its equations, or a program line.
2. See what follows from the change, everywhere it matters, at once:
   - Whether every piece still makes sense: codes commute, gadgets fit their instructions, the runtime can plan the stack
   - What each gadget computes: every measurement's value, which parities are checks, what the readouts and frames must be, and whether the circuit implements the instruction
   - How a program now lowers and decodes, and what happens when a bit flips
3. React: accept what the tool derived, fix what it flagged, or try the next idea.

There is no build, reload or reset step. The qodec, the program and the bits are the only inputs, and everything on screen is derived from them on every change.

## What the person needs to see, by Victor's principles

### Read the vocabulary

A qodec is written in references: `in[0].stabilizers[1]`, `circuit.readouts[6]`, `out[0].z[0]`. Nobody should have to resolve one in their head. Each reference is shown next to what it names: a stabilizer as a Pauli on the gadget's wires, a readout as the measurement in the circuit that produced it, a sign as the operator it belongs to. Instructions are labelled with their actions, and bits with the observable or flag they report.

### Follow the flow

A gadget is a circuit, so it is drawn as one, with a time cursor. Stepping the cursor shows the state after each call. The same holds one level up: a program is drawn as a circuit at every boundary, and `[` and `]` move between a call and the calls it became.

### See the state

The state of a stabilizer circuit is a set of Pauli operators and their signs. The tool computes it symbolically. Each sign is a parity over the incoming frame signs, the random outcomes, and the instruction's bit parameters. That one computation answers the author's questions directly:

- What does measurement 3 measure? `in[0].stabilizers[1]`, or a fresh random bit, or `in[0].z[0]` plus a random bit.
- Is this check right? Its terms sum to zero, or here is what is left over.
- Is the output in the code space, and do the declared checks determine its stabilizer signs?
- Does the circuit, with these readouts and frames, implement the instruction's action?

No state is hidden: the sign of every operator is a visible formula, and so is every measurement's value.

### Create by reacting

Something useful is on screen as soon as possible. The parts bucket of every circuit is the instruction set below it, laid out to pick from. For a gadget, the tool derives everything the spec says a tool may derive: a basis of valid checks, the readout equations, and the frames. It offers each one as a suggestion, so the author adopts derived equations instead of writing them by hand, and sees immediately what each adoption changes.

### Create by abstracting

Start concrete, then generalize. An instruction without a gadget gets an empty gadget, listed in the manifest, ready to fill. Each call added from the parts lands on a sensible label: what it takes on the live labels used most recently, what it gives on the output labels that have no state yet. The first useful circuit is a few keystrokes away, and the derived equations turn it into a verified gadget.

Two moves the essay asks for are not built yet: writing a program at a lower boundary, and turning a stretch of program lines into a new gadget. Both fit the design: the first is a program whose parts bucket is a lower instruction set, and the second is a gadget whose circuit starts as those lines.

## Partial information

A qodec under construction is wrong most of the time, and the tool has to stay useful while it is. Following _Software Design for Flexibility_ (chapter 7), each derived fact is computed by its own independent part, from whatever inputs are available:

| Part            | Needs                                 | Gives                                                      |
| --------------- | ------------------------------------- | ---------------------------------------------------------- |
| Text            | the document                          | whether the document parses                                |
| Structure       | every document                        | the loaded qodec, or the error and the last good version   |
| Code facts      | one code                              | `[[n,k]]`, commutation, independence, logical pairs        |
| Gadget analysis | one gadget, its instruction, its ISAs | measurement values, verdicts, suggestions, state over time |
| Plan            | the whole qodec, read by the runtime  | the runtime's plan, or why it refuses                      |
| Shot            | the plan and the program              | everything the first version showed                        |

A broken gadget doesn't stop the analysis of its neighbors, and a qodec the runtime refuses still shows every gadget's analysis. Each finding is attached to the item it is about and summed up as a mark on the map.

## The text stays the text

Every artifact is edited as its own YAML document, the same text the files hold, comments and all. A structured view next to the text shows what it means, and the derived suggestions are adopted as new lines of that text. This keeps every field the spec allows editable without a separate editor for each, keeps saved files readable and diffable, and lets the tool stay liberal in what it accepts (Postel's law, chapter 1) while the views show the strict meaning.

## The screen

```text
 qodec-hi  name · top ▸ … ▸ bottom · bits: zero · unsaved                    ? help
┌Map──────────────┐┌Circuit · instruction · 3/8──────────┐┌State · after 3──────────┐
│ name            ││  the gadget's calls over time, with  ││  generators on its      │
│ 0 ISA (top)     ││  its input and output blocks, and a  ││  qubits, each sign a    │
│   ✓ instruction ││  time cursor                         ││  formula                │
│   ! instruction │└──────────────────────────────────────┘└─────────────────────────┘
│   · instruction │┌Parts──────┐┌Text · file──────────┐┌Findings────────────────────┐
│   ◇ code        ││ the ISA   ││ the gadget's YAML,  ││ what each bit measures,    │
│ 1 ISA (bottom)  ││ below     ││ edited in place     ││ verdicts, derived (+) rows │
│ Program         │└───────────┘└─────────────────────┘└────────────────────────────┘
 the call at the cursor, or why the text does not load or plan
 Explain: what the selected thing is, where it came from, what a key would do
 keys
```

The map is the qodec laid out on the floor: every layer, instruction set, code and gadget, with a mark for its health. Selecting an item focuses it. A gadget is shown as above; a code as a grid of its operators beside its facts and its text; the program as its lines, a circuit at each boundary, and its flow; any other item as its text and what it declares.

## How it is built

- The domain model is the `qodec` crate's model, loaded from the text, and the runtime's model where the runtime is used. Nothing is reimplemented that either already defines.
- The gadget analysis is a symbolic stabilizer simulation. It runs the gadget's circuit, with the lower instruction set's actions, on a state where each input block's logical qubits are entangled with reference qubits. It also runs the implemented instruction's action on the same kind of state. Comparing the two answers whether the circuit implements the action, and solving for the difference derives the readouts and frames.
- Observing the runtime is unchanged from the first version: a `Traced` design wraps the runtime's design and taps every layer.
- Every change is one step in a single history, so undo covers code edits, adopted suggestions, program lines and pinned bits alike.
- The documents are every file the qodec is made of: a bundle's own documents, or a manifest and its files, plus any YAML file they refer to, such as the shared `stim.isa.yaml`. The loaded set is self-contained, and saving writes each changed document back to where it came from.

## What it does not do yet

- Non-Clifford steps, such as a rotation, can't be followed exactly. The analysis keeps what commutes with the step and marks the gadget `~`, so verdicts that depend on what it lost are unknown rather than wrong.
- The two abstraction moves above: programs at lower boundaries, and gadgets made from program lines.
