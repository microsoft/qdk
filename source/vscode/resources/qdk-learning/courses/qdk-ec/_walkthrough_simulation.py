# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Shared helpers for the QDK-EC Learning course: stage builders, QIR sampling with deq, and sweep plots."""

from __future__ import annotations

from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from functools import partial
import html
import importlib.machinery
import importlib.util
from itertools import cycle, islice
import multiprocessing
import sys
import types
from typing import TYPE_CHECKING

import qodec
from qdk import Result, TargetProfile, ec, qsharp
from qdk.simulation import NoiseConfig, run_qir
from qdk.simulation.decoders import (
    ExecutionRejected,
    prepare_deq_decoder,
    prepare_syndrome_decoder,
)

if TYPE_CHECKING:
    from deq.runtime import Runtime
    from matplotlib.axes import Axes

_SHOTS_PER_CHUNK = 1_000
_codec: qodec.Qodec | None = None
_Sweep = dict[str, list[tuple[float, Counter[str]]]]
_Pooled = dict[str, dict[float, Counter[str]]]
_HAS_DEQ = importlib.util.find_spec("deq") is not None


def _render_stim_timeline_svg(source: str) -> str:
    """Render an inline SVG circuit timeline diagram when native stim is unavailable."""
    ops: list[tuple[str, list[int]]] = []
    max_q = 0
    for raw in source.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        gate, args = parts[0], [int(x) for x in parts[1:]]
        if gate in ("DETECTOR", "OBSERVABLE_INCLUDE", "TICK", "QUBIT_COORDS", "SHIFT_COORDS"):
            continue
        arity = 2 if gate in ("CX", "CNOT", "CZ", "SWAP") else 1
        for i in range(0, len(args), arity):
            chunk = args[i : i + arity]
            ops.append((gate, chunk))
            if chunk:
                max_q = max(max_q, max(chunk))
    n_qubits = max_q + 1
    col_w, row_h, pad_l, pad_t = 48, 36, 64, 28
    width = pad_l + max(len(ops), 1) * col_w + 32
    height = pad_t + n_qubits * row_h + 16
    svg = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        'style="background:#ffffff;border:1px solid #d0d7de;border-radius:6px;font-family:monospace;font-size:12px">'
    ]
    for q in range(n_qubits):
        y = pad_t + q * row_h
        svg.append(f'<text x="14" y="{y + 4}" fill="#24292f" font-weight="bold">q{q}</text>')
        svg.append(f'<line x1="{pad_l - 12}" y1="{y}" x2="{width - 16}" y2="{y}" stroke="#8c959f" stroke-width="1.5"/>')
    for col, (gate, targets) in enumerate(ops):
        x = pad_l + col * col_w + 14
        if len(targets) == 2:
            q0, q1 = targets[0], targets[1]
            y0, y1 = pad_t + q0 * row_h, pad_t + q1 * row_h
            svg.append(f'<line x1="{x}" y1="{y0}" x2="{x}" y2="{y1}" stroke="#0969da" stroke-width="2"/>')
            svg.append(f'<circle cx="{x}" cy="{y0}" r="5" fill="#0969da"/>')
            if gate in ("CX", "CNOT"):
                svg.append(f'<circle cx="{x}" cy="{y1}" r="9" fill="#ffffff" stroke="#0969da" stroke-width="2"/>')
                svg.append(f'<line x1="{x - 6}" y1="{y1}" x2="{x + 6}" y2="{y1}" stroke="#0969da" stroke-width="2"/>')
                svg.append(f'<line x1="{x}" y1="{y1 - 6}" x2="{x}" y2="{y1 + 6}" stroke="#0969da" stroke-width="2"/>')
            else:
                svg.append(f'<circle cx="{x}" cy="{y1}" r="5" fill="#0969da"/>')
        elif len(targets) == 1:
            q0 = targets[0]
            y0 = pad_t + q0 * row_h
            fill = "#dafbe1" if gate == "R" else ("#fff8c5" if gate == "M" else "#ddf4ff")
            stroke = "#1a7f37" if gate == "R" else ("#9a6700" if gate == "M" else "#0969da")
            svg.append(
                f'<rect x="{x - 12}" y="{y0 - 11}" width="24" height="22" rx="4" '
                f'fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>'
            )
            svg.append(f'<text x="{x}" y="{y0 + 4}" text-anchor="middle" fill="#24292f" font-weight="bold">{html.escape(gate)}</text>')
    svg.append("</svg>")
    return "".join(svg)


def ensure_stim_parser() -> None:
    """Register pure-Python Stim and HiGHS fallbacks on platforms without native wheels (e.g., Arm64)."""
    if importlib.util.find_spec("highspy") is None:
        import qdk.ec._analysis.distance_solvers as _ds

        def _fallback_highs_bounds(data, upper_bound, coset, options):
            return _ds.enumeration_bounds(data, upper_bound, coset, _ds.EnumerationSolverOptions())

        _ds.highs_bounds = _fallback_highs_bounds

    if getattr(sys.modules.get("stim"), "_is_qdk_ec_shim", False) or importlib.util.find_spec("stim") is None:
        from qodec.instructions import InstructionCall

        def _pure_parse_stim(source: str, instruction_set: qodec.InstructionSet) -> list[InstructionCall]:
            decls = instruction_set.instructions
            calls: list[InstructionCall] = []
            for raw in source.splitlines():
                line = raw.split("#", 1)[0].strip()
                if not line:
                    continue
                parts = line.split()
                gate, args = parts[0], [int(x) for x in parts[1:]]
                if gate in ("DETECTOR", "OBSERVABLE_INCLUDE", "TICK", "QUBIT_COORDS", "SHIFT_COORDS"):
                    continue
                arity = max(len(decls[gate].inputs), len(decls[gate].outputs))
                for i in range(0, len(args), arity):
                    calls.append(InstructionCall(gate, operands=args[i : i + arity]))
            return calls

        qodec.register(_pure_parse_stim, format="stim")

        if "stim" not in sys.modules or getattr(sys.modules.get("stim"), "_is_qdk_ec_shim", False):
            _stim_mod = types.ModuleType("stim")
            _stim_mod.__spec__ = importlib.machinery.ModuleSpec("stim", loader=None)
            _stim_mod._is_qdk_ec_shim = True

            class _DiagramHelper:
                def __init__(self, text: str, svg_html: str):
                    self._text = text
                    self._svg_html = svg_html

                def __str__(self) -> str:
                    return self._text

                def _repr_html_(self) -> str:
                    return self._svg_html

            class _ShimStimCircuit:
                def __init__(self, source: str = ""):
                    self.source = source.strip()

                def diagram(self, type: str = "timeline-svg-html", **kwargs):
                    steps = [ln.strip() for ln in self.source.splitlines() if ln.strip() and not ln.strip().startswith("#")]
                    header = f"Stim Circuit Timeline ({len(steps)} instructions):"
                    body = "\n".join(f"  [{i:02d}] {op}" for i, op in enumerate(steps))
                    return _DiagramHelper(header + "\n" + body, _render_stim_timeline_svg(self.source))

            _stim_mod.Circuit = _ShimStimCircuit
            sys.modules["stim"] = _stim_mod


ensure_stim_parser()


def show_stim_diagram(source: str) -> None:
    """Display a Stim circuit timeline diagram on a high-contrast card safe for dark and light themes."""
    import stim
    from IPython.display import HTML, display

    if getattr(stim, "_is_qdk_ec_shim", False):
        display(HTML(_render_stim_timeline_svg(source)))
        return
    svg = str(stim.Circuit(source).diagram("timeline-svg"))
    display(
        HTML(
            '<div style="background:#ffffff;color:#1f2328;padding:10px 14px;'
            "border-radius:6px;border:1px solid #d0d7de;display:inline-block;"
            f'max-width:100%;overflow-x:auto">{svg}</div>'
        )
    )


def make_c4() -> qodec.Code:
    """Return the [[4, 2, 2]] C4 error-detecting stabilizer code."""
    return qodec.Code(
        "C4",
        stabilizers=["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
        x=["X_0 X_1", "X_0 X_2"],
        z=["Z_0 Z_2", "Z_0 Z_1"],
    )


def make_five_qubit_code() -> qodec.Code:
    """Return the [[5, 1, 3]] five-qubit stabilizer code."""
    return qodec.Code(
        "C5",
        stabilizers=[
            "X_0 Z_1 Z_2 X_3",
            "X_1 Z_2 Z_3 X_4",
            "X_0 X_2 Z_3 Z_4",
            "Z_0 X_1 X_3 Z_4",
        ],
        x=["X_0 X_1 X_2 X_3 X_4"],
        z=["Z_0 Z_1 Z_2 Z_3 Z_4"],
    )


def make_baseline_protocol() -> qodec.Qodec:
    """Build the baseline bare-css/v1 qodec for C4."""
    return ec.build_qodec(make_c4(), strategy="bare-css/v1", strict=False)


def make_upgraded_syndrome_protocol() -> qodec.Qodec:
    """Return the C4 qodec with Reichardt's interleaved distance-two syndrome gadget."""
    protocol = make_baseline_protocol()
    syn = protocol.layers[0].gadgets["syndrome"]
    syn.circuit.source = (
        "R 4 5\nH 4\nCX 4 0\nCX 2 5\nCX 0 5\nCX 1 5\n"
        "CX 4 2\nCX 4 3\nCX 4 1\nCX 3 5\nH 4\nM 4 5\n"
    )
    syn.checks = []
    protocol.layers[0].gadgets["syndrome"] = ec.filled(syn)
    return protocol


def make_upgraded_protocol() -> qodec.Qodec:
    """Return the C4 qodec with all six gadgets upgraded to gadget distance two."""
    protocol = make_upgraded_syndrome_protocol()
    prefix = "R 0 1 2 3 4\nH 0\nCX 0 4\nCX 0 1\nCX 0 2\nCX 0 3\nCX 0 4\n"
    for name, suffix in (("prepare_z_all", "M 4\n"), ("prepare_x_all", "H 0 1 2 3\nM 4\n")):
        prep = protocol.layers[0].gadgets[name]
        prep.implements.flags = ["reject"]
        prep.circuit.source = prefix + suffix
        prep.readouts = [{"reject": ["circuit.readouts[0]"]}]
        prep.checks = []
        protocol.layers[0].gadgets[name] = ec.filled(prep)
    return protocol


def make_openqasm_c4_protocol() -> qodec.Qodec:
    """Return the upgraded C4 protocol extended with `__quantum__qis__m__body` for OpenQASM `measure`."""
    protocol = make_upgraded_protocol()
    layer0 = protocol.layers[0]
    m_instr = qodec.Instruction(
        mnemonic="__quantum__qis__m__body",
        inputs=[qodec.instructions.BlockOperand("C4")],
        outputs=[],
        action=[qodec.actions.Observe(["Z_0"])],
    )
    layer0.instruction_set.instructions["__quantum__qis__m__body"] = m_instr
    layer0.gadgets["__quantum__qis__m__body"] = ec.filled(
        qodec.Gadget(
            implements=m_instr,
            inputs=list(layer0.gadgets["measure_z_all"].inputs),
            outputs=[],
            circuit=qodec.gadgets.Circuit(
                format="stim",
                source="M 0 1 2 3\n",
                instruction_set=protocol.layers[1].instruction_set,
            ),
        )
    )
    return protocol


def make_unfilled_hswap_gadget() -> qodec.Gadget:
    """Return an unfilled `hadamard_swap_all` gadget (`H 0 1 2 3`) for the Capstone exercise."""
    protocol = make_upgraded_protocol()
    layer0 = protocol.layers[0]
    instr = qodec.Instruction(
        mnemonic="hadamard_swap_all",
        inputs=[qodec.instructions.BlockOperand("C4")],
        outputs=[qodec.instructions.BlockOperand("C4")],
        action=[qodec.actions.Clifford({"X_0": "Z_1", "Z_0": "X_1", "X_1": "Z_0", "Z_1": "X_0"})],
    )
    return qodec.Gadget(
        implements=instr,
        inputs=list(layer0.gadgets["syndrome"].inputs),
        outputs=list(layer0.gadgets["syndrome"].outputs),
        circuit=qodec.gadgets.Circuit(
            format="stim",
            source="H 0 1 2 3\n",
            instruction_set=protocol.layers[1].instruction_set,
        ),
    )


def validate_logical_swap(instr: object) -> str | None:
    """Validate a custom logical SWAP `qodec.Instruction` on one `C4` block."""
    if not isinstance(instr, qodec.Instruction):
        return f"Expected a qodec.Instruction, got {type(instr).__name__}."
    expected_op = [qodec.instructions.BlockOperand("C4")]
    if list(instr.inputs) != expected_op or list(instr.outputs) != expected_op:
        return (
            "Expected one 'C4' input block and one 'C4' output block "
            f"(got inputs={list(instr.inputs)}, outputs={list(instr.outputs)})."
        )
    if instr.flags:
        return f"Logical SWAP is unitary and should not declare flags, got {list(instr.flags)}."
    if len(instr.action) != 1 or not isinstance(instr.action[0], qodec.actions.Clifford):
        return "Expected action=[qodec.actions.Clifford({...})] with all four swapped generators."
    proto = make_baseline_protocol()
    syn = proto.layers[0].gadgets["syndrome"]
    ref_gadget = qodec.Gadget(
        implements=instr,
        inputs=list(syn.inputs),
        outputs=list(syn.outputs),
        circuit=qodec.gadgets.Circuit(
            format="stim",
            source="CX 1 2\nCX 2 1\nCX 1 2\n",
            instruction_set=proto.layers[1].instruction_set,
        ),
    )
    try:
        gp = ec.GadgetProfile(ec.filled(ref_gadget))
        if gp.action != gp.objective:
            return (
                f"Declared Clifford action ({gp.objective}) does not swap logical qubits 0 and 1 "
                f"(expected {gp.action})."
            )
    except ValueError as exc:
        return f"Invalid Clifford generator mapping: {exc}"
    return None


def validate_measure_x_gadget(gadget: object) -> str | None:
    """Validate authored `readouts` and `checks` on the `C4` `measure_x_all` gadget."""
    if not isinstance(gadget, qodec.Gadget):
        return f"Expected a qodec.Gadget, got {type(gadget).__name__}."
    if gadget.circuit.source.strip() != "H 0 1 2 3\nM 0 1 2 3":
        return "Keep measure_x_all's circuit unchanged ('H 0 1 2 3\\nM 0 1 2 3'); author its readouts and checks."
    proto = make_baseline_protocol()
    proto.layers[0].gadgets["measure_x_all"] = gadget
    report = ec.audit(proto, promote_warnings=True)
    if not report.ok or report.diagnostics:
        first = report.diagnostics[0]
        return f"{first.rule}: {first.summary}"
    return None


def compile_c4_benchmarks() -> dict[str, str]:
    """Compile the six standard C4 Q# benchmark programs (SPAM, cx_all, syndrome in Z and X)."""
    qsharp.init(target_profile=TargetProfile.Adaptive_RI)
    qsharp.eval("""
    @Measurement() operation prepare_z_all(block : Qubit) : Result { body intrinsic; }
    @Measurement() operation prepare_x_all(block : Qubit) : Result { body intrinsic; }
    @Measurement() operation measure_z_all(block : Qubit) : (Result, Result) { body intrinsic; }
    @Measurement() operation measure_x_all(block : Qubit) : (Result, Result) { body intrinsic; }
    operation cx_all(control : Qubit, target : Qubit) : Unit { body intrinsic; }
    operation syndrome(block : Qubit) : Unit { body intrinsic; }

    operation TestSpamZ() : (Result[], Result[]) {
        use b = Qubit(); let r = prepare_z_all(b); let (l, rt) = measure_z_all(b); ([r], [l, rt])
    }
    operation TestSpamX() : (Result[], Result[]) {
        use b = Qubit(); let r = prepare_x_all(b); let (l, rt) = measure_x_all(b); ([r], [l, rt])
    }
    operation TestCnotZ() : (Result[], Result[]) {
        use (c, t) = (Qubit(), Qubit());
        let rc = prepare_z_all(c); let rt = prepare_z_all(t);
        cx_all(c, t);
        let (c0, c1) = measure_z_all(c); let (t0, t1) = measure_z_all(t);
        ([rc, rt], [c0, c1, t0, t1])
    }
    operation TestCnotX() : (Result[], Result[]) {
        use (c, t) = (Qubit(), Qubit());
        let rc = prepare_x_all(c); let rt = prepare_x_all(t);
        cx_all(c, t);
        let (c0, c1) = measure_x_all(c); let (t0, t1) = measure_x_all(t);
        ([rc, rt], [c0, c1, t0, t1])
    }
    operation TestSyndromeZ() : (Result[], Result[]) {
        use b = Qubit(); let r = prepare_z_all(b); syndrome(b); let (l, rt) = measure_z_all(b); ([r], [l, rt])
    }
    operation TestSyndromeX() : (Result[], Result[]) {
        use b = Qubit(); let r = prepare_x_all(b); syndrome(b); let (l, rt) = measure_x_all(b); ([r], [l, rt])
    }
    """)
    return {
        "SPAM Z": str(qsharp.compile("TestSpamZ()")),
        "cx_all Z": str(qsharp.compile("TestCnotZ()")),
        "syndrome Z": str(qsharp.compile("TestSyndromeZ()")),
        "SPAM X": str(qsharp.compile("TestSpamX()")),
        "cx_all X": str(qsharp.compile("TestCnotX()")),
        "syndrome X": str(qsharp.compile("TestSyndromeX()")),
    }


def sample_sweep(
    codec: qodec.Qodec,
    benchmarks: dict[str, str],
    probabilities: list[float],
    *,
    shots: int,
    workers: int,
    max_readout_score: float | None = None,
) -> _Sweep:
    """Return benchmark -> (probability, attempted/accepted/wrong counts) pairs."""
    if shots < 1 or workers < 1 or not benchmarks or not probabilities:
        raise ValueError("Use positive shots and workers, and a nonempty sweep.")
    full_chunks, remainder = divmod(shots, _SHOTS_PER_CHUNK)
    sizes = [_SHOTS_PER_CHUNK] * full_chunks + ([remainder] if remainder else [])
    cases = [(name, p) for p in probabilities for name in benchmarks]
    jobs = [
        (benchmarks[name], p, size, 42 + i + j * len(cases), max_readout_score)
        for i, (name, p) in enumerate(cases)
        for j, size in enumerate(sizes)
    ]
    sweep: _Sweep = {name: [] for name in benchmarks}
    main_file = getattr(sys.modules.get("__main__"), "__file__", None)
    if _HAS_DEQ and workers > 1 and main_file != "<stdin>":
        with ProcessPoolExecutor(
            max_workers=workers,
            mp_context=multiprocessing.get_context("spawn"),
            initializer=_initialize_worker,
            initargs=(codec.dumps(),),
        ) as executor:
            results = executor.map(_sample_chunk, jobs)
            for name, p in cases:
                counts = sum(islice(results, len(sizes)), Counter[str]())
                if not counts["accepted"]:
                    raise RuntimeError(f"No {name} shots accepted at p={p:g}.")
                sweep[name].append((p, counts))
    else:
        _initialize_worker(codec.dumps())
        results = map(_sample_chunk, jobs)
        for name, p in cases:
            counts = sum(islice(results, len(sizes)), Counter[str]())
            if not counts["accepted"]:
                raise RuntimeError(f"No {name} shots accepted at p={p:g}.")
            sweep[name].append((p, counts))
    return sweep


def plot_sweep(sweep: _Sweep, *, title: str) -> None:
    """Pool X/Z counts and plot probabilities with approximate 95% Wilson intervals."""
    import matplotlib.pyplot as plt

    if not sweep or any(not points for points in sweep.values()):
        raise ValueError("Cannot plot an empty sweep.")
    pooled: _Pooled = {}
    for name, points in sweep.items():
        totals = pooled.setdefault(name.removesuffix(" X").removesuffix(" Z"), {})
        for probability, counts in points:
            totals.setdefault(probability, Counter()).update(counts)
    for points in pooled.values():
        for counts in points.values():
            counts["rejected"] = counts["attempted"] - counts["accepted"]
    fig, axes = plt.subplots(1, 2, figsize=(13, 4.5), layout="constrained")
    _plot_rates(axes[0], pooled, ("wrong", "accepted"))
    _plot_rates(axes[1], pooled, ("rejected", "attempted"))
    probabilities = list(next(iter(pooled.values())))
    axes[0].plot(
        probabilities, probabilities, "k--", linewidth=1, label="Break-even (p_L = p)"
    )
    axes[0].legend(loc="upper left", fontsize="small")
    axes[0].set_title("Conditional logical error rate\nArrows: 95% upper bounds")
    axes[1].set(title="Shot rejection rate\nArrows: 95% upper bounds", ylim=(None, 1))
    axes[1].legend(loc="lower right", fontsize="small")
    fig.suptitle(title)
    plt.show()


def _plot_rates(axis: Axes, pooled: _Pooled, ratio: tuple[str, str]) -> None:
    import numpy as np
    from scipy.stats import binomtest

    numerator, denominator = ratio
    for (name, points), marker in zip(pooled.items(), cycle("os^vDPX*")):
        tests = [binomtest(c[numerator], c[denominator]) for c in points.values()]
        rates = np.array([test.statistic for test in tests])
        bounds = np.array([test.proportion_ci(method="wilson") for test in tests]).T
        zeros = rates == 0
        axis.errorbar(
            list(points),
            np.where(zeros, bounds[1], rates),
            yerr=np.where(zeros, 0, [rates - bounds[0], bounds[1] - rates]),
            uplims=zeros,
            fmt=marker,
            capsize=3,
            label=name,
        )
    axis.set(
        xscale="log",
        yscale="log",
        xlabel="Physical fault probability per location (p)",
        ylabel=f"{numerator.capitalize()} / {denominator}",
    )
    axis.grid(True, which="both", alpha=0.25)


def _prepare_postselect_syndrome_decoder(layer: qodec.Layer):
    """Fallback post-selecting syndrome decoder when optional deq-runtime wheels are absent (e.g., Arm64)."""
    from qdk.simulation._qodec.decoding import SyndromeModel, SyndromeSession, _sign
    from qdk.simulation._qodec.readout_equations import Parity

    class _PostSelectSession(SyndromeSession):
        def decode(self, invocation, readouts):
            gen = super().decode(invocation, readouts)
            try:
                while True:
                    next(gen)
                    raise ExecutionRejected("Ambiguous syndrome rejected by readout score threshold")
            except StopIteration as stop:
                decoded = stop.value
                gadget = invocation.gadget
                name = gadget.implements.mnemonic
                codes, _, _ = self.model.gadgets[name]
                system = self.model.systems[name].copy()
                for index, value in enumerate(readouts):
                    if value is not None:
                        system.add(
                            Parity(frozenset({("circuit_readout", None, None, None, index)}), bool(value))
                        )
                for (boundary, entry), decoder in codes.items():
                    for idx in range(len(decoder.stabilizers)):
                        if system.value(_sign(boundary, entry, "stabilizers", idx)):
                            raise ExecutionRejected("Non-zero stabilizer check rejected")
                return decoded
                yield

    base = SyndromeModel(layer)
    return lambda seed=None: _PostSelectSession(base)


def make_decoder(max_readout_score: float | None = None):
    """Return a circuit-level `deq` decoder when installed, or QDK's syndrome decoder fallback on Arm64."""
    if _HAS_DEQ:
        return partial(
            prepare_deq_decoder,
            runtime_factory=partial(_runtime, forced_gap=max_readout_score is not None),
            max_readout_score=max_readout_score,
        )
    if max_readout_score is not None:
        return _prepare_postselect_syndrome_decoder
    return prepare_syndrome_decoder


def _sample_chunk(chunk: tuple[str, float, int, int, float | None]) -> Counter[str]:
    program, probability, shots, seed, max_readout_score = chunk
    if _codec is None:
        raise RuntimeError("Initialize the worker with a qodec bundle before sampling.")
    noise = NoiseConfig()
    for gate in (noise.h, noise.x, noise.y, noise.z, noise.cx):
        gate.set_depolarizing(probability)
    noise.mresetz.x = probability
    decoder = make_decoder(max_readout_score=max_readout_score)
    rows = run_qir(
        program,
        qodec=_codec,
        decoder=decoder,
        noise=noise,
        shots=shots,
        seed=seed,
        type="stabilizer",
        on_shot_failure="discard",
    )
    accepted = [
        logical for flags, logical in rows if all(bit == Result.Zero for bit in flags)
    ]
    wrong = sum(any(bit != Result.Zero for bit in logical) for logical in accepted)
    return Counter(attempted=shots, accepted=len(accepted), wrong=wrong)


def _runtime(seed: int, *, forced_gap: bool) -> Runtime:
    from deq.runtime import Runtime

    return Runtime(
        decoder="black-box-relay-bp",
        decoder_config={"parallel": 1, "seed": seed},
        coordinator="monolithic",
        coordinator_config={"forced_gap": forced_gap},
        controller="jit",
    )


def _initialize_worker(bundle: str) -> None:
    global _codec
    _codec = qodec.Qodec.loads(bundle)
