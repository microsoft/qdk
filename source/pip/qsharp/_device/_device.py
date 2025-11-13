# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from enum import Enum
from .._qsharp import QirInputData


class ZoneType(Enum):
    """
    Enum representing different types of zones in the device layout.
    """

    REG = "register"
    INTER = "interaction"
    MEAS = "measurement"


class Zone:
    """
    Represents a zone in the device layout.
    """

    offset: int = 0

    def __init__(self, name: str, row_count: int, type: ZoneType):
        self.name = name
        self.row_count = row_count
        self.type = type

    def set_offset(self, offset: int):
        self.offset = offset


class Device:
    """
    Represents a quantum device with specific layout expressed as zones.
    """

    def __init__(self, column_count: int, zones: list[Zone]):
        self.column_count = column_count
        self.zones = zones
        offset = 0
        # Ensure the zones have correct offsets set based on their ordering when passed in.
        for zone in self.zones:
            zone.set_offset(offset)
            offset += zone.row_count * self.column_count

        self.home_locs = []
        self._init_home_locs()

    def _init_home_locs(self):
        """
        Initialize the home locations of qubits in the device layout.
        """
        raise NotImplementedError("Subclasses must implement _init_home_locs")

    def get_home_loc(self, qubit_id: int) -> tuple[int, int]:
        """
        Get the home location (row, column) of the qubit with the given id.

        Args:
            qubit_id (int): The id of the qubit.

        Returns:
            tuple[int, int]: The (row, column) location of the qubit.
        """
        if qubit_id < 0 or qubit_id >= len(self.home_locs):
            raise ValueError(f"Qubit id {qubit_id} is out of range")
        return self.home_locs[qubit_id]

    def get_register_zones(self) -> list[Zone]:
        """
        Get the register zones in the device.

        Returns:
            list[Zone]: The register zones.
        """
        return [zone for zone in self.zones if zone.type == ZoneType.REG]

    def get_interaction_zones(self) -> list[Zone]:
        """
        Get the interaction zones in the device.

        Returns:
            list[Zone]: The interaction zones.
        """
        return [zone for zone in self.zones if zone.type == ZoneType.INTER]

    def get_measurement_zones(self) -> list[Zone]:
        """
        Get the measurement zones in the device.

        Returns:
            list[Zone]: The measurement zones.
        """
        return [zone for zone in self.zones if zone.type == ZoneType.MEAS]

    def compile(self, program: str) -> QirInputData:
        """
        Compile the given program for the device.

        Args:
            program (str): The program to compile.
        """
        raise NotImplementedError("Subclasses must implement compile")

    def as_dict(self) -> dict:
        """
        Get the device layout as a dictionary.

        Returns:
            dict: The device layout as a dictionary.
        """
        return {
            "cols": self.column_count,
            "zones": [
                {"title": zone.name, "rows": zone.row_count, "kind": zone.type.value}
                for zone in self.zones
            ],
        }

    def get_layout(self) -> dict:
        """
        Get the device layout as a dictionary.

        Returns:
            dict: The device layout as a dictionary.
        """
        return self.as_dict()
