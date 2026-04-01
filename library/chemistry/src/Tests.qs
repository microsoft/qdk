// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arrays.IndexRange;
import Std.Arrays.Mapped;
import Std.Arrays.Reversed;
import Std.Convert.ComplexAsComplexPolar;
import Std.Convert.IntAsDouble;
import Std.Diagnostics.*;
import Std.Math.*;
import Std.StatePreparation.ApproximatelyPreparePureStateCP;

import JordanWigner.ClusterOperatorEvolutionSet.JWClusterOperatorPQRSTermSigns;
import JordanWigner.ClusterOperatorEvolutionSet.ComputeJWBitString;
import JordanWigner.ClusterOperatorEvolutionSet.ComputeJWPauliZString;
import JordanWigner.OptimizedBEOperator.OptimizedBEXY;
import JordanWigner.OptimizedBEOperator.SelectZ;
import JordanWigner.StatePreparation.PrepareSparseMultiConfigurationalState;
import JordanWigner.StatePreparation.PrepareUnitaryCoupledClusterState;
import JordanWigner.Data.JWInputState;

@Config(Unrestricted)
@Test()
operation PrepareSparseMultiConfigurationalState0Test() : Unit {
    let nQubits = 6;
    let expectedResult = 39;
    let excitations = [
        NewJordanWignerInputState(0.1, 0.0, [0, 1, 2, 5])
    ];

    use qubits = Qubit[nQubits];
    PrepareSparseMultiConfigurationalState(qs => I(qs[0]), excitations, qubits);

    Fact(MeasureInteger(qubits) == expectedResult, "PrepareSparseMultiConfigurationalState0Test failed.");
}

@Config(Unrestricted)
operation OptimizedBEOperatorZeroTestHelper(pauliBasis : Pauli, targetRegisterSize : Int, targetIndex : Int) : Unit {
    let indexRegisterSize = Ceiling(Lg(IntAsDouble(targetRegisterSize)));
    use pauliBasisQubit = Qubit();
    use indexRegister = Qubit[indexRegisterSize];
    use targetRegister = Qubit[targetRegisterSize];

    // Choose X or Y operator.
    if pauliBasis == PauliX {
        // no op
    } elif pauliBasis == PauliY {
        X(pauliBasisQubit);
    }

    // Create indexRegister state.
    ApplyXorInPlace(targetIndex, indexRegister);

    // Initialize targetRegister states in |0>
    OptimizedBEXY(pauliBasisQubit, indexRegister, targetRegister);

    for idxTest in 0..targetRegisterSize - 1 {
        let testQubit = targetRegister[idxTest];

        if targetIndex > idxTest {

            // Test Z Pauli
            // |0> -> |0>
            // |+> -> |->
            Message($"Test Z Pauli on qubit {idxTest}");
            Fact(CheckZero(testQubit), $"Error: Test {idxTest} {idxTest} Z Pauli |0>");
        } elif targetIndex == idxTest {

            // Test X Pauli
            // |0> -> |1>
            // |+> -> |+>

            // Test Y Pauli
            // |0> -> i|1>
            // |+> -> -i|->
            Message($"Test X or Y Pauli on qubit {idxTest}");
            within {
                X(testQubit);
            } apply {
                Fact(CheckZero(testQubit), $"Error: Test {idxTest} X or Y Pauli |0>");
            }
        } else {

            // Test Identify Pauli
            // |0> -> |0>
            // |+> -> |+>
            Message($"Test ZI Pauli on qubit {idxTest}");
            Fact(CheckZero(testQubit), $"Error: Test {idxTest} I Pauli |0>");
        }
    }

    OptimizedBEXY(pauliBasisQubit, indexRegister, targetRegister);
    Adjoint ApplyXorInPlace(targetIndex, indexRegister);

    // Choose X or Y operator.
    if pauliBasis == PauliX {
        // no op
    } elif pauliBasis == PauliY {
        X(pauliBasisQubit);
    }
}

@Config(Unrestricted)
@Test()
operation OptimizedBEOperatorZeroTest() : Unit {
    let paulis = [PauliX, PauliY];
    let targetRegisterSize = 7;

    for idxPauli in 0..1 {
        let pauliBasis = paulis[idxPauli];

        for targetIndex in 0..targetRegisterSize - 1 {
            Message($"pauliBasis {pauliBasis}; targetIndex {targetIndex}");
            OptimizedBEOperatorZeroTestHelper(pauliBasis, targetRegisterSize, targetIndex);
        }
    }
}

@Config(Unrestricted)
operation OptimizedBEOperatorPlusTestHelper(pauliBasis : Pauli, targetRegisterSize : Int, targetIndex : Int) : Unit {
    let indexRegisterSize = Ceiling(Lg(IntAsDouble(targetRegisterSize)));
    use pauliBasisQubit = Qubit();
    use indexRegister = Qubit[indexRegisterSize];
    use targetRegister = Qubit[targetRegisterSize];
    // Choose X or Y operator.
    if (pauliBasis == PauliX) {
        // no op
    } elif pauliBasis == PauliY {
        X(pauliBasisQubit);
    }

    // Create indexRegister state.
    ApplyXorInPlace(targetIndex, indexRegister);

    // Initialize targetRegister states in |+>
    ApplyToEachCA(H, targetRegister);
    OptimizedBEXY(pauliBasisQubit, indexRegister, targetRegister);
    for idxTest in 0..targetRegisterSize - 1 {
        let testQubit = targetRegister[idxTest];
        if (targetIndex > idxTest) {
            // Test Z Pauli
            // |0> -> |0>
            // |+> -> |->
            Message($"Test Z Pauli on qubit {idxTest}");
            within {
                H(testQubit);
                X(testQubit);
            } apply {
                Fact(CheckZero(testQubit), $"Error: Test {idxTest} Z Pauli |->");
            }
        } elif (targetIndex == idxTest) {
            // Test X Pauli
            // |0> -> |1>
            // |+> -> |+>
            if (pauliBasis == PauliX) {
                Message($"Test X Pauli on qubit {idxTest}");
                within {
                    H(testQubit);
                } apply {
                    Fact(CheckZero(testQubit), $"Error: Test {idxTest} X Pauli |+>");
                }
            }

            // Test Y Pauli
            // |0> -> i|1>
            // |+> -> -i|->
            if (pauliBasis == PauliY) {
                Message($"Test Y Pauli on qubit {idxTest}");
                within {
                    H(testQubit);
                    X(testQubit);
                } apply {
                    Fact(CheckZero(testQubit), $"Error: Test {idxTest} Y Pauli |+>");
                }
            }

        } else {
            // Test Identify Pauli
            // |0> -> |0>
            // |+> -> |+>
            Message($"Test I Pauli on qubit {idxTest}");
            within {
                H(testQubit);
            } apply {
                Fact(CheckZero(testQubit), $"Error: Test {idxTest} I Pauli |+>");
            }
        }
    }
    OptimizedBEXY(pauliBasisQubit, indexRegister, targetRegister);
    ApplyToEachCA(H, targetRegister);

    (Adjoint ApplyXorInPlace)(targetIndex, indexRegister);

    // Choose X or Y operator.
    if pauliBasis == PauliX {
        // no op
    } elif pauliBasis == PauliY {
        X(pauliBasisQubit);
    }
}

@Config(Unrestricted)
@Test()
operation OptimizedBEOperatorPlusTest() : Unit {

    let paulis = [PauliX, PauliY];
    let targetRegisterSize = 7;

    for idxPauli in 0..1 {
        let pauliBasis = paulis[idxPauli];

        for targetIndex in 0..targetRegisterSize - 1 {
            Message($"pauliBasis {pauliBasis}; targetIndex {targetIndex}");
            OptimizedBEOperatorPlusTestHelper(pauliBasis, targetRegisterSize, targetIndex);
        }
    }
}

@Config(Unrestricted)
@Test()
operation SelectZTest() : Unit {
    let targetRegisterSize = 7;
    let indexRegisterSize = Ceiling(Lg(IntAsDouble(targetRegisterSize)));

    use targetRegister = Qubit[targetRegisterSize];
    use indexRegister = Qubit[indexRegisterSize];
    for idxTest in 0..targetRegisterSize - 1 {
        H(targetRegister[idxTest]);
        ApplyXorInPlace(idxTest, indexRegister);
        SelectZ(indexRegister, targetRegister);
        within {
            H(targetRegister[idxTest]);
            X(targetRegister[idxTest]);
        } apply {
            Fact(CheckZero(targetRegister[idxTest]), $"Error: Test {idxTest} X Pauli |+>");
        }
        Z(targetRegister[idxTest]);
        Adjoint ApplyXorInPlace(idxTest, indexRegister);
        H(targetRegister[idxTest]);
    }
}

@Config(Unrestricted)
function JordanWignerClusterOperatorPQRSTermSignsTestHelper(idx : Int) : (Int[], Double[], Double) {
    let cases = [
        ([1, 2, 3, 4], [1.0,-1.0,-1.0,-1.0, 1.0, 1.0, 1.0,-1.0], 1.0),
        ([2, 1, 4, 3], [1.0,-1.0,-1.0,-1.0, 1.0, 1.0, 1.0,-1.0], 1.0),
        ([3, 4, 1, 2], [1.0,-1.0,-1.0,-1.0, 1.0, 1.0, 1.0,-1.0],-1.0),
        ([2, 1, 3, 4], [1.0,-1.0,-1.0,-1.0, 1.0, 1.0, 1.0,-1.0],-1.0),
        ([1, 3, 2, 4], [-1.0,-1.0,-1.0, 1.0,-1.0, 1.0, 1.0, 1.0], 1.0),
        ([4, 2, 3, 1], [-1.0,-1.0,-1.0, 1.0,-1.0, 1.0, 1.0, 1.0],-1.0),
        ([1, 4, 2, 3], [1.0, 1.0,-1.0, 1.0,-1.0, 1.0,-1.0,-1.0], 1.0),
        ([2, 3, 4, 1], [1.0, 1.0,-1.0, 1.0,-1.0, 1.0,-1.0,-1.0], 1.0)
    ];
    return cases[idx];
}

@Config(Unrestricted)
@Test()
function JordanWignerClusterOperatorPQRSTermSignsTest() : Unit {
    for idx in 0..7 {
        let (testCase, expectedSigns, expectedGlobalSign) = JordanWignerClusterOperatorPQRSTermSignsTestHelper(idx);
        let (sortedIndices, signs, globalSign) = JWClusterOperatorPQRSTermSigns(testCase);

        let p = sortedIndices[0];
        let q = sortedIndices[1];
        let r = sortedIndices[2];
        let s = sortedIndices[3];

        Fact(p < q and q < r and r < s, "Expected p < q < r < s.");
        NearEqualityFactD(globalSign, expectedGlobalSign);
        for i in IndexRange(signs) {
            NearEqualityFactD(signs[i], expectedSigns[i]);
        }
    }
}

@Config(Unrestricted)
function DoublesToComplexPolar(input : Double[]) : ComplexPolar[] {
    Std.Arrays.Mapped(re -> ComplexAsComplexPolar(new Complex { Real = re, Imag = 0.0 }), input)
}

@Config(Unrestricted)
operation JordanWignerUCCTermTestHelper(nQubits : Int, excitations : Int[], term : JWInputState[], result : Double[]) : Unit {
    use qubits = Qubit[nQubits];
    for idx in excitations {
        X(qubits[idx]);
    }
    PrepareUnitaryCoupledClusterState(qs => I(qs[0]), term, 1.0, qubits);
    DumpRegister(qubits);
    Adjoint ApproximatelyPreparePureStateCP(0.0, DoublesToComplexPolar(result), Reversed(qubits));
    Fact(CheckAllZero(qubits), "Expected qubits to all be in ground state.");
    ResetAll(qubits);
}

@Config(Unrestricted)
@Test()
operation JordanWignerUCCSTermTest() : Unit {
    // test using Exp(2.0 (a^\dag_1 a_3 - h.c.))
    let term0 = [NewJordanWignerInputState(2.0, 0.0, [1, 3])];
    let state0 = [0., 0.,-0.416147, 0., 0., 0., 0., 0.,-0.909297, 0., 0., 0., 0., 0., 0., 0.];
    JordanWignerUCCTermTestHelper(4, [1], term0, state0);

    // test using Exp(2.0 (a^\dag_3 a_1 - h.c.))
    let term1 = [NewJordanWignerInputState(2.0, 0.0, [3, 1])];
    let state1 = [0., 0.,-0.416147, 0., 0., 0., 0., 0., 0.909297, 0., 0., 0., 0., 0., 0., 0.];
    JordanWignerUCCTermTestHelper(4, [1], term1, state1);
}

@Config(Unrestricted)
@Test()
operation JordanWignerUCCDTermPQRSTest() : Unit {
    // test using Exp(2.0 (a^\dag_0 a^\dag_1 a_3 a_4 - h.c.))
    let term0 = [NewJordanWignerInputState(2.0, 0.0, [0, 1, 2, 4])];
    let state0 = [0., 0., 0.,-0.416147, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.909297, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.];
    JordanWignerUCCTermTestHelper(5, [0, 1], term0, state0);

    // test using Exp(2.0 (a^\dag_0 a^\dag_1 a_3 a_4 - h.c.))
    let term1 = [NewJordanWignerInputState(2.0, 0.0, [0, 1, 2, 4])];
    let state1 = [0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,-0.416147, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,-0.909297, 0., 0., 0.];
    JordanWignerUCCTermTestHelper(5, [0, 1, 3], term1, state1);

    // test using Exp(2.0 (a^\dag_1 a^\dag_0 a_2 a_4 - h.c.))
    let term2 = [NewJordanWignerInputState(2.0, 0.0, [1, 0, 2, 4])];
    let state2 = [0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,-0.416147, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.909297, 0., 0., 0.];
    JordanWignerUCCTermTestHelper(5, [0, 1, 3], term2, state2);

    // test using Exp(2.0 (a^\dag_1 a^\dag_0 a_2 a_4 - h.c.))
    let term3 = [NewJordanWignerInputState(-2.0, 0.0, [4, 2, 1, 0])];
    let state3 = [0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,-0.416147, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.909297, 0., 0., 0.];
    JordanWignerUCCTermTestHelper(5, [0, 1, 3], term2, state2);
}

@Config(Unrestricted)
// @Test()
operation JordanWignerUCCDTermPRQSTest() : Unit {
    let term0 = [NewJordanWignerInputState(2.0, 0.0, [2, 0, 4, 1])];
    let state0 = [0., 0., 0., 0., 0.,-0.416147, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.909297, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.];
    JordanWignerUCCTermTestHelper(5, [0, 2], term0, state0);

    let term1 = [NewJordanWignerInputState(2.0, 0.0, [2, 0, 4, 1])];
    let state1 = [0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,-0.416147, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,-0.909297, 0., 0., 0., 0., 0.];
    JordanWignerUCCTermTestHelper(5, [0, 2, 3], term1, state1);
}

@Config(Unrestricted)
@Test()
operation JordanWignerUCCDTermPRSQTest() : Unit {
    let term3 = [NewJordanWignerInputState(2.0, 0.0, [0, 4, 2, 3])];
    let state3 = [0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.909297, 0., 0., 0., 0.,-0.416147, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.];
    JordanWignerUCCTermTestHelper(5, [0, 4], term3, state3);
}

@Config(Unrestricted)
@Test()
function ComputeJordanWignerBitString_0Test() : Unit {
    let nFermions = 5;
    let fermionIndices = [0, 3];
    let expectedBitString = [false, true, true, false, false];
    let bitString = ComputeJWBitString(nFermions, fermionIndices);
    Fact(bitString == expectedBitString, "Bit strings not equal");
}

@Config(Unrestricted)
@Test()
function ComputeJordanWignerBitString_1Test() : Unit {
    let nFermions = 7;
    let fermionIndices = [0, 4, 2, 6];
    let expectedBitString = [false, true, false, false, false, true, false];
    let bitString = ComputeJWBitString(nFermions, fermionIndices);
    Fact(bitString == expectedBitString, "Bit strings not equal");
}

@Config(Unrestricted)
@Test()
function ComputeJordanWignerPauliZString_0Test() : Unit {
    let nFermions = 7;
    let fermionIndices = [0, 4, 2, 6];
    let expectedBitString = [PauliI, PauliZ, PauliI, PauliI, PauliI, PauliZ, PauliI];
    let bitString = ComputeJWPauliZString(nFermions, fermionIndices);
    Fact(bitString == expectedBitString, "Bit strings not equal");
}


@Config(Unrestricted)
function NearEqualityFactD(actual : Double, expected : Double) : Unit {
    let tolerance = 1e-10;
    let delta = actual - expected;
    if (delta > tolerance or delta < -tolerance) {
        fail $"Values were not equal within tolerance\nActual: {actual}, Expected: {expected}, Tolerance: {tolerance}";
    }
}

function NewJordanWignerInputState(re : Double, im : Double, indices : Int[]) : JWInputState {
    new JWInputState {
        Amplitude = new Complex { Real = re, Imag = im },
        FermionIndices = indices
    }
}
