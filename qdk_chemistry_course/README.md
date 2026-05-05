# Chemistry Course Content Extraction Process

## Prerequisites

- Access to [qBraid QuNorth staging](https://qunorth-staging.qbraid.com) with a valid session
- Local copy of the course notebooks at `C:\src\microsoft-course\chemistry\course\` (9 chapter `.ipynb` files)
- Python environment at `c:\src\qdk\.venv\`

## Step 1: Scrape the platform

Run `scrape_script.js` in the browser DevTools console while authenticated to qBraid QuNorth staging.

This fetches:

- Course chapter content (9 chapters, keys `file-f3316f` through `file-f33177`) — these contain Jupyter-style cells (markdown + code) but **code cells are stubs** (only 1-2 lines each)
- Quiz question definitions (`mcq-q/*`, `frq-q/*`) and user responses (`mcq-r/*`, `frq-r/*`)
- Course metadata and registration endpoints

Copy the JSON output from the console and save it to `content_dump.json`.

**Note**: The bearer token in the script is short-lived. Replace it with a fresh token from the browser's network tab before running.

## Step 2: Decompose JSON cells into individual files

Run `export_cells.py`:

```
c:\src\qdk\.venv\Scripts\python.exe export_cells.py
```

This reads `content_dump.json` and creates `qdk_chemistry_course/` with one file per cell:

- **Markdown cells** → `NN_Title.md` (raw HTML/Markdown source)
- **Code cells** (preceded by an h3-only markdown cell) → `NN_Title.py`
- **Question cells** (containing `FreeResponseQuestion("id")` or `MultipleChoiceQuestion("id")`) → `NN_question_id.md` with question text, choices, and rubrics pulled from the quiz endpoints in the dump

Files are numbered with two-digit prefixes (`00_`, `01_`, ...) preserving source order. When an h3-only markdown cell precedes a code cell, they're merged into a single `.py` file (the h3 becomes the filename).

**Caveat**: The `.py` files produced at this step only contain the stub code from the JSON dump (1-2 lines each). The full implementations live in the notebooks.

## Step 3: Replace .py stubs with full notebook code

Run `fix_py_files.py`:

```
c:\src\qdk\.venv\Scripts\python.exe fix_py_files.py
```

This reads the actual `.ipynb` notebooks from `C:\src\microsoft-course\chemistry\course\`, extracts (h3, code) pairs, and matches them to the exported `.py` files by normalized h3 title. Each stub is overwritten with the full notebook implementation.

**Result**: 70 of 72 `.py` files are updated with complete code. Two files have no notebook counterpart:

- `chapter_00_getting_started/07_What is the Chemistry QDK_.py` — a YouTube embed cell (JSON-only)
- `chapter_08_whats_next/02_Work with Microsoft and QuNorth to have feedback here in viable format.py` — a feedback placeholder (JSON-only)

## Output Structure

```
qdk_chemistry_course/
├── chapter_00_getting_started/       (25 files)
├── chapter_01_molecular_input/       (24 files)
├── chapter_02_classical_functionality/ (26 files)
├── chapter_03_active_space_selection/ (21 files)
├── chapter_04_hamiltonian_qubit_mapping/ (17 files)
├── chapter_05_state_preparation/     (18 files)
├── chapter_06_quantum_phase_estimation/ (18 files)
├── chapter_07_plugins_and_extending_the_qdk/ (14 files)
└── chapter_08_whats_next/            (8 files)
```

**171 files total** — `.md` for prose/questions, `.py` for executable code.

## Key Data Paths

| What                | Where                                              |
| ------------------- | -------------------------------------------------- |
| Platform            | `https://qunorth-staging.qbraid.com`               |
| Chemistry course ID | `69ab7f63bf261b602bf698f0`                         |
| QDK course ID       | `69cb199565fbbc26768b8360`                         |
| JSON cell path      | `dump[key]['body']['data']['courseData']['cells']` |
| MCQ question        | `dump['mcq-q/{id}']['body']['data']['question']`   |
| FRQ question        | `dump['frq-q/{id}']['body']['data']['question']`   |

## Utility Scripts (can be deleted)

- `check_nb.py` — one-off debug script to inspect notebook cells
- `debug_titles.py` — lists h3 titles in notebooks for matching
- `fix_remaining.py` — fixes the last few unmatched .py files
- `compare_anchors.py` — compares h3 anchors between JSON and notebooks

## Step 4: Generate complete notebooks

Run `generate_notebooks.py`:

```
c:\src\qdk\.venv\Scripts\python.exe generate_notebooks.py
```

This reassembles the individual `.md` and `.py` files back into proper `.ipynb` notebooks, one per chapter, written to `microsoft-course-main/chemistry/`. The notebooks combine the rich markdown prose from the JSON dump with the full code implementations from the original notebooks — producing a more complete artifact than either source alone.

**Output**: 9 notebooks in `microsoft-course-main/chemistry/`:

```
chapter_00_getting_started.ipynb       (25 cells: 14 md, 11 code)
chapter_01_molecular_input.ipynb       (24 cells: 15 md, 9 code)
chapter_02_classical_functionality.ipynb (26 cells: 13 md, 13 code)
chapter_03_active_space_selection.ipynb (21 cells: 11 md, 10 code)
chapter_04_hamiltonian_qubit_mapping.ipynb (17 cells: 9 md, 8 code)
chapter_05_state_preparation.ipynb     (18 cells: 11 md, 7 code)
chapter_06_quantum_phase_estimation.ipynb (18 cells: 11 md, 7 code)
chapter_07_plugins_and_extending_the_qdk.ipynb (14 cells: 8 md, 6 code)
chapter_08_whats_next.ipynb            (8 cells: 7 md, 1 code)
```
