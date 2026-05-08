# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# Deprecated: use qdk.code instead.
#
# This module proxies all attribute access to qdk.code so that dynamically
# generated Q# callables stay in sync as content is added/removed/modified,
# rather than taking a one-time snapshot via ``from qdk.code import *``.

import importlib
import sys
import types

import qdk.code as _real_code


class _CodeProxy(types.ModuleType):
    """Module proxy that forwards attribute access to ``qdk.code``."""

    def __getattr__(self, name):
        return getattr(_real_code, name)

    def __dir__(self):
        return dir(_real_code)


_proxy = _CodeProxy(__name__)
_proxy.__package__ = __package__
_proxy.__loader__ = __loader__  # type: ignore[name-defined]
_proxy.__spec__ = __spec__  # type: ignore[name-defined]
_proxy.__path__ = __path__  # type: ignore[name-defined]
_proxy.__file__ = __file__
sys.modules[__name__] = _proxy
