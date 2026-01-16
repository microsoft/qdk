// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// SVG Namespace
export const svgNS = "http://www.w3.org/2000/svg";

// Display attributes
/** Left padding of SVG. */
export const leftPadding = 20;
/** x coordinate for first operation on each register. */
export const startX = 80;
/** y coordinate of the top of the first register row. */
export const startY = 40;
/** Minimum width of each gate. */
export const minGateWidth = 40;
/** Height of each gate. */
export const gateHeight = 40;
/** Padding on each side of gate. */
export const gatePadding = 6;
/** Default font size for gate labels. */
export const labelFontSize = 14;
/** Horizontal padding on each side of gate label. */
export const labelPaddingX = 10;
/** Height between classical registers. */
export const classicalRegHeight = 20;
/** Horizontal padding inside group box. */
export const groupPaddingX = 10;
/** Vertical padding above and below a group label. */
export const groupLabelPaddingY = 2;
/** Bottom padding inside group box. */
export const groupBottomPadding = 10;
/** Top padding inside group box. */
export const groupTopPadding =
  groupBottomPadding + labelFontSize + groupLabelPaddingY;
/** Additional offset for control button. */
export const controlBtnOffset = 40;
/** Control button radius. */
export const controlBtnRadius = 15;
/** Default font size for gate arguments. */
export const argsFontSize = 12;
/** Starting x coord for each register wire. */
export const regLineStart = 40;

// Toolbox
/** Toolbox minimum height */
export const minToolboxHeight = 150;
/** Gap between gates in Toolbox Panel */
export const horizontalGap = 10;
export const verticalGap = 10;
