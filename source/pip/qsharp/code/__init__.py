# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# Deprecated: use qdk.code instead.
#
# This module replaces itself in sys.modules with the real qdk.code module
# so that dynamically generated Q# callables stay in sync.

import sys
import qdk.code as _real_code

sys.modules[__name__] = _real_code
