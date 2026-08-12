"""Convert a QDK/Chemistry tutorial chapter into a QDK Learning unit notebook.

The chapters are already marked up with the two seams this needs:

    ``.. literalinclude::`` + ``# start-cell-<name>``   ->  code cells
    ``.. admonition:: <q>`` + ``:class: quiz-question`` ->  collapsible self-checks

Everything the converter cannot infer lives in RECIPES below. That list is the
honest inventory of what a human still has to decide per chapter.

Usage:  python rst_to_notebook.py 02
"""

import argparse
import base64
import hashlib
import json
import mimetypes
import re
import sys
import textwrap
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
# The built qdk-chemistry docs are not part of this repo; override with --docs.
DOCS = REPO_ROOT.parent / "html"
RST_DIR = DOCS / "_sources/tutorials/ground_state_molecular_energies_with_qpe"
PY_DIR = DOCS / "_static/examples/python"
COURSE = (
    REPO_ROOT
    / "source/vscode/test/suites/learning/test-workspace"
    / "qdk-learning/courses/chemistry-active-space"
)

# Per-chapter human decisions. Everything else is derived from the sources.
RECIPES = {
    "index": {
        "rst": "index.rst.txt",
        # The landing page is prose and a workflow diagram, so there is nothing to run.
        "py": None,
        "unit_dir": "overview",
        "notebook": "overview.ipynb",
        "skip_sections": [],
        "extra_regions": [],
        "exercises": [],
    },
    "01": {
        "rst": "01_energy_and_accuracy.rst.txt",
        # Conceptual chapter with no companion script, so no setup cells either.
        "py": None,
        "unit_dir": "00-energy-and-accuracy",
        "notebook": "energy_and_accuracy.ipynb",
        "skip_sections": [],
        "extra_regions": [],
        "exercises": [],
    },
    "02": {
        "rst": "02_describing_the_molecule.rst.txt",
        "py": "tutorial_describe_n2.py",
        "unit_dir": "01-describe-molecule",
        "notebook": "describe_molecule.ipynb",
        # A notebook is the example, so the download instructions are noise.
        "skip_sections": ["Example download"],
        # The reader is already inside the notebook, so nothing sends them to a terminal.
        "drop_blocks": [
            "run the complete script from the Visual Studio Code integrated terminal",
            "python tutorial_describe_n2.py",
        ],
        "rewrites": [
            (
                "Run the complete script and record both fixed-geometry total energies",
                "Record both fixed-geometry total energies",
            ),
        ],
        # Regions the chapter never includes but the notebook needs to be whole.
        "extra_regions": [("Running the calculation", "compare")],
        # Promoted from the chapter's own quiz-question at 02_describing_the_molecule.rst:126.
        "exercises": [
            {
                "section": "Running the calculation",
                "prompt": "## Which basis set wins\n\n"
                "The chapter asked which basis set gives the lower energy. Answer it from your "
                "own numbers rather than from the text.\n\n"
                "Complete `lower_energy_basis` so it returns the name of the basis set with the "
                "lower Hartree\u2013Fock energy, read from the `energies` dictionary you just "
                "filled in.",
                "code": "from _unit import exercise\n\n\n"
                "@exercise\n"
                "def lower_energy_basis():\n"
                '    return "cc-pvdz"\n',
                "hint": "Lower energy means the more negative number, not the smaller magnitude. "
                "`energies` maps each basis-set name to its energy, so taking `min` over the "
                "keys with the energy as the sort key picks the winner.",
                "solution": "```python\n@exercise\ndef lower_energy_basis():\n"
                "    return min(energies, key=energies.get)\n```\n\n"
                "`cc-pvtz` wins. Hartree\u2013Fock is variational, so enlarging the basis can only "
                "lower the energy. The gap between the two is the basis-set sensitivity, not the "
                "error of either number.",
            }
        ],
    },
    "03": {
        "rst": "03_choosing_the_active_space.rst.txt",
        "py": "tutorial_choose_active_space.py",
        "unit_dir": "02-active-space",
        "notebook": "active_space.ipynb",
        "skip_sections": ["Example download"],
        # The chapter's excerpts all assume the Hartree-Fock run that precedes them.
        "pre_code": "from tutorial_choose_active_space import create_stretched_n2_structure",
        "pre_regions": ["hartree-fock"],
        "drop_blocks": [
            "run the complete script from the Visual Studio Code integrated terminal",
            "python tutorial_choose_active_space.py",
        ],
        "rewrites": [
            (
                "## Running the calculation",
                "## Running the calculation\n\nThe cells above have already run the "
                "workflow. Use their output to answer the questions below.",
            ),
        ],
        "extra_regions": [],
        "inserts": [
            {
                "section": "Candidate-orbital visualization",
                "cells": [
                    (
                        "md",
                        "The molecular viewer needs values of each orbital's spatial wavefunction "
                        "on a three-dimensional grid. The next cell evaluates all candidate "
                        "natural orbitals on such a grid and stores the sampled values as cube "
                        "data.\n\nEach orbital is annotated with its natural occupation, "
                        "single-orbital entropy, and autoCAS selection status so that you can "
                        "examine the spatial and numerical evidence together. Generating the "
                        "orbital grids may take a little longer than the preceding calculation.",
                    ),
                    (
                        "code",
                        "from qdk_chemistry.utils.cubegen import generate_cubefiles_from_orbitals\n\n"
                        "natural_orbitals = natural_orbital_casci_wavefunction.get_orbitals()\n"
                        "occupation_alpha, occupation_beta = (\n"
                        "    natural_orbital_casci_wavefunction.get_active_orbital_occupations()\n"
                        ")\n\n"
                        "# Occupation arrays use active-space positions, while cube-file labels use\n"
                        "# the original molecular-orbital indices; this dictionary connects them.\n"
                        "active_position = {\n"
                        "    orbital_index: position for position, orbital_index in enumerate(valence_indices)\n"
                        "}\n"
                        "raw_cube_data = generate_cubefiles_from_orbitals(\n"
                        "    orbitals=natural_orbitals,\n"
                        "    grid_size=(30, 30, 30),\n"
                        "    margin=10.0,\n"
                        "    indices=valence_indices,\n"
                        ")\n\n"
                        "cube_data = {}\n"
                        "for raw_label, cube_file in raw_cube_data.items():\n"
                        "    # Cube labels number orbitals from one, while QDK/Chemistry indices start\n"
                        "    # from zero, so convert before looking up occupations and entropies.\n"
                        '    orbital_index = int(raw_label.split("_")[1]) - 1\n'
                        "    position = active_position[orbital_index]\n"
                        "    # Add the alpha and beta occupations to report the total occupation of\n"
                        "    # each spatial orbital in the viewer.\n"
                        "    occupation = float(occupation_alpha[position]) + float(occupation_beta[position])\n"
                        '    cube_data[f"Orbital {orbital_index}"] = {\n'
                        '        "data": cube_file,\n'
                        '        "info": {\n'
                        '            "Occupation": f"{occupation:.3f}",\n'
                        '            "Entropy": f"{orbital_entropies[position]:.3f}",\n'
                        '            "Selected by autoCAS": "yes" if orbital_index in refined_indices else "no",\n'
                        "        },\n"
                        "    }\n\n"
                        'print(f"Generated cube data for {len(cube_data)} candidate orbitals.")\n',
                    ),
                    (
                        "md",
                        "## Inspect the natural orbitals\n\n"
                        "Launch the interactive molecular viewer and use its orbital menu to move "
                        "through the candidate natural orbitals. For each orbital, inspect the "
                        'isosurface together with the displayed "occupation", "entropy", and '
                        '"selected by autoCAS" information. Compare the selected orbitals with the '
                        "nearly doubly occupied and nearly empty orbitals that were excluded.",
                    ),
                    (
                        "code",
                        "from qdk.widgets import MoleculeViewer\n\n"
                        "MoleculeViewer(\n"
                        "    molecule_data=structure.to_xyz(),\n"
                        "    cube_data=cube_data,\n"
                        ")\n",
                    ),
                    (
                        "md",
                        "Use the viewer to inspect the following information:\n\n"
                        "- **Orbital menu** selects each candidate natural orbital for comparison. "
                        "The menu follows increasing molecular-orbital index, which corresponds "
                        "here to decreasing natural occupation. The menu is not ordered by "
                        "entropy.\n"
                        "- **Isosurface** traces points where the orbital wavefunction has a chosen "
                        "positive or negative value, revealing its lobes, nodes, and spatial "
                        "extent. The surface itself does not encode occupation or entropy.\n"
                        "- **Natural occupation** reports the average number of electrons in the "
                        "spatial orbital. A value near two indicates an almost always doubly "
                        "occupied orbital, a value near zero indicates an almost always empty "
                        "orbital, and an intermediate value indicates variable occupation across "
                        "the correlated wavefunction.\n"
                        "- **Single-orbital entropy and autoCAS selection** report the uncertainty "
                        "in the orbital's local occupation and whether autoCAS retained it. Larger "
                        "entropy indicates stronger coupling to the occupations of the other "
                        "active orbitals.\n\n"
                        "autoCAS selects the strongly coupled group from gaps in the orbital "
                        "entropies, not from orbital shapes or a cutoff applied to the natural "
                        "occupations. Use the shapes as aids to chemical interpretation, but "
                        "defend the final active space using the numerical occupation and entropy "
                        "evidence in the overlays.",
                    ),
                    (
                        "md",
                        "## Interpret the active-space choice\n\n"
                        "Identify which visual features distinguish nearly doubly occupied "
                        "orbitals, high-entropy orbitals with strongly coupled occupations, and "
                        "nearly empty orbitals. Then explain why autoCAS retains the selected "
                        "group while excluding the other candidate orbitals.\n\n"
                        "Treat the orbital shapes as aids to chemical interpretation rather than "
                        "as the selection rule. Defend the refined active-space choice using the "
                        "occupation and entropy overlays, and record your explanation in the "
                        "active-space section of the lab notebook.",
                    ),
                ],
            },
        ],
        "exercises": [
            {
                "section": "The active-space choice",
                "prompt": "## Count the determinants\n\n"
                "The refined active space is CAS$(6,6)$: six electrons in six spatial orbitals, "
                "so three $\\alpha$ and three $\\beta$ electrons. The $\\alpha$ and $\\beta$ "
                "occupations are chosen independently of each other.\n\n"
                "Fix the function below so that it returns the number of determinants this active "
                "space contains, then run the cell.",
                "code": "from math import comb\n\nfrom _unit import exercise\n\n\n"
                "@exercise\ndef determinant_count():\n"
                "    alpha = comb(6, 3)\n"
                "    beta = comb(6, 3)\n"
                "    return alpha\n",
                "hint": "Every way of placing the three $\\alpha$ electrons can be paired with "
                "every way of placing the three $\\beta$ electrons, so the two counts combine "
                "multiplicatively rather than additively.",
                "solution": "Return the product of the two counts:\n\n"
                "```python\nreturn alpha * beta\n```\n\n"
                "$\\binom{6}{3}=20$, so the refined space holds $20\\times20=400$ determinants, "
                "down from the 3,136 of the initial CAS$(10,8)$ valence space.",
            }
        ],
    },
    "04": {
        "rst": "04_putting_the_problem_on_qubits.rst.txt",
        "py": "tutorial_map_n2_to_qubits.py",
        "unit_dir": "03-map-to-qubits",
        "notebook": "map_to_qubits.ipynb",
        "skip_sections": ["Example download"],
        # Keeps the three quiz-questions in "Running the mapping" without the terminal
        # instructions around them, which a notebook reader has no use for.
        "drop_blocks": [
            "run the complete script from the Visual Studio Code integrated terminal",
            "python tutorial_map_n2_to_qubits.py",
        ],
        "rewrites": [
            (
                "## Running the mapping",
                "## Running the mapping\n\nThe cells above have already run the mapping. "
                "Use their output to answer the questions below.",
            ),
        ],
        # The chapter shows the mapper output through an :append: line the converter
        # drops, and that helper lives in a region the chapter never includes.
        "extra_regions": [("Qubit Hamiltonian in Pauli form", "pauli-preview-helpers")],
        # The chapter's excerpts are consecutive statements inside one function, so
        # only the workflow call above them and the reporting below them are missing.
        "inserts": [
            {
                "section": "Setting up",
                "cells": [
                    (
                        "md",
                        "## Running the Chapter 3 workflow\n\n"
                        "Every excerpt in this chapter starts from the active space selected in "
                        "Chapter 3. This cell reruns that workflow so the rest of the notebook "
                        "has a selected space to map. It is the expensive step in the chapter.",
                    ),
                    ("code", "active_space_result = run_active_space_workflow()\n"),
                ],
            },
            {
                "section": "Qubit Hamiltonian in Pauli form",
                "cells": [
                    (
                        "md",
                        "The chapter prints its Pauli preview from a helper that sits outside its "
                        "excerpts. The two functions above are that helper; this cell is the "
                        "preview the chapter describes.",
                    ),
                    (
                        "code",
                        "preview_terms = representative_pauli_terms(qubit_hamiltonian)\n"
                        'print(f"Representative Pauli terms ({len(preview_terms)} of {num_pauli_terms}):")\n'
                        "for pauli_string, coefficient in preview_terms:\n"
                        '    print(f"  {coefficient.real:+.12f} * {format_pauli_string(pauli_string)}")\n',
                    ),
                ],
            },
            {
                "section": "Core-energy bookkeeping",
                "cells": [
                    (
                        "md",
                        "The quantities the chapter asks you to record come from a reporting "
                        "function outside its excerpts, so they are printed here from the values "
                        "the cells above computed.",
                    ),
                    (
                        "code",
                        "print(\n"
                        '    f"Fixed-electron-number subspace: {num_alpha} alpha, {num_beta} beta "\n'
                        '    f"electrons ({len(fixed_electron_basis_indices)} basis states)"\n'
                        ")\n"
                        'print(f"Core energy: {core_energy:.12f} Hartree")\n'
                        'print(f"Mapped active-space ground state: {mapped_active_energy:.12f} Hartree")\n'
                        'print(f"Mapped selected-space total: {mapped_total_energy:.12f} Hartree")\n'
                        "print(\n"
                        '    f"CASCI algorithmic reference: {active_space_result.refined_energy:.12f} Hartree"\n'
                        ")\n"
                        'print(f"Validation difference: {mapping_energy_difference:.3e} Hartree")\n',
                    ),
                ],
            },
        ],
        # Promoted from the chapter's own quiz-question at
        # 04_putting_the_problem_on_qubits.rst:173.
        "exercises": [
            {
                "section": "Qubits for the encoded fermionic state",
                "prompt": "## How many compute qubits\n\n"
                "The chapter asked how many compute qubits the selected active space needs under "
                "Jordan\u2013Wigner. Answer it from your own reasoning rather than from the "
                "text.\n\n"
                "Complete `compute_qubit_count` so it returns the size of the compute register "
                "for the active space selected in Chapter 3.",
                "code": "from _unit import exercise\n\n\n"
                "@exercise\n"
                "def compute_qubit_count():\n"
                "    return 6\n",
                "hint": "The compute register holds one qubit per spin orbital, and every spatial "
                "orbital contributes one \u03b1 and one \u03b2 spin orbital. "
                "`num_active_spatial_orbitals` is already in scope.",
                "solution": "```python\n@exercise\ndef compute_qubit_count():\n"
                "    return 2 * num_active_spatial_orbitals\n```\n\n"
                "`12`. Six active spatial orbitals give twelve spin orbitals, and "
                "Jordan\u2013Wigner uses one qubit per spin orbital. The count excludes "
                "phase-estimation ancillas, workspace qubits, and error-correction overhead.",
            }
        ],
    },
    "05": {
        "rst": "05_preparing_the_trial_state.rst.txt",
        "py": "tutorial_prepare_trial_state.py",
        "unit_dir": "04-trial-state",
        "notebook": "trial_state.ipynb",
        "skip_sections": ["Example download"],
        # The chapter's helper calls this one, which sits outside every marked region.
        "pre_code": "from tutorial_prepare_trial_state import leading_determinants",
        "drop_blocks": [
            "run the complete script from the Visual Studio Code integrated terminal",
            "python tutorial_prepare_trial_state.py",
            "Before answering the next question, download and open",
        ],
        "rewrites": [
            (
                "## Running the preparation",
                "## Running the preparation\n\nThe cells above have already run the "
                "preparation. Use their output to answer the questions below.",
            ),
        ],
        "extra_regions": [],
        # The chapter's two excerpts are one pass of a loop inside a function, so the
        # surrounding context has to be supplied by hand. This list is that context.
        "inserts": [
            {
                "section": "Setting up",
                "cells": [
                    (
                        "md",
                        "## Workflow helpers\n\n"
                        "The chapter keeps two helpers outside its excerpts: one ranks the "
                        "reference determinants by weight, the other counts the leaf gates in a "
                        "generated circuit. Run both so the rest of the notebook can use them.",
                    ),
                    ("region", "determinant-weights"),
                    ("region", "circuit-statistics"),
                ],
            },
            {
                "section": "Ground-state fidelity",
                "cells": [
                    (
                        "md",
                        "## Running the reference workflow\n\n"
                        "Everything below builds on the selected active space from the previous "
                        "chapter. This cell reruns that workflow, builds the active-space "
                        "Hamiltonian, and ranks the leading reference determinants. It is the "
                        "expensive step in the chapter.",
                    ),
                    (
                        "code",
                        "active_space_result = run_active_space_workflow()\n"
                        "reference_wavefunction = active_space_result.refined_casci_wavefunction\n"
                        "selected_orbitals = active_space_result.refined_wavefunction.get_orbitals()\n"
                        'active_hamiltonian = create("hamiltonian_constructor", "qdk").run(\n'
                        "    selected_orbitals\n"
                        ")\n\n"
                        "reference_determinants = leading_determinant_contributions(\n"
                        "    reference_wavefunction\n"
                        ")\n"
                        "for contribution in reference_determinants[:4]:\n"
                        "    print(\n"
                        '        f"{contribution.occupation}  "\n'
                        '        f"amplitude {contribution.amplitude.real:+.4f}  "\n'
                        '        f"weight {contribution.weight:.4f}  "\n'
                        '        f"cumulative {contribution.cumulative_weight:.4f}"\n'
                        "    )\n",
                    ),
                    (
                        "md",
                        "The two excerpts that follow are one pass of the script's loop over one, "
                        "two, and four determinants. Fix the count here so they can run as "
                        "ordinary cells.",
                    ),
                    ("code", "num_determinants = 4\n"),
                ],
            },
            {
                "section": "The trial state preparation logical circuit",
                "cells": [
                    (
                        "md",
                        "The chapter reports the circuit cost from a printing function that sits "
                        "outside its excerpts, so the numbers it discusses are shown here.",
                    ),
                    (
                        "code",
                        'print(f"Compute qubits: {num_compute_qubits}")\n'
                        'print(f"Preparation logical gate count: {num_logical_gates}")\n'
                        'print(f"Logical gate-family counts: {logical_gate_counts}")\n',
                    ),
                ],
            },
            {
                "section": "Trial-state quality and preparation cost",
                "cells": [
                    (
                        "md",
                        "## Comparing the three trial states\n\n"
                        "The script repeats the same construction for one, two, and four "
                        "determinants. Repeat it here to see how fidelity responds to the "
                        "retained determinant count.",
                    ),
                    (
                        "code",
                        # The script's own workflow reads reference and trial coefficients in
                        # matching PMC order; recomputing that by hand gives lower fidelities.
                        "from tutorial_prepare_trial_state import run_trial_state_workflow\n\n"
                        "comparison = run_trial_state_workflow()\n"
                        "fidelities = {\n"
                        "    trial.num_determinants: trial.fidelity\n"
                        "    for trial in comparison.trial_states\n"
                        "}\n"
                        "for count, fidelity in fidelities.items():\n"
                        '    print(f"{count} determinants: fidelity {fidelity:.4f}")\n',
                    ),
                ],
            },
        ],
        # Promoted from the quiz-question at 05_preparing_the_trial_state.rst:246.
        "exercises": [
            {
                "section": "Running the preparation",
                "prompt": "## Where truncation starts to pay off\n\n"
                "A single determinant carries less than half the weight of the selected-space "
                "ground state. That is the direct evidence of multireference character, and it "
                "is why one determinant is not enough to start phase estimation from.\n\n"
                "Complete `first_majority_count` so it returns the smallest determinant count in "
                "`fidelities` whose fidelity is greater than 0.5.",
                "code": "from _unit import exercise\n\n\n"
                "@exercise\n"
                "def first_majority_count():\n"
                "    return 1\n",
                "hint": "`fidelities` maps a determinant count to its fidelity. Walk the counts "
                "in increasing order and return the first one whose value clears 0.5.",
                "solution": "```python\n@exercise\ndef first_majority_count():\n"
                "    return min(c for c, f in fidelities.items() if f > 0.5)\n```\n\n"
                "Two determinants. The one-determinant fidelity is about 0.4825, so no single "
                "configuration holds a majority of the ground state at this stretched geometry.",
            }
        ],
    },
}

QUIZ_OPEN = (
    '<div style="border-left:4px solid var(--vscode-textLink-foreground, #5aa9e6);'
    "background:var(--vscode-textBlockQuote-background, rgba(90,169,230,0.10));"
    'padding:0.2em 1em;margin:1em 0;border-radius:4px;">'
)

# ─── Inline markup ───

INLINE = re.compile(
    r":(?P<role>[a-z]+):`(?P<rtext>[^`]*)`"
    r"|``(?P<lit>[^`]+)``"
    r"|`(?P<ltext>[^`]+?)\s*<(?P<url>[^>]+)>`_"
    r"|`(?P<ref>[^`]+)`"
)


def _role(name, text):
    target = re.match(r"^(.*?)\s*<(.+)>$", text)
    label, ref = (target.group(1), target.group(2)) if target else (text, text)
    if name == "math":
        return f"${text}$"
    if name == "sub":
        return f"<sub>{text}</sub>"
    if name == "sup":
        return f"<sup>{text}</sup>"
    if name in ("class", "func", "meth", "mod", "attr"):
        symbol = ref.lstrip("~").split(".")[-1]
        return f"`{symbol}()`" if name in ("func", "meth") else f"`{symbol}`"
    if name == "cite":
        return ""
    if name == "download":
        return f"`{label}`"
    if name in ("doc", "ref"):
        # Anchors point outside the course, so only the wording survives.
        return "*" + (label if target else label.replace("-", " ")) + "*"
    return label


def link(text, url):
    return f"[{text}](<{url}>)" if "(" in url or ")" in url else f"[{text}]({url})"


def inline(text):
    text = re.sub(r"\\ (?=[:`])", "", text)

    def sub(m):
        if m.group("role"):
            return _role(m.group("role"), m.group("rtext"))
        if m.group("lit"):
            return f"`{m.group('lit')}`"
        if m.group("url"):
            return link(m.group("ltext"), m.group("url"))
        return f"*{m.group('ref')}*"

    text = INLINE.sub(sub, text)
    text = re.sub(r"(?<=\w)--(?=\w)", "\u2013", text)
    text = re.sub(r"\s+([.,;:])", r"\1", text)
    return re.sub(r" {2,}", " ", text).rstrip()


# ─── Block parsing ───

UNDERLINE = re.compile(r"^([#=\-~^\"'+*])\1{2,}\s*$")
DIRECTIVE = re.compile(r"^\.\. ([a-z-]+):: ?(.*)$")
ANCHOR = re.compile(r"^\.\. _[\w-]+:\s*$")


def _body(lines, start):
    """Collect the indented body of a directive starting after line `start`."""
    options, body, i = {}, [], start
    while i < len(lines) and re.match(r"^\s+:[\w-]+:", lines[i]):
        key, _, value = lines[i].strip().lstrip(":").partition(":")
        options[key.strip()] = value.strip()
        i += 1
    while i < len(lines) and not lines[i].strip():
        i += 1
    while i < len(lines) and (not lines[i].strip() or lines[i].startswith("   ")):
        body.append(lines[i][3:] if lines[i].startswith("   ") else "")
        i += 1
    while body and not body[-1].strip():
        body.pop()
    return options, body, i


def parse(text):
    lines = text.splitlines()
    blocks, i = [], 0
    while i < len(lines):
        line = lines[i]
        if i + 1 < len(lines) and UNDERLINE.match(lines[i + 1]) and line.strip():
            blocks.append(("heading", lines[i + 1][0], line.strip()))
            i += 2
            continue
        if ANCHOR.match(line):
            i += 1
            continue
        directive = DIRECTIVE.match(line)
        if directive:
            name, argument = directive.group(1), directive.group(2)
            options, body, i = _body(lines, i + 1)
            blocks.append(("directive", name, argument, options, body))
            continue
        if line.strip():
            para = []
            while i < len(lines) and lines[i].strip() and not UNDERLINE.match(lines[i]):
                if i + 1 < len(lines) and UNDERLINE.match(lines[i + 1]):
                    break
                if DIRECTIVE.match(lines[i]) or ANCHOR.match(lines[i]):
                    break
                para.append(lines[i])
                i += 1
            blocks.append(("prose", para))
            continue
        i += 1
    return blocks


# ─── Source extraction ───


def region(py_text, name):
    match = re.search(
        rf"^[ \t]*# start-cell-{re.escape(name)}\s*\n(.*?)^[ \t]*# end-cell-{re.escape(name)}",
        py_text,
        re.S | re.M,
    )
    if not match:
        raise SystemExit(f"region not found: {name}")
    return textwrap.dedent(match.group(1)).strip("\n")


def preamble(py_text):
    head = py_text.split("# start-cell-", 1)[0]
    head = re.sub(r'^\s*"""(?:.|\n)*?"""\s*', "", head, count=1)
    # A workflow function opening above the first region arrives without its body.
    head = re.split(r"^def ", head, maxsplit=1, flags=re.M)[0]
    kept = [ln for ln in head.splitlines() if not ln.lstrip().startswith("#")]
    return re.sub(r"\n{3,}", "\n\n", "\n".join(kept)).strip("\n")


# ─── Notebook cells ───


def _slug(body):
    first = body.split("\n", 1)[0].strip()
    if not first.startswith("## "):
        return None
    return re.sub(r"[^a-z0-9]+", "-", first[3:].lower()).strip("-")


def md(source, tags=None, attachments=None):
    body = source.strip("\n")
    tags = list(tags or [])
    slug = _slug(body)
    if slug and not any(t.startswith("section") for t in tags):
        tags.append(f"section:{slug}")
        cell_id = f"sec-{slug}"
    else:
        cell_id = "c-" + hashlib.sha256(body.encode()).hexdigest()[:12]
    cell = {
        "cell_type": "markdown",
        "id": cell_id,
        "metadata": {"tags": tags} if tags else {},
        "source": body,
    }
    if attachments:
        cell["attachments"] = attachments
    return cell


def code(source, tags=None):
    body = source.strip("\n")
    return {
        "cell_type": "code",
        "id": "c-" + hashlib.sha256(body.encode()).hexdigest()[:12],
        "execution_count": None,
        "metadata": {"tags": tags} if tags else {},
        "outputs": [],
        "source": body,
    }


def render_prose(block):
    lines = block[1]
    if _is_definition_list(lines):
        out = []
        for ln in lines:
            if ln.startswith("   "):
                out.append(f"  {inline(ln.strip())}")
            else:
                out.append(f"- **{inline(ln.strip())}**")
        return "\n".join(out)
    return "\n".join(inline(ln) for ln in lines)


def _is_definition_list(lines):
    """An RST definition list: each unindented term followed by an indented definition."""
    terms = 0
    for i, ln in enumerate(lines):
        if not ln.strip():
            return False
        if ln.startswith("   "):
            continue
        if re.match(r"^\s*([-*+]|\d+\.)\s", ln):
            return False
        if i + 1 >= len(lines) or not lines[i + 1].startswith("   "):
            return False
        terms += 1
    return terms >= 2


def render_directive(name, argument, options, body):
    text = "\n".join(inline(ln) for ln in body)
    if name == "math":
        return "$$\n" + "\n".join(body).strip() + "\n$$"
    if name == "code-block":
        return f"```{argument or 'text'}\n" + "\n".join(body).strip() + "\n```"
    if name == "admonition":
        title = inline(argument)
        return f"**{title}**\n\n" + "\n".join("> " + ln if ln else ">" for ln in text.splitlines())
    if name in ("figure", "image", "graphviz"):
        filename = Path(argument.strip()).name
        if name == "graphviz":
            filename = str(Path(filename).with_suffix(".png"))
        alt = inline(options.get("alt", "")).strip() or filename
        image = f"![{alt}](attachment:{filename})"
        caption = inline(options["caption"]).strip() if "caption" in options else text.strip()
        return f"{image}\n\n*{caption}*" if caption else image
    return text


def quiz(question, body):
    return (
        f"{QUIZ_OPEN}\n<details>\n<summary>&#10067;&nbsp; <b>{inline(question)}</b></summary>\n\n"
        + "\n".join(inline(ln) for ln in body).strip()
        + "\n\n</details>\n</div>"
    )


def convert(key):
    recipe = RECIPES[key]
    blocks = parse((RST_DIR / recipe["rst"]).read_text(encoding="utf-8"))
    py_text = (PY_DIR / recipe["py"]).read_text(encoding="utf-8") if recipe["py"] else ""

    cells, buffer, section = [], [], None
    owner = []
    attachments = {}
    skipping = False
    pending = {s: r for s, r in recipe["extra_regions"]}
    drop_blocks = recipe.get("drop_blocks", [])
    rewrites = recipe.get("rewrites", [])

    def rewrite(text):
        for old, new in rewrites:
            text = text.replace(old, new)
        return text

    def emit(cell):
        cells.append(cell)
        owner.append(section)

    def flush():
        if buffer:
            emit(md("\n\n".join(b for b in buffer if b.strip()), attachments=dict(attachments)))
            buffer.clear()
            attachments.clear()

    for block in blocks:
        if block[0] == "heading":
            _, char, title = block
            if char == "#":
                buffer.append(f"# {title}")
                continue
            flush()
            skipping = title in recipe["skip_sections"]
            if skipping:
                continue
            section = title
            buffer.append(rewrite(f"## {title}"))
            continue
        if skipping:
            continue
        if block[0] == "prose":
            text = render_prose(block)
            if not any(s in text for s in drop_blocks):
                buffer.append(rewrite(text))
            continue

        _, name, argument, options, body = block
        if name == "admonition" and options.get("class") == "quiz-question":
            flush()
            emit(md(quiz(argument, body)))
            continue
        if name == "literalinclude":
            flush()
            marker = options.get("start-after", "").replace("# start-cell-", "").strip()
            emit(code(region(py_text, marker)))
            if section in pending:
                emit(code(region(py_text, pending.pop(section))))
            continue
        if name == "include":
            included = DOCS / argument.strip().lstrip("/")
            if included.exists():
                for sub in parse(included.read_text(encoding="utf-8")):
                    if sub[0] == "directive":
                        buffer.append(rewrite(render_directive(sub[1], sub[2], sub[3], sub[4])))
                    elif sub[0] == "prose":
                        buffer.append(rewrite(render_prose(sub)))
            else:
                print(f"WARNING missing include {included}")
            continue
        if name == "include":
            included = DOCS / argument.strip().lstrip("/")
            if included.exists():
                for sub in parse(included.read_text(encoding="utf-8")):
                    if sub[0] == "directive":
                        buffer.append(rewrite(render_directive(sub[1], sub[2], sub[3], sub[4])))
                    elif sub[0] == "prose":
                        buffer.append(rewrite(render_prose(sub)))
            else:
                print(f"WARNING missing include {included}")
            continue
        if name in ("figure", "image", "graphviz"):
            picture = DOCS / argument.strip().lstrip("/")
            if name == "graphviz":
                picture = picture.with_suffix(".png")
            if picture.exists():
                mime = mimetypes.guess_type(picture.name)[0] or "image/png"
                attachments[picture.name] = {
                    mime: base64.b64encode(picture.read_bytes()).decode()
                }
            else:
                print(f"WARNING missing image {picture}")
        text = render_directive(name, argument, options, body)
        if not any(s in text for s in drop_blocks):
            buffer.append(rewrite(text))
    flush()

    setup = [] if not py_text else [
        (
            "Before you begin",
            md(
                "## Before you begin\n\n"
                "This course requires a Python environment with the `qdk-chemistry[jupyter]` package.\n\n"
                "`qdk-chemistry` ships compiled binaries for Linux, macOS on Apple silicon, "
                "and Windows on x86-64. On Windows on Arm, run this course inside WSL. Run the "
                "cell below to check the current environment."
            ),
        ),
        ("Before you begin", code("from _unit import check_env\n\ncheck_env()")),
        (
            "Setting up",
            md(
                "## Setting up\n\n"
                "The cell below imports the QDK/Chemistry pieces this chapter uses and quiets the "
                "solver logs."
            ),
        ),
        ("Setting up", code(preamble(py_text))),
    ] + (
        [("Setting up", code(recipe["pre_code"]))] if recipe.get("pre_code") else []
    ) + [
        ("Setting up", code(region(py_text, marker)))
        for marker in recipe.get("pre_regions", [])
    ]
    at = next(
        (i for i, c in enumerate(cells) if c["source"].startswith("## Learning objectives")),
        0,
    )
    cells[at + 1 : at + 1] = [c for _, c in setup]
    owner[at + 1 : at + 1] = [s for s, _ in setup]

    def splice(section, block):
        end = max(i for i, s in enumerate(owner) if s == section) + 1
        cells[end:end] = block
        owner[end:end] = [section] * len(block)

    kinds = {"md": md, "code": code, "region": lambda v: code(region(py_text, v))}
    for spec in recipe.get("inserts", []):
        splice(spec["section"], [kinds[k](v) for k, v in spec["cells"]])

    for spec in recipe["exercises"]:
        splice(
            spec["section"],
            [
                md(spec["prompt"]),
                code(spec["code"], tags=["exercise"]),
                md(f"**Hint**\n\n{spec['hint']}", tags=["hint"]),
                md(f"**Solution**\n\n{spec['solution']}", tags=["solution"]),
            ],
        )

    out = COURSE / recipe["unit_dir"] / recipe["notebook"]
    out.parent.mkdir(parents=True, exist_ok=True)
    notebook = {
        "cells": cells,
        "metadata": {
            "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
            "language_info": {"name": "python"},
        },
        "nbformat": 4,
        "nbformat_minor": 5,
    }
    out.write_text(
        json.dumps(notebook, indent=1, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    sections = sum(1 for c in cells if c["id"].startswith("sec-"))
    quizzes = sum(1 for c in cells if "<details>" in c["source"])
    print(
        f"wrote {out} ({len(cells)} cells, {sections} sections, "
        f"{sum(1 for c in cells if c['cell_type'] == 'code')} code, {quizzes} quizzes)"
    )
    if pending:
        print(f"WARNING unplaced extra regions: {pending}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Convert tutorial chapters into unit notebooks.")
    parser.add_argument("chapters", nargs="*", default=sorted(RECIPES))
    parser.add_argument("--docs", type=Path, default=DOCS, help="root of the built qdk-chemistry docs")
    parser.add_argument("--course", type=Path, default=COURSE, help="course directory to write into")
    args = parser.parse_args()
    DOCS = args.docs
    RST_DIR = DOCS / "_sources/tutorials/ground_state_molecular_energies_with_qpe"
    PY_DIR = DOCS / "_static/examples/python"
    COURSE = args.course
    if not RST_DIR.is_dir():
        sys.exit(f"no tutorial sources at {RST_DIR}; pass --docs")
    for chapter in args.chapters:
        convert(chapter)
