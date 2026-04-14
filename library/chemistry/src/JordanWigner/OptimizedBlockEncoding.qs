// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export OptimizedBETermIndex;
export OptimizedBEGeneratorSystem;
export OptimizedBlockEncodingGeneratorSystem;
export MixedStatePreparation;
export BlockEncodingByLCU;
export QuantumWalkByQubitization;
export PauliBlockEncoding;

import Std.Arrays.*;
import Std.Math.*;
import Std.Convert.IntAsDouble;
import Std.Arithmetic.ApplyIfGreaterLE;
import Std.StatePreparation.PreparePureStateD;
import Std.Diagnostics.Fact;

import Generators.GeneratorIndex;
import Generators.GeneratorSystem;
import Generators.HTermToGenIdx;
import Generators.MultiplexOperationsFromGenerator;
import JordanWigner.OptimizedBEOperator.JWSelect;
import JordanWigner.OptimizedBEOperator.JWSelectQubitCount;
import JordanWigner.OptimizedBEOperator.JWSelectQubitManager;
import JordanWigner.Data.JWOptimizedHTerms;
import MixedStatePreparation.PurifiedMixedState;
import MixedStatePreparation.PurifiedMixedStateRequirements;
import Utils.RangeAsIntArray;
import Utils.IsNotZero;

/// # Summary
/// Term data in the optimized block-encoding algorithm.
struct OptimizedBETermIndex {
    Coefficient : Double,
    UseSignQubit : Bool,
    ZControlRegisterMask : Bool[],
    OptimizedControlRegisterMask : Bool[],
    PauliBases : Int[],
    RegisterIndices : Int[],
}

/// # Summary
/// Function that returns `OptimizedBETermIndex` data for term `n` given an
/// integer `n`, together with the number of terms in the first `Int` and
/// the sum of absolute-values of all term coefficients in the `Double`.
struct OptimizedBEGeneratorSystem {
    NumTerms : Int,
    Norm : Double,
    SelectTerm : (Int -> OptimizedBETermIndex)
}

function JWOptimizedBlockEncoding(
    targetError : Double,
    data : JWOptimizedHTerms,
    nSpinOrbitals : Int
) : ((Int, Int), (Double, (Qubit[], Qubit[]) => Unit is Adj + Ctl)) {

    let nZ = 2;
    let nMaj = 4;
    let optimizedBEGeneratorSystem = OptimizedBlockEncodingGeneratorSystem(data);
    let nCoeffs = optimizedBEGeneratorSystem.NumTerms;
    let nIdxRegQubits = Ceiling(Lg(IntAsDouble(nSpinOrbitals)));
    let ((nCtrlRegisterQubits, nTargetRegisterQubits), rest) = JWOptimizedBlockEncodingQubitCount(
        targetError,
        nCoeffs,
        nZ,
        nMaj,
        nIdxRegQubits,
        nSpinOrbitals
    );
    let statePrepOp = JWOptimizedBlockEncodingStatePrepWrapper(
        targetError,
        nCoeffs,
        optimizedBEGeneratorSystem,
        nZ,
        nMaj,
        nIdxRegQubits,
        _
    );
    let selectOp = JWOptimizedBlockEncodingSelect(
        targetError,
        nCoeffs,
        optimizedBEGeneratorSystem,
        nZ,
        nMaj,
        nIdxRegQubits,
        _,
        _
    );
    let blockEncodingReflection = BlockEncodingByLCU(statePrepOp, selectOp);
    return (
        (nCtrlRegisterQubits, nTargetRegisterQubits),
        (optimizedBEGeneratorSystem.Norm, blockEncodingReflection)
    );
}

// Get OptimizedBEGeneratorSystem coefficients
function OptimizedBEGeneratorSystemCoeff(optimizedBEGeneratorSystem : OptimizedBEGeneratorSystem) : Double[] {
    mutable coefficients = [];
    for idx in 0..optimizedBEGeneratorSystem.NumTerms - 1 {
        coefficients += [optimizedBEGeneratorSystem.SelectTerm(idx).Coefficient];
    }
    return coefficients;
}


/// # Summary
/// Converts a `GeneratorIndex` describing a Z term to
/// an expression `GeneratorIndex[]` in terms of Paulis.
///
/// # Input
/// ## term
/// `GeneratorIndex` representing a Z term.
///
/// # Output
/// `OptimizedBETermIndex` expressing Z term as Pauli terms.
function ZTermToPauliMajIdx(term : GeneratorIndex) : OptimizedBETermIndex {
    let (_, coeff) = term.Term;
    let idxFermions = term.Subsystem;
    let signQubit = coeff[0] < 0.0;
    let selectZControlRegisters = [true];
    let optimizedBEControlRegisters = [];
    let pauliBases = [];
    let indexRegisters = idxFermions;
    return new OptimizedBETermIndex {
        Coefficient = coeff[0],
        UseSignQubit = signQubit,
        ZControlRegisterMask = selectZControlRegisters,
        OptimizedControlRegisterMask = optimizedBEControlRegisters,
        PauliBases = pauliBases,
        RegisterIndices = indexRegisters
    };
}


/// # Summary
/// Converts a GeneratorIndex describing a ZZ term to
/// an expression `GeneratorIndex[]` in terms of Paulis.
///
/// # Input
/// ## term
/// `GeneratorIndex` representing a ZZ term.
///
/// # Output
/// `OptimizedBETermIndex` expressing ZZ term as Pauli terms.
function ZZTermToPauliMajIdx(term : GeneratorIndex) : OptimizedBETermIndex {
    let (_, coeff) = term.Term;
    let idxFermions = term.Subsystem;
    let signQubit = coeff[0] < 0.0;
    let selectZControlRegisters = [true, true];
    let optimizedBEControlRegisters = [];
    let pauliBases = [];
    let indexRegisters = idxFermions;
    return new OptimizedBETermIndex {
        Coefficient = 2.0 * coeff[0],
        UseSignQubit = signQubit,
        ZControlRegisterMask = selectZControlRegisters,
        OptimizedControlRegisterMask = optimizedBEControlRegisters,
        PauliBases = pauliBases,
        RegisterIndices = indexRegisters
    };
}


/// # Summary
/// Converts a `GeneratorIndex` describing a PQ term to
/// an expression `GeneratorIndex[]` in terms of Paulis
///
/// # Input
/// ## term
/// `GeneratorIndex` representing a PQ term.
///
/// # Output
/// `OptimizedBETermIndex` expressing PQ term as Pauli terms.
function PQTermToPauliMajIdx(term : GeneratorIndex) : OptimizedBETermIndex {
    let (_, coeff) = term.Term;
    let idxFermions = term.Subsystem;
    let sign = coeff[0] < 0.0;
    let selectZControlRegisters = [];
    let optimizedBEControlRegisters = [true, true];
    let pauliBases = [1, 2];
    let indexRegisters = idxFermions;
    return new OptimizedBETermIndex {
        Coefficient = 2.0 * coeff[0],
        UseSignQubit = sign,
        ZControlRegisterMask = selectZControlRegisters,
        OptimizedControlRegisterMask = optimizedBEControlRegisters,
        PauliBases = pauliBases,
        RegisterIndices = indexRegisters
    };
}


/// # Summary
/// Converts a `GeneratorIndex` describing a PQ or PQQR term to
/// an expression `GeneratorIndex[]` in terms of Paulis
///
/// # Input
/// ## term
/// `GeneratorIndex` representing a PQ or PQQR term.
///
/// # Output
/// `OptimizedBETermIndex` expressing PQ or PQQR term as Pauli terms.
function PQandPQQRTermToPauliMajIdx(term : GeneratorIndex) : OptimizedBETermIndex {
    let (_, coeff) = term.Term;
    let idxFermions = term.Subsystem;
    let sign = coeff[0] < 0.0;

    if Length(idxFermions) == 2 {
        return PQTermToPauliMajIdx(term);
    } else {
        let qubitPidx = idxFermions[0];
        let qubitQidx = idxFermions[1];
        let qubitRidx = idxFermions[3];
        let selectZControlRegisters = [false, true];
        let optimizedBEControlRegisters = [true, false, true];
        let pauliBases = [1, 2];
        let indexRegisters = [qubitPidx, qubitQidx, qubitRidx];
        return new OptimizedBETermIndex {
            Coefficient = 2.0 * coeff[0],
            UseSignQubit = sign,
            ZControlRegisterMask = selectZControlRegisters,
            OptimizedControlRegisterMask = optimizedBEControlRegisters,
            PauliBases = pauliBases,
            RegisterIndices = indexRegisters
        };
    }
}


/// # Summary
/// Converts a `GeneratorIndex` describing a PQRS term to
/// an expression `GeneratorIndex[]` in terms of Paulis
///
/// # Input
/// ## term
/// `GeneratorIndex` representing a PQRS term.
///
/// # Output
/// `OptimizedBETermIndex[]` expressing PQRS term as Pauli terms.
function V0123TermToPauliMajIdx(term : GeneratorIndex) : OptimizedBETermIndex[] {
    let (_, v0123) = term.Term;
    let idxFermions = term.Subsystem;
    let qubitsPQ = idxFermions[0..1];
    let qubitsRS = idxFermions[2..3];
    let qubitsPQJW = RangeAsIntArray(qubitsPQ[0] + 1..qubitsPQ[1] - 1);
    let qubitsRSJW = RangeAsIntArray(qubitsRS[0] + 1..qubitsRS[1] - 1);
    let ops = [
        [1, 1, 1, 1],
        [1, 1, 2, 2],
        [1, 2, 1, 2],
        [1, 2, 2, 1],
        [2, 2, 2, 2],
        [2, 2, 1, 1],
        [2, 1, 2, 1],
        [2, 1, 1, 2]
    ];
    mutable majIdxes = Repeated(
        new OptimizedBETermIndex {
            Coefficient = 0.0,
            UseSignQubit = false,
            ZControlRegisterMask = [],
            OptimizedControlRegisterMask = [],
            PauliBases = [],
            RegisterIndices = []
        },
        4
    );
    mutable nonZero = 0;
    let selectZControlRegisters = [];
    let optimizedBEControlRegisters = [true, true, true, true];
    let indexRegisters = idxFermions;

    for idxOp in 0..3 {
        if IsNotZero(v0123[idxOp]) {
            let newCoeff = (2.0 * 0.25) * v0123[idxOp];
            majIdxes w/= nonZero <- new OptimizedBETermIndex {
                Coefficient = newCoeff,
                UseSignQubit = v0123[idxOp] < 0.0,
                ZControlRegisterMask = selectZControlRegisters,
                OptimizedControlRegisterMask = optimizedBEControlRegisters,
                PauliBases = ops[idxOp],
                RegisterIndices = indexRegisters
            };
            nonZero = nonZero + 1;
        }
    }

    return majIdxes[0..nonZero - 1];
}


/// # Summary
/// Converts a Hamiltonian described by `JWOptimizedHTerms`
/// to a `GeneratorSystem` expressed in terms of the Pauli
/// `GeneratorIndex`.
///
/// # Input
/// ## data
/// Description of Hamiltonian in `JWOptimizedHTerms` format.
///
/// # Output
/// Representation of Hamiltonian as `GeneratorSystem`.
function OptimizedBlockEncodingGeneratorSystem(data : JWOptimizedHTerms) : OptimizedBEGeneratorSystem {
    let ZData = data.HTerm0;
    let ZZData = data.HTerm1;
    let PQandPQQRData = data.HTerm2;
    let h0123Data = data.HTerm3;
    mutable majIdxes = Repeated(
        new OptimizedBETermIndex {
            Coefficient = 0.0,
            UseSignQubit = false,
            ZControlRegisterMask = [],
            OptimizedControlRegisterMask = [],
            PauliBases = [],
            RegisterIndices = []
        },
        ((Length(ZData) + Length(ZZData)) + Length(PQandPQQRData)) + 4 * Length(h0123Data)
    );
    mutable startIdx = 0;

    for idx in IndexRange(ZData) {
        // Array of Arrays of Length 1
        majIdxes w/= idx <- ZTermToPauliMajIdx(HTermToGenIdx(ZData[idx], [0]));
    }

    startIdx = Length(ZData);

    for idx in IndexRange(ZZData) {
        // Array of Arrays of Length 1
        majIdxes w/= startIdx + idx <- ZZTermToPauliMajIdx(HTermToGenIdx(ZZData[idx], [1]));
    }

    startIdx = startIdx + Length(ZZData);

    for idx in IndexRange(PQandPQQRData) {

        // Array of Arrays of Length 1
        majIdxes w/= startIdx + idx <- PQandPQQRTermToPauliMajIdx(HTermToGenIdx(PQandPQQRData[idx], [2]));
    }

    startIdx = startIdx + Length(PQandPQQRData);
    mutable finalIdx = startIdx;

    for idx in 0..Length(h0123Data) - 1 {

        // Array of Arrays of Length up to 4
        let genArr = V0123TermToPauliMajIdx(HTermToGenIdx(h0123Data[idx], [3]));

        for idx0123 in IndexRange(genArr) {
            majIdxes w/= finalIdx <- genArr[idx0123];
            finalIdx = finalIdx + 1;
        }
    }

    mutable oneNorm = 0.0;

    for idx in 0..finalIdx - 1 {
        oneNorm = oneNorm + AbsD(majIdxes[idx].Coefficient);
    }

    let majIdxes = majIdxes[0..finalIdx - 1];
    return new OptimizedBEGeneratorSystem {
        NumTerms = finalIdx,
        Norm = oneNorm,
        SelectTerm = idx -> majIdxes[idx]
    };
}


operation ToJWSelectInput(
    idx : Int,
    optimizedBEGeneratorSystem : OptimizedBEGeneratorSystem,
    signQubit : Qubit,
    selectZControlRegisters : Qubit[],
    optimizedBEControlRegisters : Qubit[],
    pauliBasesIdx : Qubit[],
    indexRegisters : Qubit[][]
) : Unit is Adj + Ctl {
    let optimizedBETermIndex = optimizedBEGeneratorSystem.SelectTerm(idx);

    // Write bit to apply - signQubit
    if optimizedBETermIndex.UseSignQubit {
        X(signQubit);
    }

    // Write bit to activate selectZ operator
    let selectZControlRegistersSet = optimizedBETermIndex.ZControlRegisterMask;
    for i in IndexRange(selectZControlRegistersSet) {
        if selectZControlRegistersSet[i] {
            X(selectZControlRegisters[i]);
        }
    }

    // Write bit to activate OptimizedBEXY operator
    let optimizedBEControlRegistersSet = optimizedBETermIndex.OptimizedControlRegisterMask;
    for i in IndexRange(optimizedBEControlRegistersSet) {
        if optimizedBEControlRegistersSet[i] {
            X(optimizedBEControlRegisters[i]);
        }
    }

    // Write bitstring to apply desired XZ... or YZ... Pauli string
    let indexRegistersSet = optimizedBETermIndex.RegisterIndices;
    for i in IndexRange(indexRegistersSet) {
        ApplyXorInPlace(indexRegistersSet[i], indexRegisters[i]);
    }

    // Crete state to select uniform superposition of X and Y operators.
    let pauliBasesSet = optimizedBETermIndex.PauliBases;
    if Length(pauliBasesSet) == 2 {
        // for PQ or PQQR terms, create |00> + |11>
        ApplyXorInPlace(0, pauliBasesIdx);
    } elif Length(pauliBasesSet) == 4 {
        // for PQRS terms, create |abcd> + |a^ b^ c^ d^>
        if pauliBasesSet[2] == 1 and pauliBasesSet[3] == 1 {
            ApplyXorInPlace(1, pauliBasesIdx);
        } elif pauliBasesSet[2] == 2 and pauliBasesSet[3] == 2 {
            ApplyXorInPlace(2, pauliBasesIdx);
        } elif pauliBasesSet[2] == 1 and pauliBasesSet[3] == 2 {
            ApplyXorInPlace(3, pauliBasesIdx);
        } elif pauliBasesSet[2] == 2 and pauliBasesSet[3] == 1 {
            ApplyXorInPlace(4, pauliBasesIdx);
        }
    }
}

operation ToPauliBases(idx : Int, pauliBases : Qubit[]) : Unit is Adj + Ctl {
    let pauliBasesSet = [[1, 1, 1, 1], [1, 1, 2, 2], [1, 2, 1, 2], [1, 2, 2, 1]];
    H(pauliBases[0]);

    if idx > 0 {
        for idxSet in 1..Length(pauliBasesSet[0]) - 1 {
            if (pauliBasesSet[idx - 1])[idxSet] == 2 {
                X(pauliBases[idxSet]);
            }

            CNOT(pauliBases[0], pauliBases[idxSet]);
        }
    }
}

// This prepares the state that selects _JWSelect_;
operation JWOptimizedBlockEncodingStatePrep(
    targetError : Double,
    optimizedBEGeneratorSystem : OptimizedBEGeneratorSystem,
    qROMIdxRegister : Qubit[],
    qROMGarbage : Qubit[],
    signQubit : Qubit,
    selectZControlRegisters : Qubit[],
    optimizedBEControlRegisters : Qubit[],
    pauliBases : Qubit[],
    pauliBasesIdx : Qubit[],
    indexRegisters : Qubit[][]
) : Unit is Adj + Ctl {

    let coefficients = OptimizedBEGeneratorSystemCoeff(optimizedBEGeneratorSystem);
    let purifiedState = PurifiedMixedState(targetError, coefficients);
    let unitaryGenerator = (
        optimizedBEGeneratorSystem.NumTerms,
        idx -> ToJWSelectInput(idx, optimizedBEGeneratorSystem, _, _, _, _, _)
    );
    let pauliBasesUnitaryGenerator = (5, idx -> (qs => ToPauliBases(idx, qs)));

    purifiedState.Prepare(qROMIdxRegister, [], qROMGarbage);
    MultiplexOperationsFromGenerator(
        unitaryGenerator,
        qROMIdxRegister,
        (signQubit, selectZControlRegisters, optimizedBEControlRegisters, pauliBasesIdx, indexRegisters)
    );
    MultiplexOperationsFromGenerator(pauliBasesUnitaryGenerator, pauliBasesIdx, pauliBases);
}

function JWOptimizedBlockEncodingQubitManager(
    targetError : Double,
    nCoeffs : Int,
    nZ : Int,
    nMaj : Int,
    nIdxRegQubits : Int,
    ctrlRegister : Qubit[]
) : (
    (Qubit[], Qubit[], Qubit, Qubit[], Qubit[], Qubit[], Qubit[], Qubit[][]),
    (Qubit, Qubit[], Qubit[], Qubit[], Qubit[][]),
    Qubit[]
) {

    let requirements = PurifiedMixedStateRequirements(targetError, nCoeffs);
    let parts = Partitioned([requirements.NumIndexQubits, requirements.NumGarbageQubits], ctrlRegister);
    let ((qROMIdx, qROMGarbage), rest0) = ((parts[0], parts[1]), parts[2]);
    let ((
        signQubit,
        selectZControlRegisters,
        optimizedBEControlRegisters,
        pauliBases,
        indexRegisters,
        tmp
    ), rest1) = JWSelectQubitManager(nZ, nMaj, nIdxRegQubits, rest0, []);
    let registers = Partitioned([3], rest1);
    let pauliBasesIdx = registers[0];
    return (
        (qROMIdx, qROMGarbage, signQubit, selectZControlRegisters, optimizedBEControlRegisters, pauliBases, pauliBasesIdx, indexRegisters),
        (signQubit, selectZControlRegisters, optimizedBEControlRegisters, pauliBases, indexRegisters),
        registers[1]
    );
}

function JWOptimizedBlockEncodingQubitCount(
    targetError : Double,
    nCoeffs : Int,
    nZ : Int,
    nMaj : Int,
    nIdxRegQubits : Int,
    nTarget : Int
) : (
    (Int, Int),
    (Int, Int, Int, Int, Int, Int, Int, Int[], Int)
) {

    let (nSelectTotal, (a0, a1, a2, a3, a4)) = JWSelectQubitCount(nZ, nMaj, nIdxRegQubits);
    let requirements = PurifiedMixedStateRequirements(targetError, nCoeffs);
    let pauliBasesIdx = 3;
    return (
        ((nSelectTotal + requirements.NumTotalQubits) + pauliBasesIdx, nTarget),
        (requirements.NumIndexQubits, requirements.NumGarbageQubits, a0, a1, a2, a3, pauliBasesIdx, a4, nTarget)
    );
}


operation JWOptimizedBlockEncodingStatePrepWrapper(
    targetError : Double,
    nCoeffs : Int,
    optimizedBEGeneratorSystem : OptimizedBEGeneratorSystem,
    nZ : Int,
    nMaj : Int,
    nIdxRegQubits : Int,
    ctrlRegister : Qubit[]
) : Unit is Adj + Ctl {

    let (statePrepRegister, _, _) = JWOptimizedBlockEncodingQubitManager(
        targetError,
        nCoeffs,
        nZ,
        nMaj,
        nIdxRegQubits,
        ctrlRegister
    );
    let statePrepOp = JWOptimizedBlockEncodingStatePrep(targetError, optimizedBEGeneratorSystem, _, _, _, _, _, _, _, _);
    statePrepOp(statePrepRegister);
}


operation JWOptimizedBlockEncodingSelect(
    targetError : Double,
    nCoeffs : Int,
    optimizedBEGeneratorSystem : OptimizedBEGeneratorSystem,
    nZ : Int,
    nMaj : Int,
    nIdxRegQubits : Int,
    ctrlRegister : Qubit[],
    targetRegister : Qubit[]
) : Unit is Adj + Ctl {

    let (statePrepRegister, selectRegister, rest) = JWOptimizedBlockEncodingQubitManager(
        targetError,
        nCoeffs,
        nZ,
        nMaj,
        nIdxRegQubits,
        ctrlRegister
    );
    let selectOp = JWSelect(_, _, _, _, _, targetRegister);
    selectOp(selectRegister);
}


function JWOptimizedQuantumWalkByQubitization(
    targetError : Double,
    data : JWOptimizedHTerms,
    nSpinOrbitals : Int
) : ((Int, Int), (Double, ((Qubit[], Qubit[]) => Unit is Adj + Ctl))) {

    let (
        (nCtrlRegisterQubits, nTargetRegisterQubits),
        (oneNorm, blockEncodingReflection)
    ) = JWOptimizedBlockEncoding(targetError, data, nSpinOrbitals);
    return (
        (nCtrlRegisterQubits, nTargetRegisterQubits),
        (oneNorm, QuantumWalkByQubitization(blockEncodingReflection))
    );
}

/// # Summary
/// Encodes an operator of interest into a `BlockEncoding`.
///
/// This constructs a `BlockEncoding` unitary $U=P\cdot V\cdot P^\dagger$ that encodes some
/// operator $H = \sum_{j}|\alpha_j|U_j$ of interest that is a linear combination of
/// unitaries. Typically, $P$ is a state preparation unitary such that
/// $P\ket{0}\_a=\sum_j\sqrt{\alpha_j/\|\vec\alpha\|\_2}\ket{j}\_a$,
/// and $V=\sum_{j}\ket{j}\bra{j}\_a\otimes U_j$.
///
/// # Input
/// ## statePreparation
/// A unitary $P$ that prepares some target state.
/// ## selector
/// A unitary $V$ that encodes the component unitaries of $H$.
///
/// # Output
/// A unitary $U$ acting jointly on registers `a` and `s` that block-
/// encodes $H$, and satisfies $U^\dagger = U$.
///
/// # Remarks
/// This `BlockEncoding` implementation gives it the properties of a
/// `BlockEncodingReflection`.
function BlockEncodingByLCU<'T, 'S>(
    statePreparation : ('T => Unit is Adj + Ctl),
    selector : (('T, 'S) => Unit is Adj + Ctl)
) : (('T, 'S) => Unit is Adj + Ctl) {
    return ApplyBlockEncodingByLCU(statePreparation, selector, _, _);
}

/// # Summary
/// Implementation of `BlockEncodingByLCU`.
operation ApplyBlockEncodingByLCU<'T, 'S>(
    statePreparation : ('T => Unit is Adj + Ctl),
    selector : (('T, 'S) => Unit is Adj + Ctl),
    auxiliary : 'T,
    system : 'S
) : Unit is Adj + Ctl {
    within {
        statePreparation(auxiliary);
    } apply {
        selector(auxiliary, system);
    }
}

/// # Summary
/// Converts a block-encoding reflection into a quantum walk.
///
/// # Description
/// Given a block encoding represented by a unitary $U$
/// that encodes some operator $H$ of interest, converts it into a quantum
/// walk $W$ containing the spectrum of $\pm e^{\pm i\sin^{-1}(H)}$.
///
/// # Input
/// ## blockEncoding
/// A unitary $U$ to be converted into a Quantum
/// walk.
///
/// # Output
/// A quantum walk $W$ acting jointly on registers `a` and `s` that block-
/// encodes $H$, and contains the spectrum of $\pm e^{\pm i\sin^{-1}(H)}$.
///
/// # References
/// - [Hamiltonian Simulation by Qubitization](https://arxiv.org/abs/1610.06546)
///   Guang Hao Low, Isaac L. Chuang
function QuantumWalkByQubitization(
    blockEncoding : (Qubit[], Qubit[]) => Unit is Adj + Ctl
) : ((Qubit[], Qubit[]) => Unit is Adj + Ctl) {
    return ApplyQuantumWalkByQubitization(blockEncoding, _, _);
}

/// # Summary
/// Implementation of `Qubitization`.
operation ApplyQuantumWalkByQubitization(
    blockEncoding : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
    auxiliary : Qubit[],
    system : Qubit[]
) : Unit is Adj + Ctl {
    Exp([PauliI], -0.5 * PI(), [Head(system)]);
    within {
        ApplyToEachCA(X, auxiliary);
    } apply {
        Controlled R1(Rest(auxiliary), (PI(), Head(system)));
    }
    blockEncoding(auxiliary, system);
}

/// # Summary
/// Creates a block-encoding unitary for a Hamiltonian.
///
/// The Hamiltonian $H=\sum_{j}\alpha_j P_j$ is described by a
/// sum of Pauli terms $P_j$, each with real coefficient $\alpha_j$.
///
/// # Input
/// ## generatorSystem
/// A `GeneratorSystem` that describes $H$ as a sum of Pauli terms
///
/// # Output
/// ## First parameter
/// The one-norm of coefficients $\alpha=\sum_{j}|\alpha_j|$.
/// ## Second parameter
/// A block encoding unitary $U$ of the Hamiltonian $H$. As this unitary
/// satisfies $U^2 = I$, it is also a reflection.
///
/// # Remarks
/// This is obtained by preparing and unpreparing the state $\sum_{j}\sqrt{\alpha_j/\alpha}\ket{j}$,
/// and constructing a multiply-controlled unitary `PrepareArbitraryStateD` and `MultiplexOperationsFromGenerator`.
function PauliBlockEncoding(generatorSystem : GeneratorSystem) : (Double, (Qubit[], Qubit[]) => Unit is Adj + Ctl) {
    let multiplexer = unitaryGenerator -> MultiplexOperationsFromGenerator(unitaryGenerator, _, _);
    return PauliBlockEncodingInner(
        generatorSystem,
        coeff -> (qs => PreparePureStateD(coeff, Reversed(qs))),
        multiplexer
    );
}

/// # Summary
/// Creates a block-encoding unitary for a Hamiltonian.
///
/// The Hamiltonian $H=\sum_{j}\alpha_j P_j$ is described by a
/// sum of Pauli terms $P_j$, each with real coefficient $\alpha_j$.
///
/// # Input
/// ## generatorSystem
/// A `GeneratorSystem` that describes $H$ as a sum of Pauli terms
/// ## statePrepUnitary
/// A unitary operation $P$ that prepares $P\ket{0}=\sum_{j}\sqrt{\alpha_j}\ket{j}$ given
/// an array of coefficients $\{\sqrt{\alpha}_j\}$.
/// ## statePrepUnitary
/// A unitary operation $V$ that applies the unitary $V_j$ controlled on index $\ket{j}$,
/// given a function $f: j\rightarrow V_j$.
///
/// # Output
/// ## First parameter
/// The one-norm of coefficients $\alpha=\sum_{j}|\alpha_j|$.
/// ## Second parameter
/// A block encoding unitary $U$ of the Hamiltonian $U$. As this unitary
/// satisfies $U^2 = I$, it is also a reflection.
///
/// # Remarks
/// Example operations the prepare and unpreparing the state $\sum_{j}\sqrt{\alpha_j/\alpha}\ket{j}$,
/// and construct a multiply-controlled unitary are
/// `PrepareArbitraryStateD` and `MultiplexOperationsFromGenerator`.
function PauliBlockEncodingInner(
    generatorSystem : GeneratorSystem,
    statePrepUnitary : (Double[] -> (Qubit[] => Unit is Adj + Ctl)),
    multiplexer : ((Int, (Int -> (Qubit[] => Unit is Adj + Ctl))) -> ((Qubit[], Qubit[]) => Unit is Adj + Ctl))
) : (Double, (Qubit[], Qubit[]) => Unit is Adj + Ctl) {
    let nTerms = generatorSystem.NumEntries;
    let op = idx -> {
        let (_, coeff) = generatorSystem.EntryAt(idx).Term;
        Sqrt(AbsD(coeff[0]))
    };
    let coefficients = MappedOverRange(op, 0..nTerms-1);
    let oneNorm = PNorm(2.0, coefficients)^2.0;
    let unitaryGenerator = (nTerms, idx -> PauliLCUUnitary(generatorSystem.EntryAt(idx)));
    let statePreparation = statePrepUnitary(coefficients);
    let selector = multiplexer(unitaryGenerator);
    let blockEncoding = (qs0, qs1) => BlockEncodingByLCU(statePreparation, selector)(qs0, qs1);
    return (oneNorm, blockEncoding);
}

/// # Summary
/// Used in implementation of `PauliBlockEncoding`
function PauliLCUUnitary(generatorIndex : GeneratorIndex) : (Qubit[] => Unit is Adj + Ctl) {
    return ApplyPauliLCUUnitary(generatorIndex, _);
}

/// # Summary
/// Used in implementation of `PauliBlockEncoding`
operation ApplyPauliLCUUnitary(
    generatorIndex : GeneratorIndex,
    qubits : Qubit[]
) : Unit is Adj + Ctl {
    let (idxPaulis, coeff) = generatorIndex.Term;
    let idxQubits = generatorIndex.Subsystem;
    let paulis = [PauliI, PauliX, PauliY, PauliZ];
    let pauliString = IntArrayAsPauliArray(idxPaulis);
    let pauliQubits = Subarray(idxQubits, qubits);

    ApplyPauli(pauliString, pauliQubits);

    if (coeff[0] < 0.0) {
        // -1 phase
        Exp([PauliI], PI(), [Head(pauliQubits)]);
    }
}

function IntArrayAsPauliArray(arr : Int[]) : Pauli[] {
    let paulis = [PauliI, PauliX, PauliY, PauliZ];
    mutable pauliString = [];
    for idxP in arr {
        pauliString += [paulis[idxP]];
    }
    pauliString
}
