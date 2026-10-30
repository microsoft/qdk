# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import importlib.metadata

import qdk
from qdk import telemetry


def test_installed_version_is_exposed_and_used_for_telemetry():
    installed_version = importlib.metadata.version("qdk")

    assert qdk.__version__ == installed_version
    assert telemetry.QSHARP_VERSION == installed_version
