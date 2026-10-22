#!/usr/bin/python3 -I
"""Machine policy for Copilot's preToolUse hook."""

import json
import sys


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError):
        payload = None
    if not isinstance(payload, dict) or not isinstance(payload.get("toolName"), str):
        decision = {
            "permissionDecision": "deny",
            "permissionDecisionReason": "Invalid tool policy payload.",
        }
    elif payload["toolName"] == "web_search":
        decision = {
            "permissionDecision": "deny",
            "permissionDecisionReason": "Remote web search is disabled in this sandbox.",
        }
    else:
        decision = {"permissionDecision": "allow"}
    json.dump(decision, sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
