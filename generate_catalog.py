#!/usr/bin/env python3
"""Generate catalog.json and catalog.edited.json for QDK and Chemistry courses."""

import json
import re
import copy
import sys
import os
from pathlib import Path
from difflib import SequenceMatcher

# ── Helpers ──────────────────────────────────────────────────────────────────

def normalize_heading(text: str) -> str:
    """Strip HTML tags, extra whitespace, and normalize for matching."""
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"\s+", " ", text).strip()
    # Normalize unicode dashes, quotes
    text = text.replace("\u2014", "--").replace("\u2013", "-")
    text = text.replace("\u2018", "'").replace("\u2019", "'")
    text = text.replace("\u201c", '"').replace("\u201d", '"')
    return text


def extract_h2(source: str):
    """Extract <h2> heading text from a cell source. Returns (heading_text, rest_of_source) or (None, source)."""
    # Match <h2 ...>...</h2> or ## heading
    m = re.match(r'<h2[^>]*>(.*?)</h2>', source, re.DOTALL)
    if m:
        heading = normalize_heading(m.group(1))
        rest = source[m.end():].strip()
        return heading, rest
    m = re.match(r'^##\s+(.+?)(?:\n|$)', source)
    if m:
        heading = m.group(1).strip()
        rest = source[m.end():].strip()
        return heading, rest
    return None, source


def extract_h3(source: str):
    """Extract <h3> heading text from a cell source."""
    m = re.match(r'<h3[^>]*>(.*?)</h3>', source, re.DOTALL)
    if m:
        return normalize_heading(m.group(1))
    m = re.match(r'^###\s+(.+?)(?:\n|$)', source)
    if m:
        return m.group(1).strip()
    return None


def extract_h1(source: str):
    """Extract <h1> heading text."""
    m = re.match(r'<h1[^>]*>(.*?)</h1>', source, re.DOTALL)
    if m:
        return normalize_heading(m.group(1))
    m = re.match(r'^#\s+(.+?)(?:\n|$)', source)
    if m:
        return m.group(1).strip()
    return None


def has_exercise_placeholder(source: str) -> bool:
    return "???" in source or "YOUR CODE HERE" in source


def is_filter_section(title: str) -> bool:
    """Return True if this section should be filtered out."""
    lower = title.lower()
    skip_patterns = [
        "how to access this course",
        "qbook (web browser)",
        "vs code (local)",
    ]
    for pat in skip_patterns:
        if pat in lower:
            return True
    # "Setup: local environment" but not "Setup:" sections with other content
    if re.match(r"setup:\s*local\s+environment", lower):
        return True
    return False


def first_code_line(source: str) -> str:
    """Get the first non-empty, non-magic line of a code cell for matching."""
    for line in source.split("\n"):
        line = line.strip()
        if not line:
            continue
        if line.startswith("%%") or line.startswith("%"):
            continue
        if line.startswith("#") or line.startswith("//"):
            # Return comment without prefix
            return re.sub(r'^[#/]+\s*', '', line).strip()
        return line
    return ""


def normalize_code_line(text: str) -> str:
    """Normalize a code comment line for fuzzy matching."""
    # Remove trailing punctuation, normalize dashes/quotes
    text = text.rstrip(".:;,")
    text = text.replace("\u2014", "--").replace("\u2013", "-")
    text = text.replace("\u2018", "'").replace("\u2019", "'")
    return text.lower().strip()


def similarity(a: str, b: str) -> float:
    return SequenceMatcher(None, a.lower(), b.lower()).ratio()


def parse_todo_quiz(source: str):
    """Parse a TODO quiz stub into structured quiz data."""
    quiz = {}
    # Determine if it starts with # (Python comment style) vs bare text
    lines = source.strip().split("\n")
    # Strip leading # from each line if present
    cleaned = []
    for line in lines:
        stripped = re.sub(r'^#\s?', '', line) if line.startswith('#') else line
        cleaned.append(stripped)
    text = "\n".join(cleaned)

    # Extract question
    m = re.search(r'Question:\s*(.+?)(?:\n[A-D]\))', text, re.DOTALL)
    if m:
        quiz["questionText"] = m.group(1).strip()

    # Extract choices
    choices = re.findall(r'([A-D])\)\s*(.+?)(?:\s*[✓✗])?(?=\n[A-D]\)|\nCorrect|\n\n|$)', text, re.DOTALL)
    if choices:
        quiz["choices"] = [c[1].strip().rstrip("✓✗ ") for c in choices]

    # Extract correct answer
    m = re.search(r'Correct\s+answer:\s*([A-D])', text)
    if m:
        letter = m.group(1)
        quiz["correctIndex"] = ord(letter) - ord('A')
        quiz["format"] = "mcq"
    else:
        # Check for ✓ marker
        for i, c in enumerate(choices):
            if "✓" in c[1]:
                quiz["correctIndex"] = i
                quiz["format"] = "mcq"
                break

    # Extract explanation
    m = re.search(r'Explanation:\s*(.+?)(?:\n\n|$)', text, re.DOTALL)
    if m:
        quiz["explanation"] = m.group(1).strip()

    if not quiz.get("format"):
        # Check if it's a free response
        if "free response" in text.lower() or "frq" in text.lower():
            quiz["format"] = "frq"
        elif quiz.get("questionText"):
            quiz["format"] = "mcq"  # default if we found a question

    return quiz if quiz.get("questionText") else None


# ── Notebook parser ──────────────────────────────────────────────────────────

def parse_notebook(path: str):
    """Parse a notebook and return heading->cellIndex map and full cells."""
    with open(path, encoding="utf-8") as f:
        nb = json.load(f)
    cells = nb.get("cells", [])
    heading_map = {}  # normalized heading text -> list of cell indices (code cells following the heading)
    code_cells = {}   # cellIndex -> source

    for i, cell in enumerate(cells):
        ct = cell.get("cell_type", "")
        src = "".join(cell.get("source", []))

        if ct == "code":
            code_cells[i] = src

        if ct == "markdown":
            # Try h3, then ##, then ###
            h3 = extract_h3(src)
            if h3:
                norm = normalize_heading(h3)
                if norm not in heading_map:
                    heading_map[norm] = []
                # Look for the next code cell
                if i + 1 < len(cells) and cells[i + 1].get("cell_type") == "code":
                    heading_map[norm].append(i + 1)
            else:
                # Check for ## headings (used in notebooks)
                h2_text = None
                m = re.match(r'^##\s+(.+?)(?:\n|$)', src)
                if m:
                    h2_text = m.group(1).strip()
                if not h2_text:
                    m = re.match(r'<h2[^>]*>(.*?)</h2>', src, re.DOTALL)
                    if m:
                        h2_text = normalize_heading(m.group(1))
                if h2_text:
                    norm = normalize_heading(h2_text)
                    if norm not in heading_map:
                        heading_map[norm] = []

    # Also build a first-code-line map for fallback matching
    code_line_map = {}  # normalized_first_line -> cellIndex
    for idx, src in code_cells.items():
        fl = first_code_line(src)
        if fl:
            normalized = normalize_code_line(fl)
            code_line_map[normalized] = idx

    return heading_map, code_cells, code_line_map, cells


# ── Section builder ──────────────────────────────────────────────────────────

def build_sections(jsonl_cells, heading_map, code_cells, code_line_map, nb_cells):
    """Split JSONL cells on <h2> boundaries and build section objects."""
    sections = []
    current_section = None
    section_counter = 0

    # First cell is usually <h1> title - capture as intro if it has content beyond the heading
    for idx, cell in enumerate(jsonl_cells):
        ct = cell.get("cell_type", "")
        src = cell.get("source", "")

        if ct == "markdown":
            h2, rest = extract_h2(src)
            if h2:
                # Start new section
                if current_section:
                    sections.append(current_section)
                section_counter += 1
                current_section = {
                    "id": "s%d" % section_counter,
                    "type": "lesson",
                    "title": h2,
                    "content": src,
                    "notebookCells": [],
                    "_code_sources": [],
                }
                continue

            # Check for h3 (code label)
            h3 = extract_h3(src)
            if h3 and current_section is not None:
                # This is a code cell label - next cell should be code
                current_section["_pending_h3"] = h3
                continue

            # Check for TODO quiz stub
            if src.strip().startswith("TODO") and "question" in src.lower():
                if current_section is None:
                    section_counter += 1
                    current_section = {
                        "id": "s%d" % section_counter,
                        "type": "quiz",
                        "title": "Quiz",
                        "content": "",
                        "notebookCells": [],
                        "_code_sources": [],
                    }
                quiz = parse_todo_quiz(src)
                if quiz:
                    current_section["quiz"] = quiz
                    current_section["type"] = "quiz"
                    current_section["_quiz_source"] = src
                continue

            # Check for <h1> - usually the chapter title cell
            h1 = extract_h1(src)
            if h1 and current_section is None:
                # Chapter title - create intro section
                section_counter += 1
                current_section = {
                    "id": "s%d" % section_counter,
                    "type": "lesson",
                    "title": h1,
                    "content": src,
                    "notebookCells": [],
                    "_code_sources": [],
                }
                continue

            # Regular prose - append to current section
            if current_section is not None:
                current_section["content"] += "\n\n" + src
            else:
                # Orphan prose before first h2
                section_counter += 1
                current_section = {
                    "id": "s%d" % section_counter,
                    "type": "lesson",
                    "title": "Introduction",
                    "content": src,
                    "notebookCells": [],
                    "_code_sources": [],
                }

        elif ct == "code":
            if current_section is None:
                section_counter += 1
                current_section = {
                    "id": "s%d" % section_counter,
                    "type": "lesson-example",
                    "title": "Setup",
                    "content": "",
                    "notebookCells": [],
                    "_code_sources": [],
                }

            # Check if this is a quiz widget call
            if re.search(r'MultipleChoiceQuestion\(|FreeResponseQuestion\(', src):
                m_quiz = re.search(r'(MultipleChoiceQuestion|FreeResponseQuestion)\(["\']([^"\']+)["\']\)', src)
                if m_quiz:
                    qtype = "mcq" if "Multiple" in m_quiz.group(1) else "frq"
                    qid = m_quiz.group(2)
                    current_section["type"] = "quiz"
                    if "quiz" not in current_section:
                        current_section["quiz"] = {}
                    current_section["quiz"]["format"] = qtype
                    current_section["quiz"]["questionId"] = qid
                    current_section["quiz"]["note"] = "Question hosted in qBook question bank (ID: %s)" % qid
                continue

            h3_text = current_section.pop("_pending_h3", None)
            is_exercise = has_exercise_placeholder(src)

            # Try to match to notebook
            cell_index = None
            match_method = None
            warning = None

            if h3_text:
                norm_h3 = normalize_heading(h3_text)
                if norm_h3 in heading_map and heading_map[norm_h3]:
                    cell_index = heading_map[norm_h3][0]
                    match_method = "exact-h3"
                else:
                    # Try first-code-line fallback
                    fl = normalize_code_line(first_code_line(src))
                    if fl and fl in code_line_map:
                        cell_index = code_line_map[fl]
                        match_method = "first-code-line"
                    else:
                        warning = "No notebook match for h3: %s" % h3_text
            else:
                # No h3 - try first code line
                fl = normalize_code_line(first_code_line(src))
                if fl and fl in code_line_map:
                    cell_index = code_line_map[fl]
                    match_method = "first-code-line"

            nb_cell = {
                "heading": h3_text or first_code_line(src)[:80],
                "cellIndex": cell_index,
                "hasExercisePlaceholder": is_exercise,
            }
            if warning:
                nb_cell["warning"] = warning
            if match_method:
                nb_cell["_matchMethod"] = match_method

            current_section["notebookCells"].append(nb_cell)
            current_section["_code_sources"].append(src)

            # Update section type
            if is_exercise:
                current_section["type"] = "exercise"
                current_section["solutionCode"] = None  # filled in later
            elif current_section["type"] == "lesson":
                current_section["type"] = "lesson-example"

    if current_section:
        sections.append(current_section)

    return sections


def resolve_solutions(sections, code_cells):
    """For exercise sections, resolve solution code from notebook."""
    for section in sections:
        if section["type"] != "exercise":
            continue
        for i, nb_cell in enumerate(section.get("notebookCells", [])):
            if not nb_cell.get("hasExercisePlaceholder"):
                continue
            cell_idx = nb_cell.get("cellIndex")
            jsonl_src = section["_code_sources"][i] if i < len(section.get("_code_sources", [])) else None

            if cell_idx is not None and cell_idx in code_cells:
                nb_src = code_cells[cell_idx]
                if not has_exercise_placeholder(nb_src):
                    section["solutionCode"] = nb_src
                    section["_solutionSource"] = "notebook"
                elif jsonl_src and not has_exercise_placeholder(jsonl_src):
                    section["solutionCode"] = jsonl_src
                    section["_solutionSource"] = "jsonl"
            elif jsonl_src and not has_exercise_placeholder(jsonl_src):
                section["solutionCode"] = jsonl_src
                section["_solutionSource"] = "jsonl"


def filter_sections(sections):
    """Remove sections that should be filtered out."""
    return [s for s in sections if not is_filter_section(s["title"])]


def clean_for_output(sections):
    """Remove internal fields from sections for JSON output."""
    result = []
    for s in sections:
        out = {}
        for k, v in s.items():
            if k.startswith("_"):
                continue
            out[k] = v
        # Clean notebookCells
        if "notebookCells" in out:
            clean_cells = []
            for nc in out["notebookCells"]:
                clean = {k: v for k, v in nc.items() if not k.startswith("_")}
                clean_cells.append(clean)
            out["notebookCells"] = clean_cells
        result.append(out)
    return result


# ── Edited catalog builder ───────────────────────────────────────────────────

def build_edited_catalog(catalog, heading_map, code_cells, code_line_map, nb_cells):
    """Create the edited catalog with best-effort corrections."""
    edited = copy.deepcopy(catalog)
    edit_log = []

    for chapter in edited["chapters"]:
        all_matched_nb_indices = set()
        for section in chapter["sections"]:
            for nc in section.get("notebookCells", []):
                if nc.get("cellIndex") is not None:
                    all_matched_nb_indices.add(nc["cellIndex"])

        # Pass 1: Fix unmatched h3 headings with fuzzy matching (heading map + code lines)
        for section in chapter["sections"]:
            for nc in section.get("notebookCells", []):
                if nc.get("cellIndex") is not None:
                    continue
                heading = nc.get("heading", "")
                if not heading:
                    continue

                best_match = None
                best_score = 0

                # Try fuzzy match against notebook headings
                for nb_heading, indices in heading_map.items():
                    score = similarity(heading, nb_heading)
                    if score > best_score and score > 0.5:
                        for idx in indices:
                            if idx not in all_matched_nb_indices:
                                best_score = score
                                best_match = (nb_heading, idx, "heading")

                # Try fuzzy match against notebook first-code-lines
                for nb_fl, idx in code_line_map.items():
                    if idx in all_matched_nb_indices:
                        continue
                    score = similarity(heading.lower(), nb_fl)
                    if score > best_score and score > 0.5:
                        best_score = score
                        best_match = (nb_fl, idx, "code-line")

                if best_match:
                    matched_text, idx, match_type = best_match
                    nc["cellIndex"] = idx
                    nc.pop("warning", None)
                    nc["editNote"] = "Fuzzy-matched %s (%.0f%% similar): '%s' -> '%s'" % (
                        match_type, best_score * 100, heading, matched_text)
                    all_matched_nb_indices.add(idx)
                    edit_log.append("Fuzzy %s match: '%s' -> '%s' in %s" % (
                        match_type, heading, matched_text, section["id"]))

        # Pass 2: Positional matching within sections
        # Build notebook section -> code cell indices map
        if nb_cells:
            nb_section_codes = {}  # normalized section title -> [code cell indices]
            current_nb_section = None
            for i, cell in enumerate(nb_cells):
                ct = cell.get("cell_type", "")
                src = "".join(cell.get("source", []))
                if ct == "markdown":
                    # Check for ## heading
                    m2 = re.match(r'^##\s+(.+?)(?:\n|$)', src)
                    if m2:
                        current_nb_section = normalize_heading(m2.group(1).strip())
                        if current_nb_section not in nb_section_codes:
                            nb_section_codes[current_nb_section] = []
                    else:
                        m2 = re.match(r'<h2[^>]*>(.*?)</h2>', src, re.DOTALL)
                        if m2:
                            current_nb_section = normalize_heading(m2.group(1))
                            if current_nb_section not in nb_section_codes:
                                nb_section_codes[current_nb_section] = []
                elif ct == "code" and current_nb_section is not None:
                    nb_section_codes[current_nb_section].append(i)

            # For each JSONL section, try to match by section title -> positional order
            for section in chapter["sections"]:
                unmatched_ncs = [(j, nc) for j, nc in enumerate(section.get("notebookCells", [])) if nc.get("cellIndex") is None]
                if not unmatched_ncs:
                    continue

                # Try to find matching notebook section
                section_title = section["title"]
                best_nb_section = None
                best_score = 0
                for nb_sec_title, nb_code_indices in nb_section_codes.items():
                    # Compare section titles (JSONL uses "1.2 Foo" while notebook uses "8.2 Foo")
                    # Strip the number prefix for comparison
                    jsonl_stripped = re.sub(r'^\d+\.\d+\s*', '', section_title)
                    nb_stripped = re.sub(r'^\d+\.\d+\s*', '', nb_sec_title)
                    score = similarity(jsonl_stripped, nb_stripped)
                    if score > best_score and score > 0.6:
                        best_score = score
                        best_nb_section = (nb_sec_title, nb_code_indices)

                if best_nb_section:
                    nb_sec_title, nb_code_indices = best_nb_section
                    # Filter out already-matched indices
                    available = [idx for idx in nb_code_indices if idx not in all_matched_nb_indices]
                    # Match by position
                    for pos, (j, nc) in enumerate(unmatched_ncs):
                        if pos < len(available):
                            idx = available[pos]
                            nc["cellIndex"] = idx
                            nc.pop("warning", None)
                            nc["editNote"] = "Positional match (pos %d) in section '%s' (%.0f%% title match)" % (
                                pos, nb_sec_title, best_score * 100)
                            all_matched_nb_indices.add(idx)
                            edit_log.append("Positional match: pos %d in nb section '%s' for %s" % (
                                pos, nb_sec_title, section["id"]))

        # Pass 3 (was 2): Map orphan notebook cells
        if chapter.get("notebook") and nb_cells:
            for i, cell in enumerate(nb_cells):
                if i in all_matched_nb_indices:
                    continue
                ct = cell.get("cell_type", "")
                src = "".join(cell.get("source", []))
                if ct != "code":
                    continue
                # Check if there's a preceding markdown h3
                h3_text = None
                if i > 0:
                    prev = nb_cells[i - 1]
                    if prev.get("cell_type") == "markdown":
                        prev_src = "".join(prev.get("source", []))
                        h3_text = extract_h3(prev_src)
                        if not h3_text:
                            m = re.match(r'^##\s+(.+?)(?:\n|$)', prev_src)
                            if m:
                                h3_text = m.group(1).strip()
                if not h3_text:
                    h3_text = first_code_line(src)[:80] or ("Notebook cell %d" % i)

                # Only add if this looks like a meaningful orphan (skip setup/import cells)
                if i <= 2:
                    continue  # Skip title and setup cells

                # Check if already covered by an existing section by content
                already_covered = False
                for section in chapter["sections"]:
                    for nc in section.get("notebookCells", []):
                        if nc.get("cellIndex") == i:
                            already_covered = True
                            break
                if already_covered:
                    continue

                # Add as orphan section
                orphan = {
                    "id": "s%d" % (len(chapter["sections"]) + 1),
                    "type": "lesson-example",
                    "title": h3_text,
                    "content": "<p>%s</p>" % h3_text,
                    "notebookCells": [{
                        "heading": h3_text,
                        "cellIndex": i,
                        "hasExercisePlaceholder": has_exercise_placeholder(src),
                    }],
                    "editNote": "notebook-only cell, no JSONL prose",
                }
                chapter["sections"].append(orphan)
                all_matched_nb_indices.add(i)
                edit_log.append("Added orphan notebook cell %d as section '%s'" % (i, h3_text))

        # Pass 4: Merge consecutive prose-only sections
        merged_indices = set()
        for i in range(len(chapter["sections"]) - 1):
            if i in merged_indices:
                continue
            s1 = chapter["sections"][i]
            s2 = chapter["sections"][i + 1]
            if (s1["type"] == "lesson" and s2["type"] == "lesson"
                    and not s1.get("notebookCells") and not s2.get("notebookCells")
                    and not s1.get("quiz") and not s2.get("quiz")):
                s1["content"] += "\n\n" + s2["content"]
                s1["editNote"] = s1.get("editNote", "") + ("Merged with following section '%s'" % s2["title"])
                merged_indices.add(i + 1)
                edit_log.append("Merged sections '%s' + '%s'" % (s1["title"], s2["title"]))
        if merged_indices:
            chapter["sections"] = [s for i, s in enumerate(chapter["sections"]) if i not in merged_indices]

        # Pass 5: Extract TODO quiz stubs
        for section in chapter["sections"]:
            if section.get("quiz") and not section["quiz"].get("explanation"):
                section["quiz"]["editNote"] = "extracted from TODO stub, not from question bank"
            elif section.get("quiz"):
                if "editNote" not in section:
                    section["editNote"] = "extracted from TODO stub, not from question bank"

        # Pass 6: Resolve exercise solutions from notebook
        for section in chapter["sections"]:
            if section["type"] == "exercise" and not section.get("solutionCode"):
                for nc in section.get("notebookCells", []):
                    idx = nc.get("cellIndex")
                    if idx is not None and idx in code_cells:
                        nb_src = code_cells[idx]
                        if not has_exercise_placeholder(nb_src):
                            section["solutionCode"] = nb_src
                            section["editNote"] = section.get("editNote", "") + "Solution inferred from notebook cell %d. " % idx
                            edit_log.append("Inferred solution from notebook for %s" % section["id"])
                            break

    return edited, edit_log


# ── Course-specific config ───────────────────────────────────────────────────

NOTEBOOK_MAP_QDK = {
    0: "chapter_00_getting_started.ipynb",
    1: "chapter_01_github_copilot_with_qdk.ipynb",
    2: "chapter_02_qsharp_essentials.ipynb",
    3: "chapter_03_simulation_backends.ipynb",
    4: "chapter_04_visualization.ipynb",
    5: "chapter_05_quantum_error_correction.ipynb",
    6: "chapter_06_quantum_algorithms.ipynb",
    7: "chapter_07_chemistry_and_domain_apps.ipynb",
    8: "chapter_08_compilation_and_qir.ipynb",
}

NOTEBOOK_MAP_CHEM = {
    0: "chapter_00_getting_started.ipynb",
    1: "chapter_01_molecular_input.ipynb",
    2: "chapter_02_classical_functionality.ipynb",
    # Chapters 3-5 have no notebook
}


# ── Main ─────────────────────────────────────────────────────────────────────

def process_course(course_id, jsonl_path, notebook_dir, notebook_map, output_dir):
    print("Processing course: %s" % course_id)
    print("  JSONL: %s" % jsonl_path)
    print("  Notebooks: %s" % notebook_dir)
    print()

    with open(jsonl_path, encoding="utf-8") as f:
        lines = f.readlines()

    # Determine chapter line mapping
    chapter_lines = []
    for i, line in enumerate(lines):
        obj = json.loads(line)
        if "_redacted" in obj:
            continue
        d = obj.get("data", obj)
        cells = d.get("courseData", {}).get("cells", [])
        if not cells:
            continue
        ci_list = d.get("courseInfo", [])
        chapter_lines.append((i, obj))

    # Get course info from first content line
    first_obj = chapter_lines[0][1]
    d = first_obj.get("data", first_obj)
    course_info_list = d.get("courseInfo", [])

    # Build chapter title map from courseInfo
    chapter_titles = {}
    for ci in course_info_list:
        num = ci.get("chapterNumber", 0)
        name = ci.get("chapterName", "Chapter %d" % (num - 1))
        chapter_titles[num - 1] = name  # chapterNumber is 1-based

    catalog = {
        "id": course_id,
        "title": "",
        "chapters": [],
    }

    # Set title from first chapter's h1
    first_cells = d.get("courseData", {}).get("cells", [])
    if first_cells:
        h1 = extract_h1(first_cells[0].get("source", ""))
        if h1:
            # For chemistry, the h1 is the course title
            # For QDK, h1 is "Chapter 0: ..."
            if "chapter" not in h1.lower():
                catalog["title"] = h1
    if not catalog["title"]:
        if course_id == "qdk":
            catalog["title"] = "The Microsoft Quantum Development Kit"
        else:
            catalog["title"] = "Advanced Chemistry Workflows"

    all_heading_maps = {}
    all_code_cells = {}
    all_code_line_maps = {}
    all_nb_cells = {}

    # Parse notebooks
    for ch_idx, nb_file in notebook_map.items():
        nb_path = os.path.join(notebook_dir, nb_file)
        if os.path.exists(nb_path):
            hm, cc, clm, nbc = parse_notebook(nb_path)
            all_heading_maps[ch_idx] = hm
            all_code_cells[ch_idx] = cc
            all_code_line_maps[ch_idx] = clm
            all_nb_cells[ch_idx] = nbc
            print("  Parsed notebook ch%d: %d heading entries, %d code cells" % (ch_idx, len(hm), len(cc)))

    print()

    # Process each chapter
    for ch_idx, (line_idx, obj) in enumerate(chapter_lines):
        d = obj.get("data", obj)
        cells = d.get("courseData", {}).get("cells", [])
        ch_title = chapter_titles.get(ch_idx, "Chapter %d" % ch_idx)
        nb_file = notebook_map.get(ch_idx)

        heading_map = all_heading_maps.get(ch_idx, {})
        code_cells = all_code_cells.get(ch_idx, {})
        code_line_map = all_code_line_maps.get(ch_idx, {})
        nb_cells = all_nb_cells.get(ch_idx, [])

        print("  Chapter %d: %s (%d JSONL cells, notebook=%s)" % (ch_idx, ch_title, len(cells), nb_file or "none"))

        # Build sections
        sections = build_sections(cells, heading_map, code_cells, code_line_map, nb_cells)
        sections = filter_sections(sections)
        resolve_solutions(sections, code_cells)

        # Count stats
        n_matched = sum(1 for s in sections for nc in s.get("notebookCells", []) if nc.get("cellIndex") is not None)
        n_unmatched = sum(1 for s in sections for nc in s.get("notebookCells", []) if nc.get("cellIndex") is None)
        n_exercises = sum(1 for s in sections if s["type"] == "exercise")
        n_quizzes = sum(1 for s in sections if s["type"] == "quiz")
        print("    Sections: %d | Matched: %d | Unmatched: %d | Exercises: %d | Quizzes: %d" % (
            len(sections), n_matched, n_unmatched, n_exercises, n_quizzes))

        chapter = {
            "id": "ch%02d" % ch_idx,
            "title": ch_title,
            "notebook": nb_file,
            "sections": clean_for_output(sections),
        }

        # Fix chapter numbering inconsistency note
        # Check if notebook title differs from JSONL title
        if nb_cells:
            nb_src0 = "".join(nb_cells[0].get("source", []))
            nb_h1 = extract_h1(nb_src0)
            jsonl_h1 = extract_h1(cells[0].get("source", "")) if cells else None
            if nb_h1 and jsonl_h1 and nb_h1 != jsonl_h1:
                chapter["_numberingNote"] = "Notebook title: '%s', JSONL title: '%s'" % (nb_h1, jsonl_h1)

        catalog["chapters"].append(chapter)

        # Store raw sections for edited pass
        chapter["_raw_sections"] = sections

    # Write catalog.json
    catalog_clean = copy.deepcopy(catalog)
    for ch in catalog_clean["chapters"]:
        ch.pop("_raw_sections", None)
        ch.pop("_numberingNote", None)

    catalog_path = os.path.join(output_dir, "catalog.json")
    with open(catalog_path, "w", encoding="utf-8") as f:
        json.dump(catalog_clean, f, indent=2, ensure_ascii=False)
    print("\n  Wrote: %s" % catalog_path)

    # Build edited catalog
    # Merge all heading maps for cross-chapter matching
    merged_heading_map = {}
    merged_code_cells = {}
    merged_code_line_map = {}

    for ch_idx in range(len(chapter_lines)):
        if ch_idx in all_heading_maps:
            merged_heading_map.update(all_heading_maps[ch_idx])
            merged_code_cells.update(all_code_cells[ch_idx])
            merged_code_line_map.update(all_code_line_maps[ch_idx])

    # For edited, process per-chapter with the chapter's own maps
    edited_catalog = copy.deepcopy(catalog_clean)
    total_edits = []

    for ch_idx, chapter in enumerate(edited_catalog["chapters"]):
        heading_map = all_heading_maps.get(ch_idx, {})
        code_cells = all_code_cells.get(ch_idx, {})
        code_line_map = all_code_line_maps.get(ch_idx, {})
        nb_cells_ch = all_nb_cells.get(ch_idx, [])

        sub_catalog = {"chapters": [chapter]}
        edited_sub, edits = build_edited_catalog(
            sub_catalog, heading_map, code_cells, code_line_map, nb_cells_ch)
        edited_catalog["chapters"][ch_idx] = edited_sub["chapters"][0]
        total_edits.extend(edits)

    edited_path = os.path.join(output_dir, "catalog.edited.json")
    with open(edited_path, "w", encoding="utf-8") as f:
        json.dump(edited_catalog, f, indent=2, ensure_ascii=False)
    print("  Wrote: %s" % edited_path)

    # Summary
    print("\n  === Edit Summary ===")
    print("  Total edits applied: %d" % len(total_edits))
    categories = {}
    for e in total_edits:
        cat = e.split(":")[0] if ":" in e else "other"
        categories[cat] = categories.get(cat, 0) + 1
    for cat, count in sorted(categories.items()):
        print("    %s: %d" % (cat, count))

    # Count unresolved
    unresolved = 0
    for ch in edited_catalog["chapters"]:
        for s in ch["sections"]:
            for nc in s.get("notebookCells", []):
                if nc.get("warning") or nc.get("cellIndex") is None:
                    unresolved += 1
    print("  Unresolved notebook matches: %d" % unresolved)

    return catalog_clean, edited_catalog, total_edits


def main():
    base = os.path.dirname(os.path.abspath(__file__))

    courses = sys.argv[1:] if len(sys.argv) > 1 else ["qdk", "chemistry"]

    for course in courses:
        if course == "qdk":
            process_course(
                "qdk",
                os.path.join(base, "microsoft-course-website", "qdk", "content.jsonl"),
                os.path.join(base, "microsoft-course-main", "qdk", "course"),
                NOTEBOOK_MAP_QDK,
                os.path.join(base, "microsoft-course-main", "qdk"),
            )
        elif course == "chemistry":
            process_course(
                "chemistry",
                os.path.join(base, "microsoft-course-website", "chemistry", "content.jsonl"),
                os.path.join(base, "microsoft-course-main", "chemistry", "course"),
                NOTEBOOK_MAP_CHEM,
                os.path.join(base, "microsoft-course-main", "chemistry"),
            )
        print("\n" + "=" * 60 + "\n")


if __name__ == "__main__":
    main()
