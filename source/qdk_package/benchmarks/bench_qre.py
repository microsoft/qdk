# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import timeit
from dataclasses import dataclass, KW_ONLY, field
from qdk.qre import linear_function, generic_function
from qdk.qre._architecture import _make_instruction
from qdk.qre.models import (
    GateBased,
    SurfaceCode,
    TwoDimensionalYokedSurfaceCode,
    Litinski19Factory,
)
from qdk.qre._enumeration import _enumerate_instances


def bench_enumerate_instances():
    # Measure performance of enumerating instances with a large domain
    @dataclass
    class LargeDomain:
        _: KW_ONLY
        param1: int = field(default=0, metadata={"domain": range(1000)})
        param2: bool

    number = 100

    duration = timeit.timeit(
        "list(_enumerate_instances(LargeDomain))",
        globals={
            "_enumerate_instances": _enumerate_instances,
            "LargeDomain": LargeDomain,
        },
        number=number,
    )

    print(f"Enumerating instances took {duration / number:.6f} seconds on average.")


def bench_enumerate_isas():
    ctx = GateBased(gate_time=50, measurement_time=100).context()

    # Hierarchical factory using from_components
    query = (
        SurfaceCode.q()
        * TwoDimensionalYokedSurfaceCode.q(source=SurfaceCode.q())
        * Litinski19Factory.q()
    )

    number = 100
    duration = timeit.timeit(
        "list(query.enumerate(ctx))",
        globals={
            "query": query,
            "ctx": ctx,
        },
        number=number,
    )

    print(f"Enumerating ISAs took {duration / number:.6f} seconds on average.")


def bench_function_evaluation_linear():
    fl = linear_function(12)

    inst = _make_instruction(42, 0, None, 1, fl, None, 1.0, {})
    number = 1000
    duration = timeit.timeit(
        "inst.space(5)",
        globals={
            "inst": inst,
        },
        number=number,
    )

    print(
        f"Evaluating linear function took {duration / number:.6f} seconds on average."
    )


def bench_function_evaluation_generic():
    def func(arity: int) -> int:
        return 12 * arity

    fg = generic_function(func)

    inst = _make_instruction(42, 0, None, 1, fg, None, 1.0, {})
    number = 1000
    duration = timeit.timeit(
        "inst.space(5)",
        globals={
            "inst": inst,
        },
        number=number,
    )

    print(
        f"Evaluating linear function took {duration / number:.6f} seconds on average."
    )


if __name__ == "__main__":
    bench_enumerate_instances()
    bench_enumerate_isas()
    bench_function_evaluation_linear()
    bench_function_evaluation_generic()
