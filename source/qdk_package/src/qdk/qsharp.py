# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Re-export of `qsharp` as `qdk.qsharp`."""

from qsharp import *  # pyright: ignore[reportWildcardImportFromLibrary]
from qsharp.utils import dump_operation  # pyright: ignore[reportUnusedImport]
