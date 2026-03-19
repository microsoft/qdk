# Feature Proposal: Interactive Circuit Editor in Quantum Katas

## Summary

For early gate exercises, the tutor opens an interactive circuit editor where the user can drag and drop gates onto qubit wires to build circuits visually -- before translating them to code. The editor also shows quantum state probabilities at each step, giving the user a live "X-ray" view of how the quantum state evolves.

## Motivation

Writing code is not how most people first learn quantum computing. They learn by drawing circuits on a whiteboard. The jump from "I understand what a CNOT gate looks like" to "I can write `CNOT(q[0], q[1])` in Q#" is a real barrier, especially for beginners who may not be comfortable programming.

The circuit editor lowers this barrier by letting users _draw_ before they _code_. They place an H gate on a wire, see the state column update from |0> to |+>, and build visual intuition for what each gate does. Then when the tutor says "now write this in Q#," the translation is natural -- they already know what the program should look like.

The state visualization columns are the real differentiator. Most circuit editors just show the gates. This one shows the quantum state _at every step_. For a learner, this is like having a debugger for quantum programs -- they can see exactly where the state changes and verify their understanding in real time.

## What the user sees

### "Build before you code" exercise

The user reaches the Multi-Qubit Gates kata. The tutor says:

> "Before we write code, let's build this circuit visually. I've opened the circuit editor -- try dragging an H gate onto the first qubit, then a CNOT from q0 to q1."

The circuit editor opens as a tab in VS Code. The user sees a blank canvas with two qubit wires. They drag gates from a palette and drop them onto wires. After placing H and CNOT, state visualization columns appear at each step:

- After H: the first qubit is in |+>, the second is in |0>
- After CNOT: the system is in the Bell state (|00> + |11>) / sqrt(2)

The tutor explains what the state columns mean:

> "See those probability columns? After the H gate, qubit 0 has a 50/50 chance of being |0> or |1>. After the CNOT, the two qubits are entangled -- they'll always agree. Now write the code that does the same thing."

The user switches to their solution file and writes the Q# equivalent, having already built the mental model visually.

### Debugging with state visualization

The user is stuck on a tricky exercise. Their code compiles but doesn't pass verification. The tutor suggests:

> "Let's step through your circuit visually and see where the state diverges from what we expect."

The tutor generates a circuit from the user's code and opens it in the editor with state columns enabled. The user can see that after step 3, the state is |01> when it should be |10> -- they applied the gate to the wrong qubit.

### Run from circuit

After building a circuit in the editor, the user can click "Run" to execute it directly. This provides an immediate feedback loop without writing any code -- useful for experimentation. The tutor might say:

> "Try adding a measurement at the end of your circuit and clicking Run. What result do you get? Run it a few more times -- do you always get the same result?"

## When to introduce it

**Mid-path, around multi-qubit gates.** The circuit editor is most valuable when circuits get complex enough that building them visually helps -- typically around 2-3 qubits with multiple gates. For single-qubit exercises, the circuit diagram (read-only) is sufficient.

The editor could also be introduced earlier as an optional tool: _"If you'd like to experiment visually before coding, I can open the circuit editor for you."_

## What already exists

The QDK extension already has a full circuit editor:

- A custom editor for `.qcirc` files that displays circuits with a gate palette and drag-and-drop editing.
- State visualization columns computed via a web worker, showing quantum state probabilities at each step of the circuit.
- A "Run" button that generates Q# from the circuit and executes it, displaying the result.
- Circuits can be generated from Q# code and opened in the editor.

No new editor features need to be built. The work is in:

- Having the tutor create `.qcirc` files for specific exercises and open them in the editor.
- Guiding the user through the visual-to-code workflow within the exercise cycle.

## Open questions

- **Scaffolding:** Should the editor open with a blank canvas, or pre-populated with some structure (e.g., qubit wires already laid out, some gates placed)?
- **Exercise integration:** Should "build in the editor" be a formal exercise step, or an optional side tool the user can request? Making it mandatory slows down experienced users; making it optional means some users never discover it.
- **Code round-trip:** After the user builds a circuit in the editor, should the tutor auto-generate the Q# code as a starting point, or should the user always translate manually? Translating manually is better for learning, but auto-generating helps users who are stuck on syntax.
- **Mobile / remote:** The circuit editor requires mouse interaction (drag and drop). Does this work well in remote VS Code sessions or Codespaces?
