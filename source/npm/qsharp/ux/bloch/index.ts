// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Dedicated entry point for the Bloch sphere widget. Keeping this behind its
// own package subpath (qsharp-lang/ux/bloch) isolates three.js to a lazily
// loaded chunk instead of bundling it into the main ux barrel.
export { BlochSphere, type BlochSphereProps } from "./bloch.js";
