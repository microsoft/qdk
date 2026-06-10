// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// `refreshAncestorTargets` cascade after a child mutation, plus
// the canonical target-order invariant (D7 / R4 territory): when
// a group's children change, every ancestor's `.targets` is
// re-derived bottom-up in deterministic order.
//
// TODO: split out from circuitActions.test.mjs.

// @ts-check
