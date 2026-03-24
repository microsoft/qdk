import Std.Diagnostics.*;
import Std.Arrays.*;

/// # Summary
/// Classically finds the index of a vertex in the list of vertices
/// whose distance to the query vertex is below the given threshold.
function GetIndexBelowThresholdClassically(
    vertices : Bool[][],
    query : Bool[],
    threshold : Int
) : Int {
    Fact(not IsEmpty(vertices), "Vertex list must not be empty.");
    mutable foundIndex = -1;
    for idx in IndexRange(vertices) {
        let vertex = vertices[idx];
        let distance = HammingDistance(vertex, query);
        if distance < threshold {
            Fact(foundIndex == -1, "More than one vertex found below the threshold.");
            set foundIndex = idx;
        }
    }
    Fact(foundIndex != -1, "No vertex found below the threshold.");
    return foundIndex;
}

/// # Summary
/// Computes the Hamming distance between two bit strings.
/// This serves as a multidimensional Manhattan distance (L₁ distance)
/// for vertex coordinates represented as bit strings.
function HammingDistance(a : Bool[], b : Bool[]) : Int {
    Fact(Length(a) == Length(b), "Arrays must be of the same length.");
    mutable distance = 0;
    for i in IndexRange(a) {
        if a[i] != b[i] {
            distance += 1;
        }
    }
    return distance;
}
