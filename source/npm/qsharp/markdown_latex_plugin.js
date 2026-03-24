// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check
/// <reference lib="es2022"/>

/***** LaTeX passthrough plug-in for the markdown-it parser *****

When converting Markdown to HTML that may contain LaTeX, but wanting to
leave the LaTeX untouched, there are a number of hazards. These include:

- Markdown escapes in the LaTeX, such as double escapes ("\\" and lines
  ending with a "\"), will be processed, corrupting the LaTeX. This sequence
  is common in \begin{bmatrix} structures (which is common in Quantum).

- The LaTeX may be processed for Markdown sequences. Common examples
  here include finding two "_" chars in the LaTeX and converting this to
  emphasis "<em>" tags around the content.

Disabling Markdown escapes can avoid the former, but not the latter, which
requires re-writing the LaTeX if possible to avoid, and can be subtle to catch.

To avoid these issues, this plug-in detects $..$ and $$..$$ content in
raw Markdown and inline HTML, and passes it through the Markdown parser as-is.

*/

// To help make type checking annotations cleaner
/** @typedef {import("markdown-it/dist/markdown-it.js")} MarkdownIt */
/** @typedef {import("markdown-it/dist/markdown-it.js").StateInline} StateInline */

// Below code to locate LaTeX blocks largely taken from @vscode/markdown-it-katex
// See https://github.com/microsoft/vscode-markdown-it-katex/blob/9f3e1dff0fa2e011c63cb6a05fa6e80b7624538f/src/index.ts

/**
 * @param {MarkdownIt} md
 */
function plugin(md) {
  // Add rules to extract LaTeX
  md.inline.ruler.after("escape", "math_inline", inlineMath);
  md.inline.ruler.after("escape", "math_inline_block", inlineMathBlock);

  // Just render the LaTeX 'as-is'
  md.renderer.rules.math_inline = (tokens, idx) => {
    return "$" + escapeHtml(tokens[idx].content) + "$";
  };

  md.renderer.rules.math_block = (tokens, idx) => {
    return "$$" + escapeHtml(tokens[idx].content) + "$$";
  };
}

/**
 * @param {string} unsafe
 */
function escapeHtml(unsafe) {
  return unsafe
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

/**
 * @param {string} char
 * @returns boolean
 */
function isWhitespace(char) {
  return /^\s$/u.test(char);
}

/**
 * @param {string} char
 * @returns boolean
 */
function isWordCharacterOrNumber(char) {
  return /^[\w\d]$/u.test(char);
}

/**
 * @param {StateInline} state
 * @param {number} pos
 */
function isValidInlineDelim(state, pos) {
  const prevChar = state.src[pos - 1];
  const char = state.src[pos];
  const nextChar = state.src[pos + 1];
  if (char !== "$") {
    return { can_open: false, can_close: false };
  }
  let canOpen = false;
  let canClose = false;
  if (
    prevChar !== "$" &&
    prevChar !== "\\" &&
    (prevChar === undefined ||
      isWhitespace(prevChar) ||
      !isWordCharacterOrNumber(prevChar))
  ) {
    canOpen = true;
  }
  if (
    nextChar !== "$" &&
    (nextChar == undefined ||
      isWhitespace(nextChar) ||
      !isWordCharacterOrNumber(nextChar))
  ) {
    canClose = true;
  }
  return { can_open: canOpen, can_close: canClose };
}

/**
 * @param {*} state
 * @param {number} pos
 */
function isValidBlockDelim(state, pos) {
  const prevChar = state.src[pos - 1];
  const char = state.src[pos];
  const nextChar = state.src[pos + 1];
  const nextCharPlus1 = state.src[pos + 2];
  if (
    char === "$" &&
    prevChar !== "$" &&
    prevChar !== "\\" &&
    nextChar === "$" &&
    nextCharPlus1 !== "$"
  ) {
    return { can_open: true, can_close: true };
  }
  return { can_open: false, can_close: false };
}

/**
 * @param {*} state
 * @param {boolean} silent
 */
function inlineMath(state, silent) {
  if (state.src[state.pos] !== "$") {
    return false;
  }

  let res = isValidInlineDelim(state, state.pos);
  if (!res.can_open) {
    if (!silent) {
      state.pending += "$";
    }
    state.pos += 1;
    return true;
  }
  // First check for and bypass all properly escaped delimieters
  // This loop will assume that the first leading backtick can not
  // be the first character in state.src, which is known since
  // we have found an opening delimieter already.
  let start = state.pos + 1;
  let match = start;
  let pos;
  while ((match = state.src.indexOf("$", match)) !== -1) {
    // Found potential $, look for escapes, pos will point to
    // first non escape when complete
    pos = match - 1;
    while (state.src[pos] === "\\") {
      pos -= 1;
    }
    // Even number of escapes, potential closing delimiter found
    if ((match - pos) % 2 == 1) {
      break;
    }
    match += 1;
  }
  // No closing delimiter found.  Consume $ and continue.
  if (match === -1) {
    if (!silent) {
      state.pending += "$";
    }
    state.pos = start;
    return true;
  }
  // Check if we have empty content, ie: $$.  Do not parse.
  if (match - start === 0) {
    if (!silent) {
      state.pending += "$$";
    }
    state.pos = start + 1;
    return true;
  }
  // Check for valid closing delimiter
  res = isValidInlineDelim(state, match);
  if (!res.can_close) {
    if (!silent) {
      state.pending += "$";
    }
    state.pos = start;
    return true;
  }
  if (!silent) {
    const token = state.push("math_inline", "math", 0);
    token.markup = "$";
    token.content = state.src.slice(start, match);
  }
  state.pos = match + 1;
  return true;
}

/**
 * @param {StateInline} state
 * @param {boolean} silent
 */
function inlineMathBlock(state, silent) {
  var start, match, token, res, pos;
  if (state.src.slice(state.pos, state.pos + 2) !== "$$") {
    return false;
  }
  res = isValidBlockDelim(state, state.pos);
  if (!res.can_open) {
    if (!silent) {
      state.pending += "$$";
    }
    state.pos += 2;
    return true;
  }
  // First check for and bypass all properly escaped delimieters
  // This loop will assume that the first leading backtick can not
  // be the first character in state.src, which is known since
  // we have found an opening delimieter already.
  start = state.pos + 2;
  match = start;
  while ((match = state.src.indexOf("$$", match)) !== -1) {
    // Found potential $$, look for escapes, pos will point to
    // first non escape when complete
    pos = match - 1;
    while (state.src[pos] === "\\") {
      pos -= 1;
    }
    // Even number of escapes, potential closing delimiter found
    if ((match - pos) % 2 == 1) {
      break;
    }
    match += 2;
  }
  // No closing delimiter found.  Consume $$ and continue.
  if (match === -1) {
    if (!silent) {
      state.pending += "$$";
    }
    state.pos = start;
    return true;
  }
  // Check if we have empty content, ie: $$$$.  Do not parse.
  if (match - start === 0) {
    if (!silent) {
      state.pending += "$$$$";
    }
    state.pos = start + 2;
    return true;
  }
  // Check for valid closing delimiter
  res = isValidBlockDelim(state, match);
  if (!res.can_close) {
    if (!silent) {
      state.pending += "$$";
    }
    state.pos = start;
    return true;
  }
  if (!silent) {
    token = state.push("math_block", "math", 0);
    token.block = true;
    token.markup = "$$";
    token.content = state.src.slice(start, match);
  }
  state.pos = match + 2;
  return true;
}

export { plugin };
