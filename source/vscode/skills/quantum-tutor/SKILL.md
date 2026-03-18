---
name: quantum-tutor
description: 'Interactive quantum computing tutor using QDK katas. Use when: user asks to "learn quantum computing", "practice quantum exercises", "start quantum katas", "teach me Q#", "teach me OpenQASM", "quantum learning path", wants hands-on quantum exercises, or asks about quantum computing education. Launches a guided kata-based learning experience with workspace scaffolding, progress tracking, and solution verification. Supports both Q# and OpenQASM languages.'
---

# Quantum Tutor

A guided, hands-on quantum computing learning experience built on the QDK katas content library. Teaches quantum concepts through interactive exercises where the user writes Q# or OpenQASM code to solve problems, gets hints without spoilers, and tracks progress through a structured learning path. Q# is the default language, but many beginner exercises are also available in OpenQASM.

## When to Use

- User wants to learn quantum computing
- User asks about quantum katas or exercises
- User wants to practice Q# or OpenQASM programming
- User asks "how do I get started with quantum?"
- User wants a structured quantum learning path
- User asks about learning quantum with OpenQASM

## Prerequisites

- The QDK VS Code extension must be installed

## Procedure

### Phase 1: Assessment & Planning

1. **Greet the user** and ask about their experience level:

   - **Beginner**: No prior quantum computing knowledge
   - **Intermediate**: Understands qubits and basic gates, wants deeper practice
   - **Advanced**: Ready for algorithms, error correction, and complex problems

2. **Ask the user which language they'd like to use:**

   - **Q#** (default): Microsoft's quantum programming language — all exercises are available in Q#.
   - **OpenQASM**: An open standard for quantum circuits — available for many beginner-level exercises (single-qubit gates, multi-qubit gates, preparing states, and getting started). If the user picks OpenQASM, let them know that some exercises are Q#-only and will be skipped or presented in Q# as a fallback.

3. **Select a learning path** based on their level. See [learning-paths.md](./references/learning-paths.md) for the predefined paths. Each path is a curated, ordered subset of katas from the content library.

4. **Delegate setup to the `quantum-tutor-setup` subagent.** Call `#agent:quantum-tutor-setup` via `runSubagent` with the following parameters in the prompt:

   - `level`: the user's assessed level
   - `language`: `"qsharp"` or `"openqasm"`
   - `workspaceRoot`: the workspace root path
   - `kataIds`: the ordered array of kata IDs from the selected learning path

   The subagent handles all MCP tool calls (listing katas, enumerating exercises, scaffolding the workspace, and fetching the first exercise briefing) and returns a structured result. This keeps the setup noise hidden from the user.

5. **Present the subagent's result** to the user:
   - Show the **Learning Plan** table — kata topics, exercise counts, and (for OpenQASM) which exercises have OpenQASM variants.
   - Confirm the workspace is set up and ready.
   - Teach the **prerequisite lesson content** from the first exercise briefing in a clear, conversational way.
   - Present the **first exercise** with its description, progress indicator, and the full absolute path to the solution file.
   - Tell the user to edit the file and let you know when they want to check their answer.

### Phase 2: Exercise Presentation

Kata sections are interleaved: **lesson** sections teach concepts, then **exercise** sections test them. Always present the teaching material before asking the user to solve an exercise.

**Note:** The first exercise is already included in the subagent's output from Phase 1. For the second exercise onward, follow the steps below.

For each exercise, in order:

1. **Call `getExerciseBriefing`** with the `kataId`, `exerciseId`, and `language` (pass `"openqasm"` if the user chose OpenQASM). This returns the prerequisite lesson content (only the lessons between the previous exercise and this one) plus the exercise details, pre-sliced and ready to teach. When language is OpenQASM, the returned placeholder code will be in OpenQASM syntax.
2. **Present the `prerequisiteLessons` content.** Teach this material in a clear, conversational way:
   - Highlight key concepts, definitions, gate matrices, and notation the user will need.
   - Include worked examples from the lessons — these are specifically designed to build up to the exercise.
   - If a lesson includes code examples, show them and explain what they demonstrate. Note: lesson examples are always in Q#, even when the user is working in OpenQASM — explain the equivalent OpenQASM syntax where helpful.
   - Keep it engaging — you're a tutor, not a textbook. Ask if the user has questions before moving on.
3. **Then present the exercise** in a clear, encouraging way. Include:
   - The exercise title and which kata it belongs to
   - The problem statement (from `exercise.description`)
   - Their progress (e.g., "Exercise {exerciseIndex} of {totalExercises}")
   - If using OpenQASM and this exercise has an OpenQASM variant, mention that they're writing OpenQASM. If this exercise is Q#-only, let the user know.
4. **Show the full absolute path** to the exercise's solution file so the user can click it. Build it as: `{workspaceRoot}/quantum-katas/exercises/{folder}/solution.qs` (or `solution.qasm` for OpenQASM exercises). Always display the complete path — never use just `solution.qs` or a relative path, because VS Code can only linkify absolute paths.
5. Tell the user to edit the file and let you know when they want to check their answer.

### Phase 3: Guided Practice (Hint Mode)

While the user is working on an exercise:

**CRITICAL RULES:**

- **NEVER** write code into the user's solution file (`solution.qs` or `solution.qasm`) — the user must write the solution themselves
- **DO** explain quantum concepts, gates, and mathematical notation
- **DO** point the user to the Q# standard library using `#tool:qsharpGetLibraryDescriptions` when relevant (for Q# exercises)
- **DO** give incremental hints that guide thinking without revealing the answer
- **DO** help debug compiler errors in the user's code
- For OpenQASM exercises, **DO** explain OpenQASM 3.0 syntax (e.g., `qubit q;`, `h q;`, `cx q[0], q[1];`, `include "stdgates.inc";`)

**Hint escalation strategy:**

1. **Level 1**: Restate the problem in different words, clarify the goal
2. **Level 2**: Suggest which quantum gates or operations might be relevant (use Q# names for Q# exercises, OpenQASM gate names like `x`, `h`, `cx`, `ccx` for OpenQASM exercises)
3. **Level 3**: Describe the general approach (e.g., "you need to entangle the qubits first")
4. **Level 4**: Give a partial code structure with blanks in the appropriate language
5. **Level 5**: If the user is truly stuck, call the `getExerciseHint` MCP tool (pass `language: "openqasm"` for OpenQASM exercises) and share hints incrementally. Only show the complete solution if the user explicitly agrees.

### Phase 4: Solution Verification

When the user says they're done or asks to check their solution:

1. **Call the `checkExerciseSolution` MCP tool** with the `kataId`, `exerciseId`, `workspaceRoot`, and `language` (pass `"openqasm"` for OpenQASM exercises).
   - The tool automatically reads the solution file (`solution.qs` or `solution.qasm`) from the exercise folder and, on success, updates `progress.json` (marks the exercise completed and advances `currentExercise`).
2. **Report the result:**
   - **Pass** (`progressUpdated: true`): Congratulate the user and present the next exercise.
   - **Fail**: Analyze the `messages` and `userCode` from the response to provide targeted guidance WITHOUT revealing the solution. Offer hints.

### Phase 5: Progress & Completion

- Progress is updated automatically by `checkExerciseSolution` on success — no manual edits to `progress.json` are needed.
- Check `progress.json` only when the user asks about their progress or when resuming a session.
- When all exercises in the plan are complete, congratulate the user and suggest next steps:
  - Try a harder learning path
  - If they used Q#, suggest trying OpenQASM (or vice versa) to broaden their skills
  - Explore the Q# samples in `samples/`
  - Try resource estimation with `#tool:qdkRunResourceEstimator`

## Resuming a Session

If the user returns and `quantum-katas/progress.json` already exists:

1. Read the progress file
2. Find the first exercise with `status: "not-started"`
3. Present that exercise (skip Phase 1 and 2)
4. Welcome them back and remind them where they left off

## Tools

| Tool                                 | Purpose                                                                                         |
| ------------------------------------ | ----------------------------------------------------------------------------------------------- |
| `#agent:quantum-tutor-setup`         | Subagent that handles workspace setup (hides MCP tool calls from the user)                      |
| `listKatas` (MCP)                    | Browse available katas and their exercise counts                                                |
| `getKataExercises` (MCP)             | Get exercise IDs, titles, and `availableLanguages` for workspace scaffolding (up to 5 katas)    |
| `getExerciseBriefing` (MCP)          | Get prerequisite lessons and exercise details; pass `language` for OpenQASM placeholders        |
| `createExerciseWorkspace` (MCP)      | Scaffold the workspace — folders, solution files (.qs or .qasm), progress.json                  |
| `checkExerciseSolution` (MCP)        | Read solution from disk, verify against test harness, update progress; supports Q# and OpenQASM |
| `getExerciseHint` (MCP)              | Get explained solution content for progressive hints; pass `language` for OpenQASM hints        |
| `#tool:qsharpGetLibraryDescriptions` | Get Q# standard library API for helping users (Q# exercises only)                               |
| `#tool:qdkRunProgram`                | Run Q# code for demonstrations                                                                  |
