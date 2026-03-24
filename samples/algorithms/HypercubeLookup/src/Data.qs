// Hypercube vertex data

import ClassicalSearch.GetIndexBelowThresholdClassically;
import Std.Diagnostics.*;
import Std.Math.BitSizeI;

/// # Summary
/// Selected hypercube vertices represented as coordinate bit strings.
function HypercubeVertices() : Bool[][] {
    [
        [false, false, false, false, false, false], // 0: d = 6
        [false, true, false, false, false, false],  // 1: d = 5
        [false, true, true, true, false, false],    // 2: d = 3
        [true, false, true, false, false, false],   // 3: d = 4
        [true, true, false, true, false, false],    // 4: d = 3
        [false, false, true, true, false, false],   // 5: d = 4
        [true, false, true, true, true, false],     // 6: d = 2
        [true, true, false, false, false, true],    // 7: d = 3
    ]
}

/// # Summary
/// The query vertex represented as a coordinate bit string.
/// We are searching for a selected vertex closest to this one.
function QueryVertex() : Bool[] {
    [true, true, true, true, true, true]
}

/// # Summary
/// The number of dimensions of the hypercube.
/// Also verifies the assumptions about the data.
function HypercubeDimentions() : Int {
    Length(QueryVertex())
}

/// # Summary
/// The number of selected vertices of the hypercube.
function TableLength() : Int {
    Length(HypercubeVertices())
}

/// # Summary
/// We are finding a vertex under this threshold.
/// Only one vertex should satisfy this condition.
function DistanceThreshold() : Int {
    3
}

/// # Summary
/// The maximum possible distance in the hypercube.
function MaxDistance() : Int {
    HypercubeDimentions()
}

/// # Summary
/// The number of bits needed to represent the maximum distance.
function MaxDistanceBits() : Int {
    BitSizeI(MaxDistance())
}

/// # Summary
/// The number of bits needed to address all selected vertices.
/// Also veifies all assumptions about the table data.
function TableAddressBits() : Int {
    let dimensions = Length(QueryVertex());
    for vertex in HypercubeVertices() {
        Fact(Length(vertex) == dimensions, "All vertices must have the same number of dimensions as the query vertex.");
    }

    let tableSize = TableLength();
    Fact((tableSize &&& (tableSize - 1)) == 0, "Table length needs to be a power of two. Otherwise, respectExcessiveAddress option must be true.");

    let index = GetIndexBelowThresholdClassically(
        HypercubeVertices(),
        QueryVertex(),
        DistanceThreshold()
    );
    Fact(index >= 0 and index < TableLength(), "One table index must satisfy the distance threshold condition.");

    BitSizeI(TableLength() - 1)
}
