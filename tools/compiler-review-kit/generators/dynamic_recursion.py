# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Seeded generator of recursion under measurement-dependent control flow, with modelled results.

Families:

* ``inner`` (``dr-``): a recursive operation measures an ancilla in every level and places its
  recursive call, its gate and its continuation in the taken branch, the untaken branch, or after
  the branch. The recursion depth is always a compile-time constant; only the branch is dynamic.
  Variants use ``Qubit[]`` parameters (inlined) or scalar parameters (eligible for Adaptive IR
  functions), and mutual recursion through a second operation.
* ``outer`` (``do-``): a static recursion (CNOT/X chains, so order and ``Adjoint`` matter) called
  from the caller's measurement-dependent branch, directly, as ``Adjoint`` or ``Controlled``.
* ``classical`` (``dc-``): classical recursive functions evaluated inside a dynamic branch.

The ancilla is always measured in a known basis state, so every program is deterministic and the
generator computes its result. Run ``review.py oracle-check`` once before trusting the model.

Usage:
    python tools/compiler-review-kit/generators/dynamic_recursion.py OUT_DIR --inner 300 --outer 150 --classical 50 --seed 1
"""

import argparse
import json
from pathlib import Path
import random

DEFAULT_PROFILES = ["Adaptive_RI", "Adaptive_RIF", "Adaptive"]


def typed_results(bits):
    return {"type": "array", "items": [{"type": "result", "value": b} for b in bits]}


def _case(case_id, family, source, entry, expected, profiles, notes):
    case = {"id": case_id, "source": source, "entry": entry,
            "oracle": {"kind": "deterministic", "expected": expected},
            "origin": f"generators/dynamic_recursion.py {family}", "notes": notes}
    if profiles:
        case["profiles"] = profiles
    return case


def inner_case(rng, index, profiles):
    """Recursion whose every level measures an ancilla and branches on it."""
    depth = rng.randint(1, 3)
    flip_r = rng.random() < 0.5
    literal = rng.choice(["Zero", "One"])
    taken = (1 if flip_r else 0) == (1 if literal == "One" else 0)
    call_at = rng.choice(["taken", "else", "after"])
    gate_at = rng.choice(["taken", "else", "after", "before"])
    scalar = rng.random() < 0.3
    mutual = rng.random() < 0.25
    name = "Rec"
    if scalar:
        params, gate, recurse, target = "n : Int, q : Qubit, r : Qubit", "X(q);", "(n - 1, q, r)", "q"
    else:
        params, gate, recurse, target = "n : Int, qs : Qubit[], r : Qubit", "X(qs[n - 1]);", "(n - 1, qs, r)", "qs"
    callee = "Hop" if mutual else name
    call = f"{callee}{recurse};"
    parts = {"before": [], "taken": [], "else": [], "after": []}
    parts[gate_at].append(gate)
    parts[call_at].append(call)
    body = [f"        {stmt}" for stmt in parts["before"]]
    if flip_r:
        body.append("        X(r);")
    body.append(f"        if MResetZ(r) == {literal} {{ {' '.join(parts['taken'])} }}"
                + (f" else {{ {' '.join(parts['else'])} }}" if parts["else"] else ""))
    body += [f"        {stmt}" for stmt in parts["after"]]
    decls = [f"operation {name}({params}) : Unit {{\n    if n > 0 {{\n" + "\n".join(body) + "\n    }\n}"]
    if mutual:
        decls.append(f"operation Hop({params}) : Unit {{ {name}(n, {'q' if scalar else 'qs'}, r); }}")

    # Model: one level flips its target when the gate's segment runs, and recurses when the call's does.
    def runs(segment):
        return segment in ("before", "after") or (segment == "taken") == taken

    flips = [0] * depth
    level = depth
    while level > 0:
        if runs(gate_at):
            flips[level - 1] ^= 1
        if not runs(call_at):
            break
        level -= 1
    if scalar:
        main = (f"operation Main() : Result {{\n    use q = Qubit();\n    use r = Qubit();\n"
                f"    {name}({depth}, q, r);\n    MResetZ(q)\n}}")
        expected = {"type": "result", "value": sum(flips) % 2}
    else:
        main = (f"operation Main() : Result[] {{\n    use qs = Qubit[{depth}];\n    use r = Qubit();\n"
                f"    {name}({depth}, qs, r);\n    MResetEachZ(qs)\n}}")
        expected = typed_results(flips)
    notes = (f"depth {depth}; branch {'taken' if taken else 'not taken'}; gate {gate_at}; call {call_at}; "
             f"{'scalar' if scalar else 'array'}{'; mutual' if mutual else ''}; target {target}")
    return _case(f"dr-{index:04d}", "inner", "\n".join(decls) + "\n" + main + "\n", "Main()", expected,
                 profiles, notes)


def outer_case(rng, index, profiles):
    """Static recursion invoked from the caller's measurement-dependent branch."""
    width = rng.randint(2, 4)
    depth = rng.randint(1, width - 1)
    init = [rng.randint(0, 1) for _ in range(width)]
    functor = rng.choice(["none", "adj", "ctl", "ctl_off"])
    literal, flip_r = rng.choice(["Zero", "One"]), rng.random() < 0.5
    taken = (1 if flip_r else 0) == (1 if literal == "One" else 0)
    decl = ("operation Chain(n : Int, qs : Qubit[]) : Unit is Adj + Ctl {\n"
            "    if n > 0 {\n        CNOT(qs[n - 1], qs[n]);\n        Chain(n - 1, qs);\n        X(qs[0]);\n    }\n}")

    def chain(bits, n):
        ops = []
        for level in range(n, 0, -1):
            ops.append(("cnot", level - 1, level))
        ops += [("x", 0, None)] * n  # every level's X(qs[0]) runs after its recursive call returns
        return ops

    def apply(bits, ops):
        bits = list(bits)
        for kind, a, b in ops:
            if kind == "cnot":
                bits[b] ^= bits[a]
            else:
                bits[a] ^= 1
        return bits

    ops = chain(init, depth)
    if functor == "adj":
        ops = list(reversed(ops))
    final = init
    if taken and functor != "ctl_off":
        final = apply(init, ops)
    call = {"none": f"Chain({depth}, qs);", "adj": f"Adjoint Chain({depth}, qs);",
            "ctl": f"Controlled Chain([c], ({depth}, qs));", "ctl_off": f"Controlled Chain([c], ({depth}, qs));"}[functor]
    lines = [f"use qs = Qubit[{width}];", "use r = Qubit();", "use c = Qubit();"]
    lines += [f"X(qs[{i}]);" for i, bit in enumerate(init) if bit]
    if functor == "ctl":
        lines.append("X(c);")
    if flip_r:
        lines.append("X(r);")
    lines.append(f"if MResetZ(r) == {literal} {{ {call} }}")
    lines.append("Reset(c);")
    lines.append("MResetEachZ(qs)")
    main = "operation Main() : Result[] {\n    " + "\n    ".join(lines) + "\n}"
    notes = f"width {width}; depth {depth}; functor {functor}; branch {'taken' if taken else 'not taken'}"
    return _case(f"do-{index:04d}", "outer", decl + "\n" + main + "\n", "Main()", typed_results(final), profiles, notes)


def classical_case(rng, index, profiles):
    """Classical recursion evaluated inside a measurement-dependent branch."""
    kind = rng.choice(["fact", "sum", "fib"])
    n = rng.randint(1, 6)
    decls = {
        "fact": "function F(n : Int) : Int { if n <= 1 { 1 } else { n * F(n - 1) } }",
        "sum": "function F(n : Int) : Int { if n == 0 { 0 } else { n + F(n - 1) } }",
        "fib": "function F(n : Int) : Int { if n < 2 { n } else { F(n - 1) + F(n - 2) } }",
    }

    def value(k):
        if kind == "fact":
            return 1 if k <= 1 else k * value(k - 1)
        if kind == "sum":
            return 0 if k == 0 else k + value(k - 1)
        return k if k < 2 else value(k - 1) + value(k - 2)

    literal, flip_r = rng.choice(["Zero", "One"]), rng.random() < 0.5
    taken = (1 if flip_r else 0) == (1 if literal == "One" else 0)
    lines = ["use r = Qubit();", "mutable x = -1;"]
    if flip_r:
        lines.append("X(r);")
    lines.append(f"if MResetZ(r) == {literal} {{ set x = F({n}); }}")
    lines.append("x")
    main = "operation Main() : Int {\n    " + "\n    ".join(lines) + "\n}"
    expected = {"type": "int", "value": value(n) if taken else -1}
    restricted = [p for p in (profiles or DEFAULT_PROFILES) if p != "Base"]
    return _case(f"dc-{index:04d}", "classical", decls[kind] + "\n" + main + "\n", "Main()", expected,
                 restricted, f"{kind}({n}); branch {'taken' if taken else 'not taken'}")


def generate(out, inner, outer, classical, seed, profiles=None):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(seed)
    profiles = profiles or DEFAULT_PROFILES
    cases = [inner_case(rng, i, profiles) for i in range(inner)]
    cases += [outer_case(rng, i, profiles) for i in range(outer)]
    cases += [classical_case(rng, i, profiles) for i in range(classical)]
    (out / "cases.json").write_text(json.dumps(cases, indent=1) + "\n")
    (out / "generator.json").write_text(json.dumps({
        "generator": "generators/dynamic_recursion.py", "seed": seed, "inner": inner, "outer": outer,
        "classical": classical, "profiles": profiles,
    }, indent=1) + "\n")
    return cases


def main():
    cli = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    cli.add_argument("out", type=Path)
    cli.add_argument("--inner", type=int, default=300)
    cli.add_argument("--outer", type=int, default=150)
    cli.add_argument("--classical", type=int, default=50)
    cli.add_argument("--seed", type=int, default=1)
    cli.add_argument("--profiles", nargs="+", choices=("Base", "Adaptive_RI", "Adaptive_RIF", "Adaptive"))
    args = cli.parse_args()
    cases = generate(args.out, args.inner, args.outer, args.classical, args.seed, args.profiles)
    print(f"wrote {len(cases)} cases to {args.out / 'cases.json'}")


if __name__ == "__main__":
    main()
