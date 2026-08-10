# `qdk.ec`

**Develop, test, and deploy quantum error correction schemes.**

Taking a quantum error correction scheme from a paper to a production pipeline
usually means writing a bespoke simulation to convince yourself it works, and then
coordinating with several teams to teach a compilation pipeline about it.

`qdk.ec` closes that gap around one artifact: a **qodec** — a declarative
description of a compilation pipeline together with the error correction schemes
that lower each layer of it. Because a qodec is just data, the file you test
against a local simulator is the file you hand to the compilation pipeline.

The [`qodec`](https://github.com/microsoft/qodec) package owns that representation:
codes, instruction sets, gadgets, and lowering layers. `qdk.ec` operates directly
on those objects rather than wrapping them in another model. `paulimer` supplies
the Pauli/Clifford algebra and exact stabilizer simulation underneath.

## Installing

`qdk.ec` is an optional extra of the `qdk` package:

```bash
pip install "qdk[ec]"               # authoring and analysis
pip install "qdk[ec,ec-backends]"   # ... plus the stim / mwpf backends
```

`qdk.ec` is never imported by `import qdk`, so a plain install pays nothing for it.

## Lifecycle

### Develop

`qdk.ec.develop` moves qodecs between disk, memory, and YAML text, and finishes
drafts that a human should not have to finish by hand.

```python
from qdk.ec import develop

codec = develop.load("protocol.qodec.yaml")
completed = develop.complete_qodec(codec)   # or complete_gadget(one_gadget)
develop.save(completed, "out/")
```

`complete_gadget` discovers checks and Pauli-bearing readouts by exact simulation,
preserves authored flag bindings, and returns a new `qodec.Gadget` without mutating
the draft. `complete_qodec` does the same for every gadget of every layer.

If you are starting from a bare stabilizer code rather than a draft qodec,
`qodec_from_code` synthesizes the whole artifact — a logical instruction set and a
verified circuit behind each of its instructions:

```python
import qodec
from qdk.ec.develop import qodec_from_code, synthesis_notes

code = qodec.Code(
    "steane",
    stabilizers=["X_0 X_3 X_4 X_6", ...],
    x=["X_0 X_1 X_3"],
    z=["Z_1 Z_2 Z_5"],
)
codec = qodec_from_code(code)
print(sorted(codec.layers[0].gadgets))    # idle, measure_x, measure_z, prepare_x, ...
print(synthesis_notes(codec)["omitted"])  # anything that could not be synthesized
```
Every synthesized gadget is completed *and* verified against the action it declares,
so an instruction ships only if its circuit provably implements it. Syndrome
extraction uses flag qubits, so the artifact inherits the code's distance rather
than losing it to hook errors; pass `verify_distance=True` to have that measured
and enforced. See `qdk.ec.develop.synthesis` for the construction and its limits.

### Test

`qdk.ec.profile` computes typed facts about a qodec. `qdk.ec.audit` applies
expectations to those facts and produces policy-bearing diagnostics.

```python
from qdk.ec import audit, develop, profile, targets

codec = develop.load("protocol.qodec.yaml")
gadget = codec.layers[0].gadgets["idle"]

expected = profile.declared_action_of(gadget)
actual = profile.realized_action_of(gadget)
report = audit.audit(codec)

distance, witness = targets.gadget_distance_of(gadget, targets.depolarizing(0.001))
```

Audit reports stable rule IDs, severities, locations, summaries, and details.
Structural errors prevent dependent semantic rules from running.

### Deploy

`qdk.ec.targets` evaluates, adapts, and executes qodec programs under external
assumptions. Exact noiseless propagation used for intrinsic discovery lives under
`profile.propagation`; target simulation is reserved for noise, shots, and backend
semantics.

```python
import qodec
from qodec.circuits import Program

from qdk.ec import develop, targets

codec = develop.load("protocol.qodec.yaml")
program = Program(
    [
        qodec.instructions.InstructionCall("prepare", outputs={"0": "q"}),
        qodec.instructions.InstructionCall("measure", inputs={"0": "q"}),
    ],
    codec.layers[0].isa,
)

sampler = targets.StimSampler(codec, noise={"p_data": 0.001, "p_meas": 0.001})
batch = sampler.execute(program, shots=100_000)
```

### Running an existing program under a qodec

You do not have to write a qodec program by hand to use one. Pass a qodec to
`qdk.simulation.run_qir` and an ordinary QIR program — compiled from Q#, OpenQASM,
or anything else — runs with its qubits encoded, its logical outcomes decoded back
into ordinary results:

```python
import qdk
from qdk import qsharp
from qdk.ec import develop
from qdk.simulation import NoiseConfig, run_qir

qsharp.init(target_profile=qdk.TargetProfile.Adaptive)
qir = qsharp.compile("{ use q = Qubit(); X(q); MResetZ(q) }")

noise = NoiseConfig()
noise.x.x = 0.05

codec = develop.load("c4.qodec.yaml")
run_qir(qir, shots=100, type="clifford", noise=noise, qodec=codec)
```

Shots in which the code detected an error are discarded, so fewer than `shots`
results may come back — that is what an error-*detecting* code buys. See
`qdk.ec.targets.run_qir_encoded` for the full options and
`encodable_gates_of(codec)` for what a given qodec can express.

## Layout

```text
qdk/ec/
├── develop/             load, save, and complete qodec objects
├── profile/             actions, checks, readouts, faults, and code distance
│   └── propagation/     exact noiseless semantic propagation
├── audit/               rules, diagnostics, reports, equivalence, audit policy
└── targets/
    ├── model.py         target fault-model boundary
    ├── distance.py      target-conditioned gadget distance
    ├── dem.py           target-conditioned detector error models
    ├── compilers/       lowering and relocation
    ├── deq/             decoded execution and qodec/deq interchange
    ├── qir.py           run an ordinary QIR program under a qodec
    ├── stim.py
    ├── qdk_sim.py
    └── paulimer.py
```

The dependency direction is:

```text
qodec + paulimer
       |
    profile
   /  |   \
develop audit targets
          |
       target model + backend

qodec -> targets.compilers -> targets.{stim, qdk_sim, deq}
```

Public functions accept qodec objects directly. `qodec.Code` is the public code
type; code characteristics such as syndrome, logical effect, distance, and an
encoding Clifford are functions under `qdk.ec.profile`.

## Optional backends

The `ec` extra installs the qodec-facing profiling and audit surface. Backend and
solver dependencies are isolated:

- `stim` — stim emission, sampling, and target-conditioned detector error models
- `mwpf` — MWPF-backed distance bounds
- `deq` — decoded execution and deq interchange (not published to PyPI)

`qdk.ec` passes decoder configuration through to `deq`. It does not define a
decoder protocol or wrap individual decoder implementations.

## Examples

[`samples/notebooks/qdk_ec/qdk_ec_simple_demo.ipynb`](../../../../samples/notebooks/qdk_ec/qdk_ec_simple_demo.ipynb)
is the shortest introduction: one program run noiseless, noisy, and noisy with
error correction applied.

[`samples/notebooks/qdk_ec/qdk_ec_walkthrough.ipynb`](../../../../samples/notebooks/qdk_ec/qdk_ec_walkthrough.ipynb)
walks the whole lifecycle on the [[4,2,2]] error-detecting code.

[`samples/notebooks/qdk_ec/qodec_from_code.ipynb`](../../../../samples/notebooks/qdk_ec/qodec_from_code.ipynb)
takes the Steane code from a list of stabilizers to a sampled memory experiment
with `qodec_from_code`, without writing a circuit by hand, and measures that the
result really does inherit the code's distance.
