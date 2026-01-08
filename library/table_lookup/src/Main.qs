// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Main wrapper operation for lookup.

export Select.Select;

// Options and available algorithms.

export Select.SelectOptions;
export Select.DefaultSelectOptions;

export Select.SelectViaStd;
export Select.SelectViaMCX;
export Select.SelectViaRecursion;
export Select.SelectViaPP;
export Select.SelectViaSplitPP;

export Select.UnselectViaStd;
export Select.UnselectViaSelect;
export Select.UnselectViaMCX;
export Select.UnselectViaPP;
export Select.UnselectViaSplitPP;

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
export Utils.GetCombinedControl;
export Utils.CombineControls;
