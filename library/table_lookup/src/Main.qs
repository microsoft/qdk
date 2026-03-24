// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Main wrapper operation for lookup.

export Lookup.Lookup;

// Options and available algorithms.

export Lookup.LookupOptions;
export Lookup.DefaultLookupOptions;

export Lookup.DoStdLookup;
export Lookup.DoMCXLookup;
export Lookup.DoRecursiveSelectLookup;
export Lookup.DoPPLookup;
export Lookup.DoSplitPPLookup;

export Lookup.DoStdUnlookup;
export Lookup.DoUnlookupViaLookup;
export Lookup.DoMCXUnlookup;
export Lookup.DoPPUnlookup;
export Lookup.DoSplitPPUnlookup;

// Lookup implementations via multicontrolled X gates.

export Multicontrolled.LookupViaMCX;
export Multicontrolled.BitLookupViaMCX;
export Multicontrolled.PhaseLookupViaMCX;

// Lookup implementations via recursive SELECT network.

export RecursiveSelect.RecursiveLookup;
export RecursiveSelect.RecursiveLookupOpt;
export RecursiveSelect.ControlledRecursiveSelect;
export RecursiveSelect.ControlledRecursiveSelectOpt;

// Lookup implementations via power products.

export PowerProducts.GetAuxCountForPP;
export PowerProducts.ConstructPowerProducts;
export PowerProducts.DestructPowerProducts;

export LookupViaPP.LookupViaPP;
export LookupViaPP.LookupViaSplitPP;
export PhaseLookup.PhaseLookupViaPP;
export PhaseLookup.PhaseLookupViaSplitPP;

// Utility functions.

export Utils.FastMobiusTransform;
export Utils.MeasureAndComputePhaseData;
export Utils.BinaryInnerProduct;
export Utils.CombineControls;
export Utils.GetCombinedControl;
