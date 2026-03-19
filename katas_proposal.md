# Quantum Katas

## Problem

The Quantum Katas experience at https://quantum.microsoft.com/en-us/tools/quantum-katas relies on a custom chat API backed by a hosted large language model (LLM). This implementation is several years old and has not kept pace with recent advances in LLM capabilities. As a result, it is significantly outdated and does not meet current user expectations. In addition, maintaining a bespoke chat API is increasingly difficult to justify given the broad range of Copilot-style solutions available today.

A further challenge is the ongoing effort required to maintain this experience on the website, which constrains what we can deliver. By contrast, the Quantum Development Kit (the VS Code extension and Python libraries) provides rich tooling, including support for multiple programming languages (OpenQASM, Python, Q#) and interactive visual capabilities such as circuit visualization and resource estimation. Hosting the learning experience on the website limits our ability to leverage these tools. At best, we would need to duplicate significant functionality to provide a comparable experience in the browser.

## Solution

To better align the learning experience with the environment in which users actually work with the Quantum Development Kit (QDK), we should move the Katas into the VS Code extension. This approach also allows us to leverage GitHub Copilot -- already available in VS Code -- to deliver a richer, AI-assisted learning experience without maintaining a separate chat stack.

### What do we gain?

- Learning directly in a real coding environment -- alongside the QDK's actual tools -- creates a smoother transition from guided exercises to productive use.
- GitHub Copilot is a capable -- and rapidly improving -- platform that we can tailor as an "AI tutor," reducing the need to build and maintain our own AI system.
- Learners can keep their workspace and progress as local files, making it easy to revisit, share, and continue outside the guided flow.

### What are the drawbacks?

- This is no longer a web-only experience; it requires installing VS Code on the desktop.
- Using GitHub Copilot requires a GitHub account (where previously no sign-in was required).
- We will not have complete control over the end-to-end experience or agent behavior, since users can customize models, settings, and tools within VS Code. Of course, VS Code itself is also an application that evolves independently.

## Experience

### 1. Starting the experience

The user prompts GitHub Copilot directly or indirectly.

**Indirectly** -- the user asks a quantum computing question:

> "I don't really understand how Shor's algorithm works."

The AI tutor recognizes the learning opportunity and responds with an offer:

> "If you'd like to build up to understanding Shor's algorithm hands-on -- starting with qubits, superposition, phase estimation, and QFT -- I can set up an interactive quantum learning path with exercises you solve in Q#. Want to try that?"

**Directly** -- the user explicitly requests the experience:

> "Start the quantum katas"

This starts the experience right away.

### 2. Learning path customization

The tutor asks the user a few questions about their background and goals:

- **Experience level:** Are they new to quantum computing, or do they already know the basics?
- **Language preference:** Would they like to work in Q# (recommended) or OpenQASM?
- **Interest area:** Do they want a broad introduction, or are they aiming at a specific topic like quantum algorithms or error correction?

Based on these answers, the tutor selects one of several curated learning paths:

| Path             | Audience                               | Topics covered                                                                   |
| ---------------- | -------------------------------------- | -------------------------------------------------------------------------------- |
| **Beginner**     | New to quantum computing               | Qubits, single- and multi-qubit gates, state preparation, measurement            |
| **Intermediate** | Comfortable with gates and measurement | State discrimination, quantum protocols (teleportation, superdense coding, QKD)  |
| **Advanced**     | Ready for algorithms                   | Oracles, Deutsch-Jozsa, Grover's search, QFT, phase estimation, error correction |
| **Full**         | End-to-end                             | All 26 katas in sequence                                                         |
| **Custom**       | Specific goal in mind                  | Tutor assembles a path from individual katas, respecting prerequisites           |

### 3. Workspace initialization

Once the path is chosen, the tutor silently sets up the learner's workspace:

- A `quantum-katas/` folder is created in the user's project.
- Inside it, each exercise gets its own numbered subfolder (e.g., `01_flip_qubit/`) containing a starter code file (`solution.qs` or `solution.qasm`) that the user will edit.
- A `progress.json` file tracks which exercises are completed and which is next.

This all happens automatically -- the user sees a summary of their learning plan and is immediately presented with the first exercise.

### 4. Exercises

Each exercise follows a consistent cycle:

**a) Lesson.** Before the coding exercise, the tutor teaches the prerequisite concept conversationally -- for example, explaining what the X gate does, how it transforms qubit states, and showing its matrix representation. This content is drawn from the curated kata lessons and adapted by the tutor to match the user's level.

**b) Challenge.** The tutor presents the exercise: a short problem description (e.g., _"Apply a gate to flip the qubit from |0> to |1>"_) and opens the starter code file for the user to edit.

**c) Guided practice.** The user writes their solution in the code file. If they get stuck, they ask the tutor for help. The tutor provides increasingly specific hints -- first restating the problem, then suggesting which gates to consider, then describing an approach, and finally offering partial code with blanks to fill in. Crucially, **the tutor never writes the answer directly into the user's file** -- the learner always completes the code themselves.

**d) Verification.** When the user is ready, they ask the tutor to check their solution. The tutor runs the code against the exercise's built-in tests and reports whether it passes. If it doesn't, the tutor explains what went wrong and encourages the user to try again.

**e) Progress.** On success, the exercise is marked complete, and the tutor moves on to the next one. The user can see at any time how far along they are (e.g., _"Exercise 5 of 18"_).

### 5. Session resumption

The user can close VS Code at any time and come back later. When they return and ask the tutor to continue, it reads the progress file, finds where they left off, and picks up from the next incomplete exercise -- no repeated setup, no lost work.

### 6. Completion and next steps

When the user finishes all exercises on their path, the tutor congratulates them and suggests next steps:

- Try a more advanced learning path.
- Explore the QDK sample programs to see real quantum algorithms in action.
- Try writing their own quantum programs with Copilot's help.
- Experiment with resource estimation to understand what it takes to run an algorithm on real hardware.

## Content

The experience draws from 26 katas covering the full spectrum of quantum computing topics:

| Category               | Katas                                                                                                              |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------ |
| **Foundations**        | Getting Started, Complex Arithmetic, Linear Algebra, Qubit                                                         |
| **Gates & State Prep** | Single-Qubit Gates, Multi-Qubit Systems, Multi-Qubit Gates, Preparing States                                       |
| **Measurement**        | Single-Qubit Measurements, Multi-Qubit Measurements, Distinguishing States, Distinguishing Unitaries               |
| **Protocols**          | Random Numbers, Key Distribution, Teleportation, Superdense Coding                                                 |
| **Algorithms**         | Oracles, Marking Oracles, Deutsch's Algorithm, Deutsch-Jozsa, Grover's Search, Solving SAT, Solving Graph Coloring |
| **Advanced**           | QFT, Phase Estimation, Shor's Code (QEC)                                                                           |

Each kata contains a mix of conceptual lessons (with math, diagrams, and worked examples) and hands-on coding exercises. There are over 100 exercises in total.

### Language support

All exercises are available in **Q#**. A growing subset (currently covering the Beginner path) also supports **OpenQASM**, letting users work in the language they prefer. When working in OpenQASM, the tutor still explains core concepts using Q# examples and bridges the syntax differences.

## Open questions

- **Python support.** Many quantum computing learners are most comfortable in Python. Should we add a Python path using the `qsharp` Python package? This would expand the audience significantly but requires creating new exercise content and a verification mechanism.
- **Discoverability.** How do users find out this experience exists? Options include surfacing it in the QDK extension's welcome view, documenting it on the quantum website, and mentioning it in Copilot's responses when users ask quantum computing questions.
- **Content updates.** The kata content lives in the QDK repository and ships with the extension. How do we keep the content fresh and add new katas over time without requiring users to update the extension for every content change?
