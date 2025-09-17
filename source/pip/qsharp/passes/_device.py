# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from enum import Enum


AC1K = {
    "cols": 36,
    "zones": [
        {"title": "Register 1", "rows": 17, "kind": "register"},
        {"title": "Interaction Zone", "rows": 4, "kind": "interaction"},
        {"title": "Register 2", "rows": 17, "kind": "register"},
        {"title": "Measurement Zone", "rows": 4, "kind": "measurement"},
    ],
}


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

    def ac1k():
        return Device(
            36,
            [
                Zone("Register 1", 17, ZoneType.REG),
                Zone("Interaction Zone", 4, ZoneType.INTER),
                Zone("Register 2", 17, ZoneType.REG),
                Zone("Measurement Zone", 4, ZoneType.MEAS),
            ],
        )

    def __init__(self, column_count: int, zones: list[Zone]):
        self.column_count = column_count
        self.zones = zones
        offset = 0
        # Ensure the zones have correct offsets set based on their ordering when passed in.
        for zone in self.zones:
            zone.set_offset(offset)
            offset += zone.row_count * self.column_count

        # Compute the home locations of qubits in the register zones.
        # The home location is the (row, column) position of the qubit in the device layout, using only
        # the register zones.
        self.home_locs = [0] * sum(
            zone.row_count * self.column_count
            for zone in zones
            if zone.type == ZoneType.REG
        )
        curr_zone = 0
        curr_id_offset = 0
        for i in range(len(self.home_locs)):
            # Distribute qubits evenly across the register zones.
            home_loc = None
            while home_loc is None:
                if curr_zone >= len(self.zones):
                    raise ValueError("Not enough register space for qubits")
                if self.zones[curr_zone].type != ZoneType.REG:
                    curr_zone += 1
                    continue
                loc = i
                if loc < self.zones[curr_zone].offset and curr_id_offset == 0:
                    curr_id_offset = (
                        self.zones[curr_zone - 1].row_count * self.column_count
                    )
                loc += curr_id_offset
                if (
                    loc
                    >= self.zones[curr_zone].offset
                    + self.zones[curr_zone].row_count * self.column_count
                ):
                    curr_zone += 1
                    continue
                # Save the (row, column) location of the qubit.
                home_loc = (loc // self.column_count, loc % self.column_count)
            self.home_locs[i] = home_loc

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
