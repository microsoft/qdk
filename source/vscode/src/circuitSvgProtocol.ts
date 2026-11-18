// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export type CircuitSvgSaveMessage = {
  type: "qdk.circuit/save-svg";
  suggestedName: string;
  contents: string;
};

const MAX_FILE_NAME_LENGTH = 128;
const MAX_SVG_BYTES = 8 * 1024 * 1024;
const XML_DECLARATION = /^<\?xml version="1\.0" encoding="UTF-8"\?>\s*/;
const STANDALONE_SVG_ROOT =
  /^<svg\b(?=[^>]*\bxmlns="http:\/\/www\.w3\.org\/2000\/svg")[^>]*>/;
// Tags as XMLSerializer writes them. Quoted attribute values never contain < or >, and text
// escapes <, so every < in an exported SVG starts one of these tags.
const SVG_TAG =
  /<\/?([A-Za-z_][\w:.-]*)((?:\s+[A-Za-z_][\w:.-]*\s*=\s*(?:"[^"<>]*"|'[^'<>]*'))*)\s*\/?>/g;
const SVG_ATTRIBUTE = /([A-Za-z_][\w:.-]*)\s*=\s*("[^"]*"|'[^']*')/g;
// Style sheets must be plain text, so that no element can split them.
const STYLE_WITH_CHILD = /<style\b[^>]*>[^<]*<(?!\/style>)/;
const STYLE_SHEET = /<style\b[^>]*>([^<]*)<\/style>/g;
// Attributes that run script, load resources, or change what a reference resolves against.
const ACTIVE_ATTRIBUTE = /^(?:on[a-z]+|(?:[\w.-]+:)?href|src|xml:base)$/i;
// Exported labels reuse glyph outlines defined in the same document.
const GLYPH_REFERENCE = /^#qdk-glyph-(?:main|math)-[0-9a-f]+$/;
// CSS that loads resources, and the escapes, comments, and character references that could hide
// it. The exporter writes none of these outside its embedded fonts.
const UNSAFE_CSS =
  /@import|\b(?:url|src|image-set|expression)\s*\(|\\|\/\*|&#/i;
const EMBEDDED_FONT = /url\(\s*data:font\/woff2;base64,[A-Za-z0-9+/=]+\s*\)/g;
const ALLOWED_SVG_ELEMENTS = new Set([
  "circle",
  "defs",
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
  "use",
]);

export function isCircuitSvgSaveRequest(
  value: unknown,
): value is Record<string, unknown> & { type: CircuitSvgSaveMessage["type"] } {
  return isRecord(value) && value.type === "qdk.circuit/save-svg";
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
  if (!isSafeStandaloneSvg(value.contents)) {
    return "The generated file is not a safe standalone SVG.";
  }
}

function isSafeStandaloneSvg(contents: string): boolean {
  const markup = contents.replace(XML_DECLARATION, "");
  if (
    !STANDALONE_SVG_ROOT.test(markup) ||
    markup.replace(SVG_TAG, "").includes("<") ||
    STYLE_WITH_CHILD.test(markup)
  ) {
    return false;
  }
  for (const [, name, attributes] of markup.matchAll(SVG_TAG)) {
    if (!ALLOWED_SVG_ELEMENTS.has(name.toLowerCase())) {
      return false;
    }
    for (const [, attribute, quoted] of attributes.matchAll(SVG_ATTRIBUTE)) {
      const value = quoted.slice(1, -1);
      const isGlyphReference =
        name === "use" &&
        attribute === "xlink:href" &&
        GLYPH_REFERENCE.test(value);
      if (
        (ACTIVE_ATTRIBUTE.test(attribute) && !isGlyphReference) ||
        UNSAFE_CSS.test(value)
      ) {
        return false;
      }
    }
  }
  for (const [, css] of markup.matchAll(STYLE_SHEET)) {
    if (UNSAFE_CSS.test(css.replace(EMBEDDED_FONT, ""))) {
      return false;
    }
  }
  return true;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
