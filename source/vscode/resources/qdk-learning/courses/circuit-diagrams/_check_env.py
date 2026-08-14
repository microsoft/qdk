"""Course environment check utility.

Called from the first code cell of each unit notebook. Validates that all
required packages are importable. Renders results as styled HTML in the
notebook output.
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
    course_json_path = _find_course_json(nb_dir)
    if course_json_path is None:
        raise FileNotFoundError(
            "Could not find course.json. Make sure you opened this notebook "
            "from the QDK course folder."
        )

    course = json.loads(course_json_path.read_text())
    env_cfg = course.get("environment", {})
    import_checks = env_cfg.get("importChecks", [])

    results: list[tuple[str, str, bool]] = []  # (label, detail, ok)
    errors: list[str] = []

    # --- Check 1: Python version ---
    py_version = sys.version.split()[0]
    results.append(("Python version", py_version, True))

    # --- Check 2: required packages ---
    missing = [m for m in import_checks if not _can_import(m)]

    if missing:
        results.append(
            ("Packages", ", ".join(f"<code>{m}</code> missing" for m in missing), False)
        )
        errors.append(
            "Install missing packages by running the following in a new cell, then re-run this cell:"
            f"<pre>  %pip install -r ../requirements.txt</pre>"
        )
    elif import_checks:
        results.append(("Packages", ", ".join(import_checks), True))

    # --- Render ---
    _render(results, errors)

    if errors:
        raise EnvironmentError(
            "Environment check failed. See output above for details."
        )


def _can_import(module_name: str) -> bool:
    """Check whether *module_name* is importable without raising."""
    try:
        return importlib.util.find_spec(module_name) is not None
    except ModuleNotFoundError:
        return False


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
        color = (
            "var(--vscode-testing-iconPassed, #00ff00)"
            if ok
            else "var(--vscode-testing-iconFailed, #ff00ff)"
        )
        rows += (
            '<tr style="border-bottom:1px solid '
            'var(--vscode-panel-border, #00ffff)">'
            f'<td style="padding:4px 12px 4px 0;font-size:1.1em">{icon}</td>'
            f'<td style="padding:4px 12px;font-weight:600">{label}</td>'
            f'<td style="padding:4px 0;color:{color}">{detail}</td>'
            f"</tr>"
        )

    html = (
        '<div style="font-family:system-ui,sans-serif;margin:8px 0;'
        'color:var(--vscode-foreground, #ff00ff)">'
        '<table style="border-collapse:collapse">'
        f"{rows}"
        "</table>"
    )

    if errors:
        error_items = "".join(f"<li style='margin:4px 0'>{e}</li>" for e in errors)
        html += (
            '<div style="margin-top:12px;padding:10px 14px;'
            "background:var(--vscode-inputValidation-warningBackground, #ffff00);"
            "color:var(--vscode-inputValidation-warningForeground, #0000ff);"
            "border-left:4px solid "
            "var(--vscode-inputValidation-warningBorder, #ff00ff);"
            'border-radius:4px">'
            f"<strong>Action needed:</strong><ul style='margin:6px 0 0 0;padding-left:18px'>{error_items}</ul>"
            "</div>"
        )
    else:
        html += (
            '<div style="margin-top:12px;padding:10px 14px;'
            "background:var(--vscode-notifications-background, #00ffff);"
            "color:var(--vscode-notifications-foreground, #ff00ff);"
            "border-left:4px solid "
            "var(--vscode-testing-iconPassed, #00ff00);"
            'border-radius:4px">'
            "<strong>Environment looks good. You're ready to continue!</strong>"
            "</div>"
        )

    html += "</div>"
    display(HTML(html))


if __name__ == "__main__":
    check()
