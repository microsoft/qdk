from typing import Any

# This helps Pyright understand that this module may have dynamic attributes.
def __getattr__(name: str) -> Any: ...
