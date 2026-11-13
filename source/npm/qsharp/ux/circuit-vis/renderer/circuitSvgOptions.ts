// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

export type CircuitSvgBackground = "transparent" | "solid";
export type CircuitSvgFontMode = "embed" | "reference";
export type CircuitSvgExpansion = "model" | "collapsed" | "expanded";

export interface CircuitSvgDocumentOptions {
  title?: string;
  description?: string;
  background?: CircuitSvgBackground;
  fontMode?: CircuitSvgFontMode;
  idPrefix?: string;
}

interface CircuitSvgRenderingOptions extends CircuitSvgDocumentOptions {
  renderDepth?: number;
  expansion?: CircuitSvgExpansion;
}

export interface CircuitRendererSvgOptions extends CircuitSvgRenderingOptions {
  circuitIndex?: never;
}

export interface CircuitSvgRenderOptions extends CircuitSvgRenderingOptions {
  circuitIndex?: number;
}

export function validateCircuitSvgDocumentOptions(
  options: CircuitSvgDocumentOptions,
): void {
  assertOptionsObject(options);
  assertOptionalString(options.title, "title");
  assertOptionalString(options.description, "description");

  if (
    options.background !== undefined &&
    options.background !== "transparent" &&
    options.background !== "solid"
  ) {
    throw new Error(
      `Circuit SVG background must be "transparent" or "solid", received "${String(options.background)}".`,
    );
  }
  if (
    options.fontMode !== undefined &&
    options.fontMode !== "embed" &&
    options.fontMode !== "reference"
  ) {
    throw new Error(
      `Circuit SVG font mode must be "embed" or "reference", received "${String(options.fontMode)}".`,
    );
  }
  if (options.idPrefix !== undefined) {
    assertOptionalString(options.idPrefix, "ID prefix");
    if (!/^[A-Za-z_][A-Za-z0-9_.-]*$/.test(options.idPrefix)) {
      throw new Error(`Invalid SVG id prefix "${options.idPrefix}".`);
    }
  }
}

export function validateCircuitRendererSvgOptions(
  options: CircuitRendererSvgOptions,
): void {
  validateCircuitSvgRenderingOptions(options);
  if ("circuitIndex" in options) {
    throw new Error(
      "Circuit index is only supported by renderCircuitSvg; an instantiated renderer already owns its circuit.",
    );
  }
}

function validateCircuitSvgRenderingOptions(
  options: CircuitSvgRenderingOptions,
): void {
  validateCircuitSvgDocumentOptions(options);
  if (
    options.renderDepth !== undefined &&
    (!Number.isInteger(options.renderDepth) || options.renderDepth < 0)
  ) {
    throw new Error(
      `Circuit render depth must be a non-negative integer, received ${String(options.renderDepth)}.`,
    );
  }
  if (
    options.expansion !== undefined &&
    options.expansion !== "model" &&
    options.expansion !== "collapsed" &&
    options.expansion !== "expanded"
  ) {
    throw new Error(
      `Circuit SVG expansion must be "model", "collapsed", or "expanded", received "${String(options.expansion)}".`,
    );
  }
}

export function validateCircuitSvgRenderOptions(
  options: CircuitSvgRenderOptions,
): void {
  validateCircuitSvgRenderingOptions(options);
  if (
    options.circuitIndex !== undefined &&
    (!Number.isInteger(options.circuitIndex) || options.circuitIndex < 0)
  ) {
    throw new Error(
      `Circuit index must be a non-negative integer, received ${String(options.circuitIndex)}.`,
    );
  }
}

function assertOptionsObject(options: unknown): asserts options is object {
  if (
    typeof options !== "object" ||
    options === null ||
    Array.isArray(options)
  ) {
    throw new Error("Circuit SVG options must be an object.");
  }
}

function assertOptionalString(value: unknown, name: string): void {
  if (value !== undefined && typeof value !== "string") {
    throw new Error(`Circuit SVG ${name} must be a string.`);
  }
}
