---
title: qodec human interface
description: A terminal workbench for building qodecs and seeing how they lower a program, layer by layer, for people rather than for line-at-a-time conversations.
---

## Why another interface

`qodec-repl` answers one line at a time. That suits a reader who keeps the whole conversation in mind, such as an LLM. A person building a qodec goes around a different loop: write a code, a gadget circuit and its equations, run a program through the stack, look at what came out, and change something. `qodec-hi` puts that whole loop on one screen:

- Every file of the qodec is editable text, comments and all, and every keystroke recomputes everything that follows from it
- Each gadget is drawn as a circuit with a time cursor, next to the stabilizer state after each call, every sign a formula
- The tool works out what each measurement measures, which checks, readouts and frames hold, whether the circuit implements its instruction, and derives the equations it is missing, ready to adopt with one key
- Each code is shown as a grid of its operators, with what does and does not fit together
- The program is run through every layer, and drawn as a circuit at each one

There is no build, reload or reset step. The design and its reasons, from Bret Victor's _Learnable Programming_ and Hanson and Sussman's _Software Design for Flexibility_, are in [DESIGN.md](./DESIGN.md).

## Running it

From the repository root, pass a manifest or bundle, as for `qodec-repl`:

```bash
cargo run -p qodec_human_interface -- source/qodec/examples/repetition3/repetition3.qodec.yaml
```

Add `--program <file>` to start from a program, one line per batch. The program syntax is the same as `qodec-repl`'s, so lines move between the two tools.

`Ctrl-S` saves every file that changed: a bundle's own documents back into the bundle, every other file the qodec refers to (such as the shared `stim.isa.yaml`) into that file, and the program into its file.

## The map

The map on the left is the whole qodec: each layer's instruction set, its instructions, the codes its blocks are encoded in, and the program. `Enter` opens an item, `Tab` moves through its panes, and `Esc` returns to the map. Each instruction carries a mark for what the tool found in its gadget:

| Mark | Meaning                                                                         |
| ---- | ------------------------------------------------------------------------------- |
| `✓`  | Verified: every declared equation holds and the circuit implements the action   |
| `!`  | Works, with something worth a look, such as an output stabilizer no check fixes |
| `✗`  | Something is wrong: an equation fails, or the circuit does something else       |
| `~`  | Has a step the tool can't follow exactly, such as a rotation                    |
| `·`  | Has no gadget yet; `g` gives it an empty one                                    |

## A gadget

```text
┌Circuit · idle · 3/8────────────────────────────────────┐┌State · after 3 · 6 generators──────┐
│0 in0───CX──────────────out0                            ││01234r sign                         │
│1 in0───┼──CX─CX────────out0                            ││ZZ···· (−1)^(in[0].stabilizers[0])  │
│2 in0───┼──┼──┼──CX─────out0                            ││·ZZ··· (−1)^(in[0].stabilizers[1])  │
│3     R─CX─CX─┼──┼──M→0                                 ││XXXX·X (−1)^(in[0].x[0])            │
│4     R───────CX─CX─M→1                                 ││Z····Z (−1)^(in[0].z[0])            │
│                                                        ││Z··Z·· +                            │
└────────────────────────────────────────────────────────┘└────────────────────────────────────┘
┌Parts · stim+rz──┐┌Text · idle.gadget.yaml──┐┌Findings────────────────────────────────────────┐
│R <out:qubit>    ││ 1 circuit:              ││What each measurement measures                  │
│M <in:qubit>     ││ 2   source:             ││  readouts[0] M 3 (Z_0) = in[0].stabilizers[0]  │
│CX <qubit> <qubit││ 3     - R: [3]          ││  readouts[1] M 4 (Z_0) = in[0].stabilizers[1]  │
│rotate_z <qubit> ││ 4     - R: [4]          ││Checks                                          │
│                 ││ 5     - CX: [0, 3]      ││✓ ["circuit.readouts[0]", "in[0].stabilizers[0]"│
└─────────────────┘└─────────────────────────┘└────────────────────────────────────────────────┘
```

| Pane     | What it shows                                                                    | Main keys                          |
| -------- | -------------------------------------------------------------------------------- | ---------------------------------- |
| Circuit  | The calls, with the input (`in`) and output (`out`) blocks; `M→j` reports bit j  | `←`/`→` step, `Home`/`End`         |
| State    | The stabilizer state after the selected call; `r` columns are reference qubits   |                                    |
| Parts    | The instruction set below, to build the circuit from                             | `Enter` adds a call at the end     |
| Text     | The gadget's file                                                                | `Enter` edits a line, `n` new      |
| Findings | What each bit measures, each equation's verdict, and what the tool derived (`+`) | `Enter` adopts one, `A` adopts all |

The analysis is a symbolic stabilizer simulation. Each input block starts in its code with its logical qubits entangled with reference qubits, so every sign is a variable: `in[0].stabilizers[1]`, `in[0].z[0]`. A measurement's value is then a formula over those variables and fresh random bits, and every finding follows from the formulas:

- A check holds when its terms sum to zero; otherwise the finding shows what is left over
- An output stabilizer needs a check that determines its sign, so the decoder knows it
- The circuit implements its instruction when, with the declared readouts and frames, every logical operator comes out as the instruction's action says

When an equation is missing, the tool solves for it and offers it as a `+` row. Adopting it writes the equation into the gadget's text, where it stays like any line you typed.

To grow a gadget, press `g` on an instruction without one. It gets an empty circuit in a new file, listed in the manifest. Add calls from Parts: operands they take go on the live labels used most recently, operands they give go on the output's labels that have no state yet. Then adopt what the findings derive.

## Codes and other files

A code shows its operators as a grid of `X`, `Y` and `Z`, beside the facts that do or do not hold: the stabilizers commute, the logical operators commute with them and pair up, and the operators are independent. Any other item shows its text and what it declares.

Every file is edited the same way, one line at a time: `Enter` edits the selected line, `n` adds one, `d` deletes, `y` copies, and `J`/`K` move it. While you type, a line that does not parse leaves the last qodec that did on screen, and the line under the map says why, with the file and line of the problem.

## The program

```text
┌Parts · repetition3───────┐┌Program───────────────────┐┌Circuit 1: stim+rz (bottom) · 13/14──┐
│prepare_z <out:repetition3││1  prepare_z 0            ││  1 2                 3              │
│idle <repetition3>        ││2  idle 0                 ││0 R───CX──────────────M→0            │
│measure_z <in:repetition3>││3  measure_z 0  ⟹ Z_0=0   ││1 R───┼──CX─CX────────M→0            │
│rotate_z <repetition3> the││                          ││2 R───┼──┼──┼──CX─────M→0            │
└──────────────────────────┘└──────────────────────────┘└─────────────────────────────────────┘
```

| Pane    | What it shows                                                    | Main keys                                   |
| ------- | ---------------------------------------------------------------- | ------------------------------------------- |
| Parts   | The top instructions, laid out to pick from                      | `Enter` adds one, ready to run              |
| Program | The lines, and what each returned or why it did not run          | `Enter` edits, `n` new, `d` delete, `J`/`K` |
| Circuit | One boundary's calls over time, a row per block id               | `←`/`→` time, `[`/`]` layer, `Space` flip   |
| Flow    | The selected line's batches through every layer, as a tree (`v`) | `↑`/`↓` scroll                              |

### Working with a program

Start from something on screen and adjust it, rather than planning a program in your head:

1. In Parts, press `Enter` on an instruction. It is added after the selected line with every operand filled in: inputs on the most recently given live block of the right type, outputs on the lowest free id, arguments at plain defaults. It runs at once.
2. Edit a line with `Enter`. The shot runs again on every keystroke, and `Tab` completes mnemonics and parameter names.
3. A line the runtime refuses (for example, a block that is not live) is marked and skipped, and Explain says which instructions would make it run. A line that loses the shot is marked, and the lines after it are greyed out.

To see how a call is implemented, select it in the circuit and press `]`: the view goes down one layer, to the first call it became. `[` goes back up to the call it is part of.

Bits come from the bottom backend, which runs nothing and makes them up: all zero, all one, or random (`o` cycles, `r` reseeds). Select a bottom measurement and press `Space` to flip its bit. The bit stays pinned, and every decoded result above it follows. Pins and random bits belong to their line, so editing one line does not change the bits of another.

## Everywhere

`?` shows every key, `u`/`U` undo and redo any change (to a file, an adopted equation, a program line or a pinned bit), and `q` quits.

## How it watches the runtime

The runtime is the reference one, unchanged. `Traced` wraps any `Design`, and gives each layer it assembles a tap: a layer around the layer that records each batch it is given and each answer it returns. A traced design is itself a design, so `QodecPlan` builds it like any other. Layers answer by calling the layer below, so the recorded batches nest into a tree whose depth is the boundary.

What is live is read off the recorded calls with the runtime model's own rules (`Isa::parse_call` and `Signature::bind`), the same way `TypeChecker` reads them. The interface and the runtime agree on liveness by construction.

| Module          | Role                                                                        |
| --------------- | --------------------------------------------------------------------------- |
| `trace.rs`      | `Traced` design and its taps, the journal, and the bottom backend with pins |
| `shot.rs`       | `Engine`: a program and its outcomes in, a `Shot` out                       |
| `state.rs`      | Block lifetimes at each boundary                                            |
| `circuit.rs`    | Laying out a boundary's calls as a circuit                                  |
| `vocab.rs`      | Names for bits, conditions, gadgets, and default operands                   |
| `syntax.rs`     | The instruction syntax shared with `qodec-repl`                             |
| `gf2.rs`        | Bit vectors and parity formulas over GF(2)                                  |
| `stabilizer.rs` | The symbolic stabilizer tableau                                             |
| `analysis.rs`   | Gadget analysis: verdicts, derived equations, state after each call         |
| `codes.rs`      | What holds of a code                                                        |
| `bundle.rs`     | The qodec as text: loading, saving, and edits that keep comments            |
| `workbench.rs`  | The map, its marks, the findings, and new gadgets                           |
| `app.rs`        | The interface state and what each key does                                  |
| `ui.rs`         | Drawing the program, and the screen around every item                       |
| `views.rs`      | Drawing the map, gadgets, codes and other files                             |

Run the tests with `cargo test -p qodec_human_interface`. They draw the interface into a test terminal, so the screens they expect are readable in `src/tests.rs`. They also check that the gadgets of the well-built examples verify, and that every part of every example opens and draws.
