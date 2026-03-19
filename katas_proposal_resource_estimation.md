# Feature Proposal: Resource Estimation in Quantum Katas

## Summary

At the end of the advanced learning path, the tutor runs resource estimation on algorithms the user has built -- showing how many physical qubits, T-factories, and how much time would be needed to run their code on real quantum hardware. This connects the abstract exercises to the realities of building a quantum computer.

## Motivation

Most quantum computing tutorials end with "congratulations, you understand Grover's algorithm." But the obvious next question -- "could this actually run on a real quantum computer?" -- goes unanswered. Resource estimation bridges that gap. It turns a theoretical exercise into a concrete engineering question: how big, how fast, how expensive?

For learners who have worked through the advanced path (oracles, Grover's search, QFT, phase estimation, error correction), resource estimation is the capstone that ties everything together. They've learned what error correction is and why it matters. Now they see _how much_ error correction a real algorithm requires -- and they understand why.

This is also a strategic feature to showcase. Resource estimation is a differentiating capability of the QDK. Many learners may not know it exists. Introducing it as the "grand finale" of the katas ensures every advanced user discovers it.

## What the user sees

### Capstone moment

The user finishes the final exercise in the advanced path (Shor's code / QEC). The tutor says:

> "You've built quantum algorithms from the ground up -- from single gates all the way to error correction. Let's see what it would take to run one of these algorithms on actual quantum hardware."
>
> "I'll run resource estimation on the Grover's search algorithm you wrote earlier."

The resource estimation panel opens, showing:

- **Physical qubits:** e.g., 11,416
- **Runtime:** e.g., 38 microseconds
- **T-factories:** e.g., 12, each using 7,680 physical qubits
- **Code distance:** e.g., 15
- **Error budget:** 0.1%

The tutor walks through the key numbers:

> "Your algorithm needs about 11,000 physical qubits. Today's largest quantum computers have around 1,000. The T-factories alone -- each one manufactures the T-states your algorithm consumes -- need most of those qubits. This is why scaling quantum hardware is such a hard engineering challenge, and why the error correction you just learned about is so critical."

### Scale intuition

The tutor then runs the same algorithm at a larger problem size:

> "That was for a small input. Let's see what happens when we scale up."

A second estimation run appears in the same panel, and the user can compare:

|                 | Small input | Large input |
| --------------- | ----------- | ----------- |
| Physical qubits | 11,416      | 4,200,000   |
| Runtime         | 38 us       | 12 hours    |
| T-factories     | 12          | 198         |

> "This is the core challenge of quantum computing: algorithms that are elegant at small scale require enormous resources at useful scale. The work you've done in this learning path -- understanding gates, measurement, and especially error correction -- is exactly the foundation needed to tackle this challenge."

### Comparing hardware

The tutor can also show estimates across different hardware architectures:

> "Different quantum hardware technologies have different tradeoffs. Here's the same algorithm on superconducting qubits vs. trapped-ion qubits."

The panel shows multiple runs (the tool already supports selecting multiple qubit types), making the hardware landscape tangible.

## When to introduce it

**End of the advanced path only.** Resource estimation requires context to be meaningful -- the user needs to understand what qubits, gates, T-states, and error correction _are_ before the numbers make sense. Showing it too early would be confusing; showing it as the finale is powerful.

The tutor can mention it earlier as a teaser: _"Later, we'll see what it would actually take to run this on a real quantum computer."_

## What already exists

The QDK extension already has full resource estimation support:

- A "Show Resource Estimates" command that lets the user select qubit types and error budget, then displays detailed results in an interactive panel.
- The `qdkRunResourceEstimator` copilot tool accepts a file path and optional parameters, runs the estimation, and opens the results panel.
- The results panel supports multiple named runs for comparison, with per-run detail expansion.
- Eight qubit type options are available, spanning superconducting, trapped-ion, and Majorana architectures.

No new features need to be built. The work is in having the tutor call resource estimation at the right moment and narrate the results meaningfully.

## Open questions

- **Which algorithm to estimate?** Should the tutor estimate the user's own code, or a pre-built example at realistic scale? The user's exercises may be too small to produce interesting numbers.
- **Simplification:** The resource estimation output is very detailed (dozens of metrics). How much should the tutor simplify vs. expose? For a learning context, focusing on physical qubits, runtime, and T-factories is probably enough.
- **Earlier teasers:** Should intermediate-path users get a brief taste of resource estimation (e.g., on teleportation), even if they haven't learned error correction yet? It could motivate them to continue to the advanced path.
