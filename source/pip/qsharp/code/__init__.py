# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Deprecated. Use :mod:`qdk.code` instead."""

import sys
import qdk.code as _real_code

# This module replaces itself in :mod:`sys.modules` with the real :mod:`qdk.code`
# module so that dynamically generated Q# callables stay in sync between the two
# import paths.
sys.modules[__name__] = _real_code
