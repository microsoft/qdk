// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arrays.*;

import Utils.*;
import Multicontrolled.*;
import SelectRecursive.*;
import LookupViaPP.*;
import PhaseLookup.*;

// ----------------------------------------------
// Select algorithm options.

// Use select algorithm defined in the standard library.
function SelectViaStd() : Int {
    return 0;
}

// Use basic select algorithm via multicontrolled X gates.
function SelectViaMCX() : Int {
    return 1;
}

// Use select algorithm via recursion.
function SelectViaRecursion() : Int {
    return 2;
}

// Use select algorithm via power products without address split.
function SelectViaPP() : Int {
    return 3;
}

// Use select algorithm via power products with address split.
function SelectViaSplitPP() : Int {
    return 4;
}

// ----------------------------------------------
// Unselect algorithm options.

// Use unselect algorithm defined in the standard library.
function UnselectViaStd() : Int {
    return 0;
}

// Perform unselect via same algorithm as select as it is self-adjoint.
function UnselectViaSelect() : Int {
    return 1;
}

// Perform unselect via multicontrolled X gates.
// This options is measurement based and returns target to zero state.
function UnselectViaMCX() : Int {
    return 2;
}

// Perform unselect via power products without address split (Phase lookup).
// This options is measurement based and returns target to zero state.
function UnselectViaPP() : Int {
    return 3;
}

// Perform unselect via power products with address split (Phase lookup).
// This options is measurement based and returns target to zero state.
function UnselectViaSplitPP() : Int {
    return 4;
}

struct SelectOptions {
    // Specifies select algorithm. Options:
    // `SelectViaStd`, `SelectViaMCX`, `SelectViaRecursion`, `SelectViaPP`, `SelectViaSplitPP`.
    selectAlgorithm: Int,

    // Specifies unselect algorithm. Options:
    // `UnselectViaStd`, `UnselectViaSelect`, `UnselectViaMCX`, `UnselectViaPP`, `UnselectViaSplitPP`.
    unselectAlgorithm: Int,

    // If `true`, an error is raised if data is longer than addressable space.
    // If `false`, longer data beyond addressable space is ignored.
    failOnLongData: Bool,

    // If `true`, an error is raised if data is shorter than addressable space.
    // If `false`, shorter data is tolerated according to respectExcessiveAddress.
    failOnShortData: Bool,

    // If `true`, all address qubits are respected and used.
    // Addressing beyond data length yields the same result as if the data was padded with `false` values.
    // If `false`, address qubits that aren't necessary for addressing data are ignored.
    // Addressing beyond data length yields undefined results.
    respectExcessiveAddress: Bool,

    // useMeasurementUncomputation: Bool,
    // usePowerProductUncomputation: Bool,
    // addressSplitPoint: Double,
}

function DefaultSelectOptions() : SelectOptions {
     new SelectOptions {
        selectAlgorithm = SelectViaSplitPP(),
        unselectAlgorithm = UnselectViaSplitPP(),
        failOnLongData = false,
        failOnShortData = false,
        respectExcessiveAddress = false,
    }
}

operation Select(
    options: SelectOptions,
    data: Bool[][],
    address: Qubit[],
    target: Qubit[]
) : Unit is Adj + Ctl {
    body (...) {
        if (options.selectAlgorithm == SelectViaStd()) {
            // Don't do anthing beyond standard library select.
            Std.TableLookup.Select(data, address, target);
            return ();
        }

        let input = PrepareAddressAndData(options, address, data);

        if options.selectAlgorithm == SelectViaMCX() {
            // Basic lookup via multicontrolled X gates.
            LookupViaMCX(input.fitData, input.fitAddress, target);
            return ();
        }

        if options.selectAlgorithm == SelectViaRecursion() {
            // Recursive select implementation.
            LookupViaRecursion(input.fitData, input.fitAddress, target);
            return ();
        }

        if options.selectAlgorithm == SelectViaPP() {
            // Lookup via power products without address split.
            LookupViaPP(input.fitData, input.fitAddress, target);
            return ();
        }

        if options.selectAlgorithm == SelectViaSplitPP() {
            LookupViaSplitPP(input.fitData, input.fitAddress, target);
            return ();
        }

        fail "Unknown select algorithm specified.";
    }

    controlled (controls, ...) {
        let control_size = Length(controls);
        if control_size == 0 {
            Select(options, data, address, target);
            return ();
        }

        if options.selectAlgorithm == SelectViaStd() {
            // Don't do anthing beyond standard library select.
            Controlled Std.TableLookup.Select(controls, (data, address, target));
            return ();
        }

        if options.selectAlgorithm == SelectViaMCX() {
            // This is already a multicontrolled approach. Just add more controls.
            Controlled LookupViaMCX(controls, (data, address, target));
            return ();
        }

        // Combine multiple controls into one.
        use aux = Qubit[control_size - 1];
        within {
            CombineControls(controls, aux);
        } apply {
            let single_control = if control_size == 1 {
                Head(controls)
            } else {
                Tail(aux)
            };

            let input = PrepareAddressAndData(options, address, data);

            if options.selectAlgorithm == SelectViaRecursion() {
                // Recursive select implementation.
                ControlledRecursiveLookup(single_control, input.fitData, input.fitAddress, target);
                return ();
            }
            
            // To use control qubit as an extra address qubit we need to respect entire address.
            let options_a = new SelectOptions {
                selectAlgorithm = options.selectAlgorithm,
                unselectAlgorithm = options.unselectAlgorithm,
                failOnLongData = options.failOnLongData,
                failOnShortData = options.failOnShortData,
                respectExcessiveAddress = true,
            };

            within {
                // Invert control so that data is selected when control is |1>
                X(single_control);
            } apply {
                // Add control as the most significant address qubit.
                if options.selectAlgorithm == SelectViaPP() {
                    LookupViaPP(input.fitData, input.fitAddress + [single_control], target);
                    return ();
                }

                if options.selectAlgorithm == SelectViaSplitPP() {
                    LookupViaSplitPP(input.fitData, input.fitAddress + [single_control], target);
                    return ();
                }

                fail "Unknown select algorithm specified.";
            }
        }
    }

    adjoint (...) {
        if (options.unselectAlgorithm == UnselectViaStd()) {
            // Don't do anthing beyond standard library select.
            Std.TableLookup.Select(data, address, target);
            return ();
        }
        if (options.unselectAlgorithm == UnselectViaSelect()) {
            // Perform same select operation as it is self-adjoint.
            Select(options, data, address, target);
            return ();
        }

        let input = PrepareAddressAndData(options, address, data);
        let phaseData = MeasureAndComputePhaseData(target, input.fitData, Length(input.fitAddress));

        if options.unselectAlgorithm == UnselectViaMCX() {
            // Phase lookup via multicontrolled X gates.
            PhaseLookupViaMCX(phaseData, input.fitAddress);
            return ();
        }

        if options.unselectAlgorithm == UnselectViaPP() {
            // Phase lookup via power products without address split.
            PhaseLookupViaPP(input.fitAddress, phaseData);
            return ();
        }

        if options.unselectAlgorithm == UnselectViaSplitPP() {
            // Phase lookup via power products with address split.
            PhaseLookupViaSplitPP(input.fitAddress, phaseData);
            return ();
        }

        fail "Unknown unselect algorithm specified.";
    }

    controlled adjoint (controls, ...) {
        let control_size = Length(controls);
        if control_size == 0 {
            Adjoint Select(options, data, address, target);
            return ();
        }

        if options.unselectAlgorithm == UnselectViaStd() {
            // Don't do anthing beyond standard library select.
            Controlled Adjoint Std.TableLookup.Select(controls, (data, address, target));
            return ();
        }

        // In all other cases we perform controlled select as
        // we cannot do controlled measurement-based uncomputation.
        Controlled Select(controls, (options, data, address, target));
    }
}
