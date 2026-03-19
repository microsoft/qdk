# Feature Proposal: Circuit Diagrams in Quantum Katas

## Summary

After a user completes a coding exercise, the tutor automatically generates and displays a circuit diagram of their solution. During lessons, the tutor also uses circuit diagrams to visually explain concepts alongside the math.

## Motivation

Quantum circuits are the universal visual language of quantum computing. Every textbook, every paper, every course uses them. Yet the current katas experience is entirely text-based -- the user writes code, gets a pass/fail result, and moves on. They never _see_ what their program looks like.

Circuit diagrams bridge the gap between code and concept. When a learner writes `CNOT(control, target)` and then sees the circuit with the control dot and target cross, the connection clicks in a way that text alone cannot achieve. This is especially impactful for beginners who are still building their visual vocabulary for quantum computing.

## What the user sees

### After completing an exercise

The user finishes the "State Flip" exercise by writing a single X gate. The tutor verifies the solution, marks it correct, and then says:

> "Here's the circuit diagram for your solution:"

A circuit panel opens in VS Code showing a single qubit wire with an X gate on it.

For more complex exercises -- say, teleportation -- the user sees their full circuit laid out: the entangled pair preparation, Alice's measurements, Bob's corrections. This is the "show me what I built" moment that turns abstract code into something visual and tangible.

### During a lesson

Before the user attempts the CNOT exercise, the tutor explains the gate. Alongside the text and matrix, it displays a circuit diagram showing the CNOT gate with labeled control and target qubits. The user has already seen what they're trying to build before they start coding.

### Comparing approaches

For exercises where multiple solutions are valid, the tutor can optionally show both circuits:

> "Your solution uses H-Z-H to flip the qubit. Here's the equivalent circuit using just the X gate. Notice they produce the same transformation -- but the X gate is simpler."

Two circuits appear side by side, making the comparison immediate.

## When to introduce it

**First exercise.** Circuit diagrams should be the very first QDK feature the user encounters. They're visually intuitive, require no explanation, and they reinforce the core activity (writing quantum gates) from the start.

## What already exists

The QDK extension already has full circuit visualization support:

- A "Show Circuit" command that renders any Q# program as an interactive circuit diagram in a VS Code panel.
- An MCP tool (`renderCircuit`) that the AI tutor can call to display circuit diagrams inline in the chat or as a side panel.
- Circuit rendering supports both static (compile-time) and simulated modes, with collapsible sub-circuits for complex operations.

No new visualization features need to be built. The work is in wiring the tutor to call circuit rendering at the right moments in the exercise flow.

## Open questions

- **Inline vs. panel:** Should the circuit appear inline in the chat, or open in a separate panel? Inline is more convenient but may be limited in size. A panel can show larger circuits with full interactivity.
- **OpenQASM exercises:** Circuit rendering currently works for Q#. Do we generate circuits for OpenQASM exercises too, or only Q#?
- **Automatic vs. opt-in:** Should the circuit always appear after a correct solution, or should the tutor offer it ("Want to see the circuit?")?
