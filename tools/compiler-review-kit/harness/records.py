# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Typed values and strict parsing of qir-runner's shot protocol."""

from collections import Counter
import json
import math
import re

from harness.common import ReviewError


def double(value):
    return {"type": "double", "value": "nan" if math.isnan(value) else value.hex()}


def encode(value):
    if value is None or (isinstance(value, tuple) and not value):
        return {"type": "unit"}
    if type(value) is bool:
        return {"type": "bool", "value": value}
    if type(value) is int:
        return {"type": "int", "value": value}
    if type(value) is float:
        return double(value)
    if isinstance(value, (list, tuple)):
        return {
            "type": "array" if isinstance(value, list) else "tuple",
            "items": [encode(v) for v in value],
        }
    if type(value).__name__ == "Result" and str(value) in ("Zero", "One"):
        return {"type": "result", "value": int(str(value) == "One")}
    if type(value) is str:
        # String-returning programs (and values the interop layer renders as text, such
        # as ranges) cannot be emitted as QIR output, but remain simulator evidence.
        return {"type": "string", "value": value}
    raise ReviewError(f"Unsupported simulator value: {type(value).__name__}: {value!r}")


def validate_value(value):
    if not isinstance(value, dict) or "type" not in value:
        raise ReviewError("Expected a typed value object with a 'type' field")
    kind = value["type"]
    if kind == "unit" and set(value) == {"type"}:
        return
    if kind in ("array", "tuple") and set(value) == {"type", "items"}:
        if not isinstance(value["items"], list):
            raise ReviewError("Typed aggregate items must be an array")
        if kind == "tuple" and not value["items"]:
            raise ReviewError("Use type=unit for an empty tuple")
        for item in value["items"]:
            validate_value(item)
        return
    if set(value) != {"type", "value"}:
        raise ReviewError(f"Invalid typed value fields: {value}")
    scalar = value["value"]
    valid = (
        (kind == "int" and type(scalar) is int)
        or (kind == "bool" and type(scalar) is bool)
        or (kind == "result" and type(scalar) is int and scalar in (0, 1))
        or (kind == "string" and type(scalar) is str)
    )
    if kind == "double" and isinstance(scalar, str):
        try:
            number = float.fromhex(scalar)
        except (ValueError, OverflowError) as error:
            raise ReviewError(f"Invalid double: {scalar}") from error
        valid = double(number)["value"] == scalar
    if not valid:
        raise ReviewError(f"Invalid typed value: {value}")


def counts(values):
    return dict(Counter(json.dumps(v, sort_keys=True) for v in values))


def parse_output(text, shots):
    frames, records, active = [], [], False
    for line in text.splitlines():
        fields = line.split("\t")
        if fields == ["START"]:
            if active:
                raise ReviewError("Nested START")
            active, records = True, []
        elif fields[0] == "END":
            if not active or fields != ["END", "0"]:
                raise ReviewError(f"Invalid END record: {line}")
            iterator = iter(records)

            def next_value():
                try:
                    kind, token = next(iterator)
                except StopIteration as error:
                    raise ReviewError("Incomplete output aggregate") from error
                if kind in ("TUPLE", "ARRAY"):
                    if not re.fullmatch(r"\d+", token):
                        raise ReviewError(f"Invalid aggregate length: {token}")
                    size = int(token)
                    if size > len(records):
                        raise ReviewError("Aggregate length exceeds record count")
                    items = [next_value() for _ in range(size)]
                    if kind == "TUPLE" and not items:
                        return {"type": "unit"}
                    return {"type": kind.lower(), "items": items}
                if kind == "INT" and re.fullmatch(r"-?\d+", token):
                    return {"type": "int", "value": int(token)}
                if kind == "BOOL" and token in ("true", "false"):
                    return {"type": "bool", "value": token == "true"}
                if kind == "RESULT" and token in ("0", "1"):
                    return {"type": "result", "value": int(token)}
                if kind == "DOUBLE":
                    try:
                        return double(float(token))
                    except ValueError as error:
                        raise ReviewError(f"Invalid DOUBLE: {token}") from error
                raise ReviewError(f"Unsupported or malformed output: {kind} {token}")

            value = next_value()
            if next(iterator, None) is not None:
                raise ReviewError("Multiple output roots in one shot")
            frames.append(value)
            active = False
        elif fields[0] == "METADATA" and active and len(fields) >= 2:
            continue
        elif fields[0] == "OUTPUT" and active and len(fields) in (3, 4):
            records.append((fields[1], fields[2]))
        else:
            raise ReviewError(f"Unexpected runner record: {line!r}")
    if active or len(frames) != shots:
        raise ReviewError(f"Incomplete shots: expected {shots}, got {len(frames)}")
    return frames
