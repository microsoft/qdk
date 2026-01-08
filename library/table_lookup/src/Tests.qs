// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.*;

import Select.*;

internal operation MatchSelectToStd(
    options : SelectOptions
) : Unit {
    let n = 3;
    let width = 4;
    let data = [[true, false, false, false], [false, true, false, false], [false, false, true, false], [false, false, false, false], [true, true, false, false], [false, true, true, false], [true, false, true, true], [true, true, true, true]];

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n + width,
        qs => Select(options, data, qs[0..n-1], qs[n...]),
        qs => Adjoint Std.TableLookup.Select(data, qs[0..n-1], qs[n...])
    );
    Fact(equal, "Select should match Std.TableLookup.Select.");
}

internal function GetSelectOptions(algorithm: Int) : SelectOptions {
    return new SelectOptions {
        selectAlgorithm = algorithm,
        unselectAlgorithm = UnselectViaSelect(),
        failOnLongData = false,
        failOnShortData = false,
        respectExcessiveAddress = false,
    };
}

@Test()
operation TestSelectMatchesStd() : Unit {
    let algorithms = [
        SelectViaStd(),
        SelectViaMCX(),
        SelectViaRecursion(),
        SelectViaPP(),
        SelectViaSplitPP()
    ];
    MatchSelectToStd(DefaultSelectOptions());
    for algorithm in algorithms {
        MatchSelectToStd(GetSelectOptions(algorithm));
    }
}
