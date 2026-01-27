# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import timeit
from dataclasses import dataclass, KW_ONLY, field
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
    import test_qre  # type: ignore

    from qsharp.qre._isa_enumeration import (
        Context,
        ISAQuery,
        ProductNode,
    )

    ctx = Context(architecture=test_qre.ExampleArchitecture())

    # Hierarchical factory using from_components
    query = ProductNode(
        sources=[
            ISAQuery(test_qre.SurfaceCode),
            ISAQuery(
                test_qre.ExampleLogicalFactory,
                source=ProductNode(
                    sources=[
                        ISAQuery(test_qre.SurfaceCode),
                        ISAQuery(test_qre.ExampleFactory),
                    ]
                ),
            ),
        ]
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


if __name__ == "__main__":
    bench_enumerate_instances()
    bench_enumerate_isas()
