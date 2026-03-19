# Feature Proposal: Histograms in Quantum Katas

## Summary

When the user reaches measurement exercises, the tutor runs their code multiple times and displays a histogram of measurement outcomes. This makes probability distributions visible and concrete -- the user doesn't just read that a qubit in superposition has a 50/50 chance, they _see_ it.

## Motivation

Measurement is the moment quantum computing becomes tangible. Before measurement, everything is abstract -- superposition, amplitudes, phases. After measurement, you get actual 0s and 1s. But a single measurement result doesn't tell you much. Running the program many times and seeing the distribution is what makes quantum behavior real.

The current katas experience verifies solutions programmatically but never shows the user what their code _produces_ at scale. A histogram of 100 or 1000 shots turns the abstract into the observable. It's the difference between "your code is correct" and "look -- your code produces a uniform random distribution, just like a real quantum random number generator would."

## What the user sees

### Measurement exercises

The user completes a measurement exercise. The tutor says:

> "Let's see your measurement in action. I'll run your code 100 times."

A histogram panel opens in VS Code showing two bars: |0> at roughly 50 shots and |1> at roughly 50 shots. The tutor explains:

> "As expected, measuring a qubit in the |+> state gives |0> and |1> with roughly equal probability. The slight variation is normal -- this is genuinely random."

### Exploring superposition during lessons

Before the user attempts a measurement exercise, the tutor demonstrates the concept by running a pre-built snippet:

> "Watch what happens when we put a qubit in superposition and measure it 200 times."

The histogram appears, showing the characteristic 50/50 split. Then the tutor modifies the example:

> "Now let's apply an Ry rotation instead of H. Notice how the distribution shifts."

A new histogram shows a 75/25 split, and the user can see how gate parameters affect measurement probabilities.

### Random number generation

The random numbers kata is an ideal showcase. The user builds a quantum random number generator, and the tutor runs it 1000 times:

> "Here's the output of your quantum random number generator over 1000 runs. Notice the distribution is close to uniform -- each outcome appears roughly the same number of times. This randomness comes from quantum mechanics, not from a pseudo-random algorithm."

### Bonus: visual verification

For some exercises, the histogram _is_ the verification. Instead of just "pass/fail," the tutor can say:

> "Your code should produce |00> and |11> with equal probability, and never produce |01> or |10>. Let's run it and check."

The histogram shows two bars (|00> and |11>) at ~50% each. This is far more informative than a boolean pass/fail.

## When to introduce it

**First measurement exercise.** Histograms appear when the user reaches the "Single-Qubit Measurements" kata -- the earliest point where measurement outcomes are meaningful. This is typically the user's 8th or 9th kata in the Beginner path, so they've already seen circuit diagrams by this point.

## What already exists

The QDK extension already has histogram support:

- A "Show Histogram" command that runs a Q# program with configurable shot count and displays results in an interactive bar chart panel.
- The `qdkRunProgram` copilot tool accepts a `shots` parameter; when shots > 1, it automatically opens the histogram panel and returns the distribution data.
- The histogram panel updates in real time as shots complete, which creates an engaging live animation effect.
- Noisy simulation is also supported, which could be used for more advanced exercises exploring decoherence.

No new visualization features need to be built. The work is in having the tutor call `qdkRunProgram` with appropriate shot counts at the right moments.

## Open questions

- **Shot count:** What's the right default? 100 shots is fast and shows the pattern. 1000 shots is smoother but slower. Should the tutor adjust based on exercise complexity?
- **Live updates:** The histogram can update in real time as shots run. This is visually engaging but might be distracting during a lesson. Should we show the live animation or just the final result?
- **Noisy simulation:** Should any katas exercises use the noisy simulator to show decoherence effects? This could be a compelling "what goes wrong in real hardware" demonstration for the advanced path.
