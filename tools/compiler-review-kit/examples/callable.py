# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Self-contained trusted adapter exercising the Python callable API."""


def prepare(ctx):
    ctx.eval(
        "operation Flip(q : Qubit) : Unit { X(q); } "
        "operation Apply(f : Qubit => Unit) : Result { use q = Qubit(); f(q); MResetZ(q) }"
    )
    return ctx.code.Apply, (ctx.code.Flip,)
