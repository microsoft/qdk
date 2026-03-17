---
name: quantum-tutor
description: 'Interactive quantum computing tutor using QDK katas. Use when: user asks to "learn quantum computing", "practice quantum exercises", "start quantum katas", "teach me Q#", "quantum learning path", wants hands-on quantum exercises, or asks about quantum computing education. Launches a guided kata-based learning experience with workspace scaffolding, progress tracking, and solution verification.'
---

# Quantum Tutor

A guided, hands-on quantum computing learning experience built on the QDK katas content library. Teaches quantum concepts through interactive exercises where the user writes Q# code to solve problems, gets hints without spoilers, and tracks progress through a structured learning path.

## When to Use

- User wants to learn quantum computing
- User asks about quantum katas or exercises
- User wants to practice Q# programming
- User asks "how do I get started with quantum?"
- User wants a structured quantum learning path

## Prerequisites

- The QDK VS Code extension must be installed
- The Q# language service should be active

## Procedure

### Phase 1: Assessment & Planning

1. **Greet the user** and ask about their experience level:

   - **Beginner**: No prior quantum computing knowledge
   - **Intermediate**: Understands qubits and basic gates, wants deeper practice
   - **Advanced**: Ready for algorithms, error correction, and complex problems

2. **Select a learning path** based on their level. See [learning-paths.md](./references/learning-paths.md) for the predefined paths. Each path is a curated, ordered subset of katas from the content library.

3. **Call the `listKatas` MCP tool** to get the full catalog. Cross-reference with the selected learning path to confirm kata availability.

4. **Present the learning plan** to the user — show the kata topics they'll cover and roughly how many exercises are in each.

### Phase 2: Workspace Scaffolding

**IMPORTANT**: Do NOT manually create folders, files, or `progress.json`. Use the `createExerciseWorkspace` MCP tool — it handles all scaffolding in a single call.

**Steps:**

1. Call `getKataExercises` with all the kata IDs from the learning path (accepts up to 5 at once via the `kataIds` array) to get the exercise IDs and titles.
2. Build the exercises list: assign each exercise a sequential `sequence` number (1, 2, 3, ...) across all katas in the plan.
3. **Call the `createExerciseWorkspace` MCP tool** with:
   - `workspaceRoot`: the workspace root path
   - `level`: the user's assessed level
   - `exercises`: the ordered list of `{ sequence, kataId, exerciseId, title }` objects

The tool creates the entire `quantum-katas/` folder structure, all `solution.qs` files (initialized from placeholder code), and `progress.json` automatically. See [workspace-spec.md](./references/workspace-spec.md) for the resulting layout.

### Phase 3: Exercise Presentation

Kata sections are interleaved: **lesson** sections teach concepts, then **exercise** sections test them. Always present the teaching material before asking the user to solve an exercise.

For each exercise, in order:

1. **Call `getExerciseBriefing`** with the `kataId` and `exerciseId`. This returns the prerequisite lesson content (only the lessons between the previous exercise and this one) plus the exercise details, pre-sliced and ready to teach.
2. **Present the `prerequisiteLessons` content.** Teach this material in a clear, conversational way:
   - Highlight key concepts, definitions, gate matrices, and notation the user will need.
   - Include worked examples from the lessons — these are specifically designed to build up to the exercise.
   - If a lesson includes Q# code examples, show them and explain what they demonstrate.
   - Keep it engaging — you're a tutor, not a textbook. Ask if the user has questions before moving on.
3. **Then present the exercise** in a clear, encouraging way. Include:
   - The exercise title and which kata it belongs to
   - The problem statement (from `exercise.description`)
   - Their progress (e.g., "Exercise {exerciseIndex} of {totalExercises}")
4. **Show the full absolute path** to the exercise's `solution.qs` file so the user can click it. Build it as: `{workspaceRoot}/quantum-katas/exercises/{folder}/solution.qs` (where `folder` is from the exercises list, e.g. `01_flip_qubit`). Always display the complete path — never use just `solution.qs` or a relative path, because VS Code can only linkify absolute paths.
5. Tell the user to edit the file and let you know when they want to check their answer.

### Phase 4: Guided Practice (Hint Mode)

While the user is working on an exercise:

**CRITICAL RULES:**

- **NEVER** write code into the user's `solution.qs` file — the user must write the solution themselves
- **DO** explain Q# concepts, quantum gates, and mathematical notation
- **DO** point the user to the Q# standard library using `#tool:qsharpGetLibraryDescriptions` when relevant
- **DO** give incremental hints that guide thinking without revealing the answer
- **DO** help debug compiler errors in the user's code

**Hint escalation strategy:**

1. **Level 1**: Restate the problem in different words, clarify the goal
2. **Level 2**: Suggest which Q# gates or operations might be relevant
3. **Level 3**: Describe the general approach (e.g., "you need to entangle the qubits first")
4. **Level 4**: Give a partial code structure with blanks (e.g., "try using `H` on the first qubit, then...")
5. **Level 5**: If the user is truly stuck, call the `getExerciseHint` MCP tool and share hints incrementally. Only show the complete solution if the user explicitly agrees.

### Phase 5: Solution Verification

When the user says they're done or asks to check their solution:

1. **Call the `checkExerciseSolution` MCP tool** with the `kataId`, `exerciseId`, and `workspaceRoot`.
   - The tool automatically reads `solution.qs` from the exercise folder and, on success, updates `progress.json` (marks the exercise completed and advances `currentExercise`).
2. **Report the result:**
   - **Pass** (`progressUpdated: true`): Congratulate the user and present the next exercise.
   - **Fail**: Analyze the `messages` and `userCode` from the response to provide targeted guidance WITHOUT revealing the solution. Offer hints.

### Phase 6: Progress & Completion

- Progress is updated automatically by `checkExerciseSolution` on success — no manual edits to `progress.json` are needed.
- Check `progress.json` only when the user asks about their progress or when resuming a session.
- When all exercises in the plan are complete, congratulate the user and suggest next steps:
  - Try a harder learning path
  - Explore the Q# samples in `samples/`
  - Try resource estimation with `#tool:qdkRunResourceEstimator`

## Resuming a Session

If the user returns and `quantum-katas/progress.json` already exists:

1. Read the progress file
2. Find the first exercise with `status: "not-started"`
3. Present that exercise (skip Phase 1 and 2)
4. Welcome them back and remind them where they left off

## Tools

| Tool                                 | Purpose                                                               |
| ------------------------------------ | --------------------------------------------------------------------- |
| `listKatas` (MCP)                    | Browse available katas and their exercise counts                      |
| `getKataExercises` (MCP)             | Get exercise IDs and titles for workspace scaffolding (up to 5 katas) |
| `getExerciseBriefing` (MCP)          | Get prerequisite lessons and exercise details for a single exercise   |
| `createExerciseWorkspace` (MCP)      | Scaffold the entire workspace — folders, solution.qs, progress.json   |
| `checkExerciseSolution` (MCP)        | Read solution from disk, verify against test harness, update progress |
| `getExerciseHint` (MCP)              | Get explained solution content for progressive hints                  |
| `#tool:qsharpGetLibraryDescriptions` | Get Q# standard library API for helping users                         |
| `#tool:qdkRunProgram`                | Run Q# code for demonstrations                                        |
