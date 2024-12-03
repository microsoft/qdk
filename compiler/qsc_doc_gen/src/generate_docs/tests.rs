// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::generate_docs;
use expect_test::expect;

#[test]
fn generates_standard_item() {
    let files = generate_docs(None, None, None);
    let (_, metadata, contents) = files
        .iter()
        .find(|(file_name, _, _)| &**file_name == "Std.Core/Length.md")
        .expect("Could not find doc file for Length");
    let full_contents = format!("{metadata}\n\n{contents}");

    expect![[r#"
        ---
        uid: Qdk.Std.Core.Length
        title: Length function
        description: "Q# Length function: Returns the number of elements in the input array `a`."
        ms.date: {TIMESTAMP}
        ms.topic: managed-reference
        qsharp.kind: function
        qsharp.package: __Core__
        qsharp.namespace: Std.Core
        qsharp.name: Length
        qsharp.summary: "Returns the number of elements in the input array `a`."
        ---

        # Length function

        Fully qualified name: Std.Core.Length

        ```qsharp
        function Length<'T>(a : 'T[]) : Int
        ```

        ## Summary
        Returns the number of elements in the input array `a`.

        ## Input
        ### a
        Input array.

        ## Output
        The total number of elements in the input array `a`.

        ## Example
        ```qsharp
        Message($"{ Length([0, 0, 0]) }"); // Prints 3
        ```
    "#]]
    .assert_eq(full_contents.as_str());
}

#[test]
fn generates_unrestricted_item() {
    let files = generate_docs(None, None, None);
    let (_, metadata, contents) = files
        .iter()
        .find(|(file_name, _, _)| &**file_name == "Std.Diagnostics/CheckZero.md")
        .expect("Could not file doc file for CheckZero");
    let full_contents = format!("{metadata}\n\n{contents}");

    expect![[r#"
        ---
        uid: Qdk.Std.Diagnostics.CheckZero
        title: CheckZero operation
        description: "Q# CheckZero operation: Checks whether a qubit is in the \|0⟩ state, returning true if it is."
        ms.date: {TIMESTAMP}
        ms.topic: managed-reference
        qsharp.kind: operation
        qsharp.package: __Std__
        qsharp.namespace: Std.Diagnostics
        qsharp.name: CheckZero
        qsharp.summary: "Checks whether a qubit is in the \|0⟩ state, returning true if it is."
        ---

        # CheckZero operation

        Fully qualified name: Std.Diagnostics.CheckZero

        ```qsharp
        operation CheckZero(qubit : Qubit) : Bool
        ```

        ## Summary
        Checks whether a qubit is in the |0⟩ state, returning true if it is.

        ## Description
        This operation checks whether a qubit is in the |0⟩ state. It will return true only
        if the qubit is deterministically in the |0⟩ state, and will return false otherwise. This operation
        does not change the state of the qubit.

        ## Input
        ### qubit
        The qubit to check.
        ## Output
        True if the qubit is in the |0⟩ state, false otherwise.

        ## Remarks
        This operation is useful for checking whether a qubit is in the |0⟩ state during simulation. It is not possible to check
        this on hardware without measuring the qubit, which could change the state.
    "#]]
    .assert_eq(full_contents.as_str());
}

#[test]
fn redirect_generation() {
    let files = generate_docs(None, None, None);
    let (_, metadata, contents) = files
        .iter()
        .find(|(file_name, _, _)| &**file_name == "Microsoft.Quantum.Core/Length.md")
        .expect("Could not find doc file for Length");
    let full_contents = format!("{metadata}\n\n{contents}");

    expect![[r#"
        ---
        uid: Qdk.Microsoft.Quantum.Core.Length
        title: Length exported item
        description: "Q# Length exported item: This is an exported item. The actual definition is found here: [Std.Core.Length](xref:Qdk.Std.Core.Length)"
        ms.date: {TIMESTAMP}
        ms.topic: managed-reference
        qsharp.kind: export
        qsharp.package: __Std__
        qsharp.namespace: Microsoft.Quantum.Core
        qsharp.name: Length
        qsharp.summary: "This is an exported item. The actual definition is found here: [Std.Core.Length](xref:Qdk.Std.Core.Length)"
        ---

        # Length exported item

        Fully qualified name: Microsoft.Quantum.Core.Length

        This is an exported item. The actual definition is found here: [Std.Core.Length](xref:Qdk.Std.Core.Length)
    "#]]
    .assert_eq(full_contents.as_str());
}

#[test]
fn index_file_generation() {
    let files = generate_docs(None, None, None);
    let (_, metadata, contents) = files
        .iter()
        .find(|(file_name, _, _)| &**file_name == "Std.Core/index.md")
        .expect("Could not find Std.Core Table of Contents file");
    let full_contents = format!("{metadata}\n\n{contents}");

    expect![[r#"
        ---
        uid: Qdk.Std.Core-toc
        title: Std.Core namespace
        description: Table of contents for the Q# Core namespace
        author: {AUTHOR}
        ms.author: {MS_AUTHOR}
        ms.date: {TIMESTAMP}
        ms.topic: landing-page
        ---

        # Std.Core

        The Std.Core namespace contains the following items:

        | Name | Description |
        |------|-------------|
        | [Length](xref:Qdk.Std.Core.Length) | Returns the number of elements in the input array `a`. |
        | [Repeated](xref:Qdk.Std.Core.Repeated) | Creates an array of given `length` with all elements equal to given `value`. `length` must be a non-negative integer. |
    "#]]
    .assert_eq(full_contents.as_str());
}

#[test]
fn top_index_file_generation() {
    let files = generate_docs(None, None, None);
    let (_, metadata, contents) = files
        .iter()
        .find(|(file_name, _, _)| &**file_name == "index.md")
        .expect("Could not find top-level Table of Contents file");
    let full_contents = format!("{metadata}\n\n{contents}");

    expect![[r#"
        ---
        uid: Microsoft.Quantum.apiref-toc
        title: Q# standard libraries for the Azure Quantum Development Kit
        description: Table of contents for the Q# standard libraries for Azure Quantum Development Kit
        author: {AUTHOR}
        ms.author: {MS_AUTHOR}
        ms.date: {TIMESTAMP}
        ms.topic: landing-page
        ---

        # Q# standard library

        The Q# standard library contains the following namespaces:

        | Namespace                                                                                                 | Description                                                  |
        | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
        | [`Microsoft.Quantum.Core`](xref:Qdk.Microsoft.Quantum.Core-toc)                                           | Re-exported functions.                                       |
        | [`Std.Arrays`](xref:Qdk.Std.Arrays-toc)                                                                   | Items for working with arrays.                               |
        | [`Std.Canon`](xref:Qdk.Std.Canon-toc)                                                                     | Canonical implementations of common classical and quantum utilities.|
        | [`Std.Convert`](xref:Qdk.Std.Convert-toc)                                                                 | Items for converting between different types.                |
        | [`Std.Core`](xref:Qdk.Std.Core-toc)                                                                       | Items for language built-in operations.                      |
        | [`Std.Diagnostics`](xref:Qdk.Std.Diagnostics-toc)                                                         | Items for debugging and testing quantum programs.            |
        | [`Std.Intrinsic`](xref:Qdk.Std.Intrinsic-toc)                                                             | Items that provide core quantum operations.                  |
        | [`Std.Logical`](xref:Qdk.Std.Logical-toc)                                                                 | Boolean Logic functions.                                     |
        | [`Std.Math`](xref:Qdk.Std.Math-toc)                                                                       | Items for classical math operations.                         |
        | [`Std.Measurement`](xref:Qdk.Std.Measurement-toc)                                                         | Items for measuring quantum results.                         |
        | [`Std.Random`](xref:Qdk.Std.Random-toc)                                                                   | Items for creating random values.                            |
        | [`Std.Range`](xref:Qdk.Std.Range-toc)                                                                     | Items for working with ranges.                               |
        | [`Std.ResourceEstimation`](xref:Qdk.Std.ResourceEstimation-toc)                                           | Items for working with the Azure Quantum Resource Estimator. |
        | [`Microsoft.Quantum.Unstable.Arithmetic`](xref:Qdk.Microsoft.Quantum.Unstable.Arithmetic-toc)             | Items for working with quantum arithmetic operations.        |
        | [`Microsoft.Quantum.Unstable.StatePreparation`](xref:Qdk.Microsoft.Quantum.Unstable.StatePreparation-toc) | Items for preparing a quantum state.                         |
        | [`Microsoft.Quantum.Unstable.TableLookup`](xref:Qdk.Microsoft.Quantum.Unstable.TableLookup-toc)           | Items for performing quantum table lookups.                  |
    "#]]
    .assert_eq(full_contents.as_str());
}
