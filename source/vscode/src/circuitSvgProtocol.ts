// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export type CircuitSvgSaveMessage = {
  type: "qdk.circuit/save-svg";
  suggestedName: string;
  contents: string;
};

const MAX_FILE_NAME_LENGTH = 128;
const MAX_SVG_BYTES = 8 * 1024 * 1024;
const STANDALONE_SVG_ROOT =
  /^(?:<\?xml version="1\.0" encoding="UTF-8"\?>\s*)?<svg\b(?=[^>]*\bxmlns="http:\/\/www\.w3\.org\/2000\/svg")[^>]*>/;
const ACTIVE_SVG_CONTENT = /<[^>]+\s(?:href|xlink:href|src|on[a-z]+)\s*=/i;
const UNSAFE_SVG_CSS =
  /@import\b|(?:^|[;:{\s])(?:url|image-set|expression)\s*\(/i;
const ALLOWED_SVG_ELEMENTS = new Set([
  "circle",
  "desc",
  "g",
  "line",
  "path",
  "rect",
  "style",
  "svg",
  "text",
  "title",
  "tspan",
]);

export function isCircuitSvgSaveRequest(
  value: unknown,
): value is Record<string, unknown> & { type: CircuitSvgSaveMessage["type"] } {
  return isRecord(value) && value.type === "qdk.circuit/save-svg";
}

export function isCircuitSvgSaveMessage(
  value: unknown,
): value is CircuitSvgSaveMessage {
  if (!isCircuitSvgSaveRequest(value)) {
    return false;
  }

  return getCircuitSvgSaveMessageError(value) === undefined;
}

export function getCircuitSvgSaveMessageError(
  value: Record<string, unknown>,
): string | undefined {
  if (
    typeof value.suggestedName !== "string" ||
    value.suggestedName.length <= 4 ||
    value.suggestedName.length > MAX_FILE_NAME_LENGTH ||
    !value.suggestedName.toLowerCase().endsWith(".svg") ||
    /[\\/:*?"<>|]/.test(value.suggestedName)
  ) {
    return "The suggested SVG filename is invalid.";
  }
  if (
    typeof value.contents !== "string" ||
    value.contents.length === 0 ||
    new TextEncoder().encode(value.contents).byteLength > MAX_SVG_BYTES
  ) {
    return "The generated SVG is empty or exceeds the 8 MB export limit.";
  }
  if (
    !STANDALONE_SVG_ROOT.test(value.contents) ||
    ACTIVE_SVG_CONTENT.test(value.contents) ||
    hasUnexpectedSvgElement(value.contents) ||
    hasUnsafeSvgCss(value.contents)
  ) {
    return "The generated file is not a safe standalone SVG.";
  }
}

function hasUnexpectedSvgElement(contents: string): boolean {
  for (const match of contents.matchAll(/<\/?([A-Za-z_][\w:.-]*)\b/g)) {
    if (!ALLOWED_SVG_ELEMENTS.has(match[1].toLowerCase())) {
      return true;
    }
  }
  return false;
}

function hasUnsafeSvgCss(contents: string): boolean {
  const withoutEmbeddedFonts = contents.replace(
    /url\(\s*data:font\/woff2;base64,[A-Za-z0-9+/=]+\s*\)/g,
    "",
  );
  return UNSAFE_SVG_CSS.test(withoutEmbeddedFonts);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
