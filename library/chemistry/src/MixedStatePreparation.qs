// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export MixedStatePreparationRequirements;
export PurifiedMixedState;
export PurifiedMixedStateRequirements;

import Std.Arrays.*;
import Std.Diagnostics.Fact;
import Std.Math.*;
import Std.Convert.IntAsDouble;
import Std.Arithmetic.ApplyIfGreaterLE;
import Std.StatePreparation.PrepareUniformSuperposition;

import Generators.MultiplexOperationsFromGenerator;
import Utils.RangeAsIntArray;

/// # Summary
/// Represents a particular mixed state that can be prepared on an index
/// and a garbage register.
///
/// # Input
/// ## Requirements
/// Specifies the size of the qubit registers required to prepare the
/// mixed state represented by this UDT value.
/// ## Norm
/// Specifies the 1-norm of the coefficients used to define this mixed
/// state.
/// ## Prepare
/// An operation that, given an index register, a data register, and a
/// garbage register initially in the $\ket{0}$, $\ket{00\dots 0}$, and
/// $\ket{00\dots 0}$ states (respectively),
/// prepares the state represented by this UDT value on those registers.
struct MixedStatePreparation {
    Requirements : MixedStatePreparationRequirements,
    Norm : Double,
    Prepare : ((Qubit[], Qubit[], Qubit[]) => Unit is Adj + Ctl),
}

/// # Summary
/// Represents the number of qubits required in order to prepare a given
/// mixed state.
///
/// # Input
/// ## NumTotalQubits
/// The total number of qubits required by the represented state preparation
/// operation.
/// ## NumIndexQubits
/// The number of qubits required for the index register used by the
/// represented state preparation operation.
/// ## NumGarbageQubits
/// The number of qubits required for the garbage register used by the
/// represented state preparation operation.
struct MixedStatePreparationRequirements {
    NumTotalQubits : Int,
    NumIndexQubits : Int,
    NumGarbageQubits : Int,
}

/// # Summary
/// Returns an operation that prepares a purification of a given mixed state.
/// A "purified mixed state" refers to states of the form |ψ⟩ = Σᵢ √𝑝ᵢ |𝑖⟩ |garbageᵢ⟩ specified by a vector of
/// coefficients {𝑝ᵢ}. States of this form can be reduced to mixed states ρ ≔ 𝑝ᵢ |𝑖⟩⟨𝑖| by tracing over the "garbage"
/// register (that is, a mixed state that is diagonal in the computational basis).
///
/// See https://arxiv.org/pdf/1805.03662.pdf?page=15 for further discussion.
///
/// # Description
/// Uses the Quantum ROM technique to represent a given density matrix,
/// returning that representation as a state preparation operation.
///
/// In particular, given a list of $N$ coefficients $\alpha_j$, this
/// function returns an operation that uses the Quantum ROM technique to
/// prepare an approximation
/// $$
/// \begin{align}
///     \tilde\rho = \sum_{j = 0}^{N - 1} p_j \ket{j}\bra{j}
/// \end{align}
/// $$
/// of the mixed state
/// $$
/// \begin{align}
///     \rho = \sum_{j = 0}^{N-1} \frac{|\alpha_j|}{\sum_k |\alpha_k|} \ket{j}\bra{j},
/// \end{align}
/// $$
/// where each $p_j$ is an approximation to the given coefficient $\alpha_j$
/// such that
/// $$
/// \begin{align}
///     \left| p_j - \frac{ |\alpha_j| }{ \sum_k |\alpha_k| } \right| \le \frac{\epsilon}{N}
/// \end{align}
/// $$
/// for each $j$.
///
/// When passed an index register and a register of garbage qubits,
/// initially in the state $\ket{0} \ket{00\cdots 0}$, the returned operation
/// prepares both registers into the purification of $\tilde \rho$,
/// $$
/// \begin{align}
///     \sum_{j=0}^{N-1} \sqrt{p_j} \ket{j}\ket{\text{garbage}_j},
/// \end{align}
/// $$
/// such that resetting and deallocating the garbage register enacts the
/// desired preparation to within the target error $\epsilon$.
///
/// # Input
/// ## targetError
/// The target error $\epsilon$.
/// ## coefficients
/// Array of $N$ coefficients specifying the probability of basis states.
/// Negative numbers $-\alpha_j$ will be treated as positive $|\alpha_j|$.
///
/// # Output
/// An operation that prepares $\tilde \rho$ as a purification onto a joint
/// index and garbage register.
///
/// # Remarks
/// The coefficients provided to this operation are normalized following the
/// 1-norm, such that the coefficients are always considered to describe a
/// valid categorical probability distribution.
///
/// # Example
/// The following code snippet prepares an purification of the $3$-qubit state
/// $\rho=\sum_{j=0}^{4}\frac{|\alpha_j|}{\sum_k |\alpha_k|}\ket{j}\bra{j}$, where
/// $\vec\alpha=(1.0, 2.0, 3.0, 4.0, 5.0)$, and the target error is
/// $10^{-3}$:
/// ```qsharp
/// let coefficients = [1.0, 2.0, 3.0, 4.0, 5.0];
/// let targetError = 1e-3;
/// let purifiedState = PurifiedMixedState(targetError, coefficients);
/// using (indexRegister = Qubit[purifiedState.Requirements.NumIndexQubits]) {
///     using (garbageRegister = Qubit[purifiedState.Requirements.NumGarbageQubits]) {
///         purifiedState.Prepare(LittleEndian(indexRegister), [], garbageRegister);
///     }
/// }
/// ```
///
/// # References
/// - [Encoding Electronic Spectra in Quantum Circuits with Linear T Complexity](https://arxiv.org/abs/1805.03662)
///   Ryan Babbush, Craig Gidney, Dominic W. Berry, Nathan Wiebe, Jarrod McClean, Alexandru Paler, Austin Fowler, Hartmut Neven
function PurifiedMixedState(targetError : Double, coefficients : Double[]) : MixedStatePreparation {
    let nBitsPrecision = -Ceiling(Lg(0.5 * targetError)) + 1;
    let positiveCoefficients = Mapped(AbsD, coefficients);
    let (oneNorm, keepCoeff, altIndex) = QuantumROMDiscretization(nBitsPrecision, positiveCoefficients);
    let nCoeffs = Length(positiveCoefficients);
    let nBitsIndices = Ceiling(Lg(IntAsDouble(nCoeffs)));

    let op = PrepareQuantumROMState(nBitsPrecision, nCoeffs, nBitsIndices, keepCoeff, altIndex, [], _, _, _);
    let qubitCounts = PurifiedMixedStateRequirements(targetError, nCoeffs);
    return new MixedStatePreparation { Requirements = qubitCounts, Norm = oneNorm, Prepare = op };
}

operation PrepareQuantumROMState(
    nBitsPrecision : Int,
    nCoeffs : Int,
    nBitsIndices : Int,
    keepCoeff : Int[],
    altIndex : Int[],
    data : Bool[][],
    indexRegister : Qubit[],
    dataQubits : Qubit[],
    garbageRegister : Qubit[]
) : Unit is Adj + Ctl {
    let garbageIdx0 = nBitsIndices;
    let garbageIdx1 = garbageIdx0 + nBitsPrecision;
    let garbageIdx2 = garbageIdx1 + nBitsPrecision;
    let garbageIdx3 = garbageIdx2 + 1;

    let altIndexRegister = garbageRegister[0..garbageIdx0 - 1];
    let keepCoeffRegister = garbageRegister[garbageIdx0..garbageIdx1 - 1];
    let uniformKeepCoeffRegister = garbageRegister[garbageIdx1..garbageIdx2 - 1];
    let flagQubit = garbageRegister[garbageIdx3 - 1];
    let dataRegister = dataQubits;
    let altDataRegister = garbageRegister[garbageIdx3...];

    // Create uniform superposition over index and alt coeff register.
    PrepareUniformSuperposition(nCoeffs, indexRegister);
    ApplyToEachCA(H, uniformKeepCoeffRegister);

    // Write bitstrings to altIndex and keepCoeff register.
    let unitaryGenerator = (nCoeffs, idx -> WriteQuantumROMBitString(idx, keepCoeff, altIndex, data, _, _, _, _));
    MultiplexOperationsFromGenerator(unitaryGenerator, indexRegister, (keepCoeffRegister, altIndexRegister, dataRegister, altDataRegister));

    // Perform comparison
    ApplyIfGreaterLE(X, uniformKeepCoeffRegister, keepCoeffRegister, flagQubit);

    let indexRegisterSize = Length(indexRegister);

    // Swap in register based on comparison
    let register = indexRegister + dataRegister;
    let altRegister = altIndexRegister + altDataRegister;
    for i in IndexRange(indexRegister) {
        Controlled SWAP([flagQubit], (register[i], altRegister[i]));
    }
}

// Classical processing
// This discretizes the coefficients such that
// |coefficient[i] * oneNorm - discretizedCoefficient[i] * discretizedOneNorm| * nCoeffs <= 2^{1-bitsPrecision}.
function QuantumROMDiscretization(bitsPrecision : Int, coefficients : Double[]) : (Double, Int[], Int[]) {
    let oneNorm = PNorm(1.0, coefficients);
    let nCoefficients = Length(coefficients);
    Fact(bitsPrecision <= 31, $"Bits of precision {bitsPrecision} unsupported. Max is 31.");
    Fact(nCoefficients > 1, "Cannot prepare state with less than 2 coefficients.");
    Fact(oneNorm >= 0.0, "State must have at least one coefficient > 0");

    let barHeight = 2^bitsPrecision - 1;

    mutable altIndex = RangeAsIntArray(0..nCoefficients - 1);
    mutable keepCoeff = Mapped(
        coefficient -> Round((AbsD(coefficient) / oneNorm) * IntAsDouble(nCoefficients) * IntAsDouble(barHeight)),
        coefficients
    );

    // Calculate difference between number of discretized bars vs. maximum
    mutable bars = 0;
    for idxCoeff in IndexRange(keepCoeff) {
        bars += keepCoeff[idxCoeff] - barHeight;
    }

    // Uniformly distribute excess bars across coefficients.
    for idx in 0..AbsI(bars) - 1 {
        keepCoeff w/= idx <- keepCoeff[idx] + (bars > 0 ? -1 | + 1);
    }

    mutable barSink = [];
    mutable barSource = [];

    for idxCoeff in IndexRange(keepCoeff) {
        if keepCoeff[idxCoeff] > barHeight {
            barSource += [idxCoeff];
        } elif keepCoeff[idxCoeff] < barHeight {
            barSink += [idxCoeff];
        }
    }

    for rep in 0..nCoefficients * 10 {
        if Length(barSink) > 0 and Length(barSource) > 0 {
            let idxSink = Tail(barSink);
            let idxSource = Tail(barSource);
            barSink = Most(barSink);
            barSource = Most(barSource);

            keepCoeff w/= idxSource <- keepCoeff[idxSource] - barHeight + keepCoeff[idxSink];
            altIndex w/= idxSink <- idxSource;

            if keepCoeff[idxSource] < barHeight {
                barSink += [idxSource];
            } elif keepCoeff[idxSource] > barHeight {
                barSource += [idxSource];
            }
        } elif Length(barSource) > 0 {
            let idxSource = Tail(barSource);
            barSource = Most(barSource);
            keepCoeff w/= idxSource <- barHeight;
        } else {
            return (oneNorm, keepCoeff, altIndex);
        }
    }

    return (oneNorm, keepCoeff, altIndex);
}

/// # Summary
/// Returns the total number of qubits that must be allocated
/// in order to apply the operation returned by
/// `PurifiedMixedState`.
///
/// # Input
/// ## targetError
/// The target error $\epsilon$.
/// ## nCoefficients
/// The number of coefficients to be specified in preparing a mixed state.
///
/// # Output
/// A description of how many qubits are required in total, and for each of
/// the index and garbage registers used by the
/// `PurifiedMixedState` function.
function PurifiedMixedStateRequirements(targetError : Double, nCoefficients : Int) : MixedStatePreparationRequirements {
    Fact(targetError > 0.0, "targetError must be positive");
    Fact(nCoefficients > 0, "nCoefficients must be positive");

    let nBitsPrecision = -Ceiling(Lg(0.5 * targetError)) + 1;
    let nIndexQubits = Ceiling(Lg(IntAsDouble(nCoefficients)));
    let nGarbageQubits = nIndexQubits + 2 * nBitsPrecision + 1;
    let nTotal = nGarbageQubits + nIndexQubits;
    return new MixedStatePreparationRequirements { NumTotalQubits = nTotal, NumIndexQubits = nIndexQubits, NumGarbageQubits = nGarbageQubits };
}

operation WriteQuantumROMBitString(idx : Int, keepCoeff : Int[], altIndex : Int[], data : Bool[][], keepCoeffRegister : Qubit[], altIndexRegister : Qubit[], dataRegister : Qubit[], altDataRegister : Qubit[]) : Unit is Adj + Ctl {
    if keepCoeff[idx] >= 0 {
        ApplyXorInPlace(keepCoeff[idx], keepCoeffRegister);
    }
    ApplyXorInPlace(altIndex[idx], altIndexRegister);
    if Length(dataRegister) > 0 {
        for i in IndexRange(data[idx]) {
            if data[idx][i] {
                X(dataRegister[i]);
            }
        }
        for i in IndexRange(data[altIndex[idx]]) {
            if data[altIndex[idx]][i] {
                X(altDataRegister[i]);
            }
        }
    }
}

