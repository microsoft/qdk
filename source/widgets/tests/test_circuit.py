# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pathlib
import tempfile
import unittest

from qsharp_widgets import Circuit


class _CircuitData:
    def json(self):
        return '{"circuits":[]}'


class CircuitTests(unittest.TestCase):
    def test_display_svg(self):
        circuit = Circuit(_CircuitData(), capture_svg=True)
        circuit.svg = '<svg xmlns="http://www.w3.org/2000/svg"></svg>'

        rendered = circuit.display_svg()

        self.assertIn('xmlns="http://www.w3.org/2000/svg"', rendered._repr_svg_())

    def test_save_svg(self):
        circuit = Circuit(_CircuitData(), capture_svg=True)
        circuit.svg = '<svg xmlns="http://www.w3.org/2000/svg"></svg>'

        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "circuit.svg"
            circuit.save_svg(str(path))

            self.assertEqual(path.read_text(encoding="utf-8"), circuit.svg)


if __name__ == "__main__":
    unittest.main()
