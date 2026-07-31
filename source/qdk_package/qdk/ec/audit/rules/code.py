"""Code audit rule extension point.

No built-in code rules are registered yet; qodec performs the current structural
code validation.
"""

from ..rule import Rule

RULES: tuple[Rule, ...] = ()

__all__ = ["RULES"]
