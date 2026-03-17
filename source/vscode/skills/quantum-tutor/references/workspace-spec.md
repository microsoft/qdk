# Workspace Structure Specification

The quantum-tutor skill creates a deterministic folder structure in the user's workspace root.

## Layout

```text
quantum-katas/
├── progress.json
└── exercises/
    ├── 01_flip_qubit/
    │   └── solution.qs
    ├── 02_learn_single_qubit_state/
    │   └── solution.qs
    ├── 03_state_flip/
    │   └── solution.qs
    └── ...
```

## Naming Convention

Exercise folders are named: `<NN>_<exercise_id>`

- `NN` — Two-digit sequence number, zero-padded (01, 02, ... 99)
- `exercise_id` — The exercise folder name from `katas/content/<kata_id>/<exercise_id>/`
- Numbers are assigned sequentially across all katas in the learning path

## `progress.json` Schema

```json
{
  "level": "beginner | intermediate | advanced | custom",
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

| Field                     | Type    | Description                                                |
| ------------------------- | ------- | ---------------------------------------------------------- |
| `level`                   | string  | The learning path level chosen during assessment           |
| `startedAt`               | string  | ISO 8601 timestamp of when the learning path was created   |
| `currentExercise`         | number  | Zero-based index into the `exercises` array                |
| `exercises`               | array   | Ordered list of exercises in the learning plan             |
| `exercises[].sequence`    | number  | One-based display number for the exercise                  |
| `exercises[].kataId`      | string  | The kata folder name in `katas/content/`                   |
| `exercises[].exerciseId`  | string  | The exercise ID as returned by `getKataExercises`          |
| `exercises[].title`       | string  | Human-readable exercise title from the `@[exercise]` macro |
| `exercises[].folder`      | string  | The folder name in `quantum-katas/exercises/`              |
| `exercises[].status`      | string  | Current status of this exercise                            |
| `exercises[].completedAt` | string? | ISO 8601 timestamp when completed, or null                 |

## `solution.qs` Content

Each `solution.qs` file is initialized with the content of `Placeholder.qs` from the corresponding exercise in `katas/content/`. The file should be copied verbatim — do not modify the placeholder code.

## Creating the Workspace

**Always use the `createExerciseWorkspace` MCP tool to scaffold the workspace.** Do NOT manually create folders, write `solution.qs` files, or generate `progress.json` by hand. The tool accepts a `workspaceRoot`, `level`, and an ordered array of exercises, and creates the entire structure in a single call — including all directories, `solution.qs` files (initialized from placeholder code), and `progress.json`.

## Discovery: Finding Exercises Within a Kata

Use the `getKataExercises` MCP tool with a `kataIds` array (up to 5 IDs at once) to enumerate exercise IDs and titles for workspace scaffolding. Exercises are returned in the order they appear in each kata.

To retrieve the teaching content for a specific exercise (prerequisite lessons and exercise details), use the `getExerciseBriefing` MCP tool with a `kataId` and `exerciseId`. It returns only the lessons between the previous exercise and this one, pre-sliced and ready to present.
