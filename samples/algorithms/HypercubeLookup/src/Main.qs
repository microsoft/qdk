/// # Sample
/// Grover Search on hypercube

// Import standard libraries
import Std.Diagnostics.*;
import Std.Arrays.*;
import Std.Convert.*;
import Std.Arithmetic.ApplyIfGreaterL;
import Std.Arithmetic.IncByI;

// Import table lookup algorithms
import TableLookup.*;

// Import grover search algorithm
import GroverSearch.*;

/// # Summary
/// This sample searches for a single selected vertex of a hypercube
/// with a distance to a query vertex below a given threshold. The distance
/// used is a multidimensional Manhattan distance (L₁ distance).
///
/// # Description
/// The sample uses Grover's search algorithm to find one of the selected
/// vertices of a hypercube represented by a bit string of coordinates. The sample
/// shows how to implement a quantum oracle that marks indices of vertices
/// whose distance to the query vertex is below a given threshold. It uses table
/// lookup library to load the vertex data and arithmetic library to compute the distance.
///
/// This sample demonstrates:
/// - How to use Table Lookup and Arithmetic libraries in Q#
/// - How to shift from a classical thinking to a quantum thinking
///   when implementing quantum oracles. See `MarkIndexIfCloser`.
///
/// Things to try out:
/// - Run Histogram to see that index 6 is the prevailing result.
/// - Run Circuit to see the generated quantum circuit.
/// - Change lookupAlgorithm and unlookupAlgorithm options in `GetCustomLookupOptions`
///   to see different circuits and resource estimates. For example DoMCXLookup will
///   trade measurements for CCZ gates.
///
/// Note that this sample is not intended to demonstrate a quantum advantage.
/// Classical search would be more efficient for this particular problem.
/// In fact, the sample includes a classical function to verify the input data.
operation Main() : Int {
    let tableSize = Data.TableLength();
    let nBits = Data.TableAddressBits();
    Message($"Using {nBits} qubits to search over {tableSize} vertices.");

    let nIterations = IterationsToMarked(nBits);
    Message($"Number of iterations needed: {nIterations}");

    // Use Grover's algorithm to find one selected vertex
    // which has a distance to the query vertex below the threshold.
    let results = GroverSearch(
        nBits,
        nIterations,
        MarkIndexIfCloser(Data.DistanceThreshold(), _)
    );

    // Return the index of the found vertex
    BoolArrayAsInt(ResultArrayAsBoolArray(results))
}

/// # Summary
/// Reflect if the distance from the query vertex is below the given threshold.
///
/// # Description
/// This operation implements the phase oracle for Grover's algorithm.
/// It demonstrates how to shift from a classical thinking to a quantum thinking.
///
/// Classically, this operation would check if the distance is below the threshold for one
/// selected vertex at a given index. In the quantum oracle, we need to do this
/// for many possible indices in superposition at once. Therefore, we need to use
/// reversible unitary operations and uncompute any temporary values we create.
operation MarkIndexIfCloser(distanceThreshold : Int, index : Qubit[]) : Unit {
    // Allocate registers for data, distance, and phase kickback
    use data = Qubit[Data.HypercubeDimentions()];
    use distance = Qubit[Data.MaxDistanceBits()];
    use phase = Qubit();

    // Do the following steps in a within/apply block to ensure that all temporary
    // values are uncomputed properly.
    within {
        // Prepare qubit for the phase kickback
        X(phase);
        H(phase);

        // Classically: This would be just an indexing into a table
        // to get one selected vertex coordinates: data = vertices[index]
        // Quantumly: Use table lookup to load the superposition of vertex coordinates
        // for all possible indices in superposition in the index register.
        Lookup(GetCustomLookupOptions(), Data.HypercubeVertices(), index, data);

        // Classically: This would XOR the query vertex coordinates with the selected vertex
        // Quantumly: Use X gate on qubits where query vertex coordinates are 1
        ApplyPauliFromBitString(PauliX, true, Data.QueryVertex(), data);

        for q in data {
            // Classically: This would increase the counter by 1 if the XORed result q is 1
            // Quantumly: Use IncByI operation to increase the distance register.
            // Use controlled version of it instead of measuring the q as operation needs to be reversible.
            Controlled IncByI([q], (1, distance));
        }
    } apply {
        // Classically: This would check if distance < distanceThreshold
        // Quantumly: Use ApplyIfGreaterL to apply phase kickback if distance < distanceThreshold
        ApplyIfGreaterL(X, IntAsBigInt(distanceThreshold), distance, phase);
    }
}

/// # Summary
/// Gets custom lookup options for a table lookup.
function GetCustomLookupOptions() : LookupOptions {
    // Note how easy it is to switch algorithms when debugging or resource estimating.
    new LookupOptions {
        // Play with these options to generate different circuits and resource estimates
        lookupAlgorithm = DoSplitPPLookup(),
        unlookupAlgorithm = DoSplitPPUnlookup(),
        preferMeasurementBasedUncomputation = false,
        // Our data length is 2^n so we can be strict here
        failOnLongData = true,
        failOnShortData = true,
        // Relaxed option as data is aligned
        respectExcessiveAddress = false,
    }
}
