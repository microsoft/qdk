#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import qsharp

table = {
    frozenset(): "IIIIIIIIIIII",
    frozenset({3, 8, 9}): "XIIIIIIIIIII",
    frozenset({0, 3, 6, 7, 8, 9}): "YIIIIIIIIIII",
    frozenset({0, 6, 7}): "ZIIIIIIIIIII",
    frozenset({3}): "IXIIIIIIIIII",
    frozenset({0, 3, 6}): "IYIIIIIIIIII",
    frozenset({0, 6}): "IZIIIIIIIIII",
    frozenset({3, 8}): "IIXIIIIIIIII",
    frozenset({0, 3, 8}): "IIYIIIIIIIII",
    frozenset({0}): "IIZIIIIIIIII",
    frozenset({3, 9}): "IIIXIIIIIIII",
    frozenset({0, 3, 7, 9}): "IIIYIIIIIIII",
    frozenset({0, 7}): "IIIZIIIIIIII",
    frozenset({4, 9}): "IIIIXIIIIIII",
    frozenset({1, 4, 7, 9}): "IIIIYIIIIIII",
    frozenset({1, 7}): "IIIIZIIIIIII",
    frozenset({4}): "IIIIIXIIIIII",
    frozenset({1, 4, 6, 7}): "IIIIIYIIIIII",
    frozenset({1, 6, 7}): "IIIIIZIIIIII",
    frozenset({4, 8, 9}): "IIIIIIXIIIII",
    frozenset({1, 4, 8, 9}): "IIIIIIYIIIII",
    frozenset({1}): "IIIIIIZIIIII",
    frozenset({4, 8}): "IIIIIIIXIIII",
    frozenset({1, 4, 6, 8}): "IIIIIIIYIIII",
    frozenset({1, 6}): "IIIIIIIZIIII",
    frozenset({5, 8}): "IIIIIIIIXIII",
    frozenset({2, 5, 6, 8}): "IIIIIIIIYIII",
    frozenset({2, 6}): "IIIIIIIIZIII",
    frozenset({5}): "IIIIIIIIIXII",
    frozenset({2, 5, 7}): "IIIIIIIIIYII",
    frozenset({2, 7}): "IIIIIIIIIZII",
    frozenset({5, 9}): "IIIIIIIIIIXI",
    frozenset({2, 5, 9}): "IIIIIIIIIIYI",
    frozenset({2}): "IIIIIIIIIIZI",
    frozenset({5, 8, 9}): "IIIIIIIIIIIX",
    frozenset({2, 5, 6, 7, 8, 9}): "IIIIIIIIIIIY",
    frozenset({2, 6, 7}): "IIIIIIIIIIIZ",
    frozenset({0, 3, 6, 8, 9}): "XZIIIIIIIIII",
    frozenset({0, 3, 6, 7}): "ZXIIIIIIIIII",
    frozenset({0, 3, 8, 9}): "XIZIIIIIIIII",
    frozenset({0, 3, 6, 7, 8}): "ZIXIIIIIIIII",
    frozenset({0, 3, 7, 8, 9}): "XIIZIIIIIIII",
    frozenset({0, 3, 6, 7, 9}): "ZIIXIIIIIIII",
    frozenset({1, 3, 7, 8, 9}): "XIIIZIIIIIII",
    frozenset({0, 4, 6, 7, 9}): "ZIIIXIIIIIII",
    frozenset({1, 3, 6, 7, 8, 9}): "XIIIIZIIIIII",
    frozenset({0, 4, 6, 7}): "ZIIIIXIIIIII",
    frozenset({1, 3, 8, 9}): "XIIIIIZIIIII",
    frozenset({0, 4, 6, 7, 8, 9}): "ZIIIIIXIIIII",
    frozenset({1, 3, 6, 8, 9}): "XIIIIIIZIIII",
    frozenset({0, 4, 6, 7, 8}): "ZIIIIIIXIIII",
    frozenset({2, 3, 6, 8, 9}): "XIIIIIIIZIII",
    frozenset({0, 5, 6, 7, 8}): "ZIIIIIIIXIII",
    frozenset({2, 3, 7, 8, 9}): "XIIIIIIIIZII",
    frozenset({0, 5, 6, 7}): "ZIIIIIIIIXII",
    frozenset({2, 3, 8, 9}): "XIIIIIIIIIZI",
    frozenset({0, 5, 6, 7, 9}): "ZIIIIIIIIIXI",
    frozenset({2, 3, 6, 7, 8, 9}): "XIIIIIIIIIIZ",
    frozenset({0, 5, 6, 7, 8, 9}): "ZIIIIIIIIIIX",
    frozenset({0, 3}): "IXZIIIIIIIII",
    frozenset({0, 3, 6, 8}): "IZXIIIIIIIII",
    frozenset({0, 3, 7}): "IXIZIIIIIIII",
    frozenset({0, 3, 6, 9}): "IZIXIIIIIIII",
    frozenset({1, 3, 7}): "IXIIZIIIIIII",
    frozenset({0, 4, 6, 9}): "IZIIXIIIIIII",
    frozenset({1, 3, 6, 7}): "IXIIIZIIIIII",
    frozenset({0, 4, 6}): "IZIIIXIIIIII",
    frozenset({1, 3}): "IXIIIIZIIIII",
    frozenset({0, 4, 6, 8, 9}): "IZIIIIXIIIII",
    frozenset({1, 3, 6}): "IXIIIIIZIIII",
    frozenset({0, 4, 6, 8}): "IZIIIIIXIIII",
    frozenset({2, 3, 6}): "IXIIIIIIZIII",
    frozenset({0, 5, 6, 8}): "IZIIIIIIXIII",
    frozenset({2, 3, 7}): "IXIIIIIIIZII",
    frozenset({0, 5, 6}): "IZIIIIIIIXII",
    frozenset({2, 3}): "IXIIIIIIIIZI",
    frozenset({0, 5, 6, 9}): "IZIIIIIIIIXI",
    frozenset({2, 3, 6, 7}): "IXIIIIIIIIIZ",
    frozenset({0, 5, 6, 8, 9}): "IZIIIIIIIIIX",
    frozenset({0, 3, 7, 8}): "IIXZIIIIIIII",
    frozenset({0, 3, 9}): "IIZXIIIIIIII",
    frozenset({1, 3, 7, 8}): "IIXIZIIIIIII",
    frozenset({0, 4, 9}): "IIZIXIIIIIII",
    frozenset({1, 3, 6, 7, 8}): "IIXIIZIIIIII",
    frozenset({0, 4}): "IIZIIXIIIIII",
    frozenset({1, 3, 8}): "IIXIIIZIIIII",
    frozenset({0, 4, 8, 9}): "IIZIIIXIIIII",
    frozenset({1, 3, 6, 8}): "IIXIIIIZIIII",
    frozenset({0, 4, 8}): "IIZIIIIXIIII",
    frozenset({2, 3, 6, 8}): "IIXIIIIIZIII",
    frozenset({0, 5, 8}): "IIZIIIIIXIII",
    frozenset({2, 3, 7, 8}): "IIXIIIIIIZII",
    frozenset({0, 5}): "IIZIIIIIIXII",
    frozenset({2, 3, 8}): "IIXIIIIIIIZI",
    frozenset({0, 5, 9}): "IIZIIIIIIIXI",
    frozenset({2, 3, 6, 7, 8}): "IIXIIIIIIIIZ",
    frozenset({0, 5, 8, 9}): "IIZIIIIIIIIX",
    frozenset({1, 3, 7, 9}): "IIIXZIIIIIII",
    frozenset({0, 4, 7, 9}): "IIIZXIIIIIII",
    frozenset({1, 3, 6, 7, 9}): "IIIXIZIIIIII",
    frozenset({0, 4, 7}): "IIIZIXIIIIII",
    frozenset({1, 3, 9}): "IIIXIIZIIIII",
    frozenset({0, 4, 7, 8, 9}): "IIIZIIXIIIII",
    frozenset({1, 3, 6, 9}): "IIIXIIIZIIII",
    frozenset({0, 4, 7, 8}): "IIIZIIIXIIII",
    frozenset({2, 3, 6, 9}): "IIIXIIIIZIII",
    frozenset({0, 5, 7, 8}): "IIIZIIIIXIII",
    frozenset({2, 3, 7, 9}): "IIIXIIIIIZII",
    frozenset({0, 5, 7}): "IIIZIIIIIXII",
    frozenset({2, 3, 9}): "IIIXIIIIIIZI",
    frozenset({0, 5, 7, 9}): "IIIZIIIIIIXI",
    frozenset({2, 3, 6, 7, 9}): "IIIXIIIIIIIZ",
    frozenset({0, 5, 7, 8, 9}): "IIIZIIIIIIIX",
    frozenset({1, 4, 6, 7, 9}): "IIIIXZIIIIII",
    frozenset({1, 4, 7}): "IIIIZXIIIIII",
    frozenset({1, 4, 9}): "IIIIXIZIIIII",
    frozenset({1, 4, 7, 8, 9}): "IIIIZIXIIIII",
    frozenset({1, 4, 6, 9}): "IIIIXIIZIIII",
    frozenset({1, 4, 7, 8}): "IIIIZIIXIIII",
    frozenset({2, 4, 6, 9}): "IIIIXIIIZIII",
    frozenset({1, 5, 7, 8}): "IIIIZIIIXIII",
    frozenset({2, 4, 7, 9}): "IIIIXIIIIZII",
    frozenset({1, 5, 7}): "IIIIZIIIIXII",
    frozenset({2, 4, 9}): "IIIIXIIIIIZI",
    frozenset({1, 5, 7, 9}): "IIIIZIIIIIXI",
    frozenset({2, 4, 6, 7, 9}): "IIIIXIIIIIIZ",
    frozenset({1, 5, 7, 8, 9}): "IIIIZIIIIIIX",
    frozenset({1, 4}): "IIIIIXZIIIII",
    frozenset({1, 4, 6, 7, 8, 9}): "IIIIIZXIIIII",
    frozenset({1, 4, 6}): "IIIIIXIZIIII",
    frozenset({1, 4, 6, 7, 8}): "IIIIIZIXIIII",
    frozenset({2, 4, 6}): "IIIIIXIIZIII",
    frozenset({1, 5, 6, 7, 8}): "IIIIIZIIXIII",
    frozenset({2, 4, 7}): "IIIIIXIIIZII",
    frozenset({1, 5, 6, 7}): "IIIIIZIIIXII",
    frozenset({2, 4}): "IIIIIXIIIIZI",
    frozenset({1, 5, 6, 7, 9}): "IIIIIZIIIIXI",
    frozenset({2, 4, 6, 7}): "IIIIIXIIIIIZ",
    frozenset({1, 5, 6, 7, 8, 9}): "IIIIIZIIIIIX",
    frozenset({1, 4, 6, 8, 9}): "IIIIIIXZIIII",
    frozenset({1, 4, 8}): "IIIIIIZXIIII",
    frozenset({2, 4, 6, 8, 9}): "IIIIIIXIZIII",
    frozenset({1, 5, 8}): "IIIIIIZIXIII",
    frozenset({2, 4, 7, 8, 9}): "IIIIIIXIIZII",
    frozenset({1, 5}): "IIIIIIZIIXII",
    frozenset({2, 4, 8, 9}): "IIIIIIXIIIZI",
    frozenset({1, 5, 9}): "IIIIIIZIIIXI",
    frozenset({2, 4, 6, 7, 8, 9}): "IIIIIIXIIIIZ",
    frozenset({1, 5, 8, 9}): "IIIIIIZIIIIX",
    frozenset({2, 4, 6, 8}): "IIIIIIIXZIII",
    frozenset({1, 5, 6, 8}): "IIIIIIIZXIII",
    frozenset({2, 4, 7, 8}): "IIIIIIIXIZII",
    frozenset({1, 5, 6}): "IIIIIIIZIXII",
    frozenset({2, 4, 8}): "IIIIIIIXIIZI",
    frozenset({1, 5, 6, 9}): "IIIIIIIZIIXI",
    frozenset({2, 4, 6, 7, 8}): "IIIIIIIXIIIZ",
    frozenset({1, 5, 6, 8, 9}): "IIIIIIIZIIIX",
    frozenset({2, 5, 7, 8}): "IIIIIIIIXZII",
    frozenset({2, 5, 6}): "IIIIIIIIZXII",
    frozenset({2, 5, 8}): "IIIIIIIIXIZI",
    frozenset({2, 5, 6, 9}): "IIIIIIIIZIXI",
    frozenset({2, 5, 6, 7, 8}): "IIIIIIIIXIIZ",
    frozenset({2, 5, 6, 8, 9}): "IIIIIIIIZIIX",
    frozenset({2, 5}): "IIIIIIIIIXZI",
    frozenset({2, 5, 7, 9}): "IIIIIIIIIZXI",
    frozenset({2, 5, 6, 7}): "IIIIIIIIIXIZ",
    frozenset({2, 5, 7, 8, 9}): "IIIIIIIIIZIX",
    frozenset({2, 5, 6, 7, 9}): "IIIIIIIIIIXZ",
    frozenset({2, 5, 8, 9}): "IIIIIIIIIIZX",
}

generators = [
    "XXXXIIIIIIII",
    "IIIIXXXXIIII",
    "IIIIIIIIXXXX",
    "ZZZZIIIIIIII",
    "IIIIZZZZIIII",
    "IIIIIIIIZZZZ",
    "XXIIIXIXXIIX",
    "XIIXXXIIIXIX",
    "ZIZIIIZZZIIZ",
    "ZIIZZIZIIIZZ",
]

logical_basis = ["XIIXIIIIIIXX", "ZIIZIIIIIZIZ", "IXIXIIIIIXXI", "ZZIIIIIIIZZI"]

expanded_logical_basis = [
    "XIIXIIIIIIXX",
    "ZIIZIIIIIZIZ",
    "YIIYIIIIIZXY",
    "IXIXIIIIIXXI",
    "ZZIIIIIIIZZI",
    "ZYIXIIIIIYYI",
]


def results_as_pauli(results: list[qsharp.Result], pauli: str = "Z") -> str:
    p = ""
    for r in results:
        if r == qsharp.Result.One:
            p += pauli
        else:
            p += "I"
    return p


def pauli_as_results(p: str, basis: str = "Z", count: int = 2):
    results = []
    chars = "XYZ".replace(basis, "")
    for i in range(count):
        if p[i] in chars:
            results.append(qsharp.Result.One)
        else:
            results.append(qsharp.Result.Zero)
    return results


def pauli_support(p: str) -> list[int]:
    return [i for i, char in enumerate(p) if char != "I"]


def logical_indexes_of(pauli: str):
    for qubit in pauli_support(pauli):
        character = pauli[qubit]
        if character == "X":
            yield 3 * qubit
        if character == "Z":
            yield 3 * qubit + 1
        if character == "Y":
            yield 3 * qubit + 2


def commutes_with(pauli1: str, pauli2: str) -> bool:
    """Check if two Pauli strings commute."""
    assert len(pauli1) == len(pauli2)
    anti_commute_count = 0
    for p1, p2 in zip(pauli1, pauli2):
        if p1 == "I" or p2 == "I":
            continue
        if p1 != p2:
            anti_commute_count += 1
    return anti_commute_count % 2 == 0


def syndrome_of(error: str) -> list[int]:
    syndrome = []
    for label, generator in enumerate(generators):
        if not commutes_with(error, generator):
            syndrome.append(label)
    return syndrome


def mult_paulis(p1: str, p2: str) -> str:
    if len(p1) < len(p2):
        p1 = p1 + "I" * (len(p2) - len(p1))
    elif len(p2) < len(p1):
        p2 = p2 + "I" * (len(p1) - len(p2))
    result = ""
    for a, b in zip(p1, p2):
        if a == "I":
            result += b
        elif b == "I":
            result += a
        elif a == b:
            result += "I"
        elif (a, b) in [("X", "Y"), ("Y", "X")]:
            result += "Z"
        elif (a, b) in [("Y", "Z"), ("Z", "Y")]:
            result += "X"
        elif (a, b) in [("Z", "X"), ("X", "Z")]:
            result += "Y"
        else:
            raise ValueError(f"Unexpected Pauli characters: {a}, {b}")
    return result


def unsigned_logical_action_of(error: str) -> str:
    character_of = ("Y", "Z", "X", "I")
    commutations = list(map(lambda lb: commutes_with(error, lb), logical_basis))
    indexes = [2 * x + z for x, z in [commutations[0:2], commutations[2:4]]]
    characters = "".join(character_of[index] for index in indexes)
    return characters


def representative_of(pauli: str) -> str:
    generators = (expanded_logical_basis[index] for index in logical_indexes_of(pauli))
    res = "I" * 12
    for gen in generators:
        res = mult_paulis(res, gen)
    return res


def logical_action_of(error: str) -> str:
    logical = unsigned_logical_action_of(error)
    return logical


def recovery_from_syndrome_measurements(
    x_meas: list[qsharp.Result], z_meas: list[qsharp.Result]
) -> str:
    error_z = results_as_pauli(x_meas, pauli="Z")
    error_x = results_as_pauli(z_meas, pauli="X")
    error = mult_paulis(error_z, error_x)
    syndrome = frozenset(syndrome_of(error))
    recovery = table.get(syndrome, "IIIIIIIIIIII")
    return logical_action_of(mult_paulis(recovery, error))


# For each tuple of shot results, mark the shot as preselect if any preselect
# measurement is One. Then use the rounds of error correction syndrome measurements
# to generate Pauli corrections, collected in a frame. Use the final Pauli frame
# to calculate the corrected logical results.
def decode_results(results, basis: str = "Z"):
    corrected_logical_results = []
    for res in results:
        corrected_logical_results.append([])
        for shot in res:
            if any([preselect == qsharp.Result.One for preselect in shot[0]]):
                corrected_logical_results[-1] += ["PREselect"]
                continue
            recovery = "IIIIIIIIIIII"
            r = None
            for ec_output in shot[1]:
                r = recovery_from_syndrome_measurements(ec_output[0], ec_output[1])
                if r is None:
                    corrected_logical_results[-1] += ["POSTselect"]
                    break
                recovery = mult_paulis(recovery, r)
            if r is None:
                corrected_logical_results[-1] += [
                    pauli_as_results(recovery, basis=basis)
                ]
                continue
            if basis == "Z":
                r = recovery_from_syndrome_measurements([], shot[2])
            else:
                assert basis == "X"
                r = recovery_from_syndrome_measurements(shot[2], [])
            if r is None:
                corrected_logical_results[-1] += ["POSTselect"]
                continue
            recovery = mult_paulis(recovery, r)
            corrected_logical_results[-1] += [pauli_as_results(recovery, basis=basis)]
    return corrected_logical_results
