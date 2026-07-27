import functools
from pathlib import Path

from qdk import Context


@functools.cache
def get_qdk_context(*, optimize: str = "") -> Context:
    # Context is cached to avoid re-compiling the library for each
    path = str(Path(__file__).resolve().parents[1])
    return Context(
        project_root=path, qdk_config={"optimize": optimize}
    )
