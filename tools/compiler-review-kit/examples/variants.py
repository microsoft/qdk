# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Example variants file for `review.py variants`: one ingredient changes per variant.

Nested closures from one factory capture callable arrays; only arrays of different
lengths are miscompiled by some revisions.
"""

PRELUDE = """
operation ApplyAll(ops : (Qubit => Unit)[], q : Qubit) : Unit { for op in ops { op(q); } }
function MakeAll(ops : (Qubit => Unit)[]) : (Qubit => Unit) { ApplyAll(ops, _) }
operation Flip(q : Qubit) : Unit { X(q); }
operation Had(q : Qubit) : Unit { H(q); }
operation Run(f : Qubit => Unit) : Result { use q = Qubit(); f(q); MResetZ(q) }
"""
PROFILES = ["Base", "Adaptive_RIF", "Adaptive"]


def body(expression):
    return f"operation Main() : Result {{ use q = Qubit(); let f = {expression}; f(q); MResetZ(q) }}"


VARIANTS = [
    {"name": "lengths-1-1", "body": body("MakeAll([MakeAll([X]), MakeAll([I])])"), "entry": "Main()", "expected": "1"},
    {"name": "lengths-1-2", "body": body("MakeAll([MakeAll([X]), MakeAll([H, H])])"), "entry": "Main()", "expected": "1"},
    {"name": "lengths-2-1", "body": body("MakeAll([MakeAll([H, H]), MakeAll([X])])"), "entry": "Main()", "expected": "1"},
    {
        "name": "callable-api-lengths-1-2",
        "build": lambda ctx: (ctx.code.Run, (ctx.code.MakeAll(
            [ctx.code.MakeAll([ctx.code.Flip]), ctx.code.MakeAll([ctx.code.Had, ctx.code.Had])]),)),
        "expected": "1",
    },
]
