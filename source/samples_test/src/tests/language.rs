// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

// Each file in the samples/language folder is compiled and run as two tests and should
// have matching expect strings in this file. If new samples are added, this file will
// fail to compile until the new expect strings are added.
pub const ARITHMETICOPERATORS_EXPECT: Expect = expect!["()"];
pub const ARITHMETICOPERATORS_EXPECT_DEBUG: Expect = expect!["()"];
pub const ARITHMETICOPERATORS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const ARITHMETICOPERATORS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const ARRAY_EXPECT: Expect = expect![[r#"
    Integer Array: [1, 2, 3, 4] of length 4
    String Array: [a, string, array]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Sliced array: [2, 4]
    Sliced array: [3, 2, 1]
    Sliced array: [1, 2, 3, 4]
    [1, 2, 3, 4]"#]];
pub const ARRAY_EXPECT_DEBUG: Expect = expect![[r#"
    Integer Array: [1, 2, 3, 4] of length 4
    String Array: [a, string, array]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Sliced array: [2, 4]
    Sliced array: [3, 2, 1]
    Sliced array: [1, 2, 3, 4]
    [1, 2, 3, 4]"#]];
pub const ARRAY_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const ARRAY_EXPECT_QIR: Expect = expect!["generated QIR of length 1674"];
pub const BIGINT_EXPECT: Expect = expect![[r#"
    Hexadecimal BigInt: 66
    Octal BigInt: 34
    Decimal BigInt: 42
    Binary BigInt: 42
    Addition result: 43
    Modulo result: 1
    Exponentiation result: 1
    1"#]];
pub const BIGINT_EXPECT_DEBUG: Expect = expect![[r#"
    Hexadecimal BigInt: 66
    Octal BigInt: 34
    Decimal BigInt: 42
    Binary BigInt: 42
    Addition result: 43
    Modulo result: 1
    Exponentiation result: 1
    1"#]];
// BigInt as output is not supported for Adaptive_RIF, so this error is expected.
pub const BIGINT_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const BIGINT_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const BITWISEOPERATORS_EXPECT: Expect = expect![[r#"
    Bitwise NOT: -6
    Bitwise NOT: 4
    Bitwise AND: 4
    Bitwise AND: 2
    Bitwise OR: 7
    Bitwise OR: -1
    Bitwise XOR: 3
    Bitwise XOR: -3
    Right Bit-shift: 1
    Right Bit-shift: -2
    Right Bit-shift: 20
    Left Bit-shift: 20
    Left Bit-shift: -20
    Left Bit-shift: 1
    ()"#]];
pub const BITWISEOPERATORS_EXPECT_DEBUG: Expect = expect![[r#"
    Bitwise NOT: -6
    Bitwise NOT: 4
    Bitwise AND: 4
    Bitwise AND: 2
    Bitwise OR: 7
    Bitwise OR: -1
    Bitwise XOR: 3
    Bitwise XOR: -3
    Right Bit-shift: 1
    Right Bit-shift: -2
    Right Bit-shift: 20
    Left Bit-shift: 20
    Left Bit-shift: -20
    Left Bit-shift: 1
    ()"#]];
pub const BITWISEOPERATORS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const BITWISEOPERATORS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const BOOL_EXPECT: Expect = expect![[r#"
    AND operation: true
    OR operation: true
    Equality comparison: false
    2 equals 2
    true"#]];
pub const BOOL_EXPECT_DEBUG: Expect = expect![[r#"
    AND operation: true
    OR operation: true
    Equality comparison: false
    2 equals 2
    true"#]];
pub const BOOL_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const BOOL_EXPECT_QIR: Expect = expect!["generated QIR of length 959"];
pub const COMMENTS_EXPECT: Expect = expect!["[]"];
pub const COMMENTS_EXPECT_DEBUG: Expect = expect!["[]"];
pub const COMMENTS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const COMMENTS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const COMPARISONOPERATORS_EXPECT: Expect = expect![[r#"
    Equality comparison: true
    Equality comparison: false
    Inequality comparison: false
    Inequality comparison: true
    Less than comparison: false
    Less than comparison: true
    Less than comparison: false
    Less than or equal comparison: true
    Less than or equal comparison: true
    Less than or equal comparison: false
    Greater than comparison: false
    Greater than comparison: false
    Greater than comparison: true
    Greater than or equal comparison: true
    Greater than or equal comparison: false
    Greater than or equal comparison: true
    ()"#]];
pub const COMPARISONOPERATORS_EXPECT_DEBUG: Expect = expect![[r#"
    Equality comparison: true
    Equality comparison: false
    Inequality comparison: false
    Inequality comparison: true
    Less than comparison: false
    Less than comparison: true
    Less than comparison: false
    Less than or equal comparison: true
    Less than or equal comparison: true
    Less than or equal comparison: false
    Greater than comparison: false
    Greater than comparison: false
    Greater than comparison: true
    Greater than or equal comparison: true
    Greater than or equal comparison: false
    Greater than or equal comparison: true
    ()"#]];
pub const COMPARISONOPERATORS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const COMPARISONOPERATORS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const CONDITIONALBRANCHING_EXPECT: Expect = expect![[r#"
    Buzz
    It is livable
    Absolute value of -40 is 40
    ()"#]];
pub const CONDITIONALBRANCHING_EXPECT_DEBUG: Expect = expect![[r#"
    Buzz
    It is livable
    Absolute value of -40 is 40
    ()"#]];
pub const CONDITIONALBRANCHING_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const CONDITIONALBRANCHING_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const COPYANDUPDATEOPERATOR_EXPECT: Expect = expect![[r#"
    Updated array: [10, 11, 100, 13]
    Updated array: [10, 100, 12, 200]
    ()"#]];
pub const COPYANDUPDATEOPERATOR_EXPECT_DEBUG: Expect = expect![[r#"
    Updated array: [10, 11, 100, 13]
    Updated array: [10, 100, 12, 200]
    ()"#]];
pub const COPYANDUPDATEOPERATOR_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const COPYANDUPDATEOPERATOR_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const CUSTOMMEASUREMENTS_EXPECT: Expect = expect!["Zero"];
pub const CUSTOMMEASUREMENTS_EXPECT_DEBUG: Expect = expect!["Zero"];
// SimulatableIntrinsic, custom measurements are not expected to work in the circuit generation.
pub const CUSTOMMEASUREMENTS_EXPECT_CIRCUIT: Expect = expect!["circuit error: circuit error"];
pub const CUSTOMMEASUREMENTS_EXPECT_QIR: Expect = expect!["generated QIR of length 1297"];
pub const DATATYPES_EXPECT: Expect = expect![[r#"
    Binary BigInt: 42
    Octal BigInt: 42
    Decimal BigInt: 42
    Hexadecimal BigInt: 42
    Complex: (real: 42.0, imaginary: 0.0)
    ()"#]];
pub const DATATYPES_EXPECT_DEBUG: Expect = expect![[r#"
    Binary BigInt: 42
    Octal BigInt: 42
    Decimal BigInt: 42
    Hexadecimal BigInt: 42
    Complex: (real: 42.0, imaginary: 0.0)
    ()"#]];
pub const DATATYPES_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 4"];
pub const DATATYPES_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const DIAGNOSTICS_EXPECT: Expect = expect![[r#"
    Program is starting.
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    ()"#]];
pub const DIAGNOSTICS_EXPECT_DEBUG: Expect = expect![[r#"
    Program is starting.
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    ()"#]];
pub const DIAGNOSTICS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 215"];
pub const DIAGNOSTICS_EXPECT_QIR: Expect = expect!["generated QIR of length 1463"];
pub const DOUBLE_EXPECT: Expect = expect!["0.1973269804"];
pub const DOUBLE_EXPECT_DEBUG: Expect = expect!["0.1973269804"];
pub const DOUBLE_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const DOUBLE_EXPECT_QIR: Expect = expect!["generated QIR of length 979"];
pub const ENTRYPOINT_EXPECT: Expect = expect!["[]"];
pub const ENTRYPOINT_EXPECT_DEBUG: Expect = expect!["[]"];
pub const ENTRYPOINT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const ENTRYPOINT_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const FAILSTATEMENT_EXPECT: Expect = expect!["()"];
pub const FAILSTATEMENT_EXPECT_DEBUG: Expect = expect!["()"];
// Fail statements are expected to cause a circuit generation error since they cannot be executed.
pub const FAILSTATEMENT_EXPECT_CIRCUIT: Expect = expect!["circuit error: partial evaluation error"];
pub const FAILSTATEMENT_EXPECT_QIR: Expect =
    expect!["QIR generation error for `FailStatement.Main()`: partial evaluation error"];
pub const FORLOOPS_EXPECT: Expect = expect!["()"];
pub const FORLOOPS_EXPECT_DEBUG: Expect = expect!["()"];
pub const FORLOOPS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const FORLOOPS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const FUNCTIONS_EXPECT: Expect = expect!["()"];
pub const FUNCTIONS_EXPECT_DEBUG: Expect = expect!["()"];
pub const FUNCTIONS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const FUNCTIONS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const GETTINGSTARTED_EXPECT: Expect = expect!["()"];
pub const GETTINGSTARTED_EXPECT_DEBUG: Expect = expect!["()"];
pub const GETTINGSTARTED_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const GETTINGSTARTED_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const INT_EXPECT: Expect = expect![[r#"
    Hexadecimal: 66
    Octal: 34
    Decimal: 42
    Binary: 42
    After addition: 43
    After modulo: 1
    After exponentiation: 1
    1"#]];
pub const INT_EXPECT_DEBUG: Expect = expect![[r#"
    Hexadecimal: 66
    Octal: 34
    Decimal: 42
    Binary: 42
    After addition: 43
    After modulo: 1
    After exponentiation: 1
    1"#]];
pub const INT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const INT_EXPECT_QIR: Expect = expect!["generated QIR of length 956"];
pub const LAMBDAEXPRESSION_EXPECT: Expect = expect![[r#"
    Lambda add function result: 5
    Sum of array using Fold: 15
    Array after incrementing each element using Map: [2, 3, 4, 5, 6]
    ()"#]];
pub const LAMBDAEXPRESSION_EXPECT_DEBUG: Expect = expect![[r#"
    Lambda add function result: 5
    Sum of array using Fold: 15
    Array after incrementing each element using Map: [2, 3, 4, 5, 6]
    ()"#]];
pub const LAMBDAEXPRESSION_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 4"];
pub const LAMBDAEXPRESSION_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const LOGICALOPERATORS_EXPECT: Expect = expect!["()"];
pub const LOGICALOPERATORS_EXPECT_DEBUG: Expect = expect!["()"];
pub const LOGICALOPERATORS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const LOGICALOPERATORS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const NAMESPACES_EXPECT: Expect = expect![[r#"
    STATE:
    No qubits allocated
    []"#]];
pub const NAMESPACES_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    No qubits allocated
    []"#]];
pub const NAMESPACES_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const NAMESPACES_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const OPERATIONS_EXPECT: Expect = expect![[r#"
    Measurement result: Zero
    Zero"#]];
pub const OPERATIONS_EXPECT_DEBUG: Expect = expect![[r#"
    Measurement result: Zero
    Zero"#]];
pub const OPERATIONS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 187"];
pub const OPERATIONS_EXPECT_QIR: Expect = expect!["generated QIR of length 1428"];
pub const PARTIALAPPLICATION_EXPECT: Expect = expect![[r#"
    five = incrementByOne(4) => 5
    Incremented array: [2, 3, 4, 5, 6]
    ()"#]];
pub const PARTIALAPPLICATION_EXPECT_DEBUG: Expect = expect![[r#"
    five = incrementByOne(4) => 5
    Incremented array: [2, 3, 4, 5, 6]
    ()"#]];
pub const PARTIALAPPLICATION_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const PARTIALAPPLICATION_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const PAULI_EXPECT: Expect = expect![[r#"
    Pauli dimension: PauliX
    Measurement result: Zero
    Zero"#]];
pub const PAULI_EXPECT_DEBUG: Expect = expect![[r#"
    Pauli dimension: PauliX
    Measurement result: Zero
    Zero"#]];
pub const PAULI_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 223"];
pub const PAULI_EXPECT_QIR: Expect = expect!["generated QIR of length 1502"];
pub const QUANTUMMEMORY_EXPECT: Expect = expect!["()"];
pub const QUANTUMMEMORY_EXPECT_DEBUG: Expect = expect!["()"];
pub const QUANTUMMEMORY_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 40"];
pub const QUANTUMMEMORY_EXPECT_QIR: Expect = expect!["generated QIR of length 961"];
pub const MEMORYQUBIT_EXPECT: Expect = expect!["One"];
pub const MEMORYQUBIT_EXPECT_DEBUG: Expect = expect!["One"];
pub const MEMORYQUBIT_EXPECT_CIRCUIT: Expect =
    expect!["generated circuit of length 434"];
pub const MEMORYQUBIT_EXPECT_QIR: Expect =
    expect!["generated QIR of length 1806"];
pub const QUBIT_EXPECT: Expect = expect![[r#"
    STATE:
    |1000⟩: 0.0000+0.5000𝑖
    |1010⟩: 0.0000+0.5000𝑖
    |1100⟩: 0.0000+0.5000𝑖
    |1110⟩: 0.0000+0.5000𝑖
    ()"#]];
pub const QUBIT_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |1000⟩: 0.0000+0.5000𝑖
    |1010⟩: 0.0000+0.5000𝑖
    |1100⟩: 0.0000+0.5000𝑖
    |1110⟩: 0.0000+0.5000𝑖
    ()"#]];
pub const QUBIT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 449"];
pub const QUBIT_EXPECT_QIR: Expect = expect!["generated QIR of length 1819"];
pub const RANGE_EXPECT: Expect = expect![[r#"
    Range: 1..3
    Range: 2..2..5
    Range: 2..2..6
    Range: 6..-2..2
    Range: 2..-2..2
    Range: 2..1
    Array: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [0..2..10]: [0, 4, 16, 36, 64, 100]
    Array slice [...4]: [0, 1, 4, 9, 16]
    Array slice [5...]: [25, 36, 49, 64, 81, 100]
    Array slice [2..3...]: [4, 25, 64]
    Array slice [...3..7]: [0, 9, 36]
    Array slice [...]: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [...-3...]: [100, 49, 16, 1]
    2..1"#]];
pub const RANGE_EXPECT_DEBUG: Expect = expect![[r#"
    Range: 1..3
    Range: 2..2..5
    Range: 2..2..6
    Range: 6..-2..2
    Range: 2..-2..2
    Range: 2..1
    Array: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [0..2..10]: [0, 4, 16, 36, 64, 100]
    Array slice [...4]: [0, 1, 4, 9, 16]
    Array slice [5...]: [25, 36, 49, 64, 81, 100]
    Array slice [2..3...]: [4, 25, 64]
    Array slice [...3..7]: [0, 9, 36]
    Array slice [...]: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [...-3...]: [100, 49, 16, 1]
    2..1"#]];
// Ranges cannot be part of program output in Adaptive_RIF, so this error is expected.
pub const RANGE_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const RANGE_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const REPEATUNTILLOOPS_EXPECT: Expect = expect!["()"];
pub const REPEATUNTILLOOPS_EXPECT_DEBUG: Expect = expect!["()"];
// RUS Loops cannot be compiled in Adaptive_RIF, so this error is expected.
pub const REPEATUNTILLOOPS_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot have a loop with a dynamic condition"];
pub const REPEATUNTILLOOPS_EXPECT_QIR: Expect =
    expect!["compilation error: cannot have a loop with a dynamic condition"];
pub const RESULT_EXPECT: Expect = expect![[r#"
    Measurement: Zero
    Zero"#]];
pub const RESULT_EXPECT_DEBUG: Expect = expect![[r#"
    Measurement: Zero
    Zero"#]];
pub const RESULT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 187"];
pub const RESULT_EXPECT_QIR: Expect = expect!["generated QIR of length 1428"];
pub const RETURNSTATEMENT_EXPECT: Expect = expect!["()"];
pub const RETURNSTATEMENT_EXPECT_DEBUG: Expect = expect!["()"];
pub const RETURNSTATEMENT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const RETURNSTATEMENT_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const SPECIALIZATIONS_EXPECT: Expect = expect!["()"];
pub const SPECIALIZATIONS_EXPECT_DEBUG: Expect = expect!["()"];
pub const SPECIALIZATIONS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 4540"];
pub const SPECIALIZATIONS_EXPECT_QIR: Expect = expect!["generated QIR of length 3106"];
pub const STRING_EXPECT: Expect = expect![[r#"
    FooBar
    interpolated: FooBar
    interpolated: FooBar"#]];
pub const STRING_EXPECT_DEBUG: Expect = expect![[r#"
    FooBar
    interpolated: FooBar
    interpolated: FooBar"#]];
// Strings as output are not supported for Adaptive_RIF, so this error is expected.
pub const STRING_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const STRING_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const TERNARY_EXPECT: Expect = expect![[r#"
    Absolute value: 40
    ()"#]];
pub const TERNARY_EXPECT_DEBUG: Expect = expect![[r#"
    Absolute value: 40
    ()"#]];
pub const TERNARY_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const TERNARY_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const TUPLE_EXPECT: Expect = expect![[r#"
    Tuple: (Id, 0, 1.0)
    Unpacked: Id, 0, 1.0
    Name: Id
    Item: 0
    myTuple: (0,)
    Tuple: (PauliX, (3, 1))
    Unpacked: PauliX, 3, 1
    Inner tuple: (3, 1)
    (0, Foo)"#]];
pub const TUPLE_EXPECT_DEBUG: Expect = expect![[r#"
    Tuple: (Id, 0, 1.0)
    Unpacked: Id, 0, 1.0
    Name: Id
    Item: 0
    myTuple: (0,)
    Tuple: (PauliX, (3, 1))
    Unpacked: PauliX, 3, 1
    Inner tuple: (3, 1)
    (0, Foo)"#]];
// Tuple with a string as output is not supported for Adaptive_RIF, so this error is expected.
pub const TUPLE_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const TUPLE_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const TYPEDECLARATIONS_EXPECT: Expect = expect!["()"];
pub const TYPEDECLARATIONS_EXPECT_DEBUG: Expect = expect!["()"];
pub const TYPEDECLARATIONS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const TYPEDECLARATIONS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const UNIT_EXPECT: Expect = expect!["()"];
pub const UNIT_EXPECT_DEBUG: Expect = expect!["()"];
pub const UNIT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const UNIT_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const VARIABLES_EXPECT: Expect = expect![[r#"
    Immutable Int: 42
    Mutable Int: 43
    Mutable Int after mutation: 42
    Mutable Int after reassignment: 10
    Shadowed Immutable Int: 0
    ()"#]];
pub const VARIABLES_EXPECT_DEBUG: Expect = expect![[r#"
    Immutable Int: 42
    Mutable Int: 43
    Mutable Int after mutation: 42
    Mutable Int after reassignment: 10
    Shadowed Immutable Int: 0
    ()"#]];
pub const VARIABLES_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const VARIABLES_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const WHILELOOPS_EXPECT: Expect = expect!["()"];
pub const WHILELOOPS_EXPECT_DEBUG: Expect = expect!["()"];
pub const WHILELOOPS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const WHILELOOPS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const WITHINAPPLY_EXPECT: Expect = expect!["()"];
pub const WITHINAPPLY_EXPECT_DEBUG: Expect = expect!["()"];
pub const WITHINAPPLY_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 87"];
pub const WITHINAPPLY_EXPECT_QIR: Expect = expect!["generated QIR of length 1278"];
pub const CLASSCONSTRAINTS_EXPECT: Expect = expect![[r#"
    true
    false
    false
    false
    false
    true
    ()"#]];
pub const CLASSCONSTRAINTS_EXPECT_DEBUG: Expect = expect![[r#"
    true
    false
    false
    false
    false
    true
    ()"#]];
pub const CLASSCONSTRAINTS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const CLASSCONSTRAINTS_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
pub const TESTATTRIBUTE_EXPECT: Expect = expect!["()"];
pub const TESTATTRIBUTE_EXPECT_DEBUG: Expect = expect!["()"];
pub const TESTATTRIBUTE_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 0"];
pub const TESTATTRIBUTE_EXPECT_QIR: Expect = expect!["generated QIR of length 960"];
