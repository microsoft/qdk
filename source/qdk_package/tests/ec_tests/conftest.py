"""Collection guard and shared fixtures for the ``qdk.ec`` test suite.

``qdk.ec`` and its dependencies are an optional extra of the ``qdk`` package
(``pip install "qdk[ec]"``). When those dependencies are absent this whole
directory is skipped rather than erroring at import time, so a plain
``pytest`` run of the ``qdk`` test suite still works on a bare install.
"""

from __future__ import annotations

import os
from importlib.util import find_spec

import pytest

#: Third-party modules every ``qdk.ec`` test needs. Backend-specific extras
#: (``stim``, ``mwpf``, ``deq``) are skipped per-module by the tests that use
#: them.
_REQUIRED = ("hypothesis", "numpy", "paulimer", "qodec")

_MISSING = [name for name in _REQUIRED if find_spec(name) is None]


def pytest_ignore_collect(collection_path, config) -> bool:  # noqa: ARG001
    """Skip the whole ``qdk.ec`` suite when the ``ec`` extra is not installed."""
    del collection_path, config
    return bool(_MISSING)


if not _MISSING:
    import qodec
    from hypothesis import Verbosity, settings

    settings.register_profile("factory")
    settings.register_profile("build", print_blob=True, deadline=1000)
    settings.register_profile("fast", max_examples=10)
    settings.register_profile("thorough", print_blob=True, max_examples=1000)
    settings.register_profile("debug", max_examples=10, verbosity=Verbosity.verbose)
    settings.register_profile("no_deadline", deadline=None)
    settings.load_profile(os.getenv("HYPOTHESIS_PROFILE", "fast"))

    # ── Shared gadget fixtures (c4 translation layer), used across the suite.
    @pytest.fixture(scope="package")
    def bundle() -> qodec.Qodec:
        from ec_tests.testing.qodecs import c4

        return c4()

    @pytest.fixture(scope="package")
    def translation(bundle: qodec.Qodec) -> qodec.Layer:
        return bundle.layers[0]

    @pytest.fixture(scope="package")
    def idle_gadget(translation: qodec.Layer) -> qodec.Gadget:
        return translation.gadgets["idle"]

    @pytest.fixture(scope="package")
    def measure_xx_gadget(translation: qodec.Layer) -> qodec.Gadget:
        return translation.gadgets["measure_xx"]

    @pytest.fixture(scope="package")
    def measure_zz_gadget(translation: qodec.Layer) -> qodec.Gadget:
        return translation.gadgets["measure_zz"]

    @pytest.fixture(scope="package")
    def prepare_xx_gadget(translation: qodec.Layer) -> qodec.Gadget:
        return translation.gadgets["prepare_xx"]

    @pytest.fixture(scope="package")
    def prepare_zz_gadget(translation: qodec.Layer) -> qodec.Gadget:
        return translation.gadgets["prepare_zz"]
