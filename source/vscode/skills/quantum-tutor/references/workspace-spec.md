# Workspace Structure Specification

The quantum-tutor skill creates a deterministic folder structure in the user's workspace root.

## Layout

```text
quantum-katas/
├── progress.json
└── exercises/
    ├── 01_flip_qubit/
    │   └── solution.qs        ← Q# exercise
    ├── 02_learn_single_qubit_state/
    │   └── solution.qs
    ├── 03_state_flip/
    │   └── solution.qasm      ← OpenQASM exercise (when language is openqasm)
    └── ...
```

When the user chooses OpenQASM, exercises that have an OpenQASM variant get a `solution.qasm` file (initialized from `Placeholder.qasm`). Exercises without an OpenQASM variant fall back to `solution.qs` (initialized from `Placeholder.qs`).

## Naming Convention

Exercise folders are named: `<NN>_<exercise_id>`

- `NN` — Two-digit sequence number, zero-padded (01, 02, ... 99)
- `exercise_id` — The exercise folder name from `katas/content/<kata_id>/<exercise_id>/`
- Numbers are assigned sequentially across all katas in the learning path

## `progress.json` Schema

```json
{
  "level": "beginner | intermediate | advanced | custom",
  "language": "qsharp | openqasm",
  "startedAt": "2026-03-12T10:00:00Z",
  "currentExercise": 0,
  "exercises": [
    {
      "sequence": 1,
      "kataId": "getting_started",
      "exerciseId": "flip_qubit",
      "title": "Flip Qubit",
      "folder": "01_flip_qubit",
      "status": "not-started | in-progress | completed",
      "completedAt": null
    }
  ]
}
```

### Fields

| Field                     | Type    | Description                                                    |
| ------------------------- | ------- | -------------------------------------------------------------- |
| `level`                   | string  | The learning path level chosen during assessment               |
| `language`                | string  | The programming language: `"qsharp"` (default) or `"openqasm"` |
| `startedAt`               | string  | ISO 8601 timestamp of when the learning path was created       |
| `currentExercise`         | number  | Zero-based index into the `exercises` array                    |
| `exercises`               | array   | Ordered list of exercises in the learning plan                 |
| `exercises[].sequence`    | number  | One-based display number for the exercise                      |
| `exercises[].kataId`      | string  | The kata folder name in `katas/content/`                       |
| `exercises[].exerciseId`  | string  | The exercise ID as returned by `listKatas`                     |
| `exercises[].title`       | string  | Human-readable exercise title from the `@[exercise]` macro     |
| `exercises[].folder`      | string  | The folder name in `quantum-katas/exercises/`                  |
| `exercises[].status`      | string  | Current status of this exercise                                |
| `exercises[].completedAt` | string? | ISO 8601 timestamp when completed, or null                     |

## Solution File Content

Each solution file is initialized with the content of the appropriate placeholder from the corresponding exercise in `katas/content/`:

- **Q# exercises**: `solution.qs` is initialized from `Placeholder.qs` — copied verbatim.
- **OpenQASM exercises**: `solution.qasm` is initialized from `Placeholder.qasm` — copied verbatim.

Do not modify the placeholder code.

## Creating the Workspace

**Always use the `createExerciseWorkspace` MCP tool to scaffold the workspace.** Do NOT manually create folders, write solution files, or generate `progress.json` by hand. The tool accepts a `workspaceRoot`, `level`, `language` (optional — defaults to `"qsharp"`), and an ordered array of exercises, and creates the entire structure in a single call — including all directories, solution files (`.qs` or `.qasm` as appropriate), and `progress.json`.

## Discovery: Finding Exercises Within a Kata

Use the `listKatas` MCP tool with `includeSections: true` to enumerate exercise IDs, titles, and available languages for workspace scaffolding. Exercises are returned in the order they appear in each kata.

To retrieve the teaching content for a specific exercise (prerequisite lessons and exercise details), use the `getExerciseBriefing` MCP tool with a `kataId` and `exerciseId`. It returns only the lessons between the previous exercise and this one, pre-sliced and ready to present.
