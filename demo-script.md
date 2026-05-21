# QDK + GitHub Copilot Live Demo Script

**Audience:** Students new to quantum computing
**Core message:** You can start exploring quantum computing right now with AI as your guide, even before you understand everything.
**Duration:** ~10-12 minutes

---

## 1. Introduction (~1 min)

The Microsoft Quantum Development Kit integrates tightly with VS Code and GitHub Copilot, so you can use AI to help you learn and explore quantum computing — even if you're just getting started.

To follow along on your own, install the QDK from aka.ms/QDK.install.

**Action:** Open VS Code, show Copilot chat is available, briefly show model selection.

---

## 2. Circuit Image to Code (~3 min)

> "I'm not going to write a single line of code in this demo."

**Setup:** Drop in a screen clipping of a quantum teleportation circuit.

**Prompt:** Ask Copilot what the circuit is and to write the Q# code to implement it.

**Talking point:** "Teleportation is one of the fundamental protocols in quantum computing — and Copilot recognized it from just a picture and generated working code."

**Action:** Show the generated code runs correctly.

---

## 3. Explain This Code to Me (~2-3 min)

> This is where Copilot becomes your personal tutor.

**Prompt:** Ask Copilot to explain how the teleportation code works.

**Talking point:** "If you're learning, you don't have to just stare at code and hope it makes sense. You can ask questions in plain English."

**Prompt:** Ask a follow-up like "what does the within/apply pattern do in Q#?"

**Talking point:** "You can use this to learn the language itself, not just the algorithms."

---

## 4. Bug Fix (~2-3 min)

> Everyone's had a typo break their code.

**Action:** Manually introduce a bug (e.g., swap a gate, duplicate a line). Show it no longer works.

**Prompt:** Start a fresh Copilot session. Ask it to find and fix the bug.

**Talking point:** "I didn't tell it what the bug was. It read the code, understood the intent, identified the mistake, and fixed it."

---

## 5. Learning Panel — Guided Exercises with AI Tutoring (~3-4 min)

> "Whether you're writing quantum programs or learning quantum computing, the tools meet you where you are."

**Action:** Open the QDK Learning panel in VS Code (comes free with the extension).

**Talking point:** "The Quantum Katas is a complete quantum computing course built right into VS Code. Human-written content, structured lessons, hands-on exercises — and Copilot is there to help whenever you need it."

**Prompt:** Ask Copilot to take you to the Grover's search unit.

**Talking point:** "This is a more advanced algorithm — one of the most famous in quantum computing. It searches through possibilities faster than any classical computer can. And here's a guided exercise to help you understand it."

**Prompt:** Ask Copilot for a hint on the exercise.

**Action:** While it responds, type in the solution. Show the built-in checker validating it.

**Talking point:** "Each exercise is a small Q# problem designed to teach one concept. You get hints from Copilot, you write the code, and a checker tells you if you got it right. It's like having a tutor available 24/7."

---

## 6. Closing & Challenge Launch (~1 min)

In about 10 minutes, Copilot wrote quantum code from a picture, explained how it works, taught me language features, and fixed a bug — all without me writing a single line of code.

Now it's your turn. You have 10 minutes. Here are your challenges.

---

## Challenge Slide

> Show this slide after the demo. Participants work on their laptops. Whoever gets the furthest wins. Ties broken by random drawing.

### Challenge 1: Install & Run (Easy)

Install the QDK extension in VS Code and run any Q# program — a sample, a Hello World, anything. Show me output in your terminal.

**How to verify:** Walk up and show me your screen with Q# output visible.

---

### Challenge 2: Match the Histogram (Medium)

_[Show a target histogram on the projector — e.g., a Bell state: 50% |00⟩, 50% |11⟩]_

Write (or ask Copilot to write) a Q# program that produces this measurement histogram. Show me the matching output.

**How to verify:** Walk up and show me your histogram matches the target.

---

### Challenge 3: Implement the Circuit (Hard)

_[Show a circuit diagram on the projector — e.g., a 3-qubit GHZ state or a simple Grover's iteration]_

Implement this circuit in Q# and show me the generated circuit diagram AND the measurement histogram.

**How to verify:** Walk up and show me both the circuit diagram and histogram on your screen.

---

### Grand Challenge: Solve a Kata (Hardest)

Open the QDK Learning Panel. Navigate to Single Qubit Gates. Complete Exercise 3. Show me the green checkmark.

**How to verify:** Walk up and show me the green check in the Learning Panel.

---

## Contest Rules

- **Time limit:** 10 minutes
- **Scoring:** Furthest challenge completed wins. Ties broken by random drawing.
- **Tools allowed:** Copilot, Google, friends, anything. This isn't a closed-book exam — it's a race.
- **Verification:** Walk up and show me your laptop screen. No screenshots needed, no data collected.
- **Prizes:** 1-2 grand prizes for the top finishers.

**Tip to announce:** "You can use Copilot to help you with all of these. I literally just showed you how."
