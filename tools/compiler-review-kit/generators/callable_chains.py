# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Seeded generator of composed callable chains with hand-modelled deterministic results.

Each program builds a callable through several stages (factories, closures, records
holding callables, static/mutable/measurement-dependent selection, callable arrays,
gate side effects, Adjoint/Controlled, within, library higher-order operations) and
applies it to a register. The oracle is built in:

* identity qubits qs[0..2] see only X/CNOT/CCX, so the program's selection,
  environment, argument and ordering decisions land on a computational-basis state;
* one probe qubit qs[3] sees Rz rotations in multiples of pi/2 between a single
  H...H pair, so a dropped/extra Adjoint or wrong captured angle flips or randomizes it;
* every correct program therefore ends in a basis state: outputs are deterministic and
  the generator computes them from its own model (no simulator needed, but run
  `review.py oracle-check` once to confirm the model against the simulator).

Families: inline (single-register callables), layout (``(Int, Qubit[]) => Unit``
callables with holes in different positions), python (factories called from Python
through the callable API, composed in Python, passed to an entry operation directly or
inside a struct).

Callable arrays appear plainly, nested (one factory, inner closures capturing arrays of
different lengths), held in structs, and as arrays of closures over captured loop counts.

Usage:
    python tools/compiler-review-kit/generators/callable_chains.py OUT_DIR --inline 1600 --layout 800 \
        --python 400 --seed 2 --profiles Adaptive_RIF Adaptive
then run OUT_DIR/cases.json with review.py. Store the command and seed, not the output.
"""

import argparse
import json
from pathlib import Path
import random

T = "(Qubit[] => Unit is Adj + Ctl)"
T2 = "((Int, Qubit[]) => Unit is Adj + Ctl)"
HALF_PI = "1.5707963267948966"

PRELUDE = f"""
operation Tag(id : Int, a : Int, qs : Qubit[]) : Unit is Adj + Ctl {{
    if id &&& 1 != 0 {{ X(qs[0]); }}
    if id &&& 2 != 0 {{ X(qs[1]); }}
    if id &&& 4 != 0 {{ X(qs[2]); }}
    Rz(Std.Convert.IntAsDouble(a) * {HALF_PI}, qs[3]);
}}
operation TagC(id : Int, a : Int, qs : Qubit[]) : Unit is Adj + Ctl {{
    Tag(id, a, qs);
    CNOT(qs[0], qs[1]);
    CCNOT(qs[0], qs[1], qs[2]);
}}
operation TagD(id : Int, theta : Double, qs : Qubit[]) : Unit is Adj + Ctl {{
    Tag(id, 0, qs);
    Rz(theta, qs[3]);
}}
operation ApplyAll(ops : {T}[], qs : Qubit[]) : Unit is Adj + Ctl {{
    for op in ops {{ op(qs); }}
}}
operation Seq(a : {T}, b : {T}, qs : Qubit[]) : Unit is Adj + Ctl {{
    a(qs);
    b(qs);
}}
operation ApplyOne(op : {T}, qs : Qubit[]) : Unit is Adj + Ctl {{ op(qs); }}
operation Outer(op : {T}, qs : Qubit[]) : Unit is Adj + Ctl {{ ApplyOne(op, qs); }}
operation InnerAdj(op : {T}, qs : Qubit[]) : Unit is Adj + Ctl {{ Adjoint op(qs); }}
operation ApplyAdjOnly(op : (Qubit[] => Unit is Adj), qs : Qubit[]) : Unit is Adj {{ op(qs); }}
operation ApplyCtlOnly(op : (Qubit[] => Unit is Ctl), qs : Qubit[]) : Unit is Ctl {{ op(qs); }}
operation ApplyPlain(op : (Qubit[] => Unit), qs : Qubit[]) : Unit {{ op(qs); }}
operation Tag4(p : Int, x : Int, a : Int, qs : Qubit[]) : Unit is Adj + Ctl {{ Tag(p ^^^ x, a, qs); }}
operation Tag4C(p : Int, x : Int, a : Int, qs : Qubit[]) : Unit is Adj + Ctl {{ TagC(p ^^^ x, a, qs); }}
operation TagT(cfg : (Int, (Int, Bool)), x : Int, qs : Qubit[]) : Unit is Adj + Ctl {{
    let (p, (a, c)) = cfg;
    if c {{ TagC(p ^^^ x, a, qs); }} else {{ Tag(p ^^^ x, a, qs); }}
}}
operation ApplyAll2(ops : {T2}[], x : Int, qs : Qubit[]) : Unit is Adj + Ctl {{
    for op in ops {{ op(x, qs); }}
}}
operation Seq2(a : {T2}, b : {T2}, x : Int, qs : Qubit[]) : Unit is Adj + Ctl {{
    a(x, qs);
    b(x, qs);
}}
operation ApplyOne2(op : {T2}, x : Int, qs : Qubit[]) : Unit is Adj + Ctl {{ op(x, qs); }}
"""


# ---------------------------------------------------------------- model ------------------------
def bit(state, index):
    return (state >> index) & 1


def cnot(state, control, target):
    return state ^ (bit(state, control) << target)


def ccnot(state, c1, c2, target):
    return state ^ ((bit(state, c1) & bit(state, c2)) << target)


class M:
    """A callable's effect: permutation of the 3 identity bits plus Rz phase in units of pi/2."""

    def __init__(self, perm, phase):
        self.perm, self.phase = tuple(perm), phase % 4

    @staticmethod
    def ident():
        return M(range(8), 0)

    @staticmethod
    def tag(id_, a, kind="plain"):
        def apply(state):
            state ^= id_
            if kind == "c":
                state = ccnot(cnot(state, 0, 1), 0, 1, 2)
            return state

        return M([apply(state) for state in range(8)], a)

    def then(self, other):
        return M([other.perm[self.perm[state]] for state in range(8)], self.phase + other.phase)

    def adj(self):
        inverse = [0] * 8
        for state, image in enumerate(self.perm):
            inverse[image] = state
        return M(inverse, -self.phase)

    def at(self, _x):
        return self


class MX:
    """Per-x models of a two-argument callable, x in 0..7."""

    def __init__(self, models):
        self.models = list(models)

    def adj(self):
        return MX(m.adj() for m in self.models)

    def then(self, other):
        return MX(a.then(b) for a, b in zip(self.models, other.models))

    def at(self, x):
        return self.models[x]


def power(model, k):
    """`model` applied k times in sequence."""
    result = model
    for _ in range(k - 1):
        result = result.then(model)
    return result


def effect(rng):
    if rng.random() < 0.4:
        index = rng.randrange(3)
        return f"X(qs[{index}]);", (lambda state, i=index: state ^ (1 << i))
    control, target = rng.sample(range(3), 2)
    return f"CNOT(qs[{control}], qs[{target}]);", (lambda state, c=control, t=target: cnot(state, c, t))


class Node:
    def __init__(self, stmts, expr, model, effects=(), fn_safe=True, dyn=False, stages=()):
        self.stmts, self.expr, self.model = list(stmts), expr, model
        self.effects, self.fn_safe, self.dyn = list(effects), fn_safe, dyn
        self.stages = set(stages)


# ---------------------------------------------------------------- chains -----------------------
class Chain:
    """Builds a chain of single-register callables (type T)."""

    T, LAM, P, HOLES, SFX = T, "qs", "qs", "_", ""

    def __init__(self, rng, allow_dyn, allow_effects, decls=None, counter=0):
        self.rng, self.n, self.decls = rng, counter, decls if decls is not None else []
        self.allow_dyn, self.allow_effects = allow_dyn, allow_effects

    def fresh(self):
        self.n += 1
        return self.n

    def leaf(self, decoy=False):
        r, n = self.rng, self.fresh()
        id_, a, kind = r.randrange(1, 8), r.choice([1, 3]), r.choice(["plain", "c"])
        op = "Tag" if kind == "plain" else "TagC"
        model = M.tag(id_, a, kind)
        forms = ["global", "partial", "lambda", "lambda_double"]
        if self.allow_dyn and not decoy:
            forms += ["dyn_capture", "dyn_angle"]
        form = r.choice(forms)
        if form == "global":
            self.decls.append(f"operation G{n}(qs : Qubit[]) : Unit is Adj + Ctl {{ {op}({id_}, {a}, qs); }}")
            return Node([], f"G{n}", model, stages={"leaf"})
        if form == "partial":
            return Node([], f"{op}({id_}, {a}, _)", model, stages={"closure"})
        if form == "lambda":
            return Node([f"let k{n} = {id_};"], f"qs => {op}(k{n}, {a}, qs)", model, stages={"closure"})
        if form == "lambda_double":
            return Node([f"let th{n} = {a}.0 * {HALF_PI};"], f"qs => TagD({id_}, th{n}, qs)", M.tag(id_, a),
                        stages={"closure"})
        if form == "dyn_capture":
            other = (id_ % 7) + 1
            return Node([f"let k{n} = if r0 == One {{ {id_} }} else {{ {other} }};"],
                        f"qs => {op}(k{n}, {a}, qs)", model, fn_safe=False, dyn=True,
                        stages={"closure", "dynamic"})
        return Node([f"let th{n} = if r1 == One {{ 0.0 }} else {{ {a}.0 * {HALF_PI} }};"],
                    f"qs => TagD({id_}, th{n}, qs)", M.tag(id_, a), fn_safe=False, dyn=True,
                    stages={"closure", "dynamic"})

    def decoy(self):
        while True:
            node = self.leaf(decoy=True)
            if node.fn_safe and not node.stmts:
                return node

    def wrap(self, inner):
        r, n = self.rng, self.fresh()
        T, LAM, P, H, SF = self.T, self.LAM, self.P, self.HOLES, self.SFX
        options = ["pass", "wrap_lambda", "wrap_partial", "adj_factory", "struct", "struct_factory", "tuple",
                   "newtype", "nested_struct", "select_static", "select_mutable_later", "array_static",
                   "array_mutable_index", "reassign", "alias", "choose_param", "array_closure", "seq",
                   "array_closure_nested", "array_in_struct"]
        if not isinstance(self, LayoutChain):  # `T` is shadowed here; repetition needs one-argument callables
            options += ["repeat_array"]
        if inner.fn_safe:
            options += ["factory_body", "factory_body"]
        if self.allow_effects and not inner.dyn:
            options += ["op_factory_body"]
        if self.allow_effects:
            options += ["effect_block", "effect_wrap", "select_effectful"]
        if self.allow_dyn:
            options += ["select_dynamic", "array_dynamic", "choose_dynamic"]
        w = r.choice(options)
        s, e, m = inner.stmts, inner.expr, inner.model
        base = dict(effects=inner.effects, fn_safe=inner.fn_safe, dyn=inner.dyn, stages=inner.stages)

        def node(stmts, expr, model=m, extra_effects=(), stages=(), **overrides):
            args = dict(base)
            args.update(overrides)
            args["effects"] = list(args["effects"]) + list(extra_effects)
            args["stages"] = set(args["stages"]) | set(stages)
            return Node(stmts, expr, model, **args)

        if w == "pass":
            self.decls.append(f"function Pass{n}(op : {T}) : {T} {{ op }}")
            return node(s, f"Pass{n}({e})", stages={"factory"})
        if w == "wrap_lambda":
            self.decls.append(f"function Wrap{n}(op : {T}) : {T} {{ {LAM} => op({P}) }}")
            return node(s, f"Wrap{n}({e})", stages={"factory", "closure"})
        if w == "wrap_partial":
            self.decls.append(f"function Wrap{n}(op : {T}) : {T} {{ ApplyOne{SF}(op, {H}) }}")
            return node(s, f"Wrap{n}({e})", stages={"factory", "closure"})
        if w == "adj_factory":
            self.decls.append(f"function Adj{n}(op : {T}) : {T} {{ Adjoint op }}")
            return node(s, f"Adj{n}({e})", m.adj(), stages={"factory", "functor"})
        if w == "factory_body":
            body = "\n    ".join(s + [e])
            self.decls.append(f"function Make{n}() : {T} {{\n    {body}\n}}")
            return node([], f"Make{n}()", stages={"factory"})
        if w == "op_factory_body":
            body = "\n    ".join(s + [e])
            self.decls.append(f"operation MakeOp{n}(qs : Qubit[]) : {T} {{\n    {body}\n}}")
            return node([f"let mo{n} = MakeOp{n}(qs);"], f"mo{n}", fn_safe=False,
                        stages={"factory", "effects"} if inner.effects else {"factory"})
        if w == "struct":
            self.decls.append(f"struct Box{n} {{ Head : Int, F : {T}, Tail : Int }}")
            return node(s + [f"let b{n} = new Box{n} {{ Head = 1, F = {e}, Tail = 2 }};"], f"b{n}.F",
                        stages={"record"})
        if w == "struct_factory":
            self.decls.append(f"struct Box{n} {{ Head : Int, F : {T}, Tail : Int }}")
            self.decls.append(f"function MkBox{n}(op : {T}) : Box{n} {{ new Box{n} {{ Head = 3, F = op, Tail = 4 }} }}")
            return node(s, f"MkBox{n}({e}).F", stages={"record", "factory"})
        if w == "tuple":
            return node(s + [f"let (h{n}, f{n}, t{n}) = (1, {e}, 2);"], f"f{n}", stages={"record"})
        if w == "newtype":
            self.decls.append(f"newtype Env{n} = (Head : Int, F : {T});")
            return node(s + [f"let e{n} = Env{n}(1, {e});"], f"e{n}::F", stages={"record"})
        if w == "nested_struct":
            self.decls.append(f"struct Deep{n} {{ Pair : (Int, {T}), Tag : Int }}")
            return node(s + [f"let d{n} = new Deep{n} {{ Pair = (5, {e}), Tag = 6 }};",
                             f"let (_, f{n}) = d{n}.Pair;"], f"f{n}", stages={"record"})
        if w == "select_static":
            d = self.decoy()
            cond, pre = r.choice([("true", []), (f"c{n}", [f"let c{n} = true;"]),
                                  (f"c{n} > 2", [f"let c{n} = 5;"])])
            if r.random() < 0.5:
                expr = f"if {cond} {{ {e} }} else {{ {d.expr} }}"
            else:
                expr = f"if not ({cond}) {{ {d.expr} }} else {{ {e} }}"
            return node(s + pre + [f"let f{n} = {expr};"], f"f{n}", stages={"selection"})
        if w == "select_mutable_later":
            d = self.decoy()
            return node(s + [f"mutable c{n} = true;", f"let f{n} = if c{n} {{ {e} }} else {{ {d.expr} }};",
                             f"set c{n} = false;"], f"f{n}", stages={"selection", "mutation"})
        if w == "array_static":
            d1, d2 = self.decoy(), self.decoy()
            index = r.randrange(3)
            items = [d1.expr, d2.expr]
            items.insert(index, e)
            return node(s + [f"let arr{n} : {T}[] = [{', '.join(items)}];"], f"arr{n}[{index}]",
                        stages={"selection", "array"})
        if w == "array_mutable_index":
            d1, d2 = self.decoy(), self.decoy()
            return node(s + [f"let arr{n} : {T}[] = [{d1.expr}, {e}, {d2.expr}];", f"mutable i{n} = 1;",
                             f"let f{n} = arr{n}[i{n}];", f"set i{n} = 0;"], f"f{n}",
                        stages={"selection", "array", "mutation"})
        if w == "reassign":
            d = self.decoy()
            return node(s + [f"mutable g{n} = {d.expr};", f"set g{n} = {e};"], f"g{n}", stages={"mutation"})
        if w == "alias":
            return node(s + [f"let a{n} = {e};", f"let b{n} = a{n};"], f"b{n}")
        if w == "choose_param":
            d = self.decoy()
            self.decls.append(f"function Choose{n}(flag : Bool, a : {T}, b : {T}) : {T} {{ if flag {{ a }} else {{ b }} }}")
            flag = r.choice(["true", "1 < 2"])
            return node(s, f"Choose{n}({flag}, {e}, {d.expr})", stages={"selection", "factory"})
        if w == "array_closure":
            d = self.decoy()
            if r.random() < 0.5:
                self.decls.append(f"function MkArr{n}(ops : {T}[]) : {T} {{ ApplyAll{SF}(ops, {H}) }}")
                return node(s, f"MkArr{n}([{e}, {d.expr}])", m.then(d.model), stages={"array", "closure", "factory"})
            return node(s + [f"let ops{n} : {T}[] = [{d.expr}, {e}];"], f"{LAM} => ApplyAll{SF}(ops{n}, {P})",
                        d.model.then(m), stages={"array", "closure"})
        if w == "array_closure_nested":
            # One factory, inner closures capturing callable arrays of different lengths.
            d1, d2 = self.decoy(), self.decoy()
            self.decls.append(f"function MkAll{n}(ops : {T}[]) : {T} {{ ApplyAll{SF}(ops, {H}) }}")
            if r.random() < 0.5:
                return node(s, f"MkAll{n}([MkAll{n}([{e}]), MkAll{n}([{d1.expr}, {d2.expr}])])",
                            m.then(d1.model).then(d2.model), stages={"array", "closure", "factory"})
            return node(s, f"MkAll{n}([MkAll{n}([{d1.expr}, {d2.expr}]), MkAll{n}([{e}])])",
                        d1.model.then(d2.model).then(m), stages={"array", "closure", "factory"})
        if w == "array_in_struct":
            d = self.decoy()
            self.decls.append(f"struct ArrBox{n} {{ Ops : {T}[], Tag : Int }}")
            self.decls.append(f"function MkFromBox{n}(b : ArrBox{n}) : {T} {{ ApplyAll{SF}(b.Ops, {H}) }}")
            return node(s, f"MkFromBox{n}(new ArrBox{n} {{ Ops = [{e}, {d.expr}], Tag = 2 }})", m.then(d.model),
                        stages={"array", "record", "closure", "factory"})
        if w == "repeat_array":
            # Closures over captured loop bounds, kept in a callable array.
            d, k1, k2 = self.decoy(), r.randint(1, 3), r.randint(1, 3)
            self.decls.append(f"operation Rep{n}(op : {T}, k : Int, qs : Qubit[]) : Unit is Adj + Ctl "
                              f"{{ for _ in 1..k {{ op(qs); }} }}")
            self.decls.append(f"function MkRep{n}(op : {T}, k : Int) : {T} {{ Rep{n}(op, k, _) }}")
            return node(s + [f"let reps{n} : {T}[] = [MkRep{n}({e}, {k1}), MkRep{n}({d.expr}, {k2})];"],
                        f"ApplyAll(reps{n}, _)", power(m, k1).then(power(d.model, k2)),
                        stages={"array", "closure", "factory", "loop"})
        if w == "seq":
            d = self.decoy()
            if r.random() < 0.5:
                return node(s, f"Seq{SF}({e}, {d.expr}, {H})", m.then(d.model), stages={"closure"})
            return node(s, f"Seq{SF}({d.expr}, {e}, {H})", d.model.then(m), stages={"closure"})
        if w == "effect_block":
            gate, fx = effect(r)
            return node(s + [f"let f{n} = {{ {gate} {e} }};"], f"f{n}", extra_effects=[fx], fn_safe=False,
                        stages={"effects"})
        if w == "effect_wrap":
            gate, fx = effect(r)
            self.decls.append(f"operation Eff{n}(op : {T}, qs : Qubit[]) : {T} {{ {gate} op }}")
            return node(s + [f"let f{n} = Eff{n}({e}, qs);"], f"f{n}", extra_effects=[fx], fn_safe=False,
                        stages={"effects", "factory"})
        if w == "select_effectful":
            gate, fx = effect(r)
            d = self.decoy()
            self.decls.append(f"operation Eff{n}(op : {T}, qs : Qubit[]) : {T} {{ {gate} op }}")
            dynamic = self.allow_dyn and r.random() < 0.5
            cond = "r0 == One" if dynamic else "true"
            return node(s + [f"let f{n} = if {cond} {{ Eff{n}({e}, qs) }} else {{ {d.expr} }};"], f"f{n}",
                        extra_effects=[fx], fn_safe=False, dyn=inner.dyn or dynamic,
                        stages={"effects", "selection"} | ({"dynamic"} if dynamic else set()))
        if w == "select_dynamic":
            d = self.decoy()
            if r.random() < 0.5:
                expr = f"if r0 == One {{ {e} }} else {{ {d.expr} }}"
            else:
                expr = f"if r1 == One {{ {d.expr} }} else {{ {e} }}"
            return node(s + [f"let f{n} = {expr};"], f"f{n}", fn_safe=False, dyn=True,
                        stages={"selection", "dynamic"})
        if w == "array_dynamic":
            d = self.decoy()
            return node(s + [f"let arr{n} : {T}[] = [{d.expr}, {e}];",
                             f"let f{n} = arr{n}[if r0 == One {{ 1 }} else {{ 0 }}];"], f"f{n}",
                        fn_safe=False, dyn=True, stages={"selection", "array", "dynamic"})
        d = self.decoy()  # choose_dynamic
        self.decls.append(f"function Choose{n}(flag : Bool, a : {T}, b : {T}) : {T} {{ if flag {{ a }} else {{ b }} }}")
        return node(s, f"Choose{n}(r0 == One, {e}, {d.expr})", fn_safe=False, dyn=True,
                    stages={"selection", "factory", "dynamic"})

    def chain(self, depth):
        node = self.leaf()
        for _ in range(depth):
            node = self.wrap(node)
        return node


class LayoutChain(Chain):
    """Builds chains of two-argument callables (type T2); x is supplied later."""

    T, LAM, P, HOLES, SFX = T2, "(x, qs)", "x, qs", "_, _", "2"

    def leaf(self, decoy=False):
        r, n = self.rng, self.fresh()
        p, a, controlled = r.randrange(8), r.choice([1, 3]), r.random() < 0.5
        kind = "c" if controlled else "plain"
        tag4 = "Tag4C" if controlled else "Tag4"
        model = MX(M.tag(p ^ x, a, kind) for x in range(8))
        flag = "true" if controlled else "false"
        forms = ["global", "hole_x", "hole_p", "tuple_cfg", "lambda", "lambda_tuple_capture"]
        if self.allow_dyn and not decoy:
            forms.append("dyn_cfg")
        form = r.choice(forms)
        if form == "global":
            self.decls.append(f"operation G{n}(x : Int, qs : Qubit[]) : Unit is Adj + Ctl {{ {tag4}({p}, x, {a}, qs); }}")
            return Node([], f"G{n}", model, stages={"leaf", "layout"})
        if form == "hole_x":
            return Node([], f"{tag4}({p}, _, {a}, _)", model, stages={"closure", "layout"})
        if form == "hole_p":
            return Node([], f"{tag4}(_, {p}, {a}, _)", model, stages={"closure", "layout"})
        if form == "tuple_cfg":
            return Node([], f"TagT(({p}, ({a}, {flag})), _, _)", model, stages={"closure", "layout", "record"})
        if form == "lambda":
            return Node([f"let k{n} = {p};"], f"(x, qs) => {tag4}(k{n}, x, {a}, qs)", model,
                        stages={"closure", "layout"})
        if form == "lambda_tuple_capture":
            return Node([f"let cfg{n} = ({p}, ({a}, {flag}));"], f"(x, qs) => TagT(cfg{n}, x, qs)", model,
                        stages={"closure", "layout", "record"})
        other = (p + 3) % 8
        return Node([f"let cfg{n} = (if r0 == One {{ {p} }} else {{ {other} }}, ({a}, {flag}));"],
                    f"TagT(cfg{n}, _, _)", model, fn_safe=False, dyn=True,
                    stages={"closure", "layout", "record", "dynamic"})


def fix_argument(chain, node, rng):
    """Turn a two-argument chain into a single-register chain by fixing x."""
    n, x = chain.fresh(), rng.randrange(8)
    forms = ["fn_lambda", "fn_partial", "local_partial", "local_partial"] + (["dyn_partial"] if chain.allow_dyn else [])
    form = rng.choice(forms)
    stages = set(node.stages) | {"layout", "closure"}
    base = dict(effects=node.effects, fn_safe=node.fn_safe, dyn=node.dyn)
    if form == "fn_lambda":
        chain.decls.append(f"function Fix{n}(op : {T2}, x : Int) : {T} {{ qs => op(x, qs) }}")
        return Node(node.stmts, f"Fix{n}({node.expr}, {x})", node.model.at(x), stages=stages | {"factory"}, **base)
    if form == "fn_partial":
        chain.decls.append(f"function Fix{n}(op : {T2}, x : Int) : {T} {{ op(x, _) }}")
        return Node(node.stmts, f"Fix{n}({node.expr}, {x})", node.model.at(x), stages=stages | {"factory"}, **base)
    if form == "local_partial":
        return Node(node.stmts + [f"let p{n} = {node.expr};"], f"p{n}({x}, _)", node.model.at(x), stages=stages,
                    **base)
    base.update(fn_safe=False, dyn=True)
    return Node(node.stmts + [f"let p{n} = {node.expr};", f"let x{n} = if r0 == One {{ {x} }} else {{ {x ^ 5} }};"],
                f"p{n}(x{n}, _)", node.model.at(x), stages=stages | {"dynamic"}, **base)


# ---------------------------------------------------------------- application ------------------
def _functor_call(functor, call, args, base, ctl_on):
    controlled = (lambda model: model) if ctl_on else (lambda model: M.ident())
    if functor == "none":
        return f"{call}({args});", base
    if functor == "adj":
        return f"Adjoint {call}({args});", base.adj()
    if functor == "ctl":
        return f"Controlled {call}([ctl], ({args}));", controlled(base)
    return f"Adjoint Controlled {call}([ctl], ({args}));", controlled(base.adj())


def application(rng, model, ctl_on):
    """Code applying `f` to qs, the resulting identity-bit model, and stage labels."""
    kind = rng.choice(["direct", "apply_one", "outer", "inner_adj", "lib", "within", "adj_only", "ctl_only",
                       "plain", "twice", "foreach"])
    functor = rng.choice(["none", "adj", "ctl", "adjctl"])
    marks = set() if functor == "none" else {"functor"}
    if kind == "direct":
        code, result = _functor_call(functor, "f", "qs", model, ctl_on)
        return code, result, marks
    if kind in ("apply_one", "outer"):
        code, result = _functor_call(functor, "ApplyOne" if kind == "apply_one" else "Outer", "f, qs", model, ctl_on)
        return code, result, marks | {"hof"}
    if kind == "inner_adj":
        code, result = _functor_call(functor, "InnerAdj", "f, qs", model.adj(), ctl_on)
        return code, result, marks | {"hof", "functor"}
    if kind == "lib":
        code, result = _functor_call(functor, "Std.Canon.ApplyToEachCA", "f, [qs]", model, ctl_on)
        return code, result, marks | {"library"}
    if kind == "foreach":
        return "let _ = Std.Arrays.ForEach(f, [qs]);", model, {"library"}
    if kind == "within":
        control, target = rng.sample(range(3), 2)
        inner = rng.choice(["f(qs);", "ApplyOne(f, qs);", "Adjoint f(qs);"])
        u = model.adj() if inner.startswith("Adjoint") else model
        body = M([cnot(state, control, target) for state in range(8)], 0)
        return (f"within {{ {inner} }} apply {{ CNOT(qs[{control}], qs[{target}]); }}",
                u.then(body).then(u.adj()), {"within", "functor"})
    if kind == "adj_only":
        if functor in ("adj", "adjctl"):
            return "Adjoint ApplyAdjOnly(f, qs);", model.adj(), {"hof", "functor", "subtype"}
        return "ApplyAdjOnly(f, qs);", model, {"hof", "subtype"}
    if kind == "ctl_only":
        if functor in ("ctl", "adjctl"):
            return ("Controlled ApplyCtlOnly([ctl], (f, qs));", model if ctl_on else M.ident(),
                    {"hof", "functor", "subtype"})
        return "ApplyCtlOnly(f, qs);", model, {"hof", "subtype"}
    if kind == "plain":
        return "ApplyPlain(f, qs);", model, {"hof", "subtype"}
    code, second = _functor_call(functor, "f", "qs", model, ctl_on)  # twice
    return f"f(qs);\n    {code}", model.then(second), marks | {"reuse"}


def layout_application(rng, model, ctl_on, x):
    kind = rng.choice(["direct", "apply_one", "lib_partial", "within", "array"])
    functor = rng.choice(["none", "adj", "ctl", "adjctl"])
    marks = set() if functor == "none" else {"functor"}
    m = model.at(x)
    if kind == "direct":
        code, result = _functor_call(functor, "f", f"{x}, qs", m, ctl_on)
        return code, result, marks
    if kind == "apply_one":
        code, result = _functor_call(functor, "ApplyOne2", f"f, {x}, qs", m, ctl_on)
        return code, result, marks | {"hof"}
    if kind == "lib_partial":
        code, result = _functor_call(functor, "Std.Canon.ApplyToEachCA", f"f({x}, _), [qs]", m, ctl_on)
        return code, result, marks | {"library", "closure"}
    if kind == "array":
        code, result = _functor_call(functor, "ApplyAll2", f"[f], {x}, qs", m, ctl_on)
        return code, result, marks | {"hof", "array"}
    control, target = rng.sample(range(3), 2)
    body = M([cnot(state, control, target) for state in range(8)], 0)
    return (f"within {{ f({x}, qs); }} apply {{ CNOT(qs[{control}], qs[{target}]); }}",
            m.then(body).then(m.adj()), {"within"})


def harness(rng, node, dynamic, name, apply, param=None, ty=T):
    """Wrap the chain in an operation; returns (code, expected bits, stages).

    `f` is bound with its callable type: an unannotated `let` of a lambda gets no functors,
    which makes `within`/`Adjoint` uses of it invalid programs rather than compiler tests.
    """
    init, ctl_on = rng.randrange(8), rng.random() < 0.7
    code, model, stages = apply(ctl_on)
    state = init
    for fx in node.effects:
        state = fx(state)
    final = model.perm[state]
    correction = (-model.phase) % 4
    lines = ["use qs = Qubit[4];", "use ctl = Qubit();"]
    if dynamic:
        lines += ["use fl = Qubit[2];", "X(fl[0]);", "let r0 = MResetZ(fl[0]);", "let r1 = MResetZ(fl[1]);"]
    lines += [f"X(qs[{i}]);" for i in range(3) if bit(init, i)]
    if ctl_on:
        lines.append("X(ctl);")
    lines.append("H(qs[3]);")
    lines += node.stmts
    if param is None:
        lines.append(f"let f : {ty} = {node.expr};")
    lines.append(code)
    if correction:
        lines.append(f"Rz({correction}.0 * {HALF_PI}, qs[3]);")
    lines += ["H(qs[3]);", "MResetEachZ(qs) + [MResetZ(ctl)]"]
    signature = f"operation {name}({param or ''}) : Result[]"
    body = "\n    ".join(lines)
    expected = [bit(final, 0), bit(final, 1), bit(final, 2), 0, 1 if ctl_on else 0]
    return f"{signature} {{\n    {body}\n}}", expected, stages


def typed(bits):
    return {"type": "array", "items": [{"type": "result", "value": b} for b in bits]}


def _case(case_id, family, stages, expected, profiles, **fields):
    case = {"id": case_id, **fields, "oracle": {"kind": "deterministic", "expected": typed(expected)},
            "origin": f"generators/callable_chains.py {family}", "notes": "stages: " + ",".join(sorted(stages))}
    if profiles:
        case["profiles"] = profiles
    return case


def inline_case(rng, index, profiles, dynamic_rate, effects_rate):
    dynamic = rng.random() < dynamic_rate
    chain = Chain(rng, allow_dyn=dynamic, allow_effects=rng.random() < effects_rate)
    node = chain.chain(rng.randint(2, 6))
    main, expected, stages = harness(rng, node, dynamic or node.dyn, "Main",
                                     lambda ctl_on: application(rng, node.model, ctl_on))
    source = PRELUDE + "\n".join(chain.decls) + "\n@EntryPoint()\n" + main + "\n"
    return _case(f"gq-{index:04d}", "inline", node.stages | stages | ({"dynamic"} if node.dyn else set()),
                 expected, profiles, source=source, entry="Main()")


def layout_case(rng, index, profiles, dynamic_rate, effects_rate):
    dynamic = rng.random() < dynamic_rate
    layout = LayoutChain(rng, allow_dyn=dynamic, allow_effects=rng.random() < effects_rate)
    node = layout.chain(rng.randint(1, 4))
    decls = layout.decls
    if rng.random() < 0.5:
        chain = Chain(rng, allow_dyn=dynamic, allow_effects=layout.allow_effects, decls=decls, counter=layout.n + 1000)
        node = fix_argument(chain, node, rng)
        for _ in range(rng.randint(0, 3)):
            node = chain.wrap(node)
        apply = lambda ctl_on: application(rng, node.model, ctl_on)  # noqa: E731
        ty = T
    else:
        x = rng.randrange(8)
        apply = lambda ctl_on: layout_application(rng, node.model, ctl_on, x)  # noqa: E731
        ty = T2
    main, expected, stages = harness(rng, node, dynamic or node.dyn, "Main", apply, ty=ty)
    source = PRELUDE + "\n".join(decls) + "\n@EntryPoint()\n" + main + "\n"
    return _case(f"gl-{index:04d}", "layout", node.stages | stages | ({"dynamic"} if node.dyn else set()),
                 expected, profiles, source=source, entry="Main()")


PY_HELPERS = [
    f"function PyPass(op : {T}) : {T} {{ op }}",
    f"function PyWrap(op : {T}) : {T} {{ qs => op(qs) }}",
    f"function PyAdj(op : {T}) : {T} {{ Adjoint op }}",
    f"function PyChoose(flag : Bool, a : {T}, b : {T}) : {T} {{ if flag {{ a }} else {{ b }} }}",
    f"function PyArr(ops : {T}[]) : {T} {{ ApplyAll(ops, _) }}",
    f"function PySeq(a : {T}, b : {T}) : {T} {{ Seq(a, b, _) }}",
    f"struct PyBox {{ Head : Int, F : {T} }}",
    f"function PyMkBox(op : {T}) : PyBox {{ new PyBox {{ Head = 7, F = op }} }}",
    f"function PyUnbox(b : PyBox) : {T} {{ b.F }}",
    f"operation PyApplyBox(b : PyBox, qs : Qubit[]) : Unit is Adj + Ctl {{ b.F(qs); }}",
    f"function PyCapBox(b : PyBox) : {T} {{ PyApplyBox(b, _) }}",
    f"operation PyRep(op : {T}, k : Int, qs : Qubit[]) : Unit is Adj + Ctl {{ for _ in 1..k {{ op(qs); }} }}",
    f"function PyMkRep(op : {T}, k : Int) : {T} {{ PyRep(op, k, _) }}",
]


def python_case(rng, index, profiles, adapters):
    chain = Chain(rng, allow_dyn=False, allow_effects=False)
    node = chain.chain(rng.randint(1, 3))
    while not node.fn_safe:
        node = chain.chain(rng.randint(1, 3))
    body = "\n    ".join(node.stmts + [node.expr])
    chain.decls.append(f"function Build() : {T} {{\n    {body}\n}}")
    chain.decls += PY_HELPERS
    expr, model, stages = "c.Build()", node.model, set(node.stages) | {"python"}
    for _ in range(rng.randint(1, 4)):
        step = rng.choice(["pass", "wrap", "adj", "choose", "arr", "box", "seq", "capbox", "nested_arr", "rep"])
        if step == "pass":
            expr = f"c.PyPass({expr})"
        elif step == "wrap":
            expr, stages = f"c.PyWrap({expr})", stages | {"closure", "factory"}
        elif step == "adj":
            expr, model, stages = f"c.PyAdj({expr})", model.adj(), stages | {"functor", "factory"}
        elif step == "box":
            expr, stages = f"c.PyUnbox(c.PyMkBox({expr}))", stages | {"record", "factory"}
        elif step == "capbox":
            expr, stages = f"c.PyCapBox(c.PyMkBox({expr}))", stages | {"record", "closure", "factory"}
        elif step == "rep":
            k = rng.randint(1, 3)
            expr, model, stages = f"c.PyMkRep({expr}, {k})", power(model, k), stages | {"closure", "factory", "loop"}
        elif step == "nested_arr":
            names = []
            for _ in range(2):
                decoy = chain.decoy()
                names.append((f"PyDecoy{chain.fresh()}", decoy))
                chain.decls.append(f"function {names[-1][0]}() : {T} {{ {decoy.expr} }}")
            (a, da), (b, db) = names
            expr = f"c.PyArr([c.PyArr([{expr}]), c.PyArr([c.{a}(), c.{b}()])])"
            model = model.then(da.model).then(db.model)
            stages |= {"array", "closure", "factory"}
        else:
            decoy = chain.decoy()
            name = f"PyDecoy{chain.fresh()}"
            chain.decls.append(f"function {name}() : {T} {{ {decoy.expr} }}")
            if step == "choose":
                if rng.random() < 0.5:
                    expr = f"c.PyChoose(True, {expr}, c.{name}())"
                else:
                    expr = f"c.PyChoose(False, c.{name}(), {expr})"
                stages |= {"selection", "factory"}
            elif step == "arr":
                if rng.random() < 0.5:
                    expr, model = f"c.PyArr([{expr}, c.{name}()])", model.then(decoy.model)
                else:
                    expr, model = f"c.PyArr([c.{name}(), {expr}])", decoy.model.then(model)
                stages |= {"array", "closure", "factory"}
            else:
                expr, model = f"c.PySeq({expr}, c.{name}())", model.then(decoy.model)
                stages |= {"closure", "factory"}
    # Sometimes the entry takes a struct holding the callable, built in Python.
    boxed = rng.random() < 0.25
    entry_node = Node(["let f = b.F;"], "f", model) if boxed else Node([], "f", model)
    run, expected, app_stages = harness(rng, entry_node, False, "Run",
                                        lambda ctl_on: application(rng, model, ctl_on),
                                        param="b : PyBox" if boxed else f"f : {T}")
    if boxed:
        app_stages = app_stages | {"record"}
    source = PRELUDE + "\n".join(chain.decls) + "\n" + run + "\n"
    path = adapters / f"gp_{index:04d}.py"
    argument = "c.PyMkBox(f)" if boxed else "f"
    path.write_text(f"SRC = {source!r}\n\n\ndef prepare(ctx):\n    ctx.eval(SRC)\n    c = ctx.code\n"
                    f"    f = {expr}\n    return c.Run, ({argument},)\n")
    return _case(f"gp-{index:04d}", "python", stages | app_stages, expected, profiles,
                 adapter=f"adapters/{path.name}")


def generate(out, inline, layout, python, seed, profiles=None, dynamic_rate=0.4, effects_rate=0.5):
    out = Path(out)
    adapters = out / "adapters"
    adapters.mkdir(parents=True, exist_ok=True)
    rng = random.Random(seed)
    cases = [inline_case(rng, i, profiles, dynamic_rate, effects_rate) for i in range(inline)]
    cases += [layout_case(rng, i, profiles, dynamic_rate, effects_rate) for i in range(layout)]
    cases += [python_case(rng, i, profiles, adapters) for i in range(python)]
    (out / "cases.json").write_text(json.dumps(cases, indent=1) + "\n")
    (out / "generator.json").write_text(json.dumps({
        "generator": "generators/callable_chains.py", "seed": seed, "inline": inline, "layout": layout,
        "python": python, "profiles": profiles, "dynamic_rate": dynamic_rate, "effects_rate": effects_rate,
    }, indent=1) + "\n")
    return cases


def main():
    cli = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    cli.add_argument("out", type=Path)
    cli.add_argument("--inline", type=int, default=200)
    cli.add_argument("--layout", type=int, default=100)
    cli.add_argument("--python", type=int, default=50)
    cli.add_argument("--seed", type=int, default=1)
    cli.add_argument("--profiles", nargs="+", choices=("Base", "Adaptive_RI", "Adaptive_RIF", "Adaptive"))
    cli.add_argument("--dynamic-rate", type=float, default=0.4,
                     help="fraction of programs that may select or capture measurement-dependent values")
    cli.add_argument("--effects-rate", type=float, default=0.5,
                     help="fraction of programs that may add gate side effects while building the callable")
    args = cli.parse_args()
    cases = generate(args.out, args.inline, args.layout, args.python, args.seed, args.profiles,
                     args.dynamic_rate, args.effects_rate)
    print(f"wrote {len(cases)} cases to {args.out / 'cases.json'}")


if __name__ == "__main__":
    main()
