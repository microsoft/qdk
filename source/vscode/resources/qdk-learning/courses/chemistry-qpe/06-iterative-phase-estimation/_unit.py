"""Unit helpers - course-infrastructure imports for the notebook."""

import sys
from pathlib import Path

_course_root = str(Path(__file__).resolve().parent.parent)
if _course_root not in sys.path:
    sys.path.insert(0, _course_root)

from _check_env import check as check_env  # noqa: E402, F401
from _course_lib import (  # noqa: E402, F401
    exercise,
    register_exercise,
    register_value_exercise,
)
from _learning_output import quiz, register_quiz  # noqa: E402, F401

# The selected grid point 010000 is k=16 on the 64-point six-bit grid.
MEASURED_PHASE = register_value_exercise("measured_phase", expected=0.25)

# Six iterations over a 12-qubit compute register plus one readout ancilla.
_EXPECTED_CIRCUIT = {
    "iteration_circuits": 6,
    "compute_qubits": 12,
    "readout_ancillas": 1,
}


def _check_circuit(result: object) -> str | None:
    if not isinstance(result, dict):
        return "Return a dictionary with the three keys named in the exercise."
    missing = sorted(set(_EXPECTED_CIRCUIT) - set(result))
    if missing:
        keys = ", ".join(f"<code>{k}</code>" for k in missing)
        return f"The returned dictionary is missing {keys}."
    wrong = sorted(k for k, v in _EXPECTED_CIRCUIT.items() if result[k] != v)
    if wrong:
        keys = ", ".join(f"<code>{k}</code>" for k in wrong)
        return f"Not right yet: {keys}. Read each one off the circuit."
    return None


VALIDATE_CIRCUIT = register_exercise(
    "validate_circuit",
    _check_circuit,
    success_message=(
        "Correct. Twelve compute qubits plus one readout ancilla, "
        "rebuilt for each of the six iterations."
    ),
)


# ---------------------------------------------------------------------------
# Self-check questions
# ---------------------------------------------------------------------------

# The chapter's collapsible questions, registered so the notebook cell can show
# them as answerable ones. They live here rather than inline because a quiz in
# the notebook would put the answers into the cell source.

register_quiz(
    "iqpe-grid-target",
    "Why does six-bit phase estimation meet a 1 mEh target here, even though "
    "adjacent grid points are much farther apart than that?",
    [
        (
            "tuned",
            "The evolution time was tuned using the classically known reference "
            "energy, so the target lands almost exactly on one six-bit grid point.",
            True,
            "The alignment is deliberate. Six bits do not give mEh resolution "
            "for an arbitrary energy at this evolution time.",
        ),
        (
            "six-bits-enough",
            "Six phase bits are enough to resolve any energy to 1 mEh.",
            False,
            "They are not. At this evolution time the grid spacing is far "
            "coarser than 1 mEh; the agreement comes from where the target sits.",
        ),
        (
            "shots-interpolate",
            "Averaging over the 20 shots interpolates between neighbouring grid points.",
            False,
            "Shots make the selected grid point more reliable. They never "
            "produce an energy that lies between two grid points.",
        ),
        (
            "trotter-cancels",
            "Trotter approximation error happens to cancel the grid spacing error.",
            False,
            "Trotter error is a separate contribution and is not controlled "
            "here, so it cannot be relied on to offset discretization.",
        ),
    ],
)

register_quiz(
    "iqpe-state-prep",
    "Why is trial-state preparation included in every IQPE iteration circuit?",
    [
        (
            "fresh-qubits",
            "Each phase bit is measured by a separate circuit, and every shot "
            "begins with newly allocated qubits in the all-zero state.",
            True,
            "So the state-preparation logical circuit has to reload the trial "
            "state before each controlled evolution.",
        ),
        (
            "measurement-collapse",
            "Measuring the readout ancilla collapses the compute register, so "
            "the trial state has to be rebuilt.",
            False,
            "Only the ancilla is measured. The register is gone anyway, but "
            "because each iteration is its own circuit starting from all zeros.",
        ),
        (
            "average-trotter",
            "Repeating it suppresses Trotter error by averaging over preparations.",
            False,
            "State preparation is not the source of Trotter error, and "
            "repeating it does not reduce the error in the evolution unitary.",
        ),
        (
            "feedback-destroys",
            "The classical feedback rotation destroys the trial state each iteration.",
            False,
            "The feedback rotation acts on the readout ancilla, not on the "
            "compute register holding the molecular state.",
        ),
    ],
)

register_quiz(
    "iqpe-grid-control",
    "Which controls change the spacing of the energy grid?",
    [
        (
            "phase-bits",
            "The number of phase bits.",
            True,
            "It sets how finely the phase interval is discretized, and changes "
            "nothing else.",
        ),
        (
            "evolution-time",
            "The evolution time.",
            True,
            "It rescales the grid in energy units — but unlike the bit count it "
            "also changes the unaliased interval and the simulated evolution, so "
            "it is the blunter of the two controls.",
        ),
        (
            "shots",
            "The number of shots per bit.",
            False,
            "More shots make each bit majority more stable. Grid spacing is untouched.",
        ),
        (
            "active-space",
            "The size of the active space.",
            False,
            "That changes the Hamiltonian being measured, not how finely the "
            "phase is resolved.",
        ),
    ],
    multi_select=True,
)

register_quiz(
    "iqpe-readout-ancilla",
    "Which of these are true of the readout ancilla in the rendered circuit?",
    [
        (
            "h-gates",
            "It receives the H gates and the feedback rotation.",
            True,
            "That pair is what puts it in superposition and applies the phase "
            "learned from earlier iterations.",
        ),
        (
            "controls",
            "It controls the Hamiltonian evolution.",
            True,
            "The controlled-unitary hangs off this wire, which is how the phase "
            "is kicked back onto it.",
        ),
        (
            "measured",
            "It is measured to obtain the phase bit.",
            True,
            "One measurement per iteration, and that bit feeds the next one.",
        ),
        (
            "molecular-state",
            "It holds the prepared molecular state.",
            False,
            "The other twelve wires do that — they are the compute register. "
            "The ancilla is algorithm workspace.",
        ),
    ],
    multi_select=True,
)

register_quiz(
    "iqpe-circuit-shape",
    "Why do all six iteration circuits have the same width but different lengths?",
    [
        (
            "power-varies",
            "Every iteration uses the same twelve-qubit compute register and one "
            "readout ancilla, while different controlled powers repeat the "
            "evolution unitary different numbers of times.",
            True,
            "Width is register size, thirteen logical qubits every time. Length "
            "is logical gate count, which the controlled power sets.",
        ),
        (
            "more-qubits",
            "Later iterations act on more qubits, because they resolve more "
            "significant bits.",
            False,
            "The register is fixed at thirteen qubits. Resolving a different "
            "bit changes the controlled power, not the width.",
        ),
        (
            "feedback-ancilla",
            "Each iteration adds another ancilla to carry the feedback.",
            False,
            "The feedback is classical. It changes a rotation angle, not the "
            "number of qubits.",
        ),
        (
            "growing-space",
            "The Trotter step count grows with the active-space size across iterations.",
            False,
            "The active space is fixed for the whole run. What varies between "
            "iterations is the controlled power.",
        ),
    ],
)

register_quiz(
    "iqpe-bit-feedback",
    "How does IQPE turn the result of each iteration into the final bitstring "
    "and phase fraction?",
    [
        (
            "feedback-chain",
            "The majority measurement for each iteration selects a phase bit, "
            "which updates the classical phase feedback used by the next "
            "iteration; after six iterations the feedback calculation combines "
            "the bits into one fraction.",
            True,
            "The script writes that fraction as a conventional six-bit string, "
            "with the most significant bit first.",
        ),
        (
            "one-circuit",
            "All six bits are measured together in a single circuit and read off "
            "at the end.",
            False,
            "That is textbook QPE. The iterative variant deliberately measures "
            "one bit per circuit, which is what keeps the register small.",
        ),
        (
            "independent-bits",
            "The bits are independent, so they can be measured in any order and "
            "concatenated.",
            False,
            "They are not independent. Each measured bit updates the phase "
            "feedback for the next iteration, so the order is fixed.",
        ),
        (
            "average-estimates",
            "The phase fraction is the average of the six per-iteration phase "
            "estimates.",
            False,
            "Each iteration yields a single bit, not a phase estimate. "
            "Averaging them would throw away each bit's place value.",
        ),
    ],
)

register_quiz(
    "iqpe-aggregation",
    "Why should the final aggregation use complete bitstrings rather than vote "
    "on each bit across complete runs?",
    [
        (
            "joint",
            "Each complete bitstring is one phase-grid point with a corresponding "
            "energy, and voting per bit could assemble a bitstring that no run "
            "ever produced.",
            True,
            "Voting bit by bit also discards the joint distribution that was "
            "actually observed.",
        ),
        (
            "slower",
            "Per-bit voting gives the same answer but takes longer to compute.",
            False,
            "It does not give the same answer: it can synthesize a result that "
            "never occurred in any run.",
        ),
        (
            "simultaneous",
            "Complete bitstrings are required because the bits are measured "
            "simultaneously.",
            False,
            "They are measured one per iteration. The reason is that a "
            "bitstring is only meaningful as a whole grid point.",
        ),
        (
            "msb-bias",
            "Per-bit voting would bias the result toward the most significant bit.",
            False,
            "The problem is not bias toward one bit. It is that the assembled "
            "string may correspond to no observed run at all.",
        ),
    ],
)

register_quiz(
    "iqpe-energy-comparison",
    "Which energy comparison determines whether the IQPE workflow meets the "
    "teaching target?",
    [
        (
            "casci-same-space",
            "The reconstructed IQPE total energy against the CASCI energy of the "
            "same selected active-space Hamiltonian.",
            True,
            "The same Hamiltonian sits on both sides, so the difference isolates "
            "algorithmic error.",
        ),
        (
            "experiment",
            "The reconstructed IQPE total energy against an experimental "
            "measurement for the molecule.",
            False,
            "That would mix algorithmic error with molecular-model error and "
            "could not tell you which one you were looking at.",
        ),
        (
            "larger-space",
            "The reconstructed IQPE total energy against a CASCI energy computed "
            "in a larger active space.",
            False,
            "Changing the space changes the Hamiltonian, so the comparison would "
            "no longer isolate the algorithm.",
        ),
        (
            "hartree-fock",
            "The active-space energy against the Hartree-Fock energy.",
            False,
            "That measures how much correlation energy was recovered, not "
            "whether phase estimation reached its target.",
        ),
    ],
)

register_quiz(
    "iqpe-observed-result",
    "What bitstring distribution did the script produce?",
    [
        (
            "19-1",
            "`010000` appeared 19 times and `001111` once, so `010000` is the "
            "most frequent result.",
            True,
            "It gives an active-space energy of -9.652276065987 Eh and a "
            "reconstructed total of -108.770051792909 Eh once the core energy "
            "is added back.",
        ),
        (
            "reversed",
            "`001111` appeared 19 times and `010000` once.",
            False,
            "Reversed. `010000` is the majority result; `001111` is the "
            "neighbouring grid point that turned up once.",
        ),
        (
            "unanimous",
            "All 20 runs produced `010000`.",
            False,
            "Close, but one run landed on the adjacent grid point `001111`. "
            "Finite sampling and Trotter error still move the outcome sometimes.",
        ),
        (
            "spread",
            "The 20 runs were spread across six different bitstrings, one per "
            "phase bit.",
            False,
            "The distribution is far tighter: two grid points in total, one of "
            "them nineteen times.",
        ),
    ],
)

register_quiz(
    "iqpe-target-met",
    "Does the result meet the teaching target, and what does that establish?",
    [
        (
            "boundary",
            "Yes, at the boundary: the reconstructed total is 1 mEh above the "
            "selected-space CASCI reference, which validates this configured "
            "teaching workflow.",
            True,
            "It does not remove molecular-model error or establish agreement "
            "with experiment. The offset itself was set by the reference-guided "
            "phase-grid alignment.",
        ),
        (
            "matches-experiment",
            "Yes, and it establishes that the workflow reproduces the "
            "experimental energy of the molecule to 1 mEh.",
            False,
            "Nothing here is compared against experiment. The reference is a "
            "CASCI energy in the same active space.",
        ),
        (
            "misses",
            "No, a 1 mEh offset is outside the teaching target.",
            False,
            "The target is 1 mEh, so this meets it, though only exactly at the "
            "boundary.",
        ),
        (
            "errors-negligible",
            "Yes, and it shows that Trotter and sampling error are negligible.",
            False,
            "Both can still affect which bitstring is selected. The agreement "
            "comes from the deliberate grid alignment, not from those errors "
            "vanishing.",
        ),
    ],
)

register_quiz(
    "iqpe-more-bits",
    "What happens if the number of phase bits increases while the repeated-power "
    "strategy stays fixed?",
    [
        (
            "finer-grid",
            "The phase grid becomes finer.",
            True,
            "That is the point of adding a bit — the interval is divided more finely.",
        ),
        (
            "extra-circuit",
            "One more iteration circuit is needed.",
            True,
            "One circuit per bit, so an extra bit is an extra circuit to run.",
        ),
        (
            "power-doubles",
            "The largest controlled-unitary power doubles.",
            True,
            "For repeated-power Trotter evolution that is the expensive part: it "
            "increases circuit size and simulator runtime substantially.",
        ),
        (
            "shorter",
            "The circuits get shorter, because each bit carries less information.",
            False,
            "The opposite — the longest circuit grows, because the largest "
            "controlled power doubles.",
        ),
    ],
    multi_select=True,
)

register_quiz(
    "iqpe-more-shots",
    "Would increasing the number of shots per bit make the phase grid finer?",
    [
        (
            "no-spacing-fixed",
            "No. More shots can make each bit majority more stable, but grid "
            "spacing is set by the evolution time and the number of phase bits.",
            True,
            "Shots change how confidently one grid point is selected, never the "
            "spacing between grid points.",
        ),
        (
            "yes-interpolate",
            "Yes, averaging more shots interpolates between grid points.",
            False,
            "The majority vote selects one grid point. It never produces a value "
            "between two of them.",
        ),
        (
            "yes-trotter",
            "Yes, more shots reduce Trotter error, which is what sets the spacing.",
            False,
            "Trotter error is not what sets grid spacing, and repeating shots "
            "does not reduce it.",
        ),
        (
            "no-active-space",
            "No, because the grid is fixed by the size of the active space.",
            False,
            "The right verdict for the wrong reason. Spacing comes from the "
            "evolution time and the number of phase bits.",
        ),
    ],
)
