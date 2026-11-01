# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""C4 walkthrough support: sample QIR with deq (or QDK syndrome fallback on ARM64) and plot shot counts."""

from __future__ import annotations

from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from functools import partial
import importlib.util
from itertools import cycle, islice
import multiprocessing
import sys
from typing import TYPE_CHECKING

import qodec
from qdk import Result
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
    import html as _html

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
    col_w = 48
    row_h = 36
    pad_l = 64
    pad_t = 28
    width = pad_l + max(len(ops), 1) * col_w + 32
    height = pad_t + n_qubits * row_h + 16
    svg = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'style="background:#ffffff;border:1px solid #d0d7de;border-radius:6px;font-family:monospace;font-size:12px">'
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
            svg.append(f'<text x="{x}" y="{y0 + 4}" text-anchor="middle" fill="#24292f" font-weight="bold">{_html.escape(gate)}</text>')
    svg.append("</svg>")
    return "".join(svg)


def _ensure_stim_parser() -> None:
    """Register cross-platform shims when native stim or highspy wheels are unavailable (e.g., Windows ARM64)."""
    import importlib.machinery
    import sys
    import types

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
                    steps = [
                        ln.strip()
                        for ln in self.source.splitlines()
                        if ln.strip() and not ln.strip().startswith("#")
                    ]
                    header = f"Stim Circuit Timeline ({len(steps)} instructions):"
                    body = "\n".join(f"  [{i:02d}] {op}" for i, op in enumerate(steps))
                    return _DiagramHelper(header + "\n" + body, _render_stim_timeline_svg(self.source))

            _stim_mod.Circuit = _ShimStimCircuit
            sys.modules["stim"] = _stim_mod


_ensure_stim_parser()


def ensure_course_state(ns: dict, stage: str = "baseline") -> None:
    """Self-healing helper so notebook cells run cleanly even if executed out of order or after a kernel switch."""
    import qdk.ec as ec

    _ensure_stim_parser()
    ns.setdefault("qodec", qodec)
    ns.setdefault("ec", ec)
    if "c4" not in ns:
        ns["c4"] = qodec.Code(
            "C4",
            stabilizers=["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
            x=["X_0 X_1", "X_0 X_2"],
            z=["Z_0 Z_2", "Z_0 Z_1"],
        )
    if "c4_profile" not in ns:
        ns["c4_profile"] = ec.CodeProfile(ns["c4"])
    if stage in ("baseline", "upgraded_syndrome", "upgraded_all") and "protocol" not in ns:
        ns["protocol"] = ec.build_qodec(ns["c4"], strategy="bare-css/v1", strict=False)
    if stage in ("upgraded_syndrome", "upgraded_all") and "protocol" in ns:
        g_syn = ns["protocol"].layers[0].gadgets["syndrome"]
        if ec.GadgetProfile(g_syn).distance().value < 2:
            g_syn.circuit.source = (
                "R 4 5\nH 4\nCX 4 0\nCX 2 5\nCX 0 5\nCX 1 5\n"
                "CX 4 2\nCX 4 3\nCX 4 1\nCX 3 5\nH 4\nM 4 5\n"
            )
            g_syn.checks = []
            ns["protocol"].layers[0].gadgets["syndrome"] = ec.filled(g_syn)
    if stage == "upgraded_all" and "protocol" in ns:
        pref = "R 0 1 2 3 4\nH 0\nCX 0 4\nCX 0 1\nCX 0 2\nCX 0 3\nCX 0 4\n"
        for gname, suf in [("prepare_z_all", "M 4\n"), ("prepare_x_all", "H 0 1 2 3\nM 4\n")]:
            g_prep = ns["protocol"].layers[0].gadgets[gname]
            if ec.GadgetProfile(g_prep).distance().value < 2:
                g_prep.implements.flags = ["reject"]
                g_prep.circuit.source = pref + suf
                g_prep.readouts = [{"reject": ["circuit.readouts[0]"]}]
                g_prep.checks = []
                ns["protocol"].layers[0].gadgets[gname] = ec.filled(g_prep)
        if "benchmarks" not in ns or "two_block_qir" not in ns:
            from qdk import TargetProfile, qsharp

            qsharp.init(target_profile=TargetProfile.Adaptive_RI)
            qsharp.eval("""
@Measurement()
operation prepare_z_all(block : Qubit) : Result { body intrinsic; }
@Measurement()
operation prepare_x_all(block : Qubit) : Result { body intrinsic; }
@Measurement()
operation measure_z_all(block : Qubit) : (Result, Result) { body intrinsic; }
@Measurement()
operation measure_x_all(block : Qubit) : (Result, Result) { body intrinsic; }
operation cx_all(control : Qubit, target : Qubit) : Unit { body intrinsic; }
operation syndrome(block : Qubit) : Unit { body intrinsic; }
operation TestSpamZ() : (Result[], Result[]) {
    use block = Qubit();
    let reject = prepare_z_all(block);
    let (left, right) = measure_z_all(block);
    ([reject], [left, right])
}
operation TestSpamX() : (Result[], Result[]) {
    use block = Qubit();
    let reject = prepare_x_all(block);
    let (left, right) = measure_x_all(block);
    ([reject], [left, right])
}
operation TestSyndromeZ() : (Result[], Result[]) {
    use block = Qubit();
    let reject = prepare_z_all(block);
    syndrome(block);
    let (left, right) = measure_z_all(block);
    ([reject], [left, right])
}
operation TestSyndromeX() : (Result[], Result[]) {
    use block = Qubit();
    let reject = prepare_x_all(block);
    syndrome(block);
    let (left, right) = measure_x_all(block);
    ([reject], [left, right])
}
operation TestTwoBlockCX() : (Result[], Result[]) {
    use ctl = Qubit();
    use tgt = Qubit();
    let r_ctl = prepare_x_all(ctl);
    let r_tgt = prepare_z_all(tgt);
    cx_all(ctl, tgt);
    syndrome(ctl);
    syndrome(tgt);
    cx_all(ctl, tgt);
    let (x_c0, x_c1) = measure_x_all(ctl);
    let (z_t0, z_t1) = measure_z_all(tgt);
    ([r_ctl, r_tgt], [x_c0, x_c1, z_t0, z_t1])
}
""")
            ns.setdefault(
                "benchmarks",
                {
                    "SPAM Z": str(qsharp.compile("TestSpamZ()")),
                    "SPAM X": str(qsharp.compile("TestSpamX()")),
                    "syndrome Z": str(qsharp.compile("TestSyndromeZ()")),
                    "syndrome X": str(qsharp.compile("TestSyndromeX()")),
                },
            )
            ns.setdefault("two_block_qir", str(qsharp.compile("TestTwoBlockCX()")))
    if "protocol" in ns:
        ns["gadgets"] = ns["protocol"].layers[0].gadgets


def sample_sweep(
    codec: qodec.Qodec,
    benchmarks: dict[str, str],
    probabilities: list[float],
    *,
    shots: int,
    workers: int,
    max_readout_score: float | None = None,
) -> _Sweep:
    """Return benchmark -> (probability, attempted/accepted/wrong counts) pairs.

    Fixed chunks and seeds are independent of worker count. Reject flagged shots
    and, optionally, shots with high readout scores. Failures abort the whole sweep.
    """
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
    (line,) = axes[0].plot(
        probabilities, probabilities, "k--", linewidth=1, label="Break-even"
    )
    axes[0].legend(handles=[line], loc="lower left", fontsize="small")
    axes[0].set_title("Conditional logical error rate\nArrows: 95% upper bounds")
    axes[1].set(title="Rejection rate\nArrows: 95% upper bounds", ylim=(None, 1))
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
        xlabel="Physical fault probability per location",
        ylabel=f"{numerator.capitalize()} / {denominator}",
    )
    axis.grid(True, which="both", alpha=0.25)


def _prepare_postselect_syndrome_decoder(layer: qodec.Layer):
    """Fallback score-filtered decoder when deq-runtime is unavailable on ARM64."""
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


def _sample_chunk(chunk: tuple[str, float, int, int, float | None]) -> Counter[str]:
    program, probability, shots, seed, max_readout_score = chunk
    if _codec is None:
        raise RuntimeError("Initialize the worker with a qodec bundle before sampling.")
    noise = NoiseConfig()
    for gate in (noise.h, noise.x, noise.y, noise.z, noise.cx):
        gate.set_depolarizing(probability)
    noise.mresetz.x = probability
    if _HAS_DEQ:
        decoder = partial(
            prepare_deq_decoder,
            runtime_factory=partial(_runtime, forced_gap=max_readout_score is not None),
            max_readout_score=max_readout_score,
        )
    elif max_readout_score is not None:
        decoder = _prepare_postselect_syndrome_decoder
    else:
        decoder = prepare_syndrome_decoder
    rows = run_qir(
        program,
        qodec=_codec,
        decoder=decoder,
        noise=noise,
        shots=shots,
        seed=seed,
        type="clifford",
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
    _ensure_stim_parser()
    _codec = qodec.Qodec.loads(bundle)
