// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.*;

import Main.*;

internal operation MatchLookupToStd(
    options : LookupOptions
) : Unit {
    let n = 3;
    let width = 4;
    let data = [[true, false, false, false], [false, true, false, false], [false, false, true, false], [false, false, false, false], [true, true, false, false], [false, true, true, false], [true, false, true, true], [true, true, true, true]];

    // Use adjoint Std.TableLookup.Select because this check takes adjoint of that.
    let equal = CheckOperationsAreEqual(
        n + width,
        qs => Lookup(options, data, qs[0..n-1], qs[n...]),
        qs => Adjoint Std.TableLookup.Select(data, qs[0..n-1], qs[n...])
    );
    Fact(equal, "Lookup should match Std.TableLookup.Select.");
}

internal operation MatchControlledLookupToMCX(
    options : LookupOptions
) : Unit {
    let n = 2;
    let width = 3;
    let data = [[true, false, false], [false, true, false], [false, false, true], [true, true, true]];


    // CheckOperationsAreEqual uses adjoint variant of the reference operation (seond operation).
    // Select from the standard library uses assumptions that the target is in zero state,
    // so its adjoint always returns target to zero state. So it won't work for CheckOperationsAreEqual directly.
    // Instead, we compare controlled Select to controlled LookupViaMCX, which works in all cases.
    let equal = CheckOperationsAreEqual(
        1 + n + width,
        qs => Controlled Lookup(
            [qs[0]],
            (options, data, qs[1..n], qs[n + 1...])
        ),
        qs => Controlled LookupViaMCX(
            [qs[0]],
            (data, qs[1..n], qs[n + 1...])
        )
    );
    Fact(equal, "Controlled Lookup should match controlled LookupViaMCX.");
}

internal operation TestOnAllAlgorithms(op : LookupOptions => Unit) : Unit {
    let algorithms = [
        DoStdLookup(),
        DoMCXLookup(),
        DoRecursiveSelectLookup(),
        DoPPLookup(),
        DoSplitPPLookup()
    ];
    for algorithm in algorithms {
        let options = new LookupOptions {
            lookupAlgorithm = algorithm,
            unlookupAlgorithm = DoUnlookupViaLookup(),
            failOnLongData = false,
            failOnShortData = false,
            respectExcessiveAddress = false,
            preferMeasurementBasedUncomputation = true,
        };
        op(options);
    }
}

@Test()
operation TestDefaultLookupMatchesStd() : Unit {
    MatchLookupToStd(DefaultLookupOptions());
}

@Test()
operation TestLookupMatchesStd() : Unit {
    TestOnAllAlgorithms(MatchLookupToStd);
}

@Test()
operation TestControlledLookupMatchesMCX() : Unit {
    TestOnAllAlgorithms(MatchControlledLookupToMCX);
}
