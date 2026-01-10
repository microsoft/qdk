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
export const gatePadding = 7;
/** Padding on each side of gate label. */
export const labelPadding = 10;
/** Height between each qubit register. */
export const gateHeightWithPadding: number = gateHeight + gatePadding * 2;
/** Height between classical registers. */
export const classicalRegHeight = 30;
/** Group box inner padding. */
export const groupBoxPaddingX = 23;
/** Padding around group label text */
export const groupLabelPaddingY = 2;
/** Padding between nested groups. */
export const nestedGroupPaddingBottom = 17;
/** Additional offset for control button. */
export const controlBtnOffset = 40;
/** Control button radius. */
export const controlBtnRadius = 15;
/** Default font size for gate labels. */
export const labelFontSize = 14;
/** Default font size for gate arguments. */
export const argsFontSize = 12;
/** Starting x coord for each register wire. */
export const regLineStart = 40;
/** Top padding between nested groups. */
export const nestedGroupPaddingTop = nestedGroupPaddingBottom + labelFontSize;

// Toolbox
/** Toolbox minimum height */
export const minToolboxHeight = 150;
/** Gap between gates in Toolbox Panel */
export const horizontalGap = 10;
export const verticalGap = 10;
