// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * The renderer build loads `.css` with esbuild's `text` loader (see the
 * `renderer` target in `build.mjs`), so a CSS import yields the stylesheet
 * source as a string for us to inject.
 */
declare module "*.css" {
  const content: string;
  export default content;
}
