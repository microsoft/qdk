# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
_http.py

This module provides HTTP utility functions for interacting with
GitHub repositories.
"""


def fetch_github(owner: str, repo: str, ref: str, path: str) -> str:
    """
    Fetches the content of a file from a GitHub repository.

    Args:
        owner (str): The owner of the GitHub repository.
        repo (str): The name of the GitHub repository.
        ref (str): The reference (branch, tag, or commit) of the repository.
        path (str): The path to the file within the repository.

    Returns:
        str: The content of the file as a string.

    Raises:
        urllib.error.HTTPError: If there is an error fetching the file from GitHub.
        urllib.error.URLError: If there is an error with the URL.
    """

    import urllib.request

    path_no_leading_slash = path[1:] if path.startswith("/") else path
    url = f"https://raw.githubusercontent.com/{owner}/{repo}/{ref}/{path_no_leading_slash}"
    return urllib.request.urlopen(url).read().decode("utf-8-sig")
