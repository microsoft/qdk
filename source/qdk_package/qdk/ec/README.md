---
description: Develop and test quantum error correction schemes with qdk.ec
---

# `qdk.ec`

**Develop and test quantum error correction schemes.**

Taking a quantum error correction scheme from a paper to a declarative artifact
requires deriving checks and readouts, validating circuits, and keeping the
results consistent as the design changes.

`qdk.ec` closes that gap around one artifact: a **qodec**, a declarative
description of a compilation pipeline together with the error correction schemes
that lower each layer of it.

The [`qodec`](https://github.com/microsoft/qodec) package owns that representation:
codes, instruction sets, gadgets, and lowering layers. `qdk.ec` operates directly
on those objects rather than wrapping them in another model. `paulimer` supplies
the Pauli/Clifford algebra and exact stabilizer simulation underneath.

## Installing

`qdk.ec` is an optional extra of the `qdk` package:

```bash
pip install "qdk[ec]"
```

`qdk.ec` is never imported by `import qdk`, so a plain install pays nothing for it.

## Lifecycle

### Develop

`qdk.ec` moves qodecs between disk, memory, and YAML text, and finishes drafts
that a human should not have to finish by hand.

```python
import qdk.ec as ec

qodec = ec.load_yaml("protocol.qodec.yaml")
completed = ec.complete_qodec(qodec)   # or complete_gadget(one_gadget)
ec.save_yaml(completed, "out/")
```

`complete_gadget` discovers checks and Pauli-bearing readouts by exact simulation,
preserves authored flag bindings, and returns a new `qodec.Gadget` without mutating
the draft. `complete_qodec` does the same for every gadget of every layer.

If you are starting from a bare stabilizer code rather than a draft qodec,
`qodec_from_code` synthesizes the whole artifact: a logical instruction set and a
verified circuit behind each of its instructions:

```python
import qodec as qc
from qdk.ec import qodec_from_code, synthesis_notes

code = qc.Code(
    "steane",
    stabilizers=["X_0 X_3 X_4 X_6", ...],
    x=["X_0 X_1 X_3"],
    z=["Z_1 Z_2 Z_5"],
)
qodec = qodec_from_code(code)
print(sorted(qodec.layers[0].gadgets))    # idle, measure_x, measure_z, prepare_x, ...
print(synthesis_notes(qodec)["omitted"])  # anything that could not be synthesized
```

Every synthesized gadget is completed *and* verified against the action it declares,
so an instruction ships only if its circuit provably implements it. Syndrome
extraction uses flag qubits to catch hook errors that would otherwise propagate
from an ancilla onto multiple data qubits.

### Test

One module per question computes typed facts about a qodec: `action`, `checks`,
`code`, `distance`, `faults`, `readouts`. `qdk.ec.equivalence` compares two
artifacts, and `qdk.ec.lint` applies expectations and produces policy-bearing
diagnostics.

```python
import qdk.ec as ec
from qdk.ec import action, distance, equivalence, lint

qodec = ec.load_yaml("protocol.qodec.yaml")
gadget = qodec.layers[0].gadgets["idle"]
code = next(iter(qodec.codes.values()))

expected = action.declared_action_of(gadget)
actual = action.realized_action_of(gadget)
report = lint.diagnose(qodec)
code_distance, witness = distance.code_distance_of(code)
```

Diagnostics carry stable rule IDs, severities, locations, summaries, and details.
Structural errors prevent dependent semantic rules from running.

## Layout

The API is flat: develop, profile, and test are *groupings* of the surface, not
packages you import.

```text
qdk/ec/
├── __init__.py          load / save / from_yaml / to_yaml,
│                        complete_gadget / complete_qodec / qodec_from_code
├── action.py            declared vs realized gadget action
├── checks.py            deterministic parity structure of outcomes
├── code.py              characteristics of qodec.Code objects
├── distance.py          code distance, exact and bounded
├── faults.py            fault propagation to the gadget boundary
├── readouts.py          what measurement outcomes mean
├── equivalence.py       does one artifact match another?
├── lint/                rules, diagnostics, reports, diagnose()
└── _analysis/           private engines (propagation, algebra, solvers)
```

The dependency direction is:

```text
qodec + paulimer + binar + mwpf
                 |
             _analysis
                 |
  profiling modules (action, checks, code, distance, faults, readouts)
                 |
       develop functions + equivalence + lint
```

Public functions accept qodec objects directly. `qodec.Code` is the public code
type; code characteristics such as syndrome, logical effect, and an encoding
Clifford live in `qdk.ec.code`, with distance in `qdk.ec.distance`.

## Dependencies

The `ec` extra installs the qodec object model, Pauli and binary algebra,
collection helpers, and the MWPF solver used by distance bounds:

* `qodec`
* `paulimer`
* `binar`
* `more-itertools`
* `mwpf`

## Examples

[`samples/notebooks/qdk_ec/qdk_ec_walkthrough.ipynb`](../../../../samples/notebooks/qdk_ec/qdk_ec_walkthrough.ipynb)
walks through authoring, profiling, completion, and linting on the [[4,2,2]]
error-detecting code.

[`samples/notebooks/qdk_ec/qodec_from_code.ipynb`](../../../../samples/notebooks/qdk_ec/qodec_from_code.ipynb)
takes the Steane code from a list of stabilizers to a complete qodec with
`qodec_from_code`, without writing a circuit by hand.
