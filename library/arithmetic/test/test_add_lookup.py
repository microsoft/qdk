"""Tests for AddLookup.qs."""

import random

from qdk import Context
from qdk.test_utils import ArithmeticOpTester


def _list_to_qs(data: list[int]) -> str:
    return "[" + ",".join(f"{x}L" for x in data) + "]"


def test_add_lookup_simple(context: Context):
    """Simple deterministic smoke test for AddLookup.AddLookup."""
    address_size = 2
    n = 4
    data = [1, 3, 5, 7]
    op = f"AddLookup.AddLookup(_,_,{_list_to_qs(data)})"
    op_tester = ArithmeticOpTester(op, arg_sizes=[address_size, n], context=context)

    for address, y in [(0, 0), (1, 2), (2, 14), (3, 15)]:
        expected = [address, (y + data[address]) % (2**n)]
        assert op_tester.run([address, y]) == expected


def test_add_lookup_non_mod(context: Context):
    """Tests AddLookup.AddLookup for addition modulo 2^n."""
    address_size = 3
    n = 8
    modulus = 2**n
    table_length = 2**address_size

    data = [random.randint(0, modulus - 1) for _ in range(table_length)]
    op = f"AddLookup.AddLookup(_,_,{_list_to_qs(data)})"
    op_tester = ArithmeticOpTester(op, arg_sizes=[address_size, n], context=context)

    for _ in range(20):
        address = random.randint(0, table_length - 1)
        y = random.randint(0, modulus - 1)
        assert op_tester.run([address, y]) == [address, (y + data[address]) % modulus]


def test_parallel_mod_add_lookup(context: Context):
    """Tests AddLookup.ParallelModAddLookup for modular table adds."""
    address_size = 3
    n = 8
    modulus = 211
    table_length = 2**address_size

    table0 = [random.randint(0, 4 * modulus) for _ in range(table_length)]
    table1 = [random.randint(0, 4 * modulus) for _ in range(table_length)]

    table0_qs = _list_to_qs(table0)
    table1_qs = _list_to_qs(table1)
    op = (
        "((address, target0, target1) => AddLookup.ParallelModAddLookup("
        f"address, [target0, target1], [{table0_qs}, {table1_qs}], {modulus}L))"
    )
    op_tester = ArithmeticOpTester(op, arg_sizes=[address_size, n, n], context=context)

    for _ in range(20):
        address = random.randint(0, table_length - 1)
        y0 = random.randint(0, modulus - 1)
        y1 = random.randint(0, modulus - 1)

        expected = [
            address,
            (y0 + (table0[address] % modulus)) % modulus,
            (y1 + (table1[address] % modulus)) % modulus,
        ]
        assert op_tester.run([address, y0, y1]) == expected
