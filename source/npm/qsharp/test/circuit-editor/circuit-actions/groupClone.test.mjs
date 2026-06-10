// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// `addOperation` clone-copy semantics when the template is a group:
// children/targets/controls are deep-copied so the placed instance
// has no shared references with the source.
//
// TODO: split out from circuitActions.test.mjs.

// @ts-check
