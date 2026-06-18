// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Minimal ambient declaration for the `jsdom` import used by the
// circuit-editor tests. `@types/jsdom` is not installed in this repo,
// so without this shim every test file that imports `JSDOM` would
// trigger `TS7016: Could not find a declaration file for module
// 'jsdom'` under `// @ts-check`.
//
// The tests only consult `new JSDOM(html).window` (and accessors on
// that window) and then close it. Typing those as `any` is intentional:
// the test bodies hand the JSDOM window into globals typed by the DOM
// lib, and we don't want every assignment to require a suppression.
declare module "jsdom" {
  export class JSDOM {
    constructor(html?: string, options?: unknown);
    readonly window: any;
  }
}
