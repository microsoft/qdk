"""Public package-tree contract.

The exhaustive surface lives in ``test_api_surface.py``; this covers the
structural properties that do not belong to any one module.
"""

import qdk.ec


def test_target_contracts_load_without_a_backend() -> None:
    from qdk.ec.targets import (
        Batch,
        Readouts,
        Sampler,
        Target,
        TargetModel,
        detector_error_model_of,
        gadget_distance_of,
    )

    assert Batch is not None
    assert Readouts is not None
    assert Sampler is not None
    assert Target is not None
    assert TargetModel is not None
    assert detector_error_model_of is not None
    assert gadget_distance_of is not None


def test_exact_propagation_is_not_a_target_package() -> None:
    from qdk.ec import targets
    from qdk.ec._analysis import propagation

    assert propagation is not None
    assert "simulation" not in targets.__all__
