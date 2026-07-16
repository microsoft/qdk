"""Course environment check utility.

Called from the first code cell of each unit notebook. Validates that the
notebook kernel is running in the course .venv and that all required packages
are importable. Renders results as styled HTML in the notebook output.
"""

import importlib.util
import json
import sys
from pathlib import Path

from IPython.display import HTML, display


def check(notebook_dir: str | Path | None = None) -> None:
    """Run the environment check and display results.

    Raises EnvironmentError if anything is wrong, which stops "Run All"
    from continuing past this cell.

    Parameters
    ----------
    notebook_dir : path-like, optional
        Directory containing the notebook. Defaults to Path.cwd().
    """
    nb_dir = Path(notebook_dir) if notebook_dir else Path.cwd()

    # --- Locate course.json ---
    course_json = _find_course_json(nb_dir)
    if course_json is None:
        raise FileNotFoundError(
            "Could not find course.json. Make sure you opened this notebook "
            "from the QDK course folder."
        )

    course = json.loads(course_json.read_text())
    env_cfg = course.get("environment", {})
    requirements = env_cfg.get("requirements", [])
    import_checks = env_cfg.get("importChecks", [])

    results: list[tuple[str, str, bool]] = []  # (label, detail, ok)
    errors: list[str] = []

    # --- Check 1: Python version ---
    py_version = sys.version.split()[0]
    results.append(("Python version", py_version, True))

    # --- Check 2: course .venv exists and has a Python interpreter ---
    course_root = course_json.resolve().parent
    expected_venv = (course_root / ".venv").resolve()
    venv_exists = expected_venv.is_dir()
    venv_python = _find_venv_python(expected_venv) if venv_exists else None

    if not venv_exists:
        results.append(("Course venv", f"{expected_venv} — not found", False))
        errors.append(
            "The course virtual environment does not exist yet.<br>"
            "Run <b>QDK Learning: Doctor</b> from the Command Palette "
            "(<code>Ctrl+Shift+P</code> / <code>Cmd+Shift+P</code>) "
            "and choose <b>Set up environment</b>."
            + _command_link("qsharp-vscode.learningDoctor", "Run Doctor now")
        )
    elif not venv_python:
        results.append(("Course venv", f"{expected_venv} — corrupt (no python)", False))
        errors.append(
            "The course virtual environment exists but has no Python interpreter.<br>"
            "Run <b>QDK Learning: Doctor</b> from the Command Palette "
            "and choose <b>Set up environment</b> to recreate it."
            + _command_link("qsharp-vscode.learningDoctor", "Run Doctor now")
        )
    else:
        results.append(("Course venv", str(expected_venv), True))

    # --- Check 3: kernel is actually using the course .venv ---
    prefix = Path(sys.prefix).resolve()

    in_course_venv = False
    if venv_exists:
        try:
            prefix.relative_to(expected_venv)
            in_course_venv = True
        except ValueError:
            pass

    if venv_exists and venv_python and not in_course_venv:
        results.append(("Kernel", f"Expected {expected_venv}, got {prefix}", False))
        errors.append(
            "This kernel is not the course environment. "
            "Click <b>Select Kernel</b> (top-right of the notebook) "
            "and pick the course <code>.venv</code>, then re-run this cell."
        )

    # --- Check 4: required packages ---
    missing = [m for m in import_checks if importlib.util.find_spec(m) is None]

    if missing:
        results.append(
            ("Packages", ", ".join(f"<code>{m}</code> missing" for m in missing), False)
        )
        # Check if this course uses pyproject.toml (uv sync) or legacy requirements.
        has_pyproject = (course_root / "pyproject.toml").exists()
        if has_pyproject:
            errors.append(
                "Some packages are missing from the course environment.<br>"
                "Run <b>QDK Learning: Doctor</b> to re-sync, or manually run "
                "<code>uv sync</code> in the course folder."
                + _command_link("qsharp-vscode.learningDoctor", "Run Doctor now")
            )
        else:
            pip_cmd = f"%pip install {' '.join(requirements)}"
            errors.append(
                "Install missing packages by running this in a new cell, then re-run this one:"
                f"<pre>  {pip_cmd}</pre>"
                "Or run <b>QDK Learning: Doctor</b> to set up the full environment."
                + _command_link("qsharp-vscode.learningDoctor", "Run Doctor now")
            )
    elif import_checks and in_course_venv:
        results.append(("Packages", ", ".join(import_checks), True))

    # --- Render ---
    _render(results, errors)

    if errors:
        raise EnvironmentError(
            "Environment check failed. See output above for details."
        )


def _find_venv_python(venv: Path) -> Path | None:
    """Return the venv's Python interpreter path, or None if missing."""
    candidates = [
        venv / "bin" / "python",
        venv / "bin" / "python3",
        venv / "Scripts" / "python.exe",
    ]
    for c in candidates:
        if c.exists():
            return c
    return None


def _command_link(command_id: str, label: str) -> str:
    """Return an HTML link that invokes a VS Code command when clicked.

    VS Code renders `vscode://` and `command:` URIs in trusted notebook
    HTML output, so clicking the link runs the command directly.
    """
    from urllib.parse import quote

    return (
        f'<br><a href="command:{quote(command_id)}" '
        f'style="display:inline-block;margin-top:6px;padding:4px 10px;'
        f"background:#1976d2;color:#fff;border-radius:4px;"
        f'text-decoration:none;font-size:0.9em">'
        f"{label}</a>"
    )


def _find_course_json(nb_dir: Path) -> Path | None:
    """Walk up from nb_dir looking for course.json."""
    candidate = nb_dir / "course.json"
    if candidate.exists():
        return candidate
    # One level up (unit notebook inside a subdirectory).
    candidate = (nb_dir / ".." / "course.json").resolve()
    if candidate.exists():
        return candidate
    # Two levels up (deeply nested unit).
    candidate = (nb_dir / ".." / ".." / "course.json").resolve()
    if candidate.exists():
        return candidate
    return None


def _render(results: list[tuple[str, str, bool]], errors: list[str]) -> None:
    """Display a styled HTML summary."""
    rows = ""
    for label, detail, ok in results:
        icon = "&#x2705;" if ok else "&#x274C;"
        color = "#2e7d32" if ok else "#c62828"
        rows += (
            f'<tr style="border-bottom:1px solid #eee">'
            f'<td style="padding:4px 12px 4px 0;font-size:1.1em">{icon}</td>'
            f'<td style="padding:4px 12px;font-weight:600">{label}</td>'
            f'<td style="padding:4px 0;color:{color}">{detail}</td>'
            f"</tr>"
        )

    html = (
        '<div style="font-family:system-ui,sans-serif;margin:8px 0">'
        '<table style="border-collapse:collapse">'
        f"{rows}"
        "</table>"
    )

    if errors:
        error_items = "".join(f"<li style='margin:4px 0'>{e}</li>" for e in errors)
        html += (
            '<div style="margin-top:12px;padding:10px 14px;'
            "background:#fff3e0;border-left:4px solid #e65100;"
            'border-radius:4px">'
            f"<strong>Action needed:</strong><ul style='margin:6px 0 0 0;padding-left:18px'>{error_items}</ul>"
            "</div>"
        )
    else:
        html += (
            '<div style="margin-top:12px;padding:10px 14px;'
            "background:#e8f5e9;border-left:4px solid #2e7d32;"
            'border-radius:4px">'
            "<strong>Environment looks good. You're ready to continue!</strong>"
            "</div>"
        )

    html += "</div>"
    display(HTML(html))


if __name__ == "__main__":
    check()
