# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
_fs.py

This module provides file system utility functions for working with the file
system as Python sees it. These are used as callbacks passed into native code
to allow the native code to interact with the file system in an
environment-specific way.
"""

import os
from typing import Dict, List, Tuple


def read_file(path: str) -> Tuple[str, str]:
    """
    Read the contents of a file.

    :param path: The path to the file.
    :return: A tuple containing the path and the file contents.
    :rtype: Tuple[str, str]
    """
    with open(path, mode="r", encoding="utf-8-sig") as f:
        return (path, f.read())


def list_directory(dir_path: str) -> List[Dict[str, str]]:
    """
    Lists the contents of a directory and returns a list of dictionaries,
    where each dictionary represents an entry in the directory.

    :param dir_path: The path of the directory to list.
    :return: A list of dictionaries representing the entries in the directory.
        Each dictionary contains the following keys:
        - ``"path"``: The full path of the entry.
        - ``"entry_name"``: The name of the entry.
        - ``"type"``: The type of the entry: ``"file"``, ``"folder"``, or ``"unknown"``.
    :rtype: List[Dict[str, str]]
    """

    def map_dir(e: str) -> Dict[str, str]:
        path = os.path.join(dir_path, e)
        return {
            "path": path,
            "entry_name": e,
            "type": (
                "file"
                if os.path.isfile(path)
                else "folder" if os.path.isdir(path) else "unknown"
            ),
        }

    return list(map(map_dir, os.listdir(dir_path)))


def resolve(base: str, path: str) -> str:
    """
    Resolves a relative path with respect to a base path.

    :param base: The base path.
    :param path: The relative path.
    :return: The resolved path.
    :rtype: str
    """
    return os.path.normpath(join(base, path))


def exists(path) -> bool:
    """
    Check if a file or directory exists at the given path.

    :param path: The path to the file or directory.
    :return: ``True`` if the file or directory exists, ``False`` otherwise.
    :rtype: bool
    """
    return os.path.exists(path)


def join(path: str, *paths) -> str:
    """
    Joins one or more path components intelligently.

    :param path: The base path.
    :param *paths: Additional path components to be joined.
    :return: The concatenated path.
    :rtype: str
    """
    return os.path.join(path, *paths)
