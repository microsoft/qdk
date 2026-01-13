// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arrays.*;

import Utils.*;
import Multicontrolled.*;
import RecursiveSelect.*;
import LookupViaPP.*;
import PhaseLookup.*;

// ----------------------------------------------
// Lookup algorithm options.

/// Use lookup algorithm defined in the standard library.
function DoStdLookup() : Int {
    0
}

/// Use basic lookup algorithm with multicontrolled X gates.
function DoMCXLookup() : Int {
    1
}

/// Use recursive SELECT network as lookup algorithm.
function DoRecursiveSelectLookup() : Int {
    2
}

/// Use lookup algorithm via power products without address split.
function DoPPLookup() : Int {
    3
}

/// Use lookup algorithm via power products with address split.
function DoSplitPPLookup() : Int {
    4
}

// ----------------------------------------------
// Unlookup algorithm options.

/// Use unlookup algorithm defined in the standard library.
function DoStdUnlookup() : Int {
    0
}

/// Use the same unlookup algorithm as lookup.
/// This is always possible as lookup is self-adjoint.
function DoUnlookupViaLookup() : Int {
    1
}

/// Use unlookup algorithm with multicontrolled X gates.
/// This options is measurement based and returns target to zero state.
function DoMCXUnlookup() : Int {
    2
}

/// Use unlookup algorithm via power products without address split (Phase lookup).
/// This options is measurement based and returns target to zero state.
function DoPPUnlookup() : Int {
    3
}

/// Use unlookup algorithm via power products with address split (Phase lookup).
/// This options is measurement based and returns target to zero state.
function DoSplitPPUnlookup() : Int {
    4
}

/// # Summary
/// Options for table lookup and unlookup operations.
struct LookupOptions {
    // Specifies lookup algorithm. Options:
    // `DoStdLookup`, `DoMCXLookup`, `DoRecursiveSelectLookup`, `DoPPLookup`, `DoSplitPPLookup`.
    lookupAlgorithm : Int,

    // Specifies unlookup algorithm. Options:
    // `DoStdUnlookup`, `DoUnlookupViaLookup`, `DoMCXUnlookup`, `DoPPUnlookup`, `DoSplitPPUnlookup`.
    unlookupAlgorithm : Int,
    // Suggests using measurement-based uncomputation where applicable.
    // Note that some algorithms are measurement-based only and some cannot use measurements.
    // If `true`, use measurement-based uncomputations. Example: prefer adjoint AND.
    // If `false`, avoid measurement-based uncomputations. Example: prefer adjoint CCNOT.
    preferMeasurementBasedUncomputation : Bool,

    // If `true`, an error is raised if data is longer than addressable space.
    // If `false`, longer data beyond addressable space is ignored.
    failOnLongData : Bool,

    // If `true`, an error is raised if data is shorter than addressable space.
    // If `false`, shorter data is tolerated according to respectExcessiveAddress.
    failOnShortData : Bool,

    // If `true`, all address qubits are respected and used.
    // Addressing beyond data length yields the same result as if the data was padded with `false` values.
    // If `false`, addressing beyond data length yields undefined results.
    // As one consequence, when data is shorter than addressable space, higher address qubits are ignored.
    respectExcessiveAddress : Bool,
}

/// # Summary
/// Default lookup options. Use power products with register split for lookup and unlookup.
function DefaultLookupOptions() : LookupOptions {
    new LookupOptions {
        lookupAlgorithm = DoSplitPPLookup(),
        unlookupAlgorithm = DoSplitPPUnlookup(),
        failOnLongData = false,
        failOnShortData = false,
        respectExcessiveAddress = false,
        preferMeasurementBasedUncomputation = true,
    }
}

/// # Summary
/// Performs table lookup/unlookup operation using specified algorithm and other options.
///
/// # Input
/// ## options
/// LookupOptions defining lookup and unlookup algorithms and other parameters.
/// ## data
/// The data table to be looked up. Each entry is a Bool array the size of the target register.
/// ## address
/// Qubit register representing the address in little-endian format.
/// If the state of this register is one of the basis states |i⟩, and the target register is in |0⟩,
/// the target register will be set to the value data[i] during lookup. Address can also be in superposition.
/// ## target
/// Qubit register to accept the target data. Must be in the |0⟩ state for some algorithms.
/// For specifics see the corresponding algorithm implementation.
operation Lookup(
    options : LookupOptions,
    data : Bool[][],
    address : Qubit[],
    target : Qubit[]
) : Unit is Adj + Ctl {
    body (...) {
        if (options.lookupAlgorithm == DoStdLookup()) {
            // Don't do anthing beyond standard library select.
            Std.TableLookup.Select(data, address, target);
            return ();
        }

        let input = PrepareAddressAndData(options, address, data);

        if options.lookupAlgorithm == DoMCXLookup() {
            // Basic lookup via multicontrolled X gates.
            LookupViaMCX(input.fitData, input.fitAddress, target);
            return ();
        }

        if options.lookupAlgorithm == DoRecursiveSelectLookup() {
            // Recursive select implementation.
            if (options.respectExcessiveAddress) {
                RecursiveLookup(options.preferMeasurementBasedUncomputation, input.fitData, input.fitAddress, target);
            } else {
                RecursiveLookupOpt(options.preferMeasurementBasedUncomputation, input.fitData, input.fitAddress, target);
            }
            return ();
        }

        if options.lookupAlgorithm == DoPPLookup() {
            // Lookup via power products without address split.
            LookupViaPP(input.fitData, input.fitAddress, target);
            return ();
        }

        if options.lookupAlgorithm == DoSplitPPLookup() {
            LookupViaSplitPP(input.fitData, input.fitAddress, target);
            return ();
        }

        fail $"Unknown lookup algorithm specified ({options.lookupAlgorithm}).";
    }

    controlled (controls, ...) {
        let control_size = Length(controls);
        if control_size == 0 {
            Lookup(options, data, address, target);
            return ();
        }

        if options.lookupAlgorithm == DoStdLookup() {
            // Don't do anthing beyond standard library select.
            Controlled Std.TableLookup.Select(controls, (data, address, target));
            return ();
        }

        let input = PrepareAddressAndData(options, address, data);

        if options.lookupAlgorithm == DoMCXLookup() {
            // This is already a multicontrolled approach. Just add more controls.
            Controlled LookupViaMCX(controls, (data, address, target));
            return ();
        }

        // Combine multiple controls into one.
        use aux = Qubit[control_size - 1];
        within {
            CombineControls(controls, aux);
        } apply {
            let single_control = GetCombinedControl(controls, aux);

            if options.lookupAlgorithm == DoRecursiveSelectLookup() {
                // Recursive select implementation.
                if (options.respectExcessiveAddress) {
                    ControlledRecursiveSelect(
                        options.preferMeasurementBasedUncomputation,
                        single_control,
                        input.fitData,
                        input.fitAddress,
                        target
                    );
                } else {
                    ControlledRecursiveSelectOpt(
                        options.preferMeasurementBasedUncomputation,
                        single_control,
                        input.fitData,
                        input.fitAddress,
                        target
                    );
                }
            } else {
                // To use control qubit as an extra address qubit we need to respect entire address.
                // Power products implementation already does that.
                within {
                    // Invert control so that data is selected when control is |1⟩.
                    X(single_control);
                } apply {
                    // Add control as the most significant address qubit.
                    if options.lookupAlgorithm == DoPPLookup() {
                        LookupViaPP(input.fitData, input.fitAddress + [single_control], target);
                    } elif options.lookupAlgorithm == DoSplitPPLookup() {
                        LookupViaSplitPP(input.fitData, input.fitAddress + [single_control], target);
                    } else {
                        fail $"Unknown lookup algorithm specified ({options.lookupAlgorithm}).";
                    }
                }
            }
        }
    }

    adjoint (...) {
        if (options.unlookupAlgorithm == DoStdUnlookup()) {
            // Don't do anthing beyond standard library select.
            Adjoint Std.TableLookup.Select(data, address, target);
            return ();
        }
        if (options.unlookupAlgorithm == DoUnlookupViaLookup()) {
            // Perform same lookup operation (as it is self-adjoint).
            Lookup(options, data, address, target);
            return ();
        }

        // Perform measurement-based uncomputation.
        let input = PrepareAddressAndData(options, address, data);
        let phaseData = MeasureAndComputePhaseData(target, input.fitData, Length(input.fitAddress));
        // Now apply phase corrections after measurement-based uncomputation.

        if options.unlookupAlgorithm == DoMCXUnlookup() {
            // Phase lookup via multicontrolled X gates.
            PhaseLookupViaMCX(phaseData, input.fitAddress);
            return ();
        }

        if options.unlookupAlgorithm == DoPPUnlookup() {
            // Phase lookup via power products without address split.
            PhaseLookupViaPP(input.fitAddress, phaseData);
            return ();
        }

        if options.unlookupAlgorithm == DoSplitPPUnlookup() {
            // Phase lookup via power products with address split.
            PhaseLookupViaSplitPP(input.fitAddress, phaseData);
            return ();
        }

        fail $"Unknown unlookup algorithm specified ({options.unlookupAlgorithm}).";
    }

    controlled adjoint (controls, ...) {
        if options.unlookupAlgorithm == DoStdUnlookup() {
            // Don't do anthing beyond standard library select.
            Controlled Adjoint Std.TableLookup.Select(controls, (data, address, target));
            return ();
        }

        // In all other cases we perform controlled lookup as
        // we cannot do controlled measurement-based uncomputation.
        Controlled Lookup(controls, (options, data, address, target));
    }
}
