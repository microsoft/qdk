"""Pass-through configuration for the deq runtime."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class DeqOptions:
    """Runtime and decoder options passed directly to deq."""

    decoder: str = "black-box-relay-bp"
    decoder_config: dict[str, Any] | None = None
    binary: str = "deq"


__all__ = ["DeqOptions"]
