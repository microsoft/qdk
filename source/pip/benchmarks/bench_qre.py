# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import timeit
from dataclasses import dataclass, KW_ONLY, field
from qsharp.qre import linear_function, generic_function, instruction
from qsharp.qre.models import AQREGateBased, SurfaceCode
from qsharp.qre._enumeration import _enumerate_instances


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
    import os
    import sys

    # Add the tests directory to sys.path to import test_qre
    # TODO: Remove this once the models in test_qre are moved to a proper module
    sys.path.append(os.path.join(os.path.dirname(__file__), "../tests"))
    from test_qre import ExampleLogicalFactory, ExampleFactory  # type: ignore

    ctx = AQREGateBased().context()

    # Hierarchical factory using from_components
    query = SurfaceCode.q() * ExampleLogicalFactory.q(
        source=SurfaceCode.q() * ExampleFactory.q()
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

    inst = instruction(42, arity=None, space=fl, time=1, error_rate=1.0)
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

    inst = instruction(42, arity=None, space=fg, time=1, error_rate=1.0)
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
