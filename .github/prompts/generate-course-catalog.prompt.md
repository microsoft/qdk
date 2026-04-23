---
description: "Generate a structured course catalog (catalog.json) from JSONL web content and Jupyter notebooks for the QDK unified learning experience"
agent: "agent"
argument-hint: "course name (qdk or chemistry)"
---

# Generate Course Catalog

You are building a `catalog.json` file for the QDK unified learning system. This system powers an MCP-based interactive learning experience in the VS Code extension, where users navigate structured lessons, exercises, quizzes, and labs through Copilot Chat.

## Input sources

Two complementary source files exist for each course:

1. **JSONL web content** (`microsoft-course-website/<course>/content.jsonl`) — one JSON object per line, each containing a chapter's `courseData.cells[]` array. This is the **prose source of truth**: full teaching narrative, `<h2>` section structure, images, and quiz questions (`MultipleChoiceQuestion`/`FreeResponseQuestion` calls or TODO markdown stubs).

2. **Jupyter notebooks** (`microsoft-course-main/<course>/course/*.ipynb`) — the **executable artifacts**. Contain only `<h3>` label cells + complete runnable code. No lesson prose.

The JSONL and notebooks are **complementary, not copies**. The JSONL has ~2x the cells because it includes prose, section headers, images, and quiz widgets. The notebook has only the code-cell pairs.

## How to correlate them

Each code cell in a notebook is preceded by a markdown cell with an `<h3>` heading (or `###` in later chapters). The same heading text appears in the JSONL. Match on this heading text to link JSONL sections to notebook cell indices.

**Known issues:**
- QDK Chapter 0 has poor heading overlap between JSONL and notebook — the content diverged. Requires manual mapping or best-effort matching on first-line code comments.
- QDK Chapters 5–8 use `##` markdown headings instead of `<h3>` HTML tags.
- Chemistry Chapters 3–5 exist only in the JSONL (no notebook). These become lesson-only chapters.
- JSONL has web-delivery sections ("qBook (web browser)", "VS Code (local)", "Step N: ...") that should be **filtered out** — they are not teaching content.

## Section detection rules

1. **Split JSONL cells on `<h2>` headings** (styled or plain markdown `##`). Each `<h2>` starts a new section.
2. Within each section, identify:
   - **Lesson text**: markdown cells with teaching prose (not `<h3>` labels)
   - **Code cell references**: `<h3>` label + code cell pairs. Match the `<h3>` text to the notebook to get the notebook cell index.
   - **Exercises**: code cells containing `???` or `// YOUR CODE HERE` or `# YOUR CODE HERE`. Extract the solution from the JSONL cell source (which may have the complete code) or from the notebook (which has complete implementations).
   - **Quiz questions**: cells containing `MultipleChoiceQuestion("id")` or `FreeResponseQuestion("id")`. Extract question data from the `chapterQuestions` field in `courseInfo`, or parse TODO markdown stubs for unpublished questions.
3. Classify each section:
   - `"lesson"` — prose only, no code cells
   - `"lesson-example"` — prose + demonstration code cell(s)
   - `"exercise"` — prose + code cell with `???`/`YOUR CODE HERE` placeholder
   - `"quiz"` — MCQ or FRQ question

## Filter out these JSONL sections

Skip sections whose `<h2>` title matches any of:
- "How to access this course"
- "Setup: local environment" (keep "Setup" sections that have code)
- Any section containing only qBook/web-browser delivery instructions

## Output format

Generate `catalog.json` with this structure:

```json
{
  "id": "<course-id>",
  "title": "<course title>",
  "chapters": [
    {
      "id": "ch<NN>",
      "title": "<chapter title from JSONL courseInfo.chapterName>",
      "notebook": "<filename>.ipynb or null if no notebook",
      "sections": [
        {
          "id": "s<N>",
          "type": "lesson | lesson-example | exercise | quiz",
          "title": "<from h2 heading>",
          "content": "<teaching prose as HTML, from JSONL markdown cells>",
          "notebookCells": [
            {
              "heading": "<h3 text>",
              "cellIndex": <0-based index in the .ipynb cells array>,
              "hasExercisePlaceholder": true/false
            }
          ],
          "solutionCode": "<for exercises: the complete code from the JSONL or notebook>",
          "quiz": {
            "format": "mcq | frq",
            "questionText": "<question>",
            "choices": ["A", "B", "C", "D"],
            "correctIndex": 1,
            "explanation": "<why this answer is correct>",
            "rubric": "<for FRQ: grading criteria>"
          }
        }
      ]
    }
  ]
}
```

## Steps

1. Read the JSONL file for the specified course. Each line is a JSON object — parse `data.courseData.cells` and `data.courseInfo`.
2. Read each corresponding notebook file. Build a heading→cellIndex map from its cells.
3. For each JSONL chapter, walk the cells array and split on `<h2>` boundaries.
4. For each section, extract prose, match code cells to notebook indices, detect exercises and quizzes.
5. Write **two** output files to `microsoft-course-main/<course>/`:

### `catalog.json` — strict mechanical output
Contains only what the data unambiguously supports. Sections where heading matching failed get `"notebookCells": []` with a `"warning"` field explaining the mismatch. No guesses, no fabricated content.

### `catalog.edited.json` — agent-curated output
A copy of `catalog.json` with the agent's best-effort corrections applied. Use your judgment to:

- **Resolve ambiguous heading matches**: If a notebook heading is similar but not identical to a JSONL heading (e.g., "End-to-end preview: Bell state to resource estimate" vs "End-to-end preview"), match them and add `"editNote"` explaining the fuzzy match.
- **Fill in missing notebook mappings**: If a JSONL section has code cells but no heading match in the notebook, try matching on the first-line comment of the code cell (e.g., `# Run SCF with STO-3G basis`). Add `"editNote"` when using this fallback.
- **Map orphan notebook cells**: If a notebook cell has no JSONL match, create a minimal section for it with `"type": "lesson-example"`, using the `<h3>` text as both title and content. Mark with `"editNote": "notebook-only cell, no JSONL prose"`.
- **Fix chapter numbering inconsistencies**: Some chapters have mismatched numbering across notebook title, JSONL content, and JSONL metadata. Normalize to the JSONL metadata numbering and note the discrepancy in `"editNote"`.
- **Extract TODO quiz stubs**: For QDK chapters, parse the markdown TODO comment blocks to extract question text, choices, correct answer, and explanation. Mark with `"editNote": "extracted from TODO stub, not from question bank"`.
- **Merge consecutive prose-only sections**: If two adjacent `<h2>` sections are both lesson-only with no code, consider merging them into one section if they're clearly part of the same topic. Note in `"editNote"`.
- **Infer exercise solutions**: If a JSONL code cell is a stub (`???`) but the notebook has the complete implementation, pull the solution from the notebook. If both are complete, prefer the notebook version. Note the source in `"editNote"`.

Every correction must include an `"editNote"` string on the affected section explaining what was changed and why. This makes it easy to review the agent's decisions and accept or revert each one.

6. Report a summary of all edits made in `catalog.edited.json` — how many sections were corrected, what categories of fixes were applied, and any sections that remain unresolved even after best-effort matching.

## Important constraints

- **Do not modify the notebooks.** They are the executable artifacts and must remain unchanged.
- **Do not fabricate content.** All prose, code, and quiz data must come from the JSONL or notebooks.
- **Preserve HTML formatting** in the `content` field — it contains styled headings, tables, images, and math.
- **Report unmatched headings** — if a notebook `<h3>` doesn't appear in the JSONL (or vice versa), note it as a warning rather than silently dropping it.
- For images with signed GCS URLs, keep the `<img>` tags as-is. They'll be resolved separately.
