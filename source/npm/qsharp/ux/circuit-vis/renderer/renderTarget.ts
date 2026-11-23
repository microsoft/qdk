// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  formatLabel,
  type LabelRenderData,
} from "./formatters/labelFormatter.js";

/**
 * Where the formatters draw a circuit: the interactive view in the page, or a standalone SVG file.
 * Both are drawn from the same layout. They differ in how labels are drawn and in whether the
 * interactive parts are drawn.
 */
export interface RenderTarget {
  /**
   * Whether to draw the parts only the interactive view uses: expand and collapse buttons, links
   * to the source, tooltips, and the `data-*` attributes the editor reads.
   */
  readonly interactive: boolean;
  /** Draws a label. */
  drawLabel(label: LabelRenderData): SVGElement;
}

/** The interactive view in the page, which draws labels as SVG text styled by the page's CSS. */
export const screenTarget: RenderTarget = {
  interactive: true,
  drawLabel: formatLabel,
};
