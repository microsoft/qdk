## Test Report: qdk-learning-\* Tools

### Overall Assessment

All tools are **functional** and the content is **accurate and doable**. I completed the entire "Measurements in Multi-Qubit Systems" unit (15/15 activities, 5 exercises solved). All exercises had solvable problems, correct test harnesses, and matching reference solutions.

### Tool Output Size Issues

| Tool                                  | Typical Size  | Assessment                                                                                                                                             |
| ------------------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `get-progress`                        | **92KB**      | **Excessively large** — returns all 387 activities across 26 units. Should support filtering by unit.                                                  |
| `next`/`goto` on lessons              | **8KB–224KB** | **Some wildly large** — the rendered HTML with KaTeX SVG markup dominates. The "Full Measurements" lesson was 224KB, "Partial Measurements" was 141KB. |
| `list-units`                          | 11KB          | Reasonable for 26 units.                                                                                                                               |
| `check`/`hint`/`solution`/`read-code` | 8–19KB        | Moderate. Every response redundantly includes the full state + HTML description (~6KB of boilerplate per call).                                        |
| `get-state`                           | 8KB           | Acceptable.                                                                                                                                            |
| `run` on examples                     | 11KB          | Fine.                                                                                                                                                  |

**Recommendation**: Strip the rendered KaTeX HTML from tool outputs — the LLM doesn't need SVG path data. Return markdown or LaTeX source instead. Also, stop repeating the full exercise description in every tool response.

### Bugs Found

1. **Generic error messages** — `check`, `hint`, `solution` on a lesson all return bare `"An error occurred."` with no detail. `read-code` returns a proper message (`"Current activity is not an exercise or example — there is no code to read."`), proving the others _could_ do the same.

2. **`goto` on invalid unit/activity returns generic `"An error occurred."`** — Should say "Unit not found" or "Activity not found."

3. **`goto` does not mark activities as complete** — Using `goto` to jump to exercises skips the intervening lessons, leaving them permanently incomplete. Only `next` marks the departing activity as complete. This is a significant issue for LLM-driven navigation since agents naturally use `goto` to skip to exercises.

4. **`hint` returns `hints: []` but state shows `hintCount: 2`** — Tested on a completed exercise. Possible that hints are suppressed post-completion, but the response is contradictory.

5. **`reset` executes without confirmation** — The tool description says "Destructive — requires confirmation" but it executes immediately, resetting the exercise code and clearing completion status. No confirmation dialog surfaced.

6. **`run` on exercises gives unhelpful `"entry point not found"`** — Could say "Exercises cannot be run directly — use Check to test your solution."

### Design Observations

- **`previous` across unit boundaries works correctly** — Going backward from the first activity of a unit correctly lands on the last activity of the previous unit.
- **`next` across unit boundaries works correctly** — Going forward from the last activity crosses into the next unit.
- **`check` error messages on wrong solutions are excellent** — e.g., "Misclassified |01⟩ as |00⟩ in 31 test runs." Very helpful for debugging.
- **Syntax error reporting is clear** — Returns specific error messages from the compiler.
- **`solution` returns clean reference code** — Matches the expected solutions correctly.

### Content Assessment

All 5 exercises in the unit were solvable with reasonable Q# knowledge. The progression from full measurements → partial measurements → post-selection → joint measurements is pedagogically sound. The reference solutions are correct and idiomatic Q#.

Completed: _Report findings_ (5/5)

Made changes.
