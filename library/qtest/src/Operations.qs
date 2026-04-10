// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

import Util.TestCaseResult, Util.OutputMessage;
import Std.Arrays.Mapped, Std.Arrays.All, Std.Arrays.Enumerated;

/// # Summary
/// Runs a number of test cases and returns true if all tests passed, false otherwise.
/// Prints a report of what passed and what failed as output.
///
/// For a more flexible test running function, see `RunAllTestCases` which returns
/// test results instead of printing out to output.
///
/// # Input
/// Takes a list of test cases. A test case is a tuple of `(String, Int, Qubit[] => Unit, Qubit[] => 'T, 'T)`, where
/// the first String is the name of the test, the int is the number of qubits to allocate for this test,
/// the first function is a qubit state prep function to be run before the test, the second function is the test case itself, and the
/// final element of the tuple is the expected return value from the test case.
///
/// # Example
/// ```qsharp
/// CheckAllTestCases([("0b0001 == 1", 4, (qs) => X(qs[0]), (qs) => MeasureSignedInteger(qs, 4), 1)]);
/// ```
operation CheckAllTestCases<'T : Eq + Show>(test_cases : (String, Int, Qubit[] => (), Qubit[] => 'T, 'T)[]) : Bool {
    let test_results = RunAllTestCases(test_cases);

    OutputMessage(test_results);

    All(test_case -> test_case.did_pass, test_results)
}

/// # Summary
/// Runs all given test cases and returns a `TestCaseResult` for each test, representing whether or not it passed
/// and what the failure message, if any.
/// This is a good alternative to `CheckAllTestCases` when you want custom output based on the results of your tests,
/// or more control over how test results are rendered.
/// # Input
/// Takes a list of test cases. A test case is a tuple of `(String, Int, Qubit[] => Unit, Qubit[] => 'T, 'T)`, where
/// the first String is the name of the test, the Int is the number of qubits, the first operation prepares the state,
/// the second operation is the test case itself, and the final element is the expected return value.
///
/// # Example
/// ```qsharp
/// RunAllTestCases([("0b0001 == 1", 4, (qs) => X(qs[0]), (qs) => MeasureSignedInteger(qs, 4), 1)]);
/// ```
operation RunAllTestCases<'T : Eq + Show>(test_cases : (String, Int, Qubit[] => Unit, Qubit[] => 'T, 'T)[]) : TestCaseResult[] {
    MappedOperation((name, num_qubits, prepare_state, case, result) => {
        use qubits = Qubit[num_qubits];
        prepare_state(qubits);
        let res = TestCase(name, qubits, case, result);
        ResetAll(qubits);
        res
    }, test_cases)
}

/// Helper function, copy of `Std.Arrays.Mapped` which works on operations instead
/// of functions.
operation MappedOperation<'T, 'U>(mapper : ('T => 'U), array : 'T[]) : 'U[] {
    mutable mapped = [];
    for element in array {
        set mapped += [mapper(element)];
    }
    mapped
}


/// # Summary
/// Given an operation on some qubits `func` which returns some value to test and a number of qubits to use `num_qubits`,
/// runs a number of test cases of the form `(Qubit[] => Unit, 'O)` where the first element is a qubit
/// state preparation operation and the second element is the expected output of the operation.
/// Returns the result of the `mode` function which takes a list of test cases and returns a value of type `'U`.
///
/// # Input
/// - `test_suite_name` : A string representing the name of the test suite.
/// - `func` : An operation which takes an array of qubits and returns a value of type `'O`.
/// - `num_qubits` : The number of qubits to use in the test. These are allocated before the test and reset before each test case.
/// - `test_cases` : A list of test cases, each of the form `(Qubit[] => Unit, 'O)`. The lambda operation should set up the qubits
///    in a specific state for `func` to operate on.
/// - `mode` : A function which takes a list of test cases and returns a value of type `'U`. Intended to be either `Qtest.Operations.CheckAllTestCases` or `Qtest.Operations.RunAllTestCases`.
///
/// # Example
/// ```qsharp
///    let test_cases: (Qubit[] => Unit, Int)[] = [
///        (qs => { X(qs[0]); X(qs[3]); }, 0b1001),
///        (qs => { X(qs[0]); X(qs[1]); }, 0b0011)
///    ];
///
///    let res : Util.TestCaseResult[] = Operations.TestMatrix(
///        // test name
///        "QubitTestMatrix",
///        // operation to test
///        qs => MeasureInteger(qs),
///        // number of qubits
///        4,
///        // test cases
///        test_cases,
///        // test mode
///        Operations.RunAllTestCases
///    );
/// ```

operation TestMatrix<'O : Show + Eq, 'U>(
    test_suite_name : String,
    func : Qubit[] => 'O,
    num_qubits : Int,
    test_cases : (Qubit[] => Unit, 'O)[],
    mode : ((String, Int, Qubit[] => Unit, Qubit[] => 'O, 'O)[]) => 'U
) : 'U {
    let test_cases_qs = Mapped((ix, (qubit_prep_function, expected)) -> (test_suite_name + $" {ix + 1}", num_qubits, qubit_prep_function, func, expected), Enumerated(test_cases));
    mode(test_cases_qs)
}

operation CheckTestMatrix<'O : Show + Eq>(
    test_suite_name : String,
    func : Qubit[] => 'O,
    num_qubits : Int,
    test_cases : (Qubit[] => Unit, 'O)[]
) : Bool {
    TestMatrix(test_suite_name, func, num_qubits, test_cases, CheckAllTestCases)
}

operation RunTestMatrix<'O : Show + Eq>(
    test_suite_name : String,
    func : Qubit[] => 'O,
    num_qubits : Int,
    test_cases : (Qubit[] => Unit, 'O)[]
) : TestCaseResult[] {
    TestMatrix(test_suite_name, func, num_qubits, test_cases, RunAllTestCases)
}

/// Internal (non-exported) helper function. Runs a test case and produces a `TestCaseResult`
operation TestCase<'T : Eq + Show>(name : String, qubits : Qubit[], test_case : (Qubit[]) => 'T, expected : 'T) : TestCaseResult {
    let result = test_case(qubits);
    if result == expected {
        new TestCaseResult { did_pass = true, message = "" }
    } else {
        new TestCaseResult { did_pass = false, message = $"{name}: expected: {expected}, got: {result}" }
    }
}

export CheckAllTestCases, RunAllTestCases, TestMatrix, CheckTestMatrix, RunTestMatrix; 