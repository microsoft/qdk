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
        "skip_sections": ["Cumulative assignment"],
        # What survives of the lab-notebook paragraph once its own sentences go.
        "drop_blocks": ["This is a note-taking document"],
        "rewrites": [
            ("where appropriate; and", "where appropriate."),
        ],
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
                "hint": "Watch the signs. Both energies are negative, so the lower one is the "
                "more negative number rather than the one closer to zero. `energies` holds a "
                "basis-set name for each energy, and the function has to return the name.",
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
            # The learner is already in the notebook that shows the viewer.
            "Download and open",
        ],
        "rewrites": [
            (
                "## Running the calculation",
                "## Running the calculation\n\nThe cells above have already run the "
                "workflow. Use their output to answer the questions below.",
            ),
            (
                "The Jupyter notebook displays every candidate natural orbital",
                "The viewer below displays every candidate natural orbital",
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
                        "occupation and entropy overlays.",
                    ),
                ],
            },
        ],
        "exercises": [
            {
                "section": "The active-space choice",
                "prompt": "## Count the determinants\n\n"
                "The refined active space is CAS $(6,6)$: six electrons in six spatial orbitals, "
                "so three $\\alpha$ and three $\\beta$ electrons. The $\\alpha$ and $\\beta$ "
                "occupations are chosen independently of each other.\n\n"
                "Fix the function below so that it returns the number of determinants this active "
                "space contains, then run the cell.",
                "code": "from math import comb\n\nfrom _unit import exercise\n\n\n"
                "@exercise\ndef determinant_count():\n"
                "    alpha = comb(6, 3)\n"
                "    beta = comb(6, 3)\n"
                "    return alpha\n",
                "hint": "`alpha` and `beta` already count the arrangements for each spin on its "
                "own. Nothing ties the two choices together, so ask yourself how many "
                "whole-determinant arrangements a single $\\alpha$ choice can appear in.",
                "solution": "Return the product of the two counts:\n\n"
                "```python\nreturn alpha * beta\n```\n\n"
                "$\\binom{6}{3}=20$, so the refined space holds $20\\times20=400$ determinants, "
                "down from the 3,136 of the initial CAS $(10,8)$ valence space.",
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
            (
                "so the complete script displays the all-identity term",
                "so the preview below displays the all-identity term",
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
                        "### Pauli term preview\n\n"
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
                        "### Recording the results\n\n"
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
                "hint": "Two traps here. The answer is not the number of spatial orbitals, and "
                "it does not include the phase-estimation ancilla. Work out what the register "
                "stores one qubit of. `num_active_spatial_orbitals` is in scope from above.",
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
                        "### Choosing the determinant count\n\n"
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
                        "### Circuit cost\n\n"
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
                "hint": "`fidelities` maps each determinant count to its fidelity. You are asked "
                "for a count, not a fidelity, so you need the keys, filtered by what their "
                "values do. More than one count may qualify.",
                "solution": "```python\n@exercise\ndef first_majority_count():\n"
                "    return min(c for c, f in fidelities.items() if f > 0.5)\n```\n\n"
                "Two determinants. The one-determinant fidelity is about 0.4825, so no single "
                "configuration holds a majority of the ground state at this stretched geometry.",
            }
        ],
    },
    "06": {
        "rst": "06_iterative_phase_estimation.rst.txt",
        "py": "tutorial_run_iqpe.py",
        "unit_dir": "05-iterative-phase-estimation",
        "notebook": "iterative_phase_estimation.ipynb",
        "skip_sections": ["Example download"],
        # Every marked region here only defines a function, so the pieces that
        # actually run the calculation are imported from the shipped scripts.
        "pre_code": "from tutorial_prepare_trial_state import circuit_statistics\n"
        "from tutorial_run_iqpe import (\n"
        "    prepare_iqpe_problem,\n"
        "    print_iqpe_results,\n"
        "    run_iqpe_workflow,\n"
        ")",
        # The chapter sends the reader to a separate notebook for the circuit
        # viewer and to a terminal for the run; both happen in cells here.
        "drop_blocks": [
            "Before running the simulator, open",
            "Record the evidence you used in the Jupyter notebook",
            "run the script from the Visual Studio Code integrated terminal",
            "python tutorial_run_iqpe.py",
        ],
        "rewrites": [
            (
                "## IQPE circuit visualization",
                "## IQPE circuit visualization\n\nThe cells below build the six "
                "iteration circuits and render the shortest one. No quantum "
                "simulation runs here.",
            ),
            (
                "## The complete workflow",
                "## The complete workflow\n\nThe cell below runs the complete "
                "workflow. It is the long step in this chapter and reports progress "
                "after each complete run.",
            ),
        ],
        "extra_regions": [],
        "inserts": [
            {
                "section": "IQPE circuit visualization",
                "cells": [
                    (
                        "code",
                        "import json\n\n"
                        "from IPython.display import Markdown, display\n"
                        "from qdk.widgets import Circuit\n\n"
                        "problem = prepare_iqpe_problem()\n\n"
                        "circuit_powers = [32, 16, 8, 4, 2, 1]\n\n"
                        "circuit_rows = [\n"
                        "    (power, *circuit_statistics(circuit)[:2])\n"
                        "    for power, circuit in zip(\n"
                        "        circuit_powers, problem.iteration_circuits, strict=True\n"
                        "    )\n"
                        "]\n\n"
                        "table_rows = [\n"
                        '    "| Controlled power | Logical qubits | Decomposed logical gates |",\n'
                        '    "|---:|---:|---:|",\n'
                        "]\n\n"
                        "for power, qubits, gates in circuit_rows:\n"
                        '    table_rows.append(f"| {power} | {qubits} | {gates:,} |")\n\n'
                        'display(Markdown("\\n".join(table_rows)))\n\n'
                        "shortest_circuit = problem.iteration_circuits[-1]\n"
                        "num_qubits, num_gates, gate_counts = circuit_statistics(shortest_circuit)\n",
                    ),
                    (
                        "md",
                        "## Register roles\n\n"
                        "The circuit viewer numbers wires from top to bottom:\n\n"
                        "- **q0** is the readout ancilla. Its H gates, feedback rotation, "
                        "controlled evolution, and measurement extract one phase bit.\n"
                        "- **q1 to q12** are the compute register. They hold the encoded "
                        "molecular trial state and receive the controlled Hamiltonian "
                        "evolution.\n\n"
                        "The readout ancilla is algorithm workspace; it is not a thirteenth "
                        "molecular spin orbital.",
                    ),
                    (
                        "md",
                        "## Render the shortest circuit\n\n"
                        "The complete decomposed circuit is long because it contains one state "
                        "preparation and one controlled first-order Trotter sequence for a "
                        "247-term Hamiltonian. Use the viewer's pan and zoom controls to inspect "
                        "the ancilla operations and the controlled gates connecting q0 to the "
                        "compute register.",
                    ),
                    ("code", "display(Circuit(shortest_circuit.get_qsharp_circuit()))\n"),
                ],
            },
            {
                "section": "The complete workflow",
                "cells": [
                    (
                        "code",
                        "iqpe_result = run_iqpe_workflow()\n"
                        "print_iqpe_results(iqpe_result)\n",
                    ),
                ],
            },
        ],
        "exercises": [
            {
                "section": "IQPE circuit visualization",
                "prompt": "## Validate the circuit structure\n\n"
                "Before trusting any phase-estimation result, confirm the circuit is the one "
                "you think it is. Three numbers describe it, and each is somewhere learners "
                "commonly go wrong.\n\n"
                "Complete `validate_circuit` so it reports:\n\n"
                "- `iteration_circuits`, how many iteration circuits one complete run uses\n"
                "- `compute_qubits`, how many wires hold the encoded molecular state\n"
                "- `readout_ancillas`, how many wires are algorithm workspace rather than "
                "spin orbitals\n\n"
                "Read each one from `problem` or from the statistics computed above, rather "
                "than typing the numbers in. The stub below gets all three wrong.",
                "code": "from _unit import exercise\n\n\n"
                "@exercise\n"
                "def validate_circuit():\n"
                "    return {\n"
                '        "iteration_circuits": 1,\n'
                '        "compute_qubits": num_qubits,\n'
                '        "readout_ancillas": 0,\n'
                "    }\n",
                "hint": "`problem.iteration_circuits` is a list, which settles the first one. "
                "`problem.mapping` knows how wide the compute register is. For the third, "
                "compare that width with `num_qubits`, the width of the circuit you rendered, "
                "and re-read the note above about what the extra wire is.",
                "solution": "```python\n@exercise\ndef validate_circuit():\n"
                "    compute = problem.mapping.num_compute_qubits\n"
                "    return {\n"
                '        "iteration_circuits": len(problem.iteration_circuits),\n'
                '        "compute_qubits": compute,\n'
                '        "readout_ancillas": num_qubits - compute,\n'
                "    }\n```\n\n"
                "Six, twelve, and one. The rendered circuit is thirteen wires wide, but only "
                "twelve carry the molecule: two per active spatial orbital. The thirteenth is "
                "the readout ancilla, and it is rebuilt along with the trial state on every "
                "one of the six iterations, which is why preparation cost from Chapter 5 is "
                "paid six times over.",
            },
            {
                "section": "Molecular energy reconstruction",
                "prompt": "## Read a phase off the grid\n\n"
                "Each complete run returns a six-bit string. That string is the binary "
                "representation of a grid index $k$, and the estimated phase is "
                "$\\varphi_k = k / 2^{6}$.\n\n"
                "Complete `measured_phase` so it converts the measured bitstring into "
                "its phase fraction.",
                "code": "from _unit import exercise\n\n"
                'measured_bitstring = "010000"\n\n\n'
                "@exercise\n"
                "def measured_phase():\n"
                "    return 0.0\n",
                "hint": "Two steps. First turn the bitstring into the grid index it represents, "
                "which `int` can do if you tell it the base. Then scale that index by the size "
                "of the grid, which follows from how many bits the string has.",
                "solution": "```python\n@exercise\ndef measured_phase():\n"
                "    return int(measured_bitstring, 2) / 2 ** len(measured_bitstring)\n```\n\n"
                "The selected grid point `010000` is $k=16$, so $\\varphi = 16/64 = 0.25$.",
            }
        ],
    },
}

# Accent colours match the published tutorial's callout styling. Bodies use an
# alpha tint so they stay readable over both light and dark editor themes.
ADMONITION_ACCENTS = {
    "chapter-focus": "#6d3f8c",
}
QUIZ_ACCENT = "#8c4a00"
# Cross-references to the published documentation become real links; references to
# other tutorial chapters stay as text, because those chapters are units in here.
DOCS_SITE = "https://microsoft.github.io/qdk-chemistry/"


def _badge(accent, glyph):
    """A round marker for a callout header, inverted against the accent bar."""
    return (
        '<span style="display:inline-block;width:1.15em;height:1.15em;'
        "line-height:1.15em;border-radius:50%;background:#ffffff;"
        f'color:{accent};text-align:center;font-weight:700;margin-right:0.5em;">'
        f"{glyph}</span>"
    )


def callout(title, body, accent, icon=""):
    return (
        f'<div style="border-left:4px solid {accent};background:{accent}1a;'
        'border-radius:4px;margin:1em 0;">'
        f'<div style="background:{accent};color:#ffffff;padding:0.35em 0.8em;'
        'font-weight:600;">'
        f"{icon}{title}</div>\n\n"
        f'<div style="padding:0.1em 1em;">\n\n{body}\n\n</div>\n\n</div>'
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
        if ref.startswith("../"):
            return link(label, DOCS_SITE + ref.lstrip("./") + ".html")
        # Anchors point inside the tutorial, so only the wording survives.
        return "*" + (label if target else label.replace("-", " ")) + "*"
    return label


def link(text, url):
    # Mark links that leave the notebook for the documentation site.
    label = f"{text} \u2197" if url.startswith("http") else text
    return f"[{label}](<{url}>)" if "(" in url or ")" in url else f"[{label}]({url})"


def inline(text):
    # RST's escaped space glues a role to the previous word, but markdown will not
    # open inline math on a dollar that follows a word character.
    text = re.sub(r"\\ (?=:math:)", " ", text)
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
DIRECTIVE = re.compile(r"^([ \t]*)\.\. ([a-z-]+):: ?(.*)$")
ANCHOR = re.compile(r"^\.\. _[\w-]+:\s*$")
LAB_NOTEBOOK = re.compile(r"lab[ -]notebook", re.I)


def strip_lab_notebook(blocks):
    """Remove the tutorial's cumulative lab-notebook assignment.

    The course does not ship the assignment, so its callouts go entirely and its
    sentences go line by line. The sources use one sentence per line, so this
    leaves the surrounding paragraphs intact.
    """
    kept = []
    for block in blocks:
        if block[0] == "directive":
            if block[3].get("class") == "lab-notebook-assignment":
                continue
            body = [ln for ln in block[4] if not LAB_NOTEBOOK.search(ln)]
            kept.append((block[0], block[1], block[2], block[3], body))
        elif block[0] == "prose":
            lines = [ln for ln in block[1] if not LAB_NOTEBOOK.search(ln)]
            if lines:
                kept.append(("prose", lines))
        else:
            kept.append(block)
    return kept


def _body(lines, start, indent=0):
    """Collect the indented body of a directive starting after line `start`."""
    options, body, i = {}, [], start
    pad = " " * (indent + 3)
    while i < len(lines) and re.match(r"^\s+:[\w-]+:", lines[i]):
        key, _, value = lines[i].strip().lstrip(":").partition(":")
        options[key.strip()] = value.strip()
        i += 1
    while i < len(lines) and not lines[i].strip():
        i += 1
    while i < len(lines) and (not lines[i].strip() or lines[i].startswith(pad)):
        body.append(lines[i][len(pad):] if lines[i].startswith(pad) else "")
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
            indent, name, argument = directive.group(1), directive.group(2), directive.group(3)
            options, body, i = _body(lines, i + 1, len(indent.expandtabs()))
            blocks.append(("directive", name, argument, options, body))
            continue
        if line.startswith(".. "):
            # Neither a directive nor an anchor, so it is an RST comment.
            i += 1
            while i < len(lines) and lines[i].startswith("   "):
                i += 1
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
    """An RST definition list: each unindented term followed by an indented definition.

    A blank line between entries splits them into separate blocks, so a block
    carrying a single term is still a definition list.
    """
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
    return terms >= 1


def render_directive(name, argument, options, body):
    text = "\n".join(inline(ln) for ln in body)
    if name == "math":
        return "$$\n" + "\n".join(body).strip() + "\n$$"
    if name == "code-block":
        return f"```{argument or 'text'}\n" + "\n".join(body).strip() + "\n```"
    if name == "admonition":
        title = inline(argument)
        accent = ADMONITION_ACCENTS.get(options.get("class", ""))
        if accent:
            return callout(title, text.strip(), accent, icon=_badge(accent, "!"))
        return f"**{title}**\n\n" + "\n".join("> " + ln if ln else ">" for ln in text.splitlines())
    if name == "toctree":
        return ""
    if name == "list-table":
        rows, current = [], None
        for ln in body:
            stripped = ln.strip()
            if stripped.startswith("* -"):
                if current:
                    rows.append(current)
                current = [stripped[3:].strip()]
            elif stripped.startswith("-") and current is not None:
                current.append(stripped[1:].strip())
            elif stripped and current:
                current[-1] = (current[-1] + " " + stripped).strip()
        if current:
            rows.append(current)
        if not rows:
            return text
        width = max(len(r) for r in rows)
        rows = [r + [""] * (width - len(r)) for r in rows]
        headed = bool(options.get("header-rows"))
        head = rows[0] if headed else [""] * width
        rest = rows[1:] if headed else rows
        out = ["| " + " | ".join(inline(c) for c in head) + " |", "|" + "---|" * width]
        out += ["| " + " | ".join(inline(c) for c in r) + " |" for r in rest]
        caption = inline(argument).strip()
        table = "\n".join(out)
        block = f"{table}\n\n*{caption}*" if caption else table
        if options.get("align") == "center":
            # A flex column centres the table itself, which text-align cannot do.
            return (
                '<div style="display:flex;flex-direction:column;align-items:center;">'
                f"\n\n{block}\n\n</div>"
            )
        return block
    if name in ("figure", "image", "graphviz"):
        filename = Path(argument.strip()).name
        if name == "graphviz":
            filename = str(Path(filename).with_suffix(".png"))
        alt = inline(options.get("alt", "")).strip() or filename
        image = f"![{alt}](attachment:{filename})"
        caption = inline(options["caption"]).strip() if "caption" in options else text.strip()
        centred = f"{image}\n\n*{caption}*" if caption else image
        return f'<div style="text-align:center;">\n\n{centred}\n\n</div>'
    return text


MATH_SPAN = re.compile(r"\$([^$]+)\$")


def _math_to_html(text, where=""):
    """Render inline math as HTML for text that sits inside a raw HTML tag.

    Markdown, and therefore KaTeX, never runs inside an HTML tag's content, so a
    quiz question written into ``<summary>`` would show its dollar signs. Only the
    handful of constructs the questions actually use are covered.
    """

    def one(match):
        body = match.group(1)
        # \mathrm marks upright text; anything else is a variable, so italic.
        parts = re.split(r"(\\mathrm\{[^}]*\})", body)
        for i, part in enumerate(parts):
            if part.startswith("\\mathrm{"):
                parts[i] = part[len("\\mathrm{") : -1]
            else:
                parts[i] = re.sub(r"[A-Za-z]+", lambda m: f"<i>{m.group(0)}</i>", part)
        body = "".join(parts)
        body = re.sub(r"_\{([^}]*)\}", r"<sub>\1</sub>", body)
        body = re.sub(r"\^\{([^}]*)\}", r"<sup>\1</sup>", body)
        body = re.sub(r"_(\w)", r"<sub>\1</sub>", body)
        body = re.sub(r"\^(\w)", r"<sup>\1</sup>", body)
        body = body.replace("\\ ", "&nbsp;").replace("\\,", "&nbsp;")
        if "\\" in body:
            print(f"WARNING unrendered math in {where or 'a quiz question'}: {body}")
        return body

    return MATH_SPAN.sub(one, text)


def quiz(question, body):
    answer = "\n".join(inline(ln) for ln in body).strip()
    title = _math_to_html(inline(question), where="quiz question")
    return (
        f'<div style="border-left:4px solid {QUIZ_ACCENT};background:{QUIZ_ACCENT}1a;'
        'border-radius:4px;margin:1em 0;">'
        "<details>"
        f'<summary style="background:{QUIZ_ACCENT};color:#ffffff;padding:0.35em 0.8em;'
        'cursor:pointer;font-weight:600;">'
        f"&#10067;&nbsp;{title}</summary>\n\n"
        f'<div style="padding:0.1em 1em;">\n\n{answer}\n\n</div>\n\n'
        "</details></div>"
    )


AUTHORING_TAGS = {"hint", "solution", "exercise"}


def merge_markdown_runs(cells):
    """Collapse each section's prose, figures and quizzes into one markdown cell so the
    heading stays next to the code that follows it. Section starts break the run."""
    merged = []
    for cell in cells:
        previous = merged[-1] if merged else None
        tags = set(cell["metadata"].get("tags", []))
        previous_tags = set(previous["metadata"].get("tags", [])) if previous else set()
        if (
            previous is not None
            and previous["cell_type"] == "markdown"
            and cell["cell_type"] == "markdown"
            and not any(t.startswith("section") for t in tags)
            and not (tags & AUTHORING_TAGS)
            and not (previous_tags & AUTHORING_TAGS)
        ):
            previous["source"] = (
                previous["source"].rstrip("\n") + "\n\n" + cell["source"].lstrip("\n")
            )
            attachments = {**previous.get("attachments", {}), **cell.get("attachments", {})}
            if attachments:
                previous["attachments"] = attachments
            continue
        merged.append(cell)
    return merged


def merge_code_runs(cells):
    """The progress tree names a code cell after the heading above it, so a run of
    adjacent code cells would repeat one name. Join each run into a single cell."""
    merged = []
    for cell in cells:
        previous = merged[-1] if merged else None
        if (
            previous is not None
            and previous["cell_type"] == "code"
            and cell["cell_type"] == "code"
            and not previous["metadata"].get("tags")
            and not cell["metadata"].get("tags")
            and not previous.get("outputs")
            and not cell.get("outputs")
        ):
            body = previous["source"].rstrip("\n") + "\n\n" + cell["source"].lstrip("\n")
            previous["source"] = body
            previous["id"] = "c-" + hashlib.sha256(body.encode()).hexdigest()[:12]
            continue
        merged.append(cell)
    return merged


def convert(key):
    recipe = RECIPES[key]
    blocks = strip_lab_notebook(
        parse((RST_DIR / recipe["rst"]).read_text(encoding="utf-8"))
    )
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
                for sub in strip_lab_notebook(
                    parse(included.read_text(encoding="utf-8"))
                ):
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

    before_you_begin = [
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
    ]
    setup = before_you_begin if not py_text else before_you_begin + [
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

    cells = merge_markdown_runs(cells)
    cells = merge_code_runs(cells)
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
