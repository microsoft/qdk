// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::generate_docs::{generate_docs, generate_summaries_map, metadata_to_markdown};
use expect_test::expect;
use std::collections::BTreeMap;

/// Testing helper function that returns summaries as a structured map
/// for easier test validation. Returns a map where each namespace maps to
/// a vector of markdown strings, one per item.
pub fn generate_summaries_for_testing() -> BTreeMap<String, Vec<String>> {
    let summaries_map = generate_summaries_map();

    let mut result = BTreeMap::new();

    for (ns, items) in summaries_map {
        let mut item_markdowns = Vec::new();

        for item in items {
            let markdown = metadata_to_markdown(&item);
            item_markdowns.push(markdown);
        }

        result.insert(ns, item_markdowns);
    }

    result
}

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
        description: "Q# CheckZero operation: Checks whether a qubit is in the |0⟩ state, returning true if it is."
        ms.date: {TIMESTAMP}
        qsharp.kind: operation
        qsharp.package: __Std__
        qsharp.namespace: Std.Diagnostics
        qsharp.name: CheckZero
        qsharp.summary: "Checks whether a qubit is in the |0⟩ state, returning true if it is."
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
        ms.date: {TIMESTAMP}
        ms.topic: landing-page
        ---

        # Std.Core

        The Std.Core namespace contains the following items:

        | Name | Description |
        |------|-------------|
        | [Complex](xref:Qdk.Std.Core.Complex) | Represents a complex number by its real and imaginary components. The real component can be accessed via the `Real` field, and the imaginary component via the `Imag` field. Complex literals can be written using the form `a + bi`, where `a` is the Double literal for the real part and `b` is the Double literal for the imaginary part. |
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
        ms.date: {TIMESTAMP}
        ms.topic: landing-page
        ---

        # Q# standard library

        The Q# standard library contains the following namespaces:

        | Namespace                                                       | Description                                                          |
        | --------------------------------------------------------------- | -------------------------------------------------------------------- |
        | [`Microsoft.Quantum.Core`](xref:Qdk.Microsoft.Quantum.Core-toc) | Re-exported functions.                                               |
        | [`Std.Arithmetic`](xref:Qdk.Std.Arithmetic-toc)                 | Items for working with quantum arithmetic operations.                |
        | [`Std.Arrays`](xref:Qdk.Std.Arrays-toc)                         | Items for working with arrays.                                       |
        | [`Std.Canon`](xref:Qdk.Std.Canon-toc)                           | Canonical implementations of common classical and quantum utilities. |
        | [`Std.Convert`](xref:Qdk.Std.Convert-toc)                       | Items for converting between different types.                        |
        | [`Std.Core`](xref:Qdk.Std.Core-toc)                             | Items for language built-in operations.                              |
        | [`Std.Diagnostics`](xref:Qdk.Std.Diagnostics-toc)               | Items for debugging and testing quantum programs.                    |
        | [`Std.Intrinsic`](xref:Qdk.Std.Intrinsic-toc)                   | Items that provide core quantum operations.                          |
        | [`Std.Logical`](xref:Qdk.Std.Logical-toc)                       | Boolean Logic functions.                                             |
        | [`Std.Math`](xref:Qdk.Std.Math-toc)                             | Items for classical math operations.                                 |
        | [`Std.Measurement`](xref:Qdk.Std.Measurement-toc)               | Items for measuring quantum results.                                 |
        | [`Std.Random`](xref:Qdk.Std.Random-toc)                         | Items for creating random values.                                    |
        | [`Std.Range`](xref:Qdk.Std.Range-toc)                           | Items for working with ranges.                                       |
        | [`Std.ResourceEstimation`](xref:Qdk.Std.ResourceEstimation-toc) | Items for working with the Azure Quantum Resource Estimator.         |
        | [`Std.StatePreparation`](xref:Qdk.Std.StatePreparation-toc)     | Items for preparing a quantum state.                                 |
        | [`Std.TableLookup`](xref:Qdk.Std.TableLookup-toc)               | Items for performing quantum table lookups.                          |
    "#]]
    .assert_eq(full_contents.as_str());
}

#[test]
fn generates_standard_item_summary() {
    let summaries = generate_summaries_for_testing();
    // Find a summary for a known item, e.g., Std.Core.Length
    let core_summaries = summaries
        .get("Std.Core")
        .expect("Could not find Std.Core namespace");
    let length_summary = core_summaries
        .iter()
        .find(|item| item.contains("## Length"))
        .expect("Could not find summary for Length");

    expect![[r#"
        ## Length

        ```qsharp
        function Length<'T>(a : 'T[]) : Int
        ```

        Returns the number of elements in the input array `a`.

    "#]]
    .assert_eq(length_summary);
}

#[test]
fn generates_std_core_summary() {
    let summaries = generate_summaries_for_testing();
    let core_summaries = summaries
        .get("Std.Core")
        .expect("Could not find Std.Core namespace");

    // Combine all summaries for the namespace
    let combined_summary = core_summaries.join("\n\n");

    expect![[r#"
        ## Complex

        ```qsharp
        struct Complex { Real : Double, Imag : Double }
        ```

        Represents a complex number by its real and imaginary components. The real component can be accessed via the `Real` field, and the imaginary component via the `Imag` field. Complex literals can be written using the form `a + bi`, where `a` is the Double literal for the real part and `b` is the Double literal for the imaginary part.



        ## Length

        ```qsharp
        function Length<'T>(a : 'T[]) : Int
        ```

        Returns the number of elements in the input array `a`.



        ## Repeated

        ```qsharp
        function Repeated<'T>(value : 'T, length : Int) : 'T[]
        ```

        Creates an array of given `length` with all elements equal to given `value`. `length` must be a non-negative integer.

    "#]]
    .assert_eq(&combined_summary);
}

#[test]
fn generates_summary_for_reexport() {
    let summaries = generate_summaries_for_testing();
    let length_summary = summaries
        .get("Microsoft.Quantum.Core")
        .expect("Could not find Microsoft.Quantum.Core namespace")
        .iter()
        .find(|item| item.contains("## Length"))
        .expect("Could not find summary for Length");

    expect![[r#"
        ## Length

        ```qsharp

        ```

        This is an exported item. The actual definition is found here: [Std.Core.Length](xref:Qdk.Std.Core.Length)

    "#]]
    .assert_eq(length_summary);
}
