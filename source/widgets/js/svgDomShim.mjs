// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Minimal SVG DOM shim for server-side circuit rendering.
 *
 * Implements just enough of the DOM API so that the circuit-vis rendering
 * path (formatUtils → gateFormatter/inputFormatter/registerFormatter → sqore)
 * can build an SVG element tree and serialise it to markup via `outerHTML`.
 *
 * This eliminates the runtime dependency on jsdom, which is large and not
 * typically available in user environments without explicit installation.
 */

// Characters that need escaping in XML text content / attribute values.
const XML_ESCAPE_MAP = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" };
const xmlEscape = (s) => s.replace(/[&<>"]/g, (c) => XML_ESCAPE_MAP[c]);

// --------------------------------------------------------------------------
// ShimStyleDeclaration – trivial style property bag
// --------------------------------------------------------------------------

class ShimStyleDeclaration {
  constructor() {
    /** @type {Map<string, string>} */
    this._props = new Map();
  }

  setProperty(name, value) {
    this._props.set(name, value);
  }

  getPropertyValue(name) {
    return this._props.get(name) ?? "";
  }

  // Allow direct property assignment (e.g. el.style.pointerEvents = "none").
  // We convert camelCase to kebab-case for the serialized style attribute.
  get pointerEvents() {
    return this.getPropertyValue("pointer-events");
  }
  set pointerEvents(v) {
    this.setProperty("pointer-events", v);
  }

  get maxWidth() {
    return this.getPropertyValue("max-width");
  }
  set maxWidth(v) {
    this.setProperty("max-width", v);
  }

  toString() {
    const parts = [];
    for (const [k, v] of this._props) {
      parts.push(`${k}: ${v}`);
    }
    return parts.join("; ");
  }
}

// --------------------------------------------------------------------------
// ShimClassList – minimal classList implementation
// --------------------------------------------------------------------------

class ShimClassList {
  /** @param {ShimElement} owner */
  constructor(owner) {
    this._owner = owner;
    /** @type {Set<string>} */
    this._set = new Set();
  }

  add(...names) {
    for (const n of names) this._set.add(n);
    this._sync();
  }

  remove(...names) {
    for (const n of names) this._set.delete(n);
    this._sync();
  }

  contains(name) {
    return this._set.has(name);
  }

  _sync() {
    if (this._set.size > 0) {
      this._owner._attributes.set("class", [...this._set].join(" "));
    } else {
      this._owner._attributes.delete("class");
    }
  }

  /** Re-populate from the class attribute string. */
  _fromAttr(val) {
    this._set.clear();
    if (val) {
      for (const c of val.split(/\s+/)) {
        if (c) this._set.add(c);
      }
    }
  }
}

// --------------------------------------------------------------------------
// ShimNode – base class for TextNode and Element
// --------------------------------------------------------------------------

class ShimNode {
  constructor(nodeType) {
    this.nodeType = nodeType;
    this.parentNode = null;
    this.parentElement = null;
  }
}

// --------------------------------------------------------------------------
// ShimTextNode
// --------------------------------------------------------------------------

class ShimTextNode extends ShimNode {
  constructor(text) {
    super(3 /* TEXT_NODE */);
    this._text = text;
  }

  get textContent() {
    return this._text;
  }
  set textContent(v) {
    this._text = v;
  }

  /** Serialise to XML-safe text. */
  get outerHTML() {
    return xmlEscape(this._text);
  }
}

// --------------------------------------------------------------------------
// ShimElement – the core of the shim
// --------------------------------------------------------------------------

class ShimElement extends ShimNode {
  constructor(namespaceURI, tagName) {
    super(1 /* ELEMENT_NODE */);
    this.namespaceURI = namespaceURI;
    this.tagName = tagName;
    /** @type {Map<string, string>} */
    this._attributes = new Map();
    /** @type {ShimNode[]} */
    this._children = [];
    this.style = new ShimStyleDeclaration();
    this.classList = new ShimClassList(this);
  }

  // --- Attribute methods ---

  setAttribute(name, value) {
    this._attributes.set(name, String(value));
    if (name === "class") this.classList._fromAttr(String(value));
  }

  getAttribute(name) {
    return this._attributes.get(name) ?? null;
  }

  removeAttribute(name) {
    this._attributes.delete(name);
  }

  // --- Child methods ---

  appendChild(child) {
    if (child.parentNode) {
      child.parentNode.removeChild(child);
    }
    child.parentNode = this;
    child.parentElement = this;
    this._children.push(child);
    return child;
  }

  removeChild(child) {
    const idx = this._children.indexOf(child);
    if (idx !== -1) {
      this._children.splice(idx, 1);
      child.parentNode = null;
      child.parentElement = null;
    }
    return child;
  }

  replaceChild(newChild, oldChild) {
    const idx = this._children.indexOf(oldChild);
    if (idx !== -1) {
      if (newChild.parentNode) newChild.parentNode.removeChild(newChild);
      oldChild.parentNode = null;
      oldChild.parentElement = null;
      newChild.parentNode = this;
      newChild.parentElement = this;
      this._children[idx] = newChild;
    }
    return oldChild;
  }

  insertBefore(newChild, refChild) {
    if (newChild.parentNode) newChild.parentNode.removeChild(newChild);
    const idx = refChild ? this._children.indexOf(refChild) : -1;
    newChild.parentNode = this;
    newChild.parentElement = this;
    if (idx === -1) {
      this._children.push(newChild);
    } else {
      this._children.splice(idx, 0, newChild);
    }
    return newChild;
  }

  get firstChild() {
    return this._children[0] ?? null;
  }

  get children() {
    return this._children.filter((c) => c.nodeType === 1);
  }

  get childNodes() {
    return this._children;
  }

  // --- textContent ---

  get textContent() {
    return this._children
      .map((c) => c.textContent ?? "")
      .join("");
  }

  set textContent(val) {
    // Remove all existing children
    for (const c of this._children) {
      c.parentNode = null;
      c.parentElement = null;
    }
    this._children = [];
    if (val) {
      this._children.push(new ShimTextNode(val));
    }
  }

  // --- innerHTML (set only – needed by gateFormatter + inputFormatter) ---

  set innerHTML(markup) {
    // Remove existing children
    for (const c of this._children) {
      c.parentNode = null;
      c.parentElement = null;
    }
    this._children = [];
    // Parse the simple SVG/XML fragments used by the circuit-vis code.
    parseFragmentInto(this, markup);
  }

  get innerHTML() {
    return this._children.map((c) => c.outerHTML).join("");
  }

  // --- querySelector (basic: supports "tagname" and "tagname.class") ---

  querySelector(selector) {
    return this._querySelectorOne(selector);
  }

  /** @returns {ShimElement | null} */
  _querySelectorOne(selector) {
    for (const child of this._children) {
      if (child.nodeType !== 1) continue;
      if (_matchesSelector(child, selector)) return child;
      const found = child._querySelectorOne(selector);
      if (found) return found;
    }
    return null;
  }

  querySelectorAll(selector) {
    const results = [];
    this._querySelectorAll(selector, results);
    return results;
  }

  _querySelectorAll(selector, results) {
    for (const child of this._children) {
      if (child.nodeType !== 1) continue;
      if (_matchesSelector(child, selector)) results.push(child);
      child._querySelectorAll(selector, results);
    }
  }

  // --- addEventListener (no-op for SSR) ---
  addEventListener() {}
  removeEventListener() {}

  // --- Serialisation ---

  get outerHTML() {
    const attrs = [];
    for (const [k, v] of this._attributes) {
      attrs.push(` ${k}="${xmlEscape(v)}"`);
    }
    // Merge style into attributes if non-empty
    const styleStr = this.style.toString();
    if (styleStr) {
      attrs.push(` style="${xmlEscape(styleStr)}"`);
    }

    const attrStr = attrs.join("");

    if (this._children.length === 0) {
      return `<${this.tagName}${attrStr}/>`;
    }

    // For <style> and <script> elements, emit raw text content rather than
    // XML-escaping it — CSS/JS naturally contains characters like > and "
    // that must not be entity-encoded.
    const isRawText = this.tagName === "style" || this.tagName === "script";
    const inner = isRawText
      ? this._children.map((c) => (c.nodeType === 3 ? c._text : c.outerHTML)).join("")
      : this._children.map((c) => c.outerHTML).join("");
    return `<${this.tagName}${attrStr}>${inner}</${this.tagName}>`;
  }
}

// --------------------------------------------------------------------------
// Basic selector matching – handles the patterns used by circuit-vis:
//   "text"         → tag name
//   "svg.qviz"     → tag.class
//   ".some-class"  → any tag with class
// --------------------------------------------------------------------------

function _matchesSelector(el, selector) {
  // "tag.class" or ".class" or "tag"
  const m = selector.match(/^([a-zA-Z][\w-]*)(?:\.([a-zA-Z][\w-]*))?$/);
  if (m) {
    const [, tag, cls] = m;
    if (tag && el.tagName !== tag) return false;
    if (cls && !el.classList.contains(cls)) return false;
    return true;
  }
  // ".class" only
  const m2 = selector.match(/^\.([a-zA-Z][\w-]*)$/);
  if (m2) {
    return el.classList.contains(m2[1]);
  }
  return false;
}

// --------------------------------------------------------------------------
// Minimal XML/SVG fragment parser (for innerHTML)
//
// The fragments generated by gateFormatter and inputFormatter are simple:
//   - <tspan class='...' baseline-shift="..." font-size="...">text</tspan>
//   - <tspan dx="2" dy="-3" style="font-size: 0.8em;">†</tspan>
//   - Mixed text + tspans:  |<tspan ...>ψ</tspan>⟩
// We support: elements, text nodes, self-closing tags, and attributes with
// single or double quotes.
// --------------------------------------------------------------------------

function parseFragmentInto(parent, markup) {
  let pos = 0;
  const len = markup.length;

  while (pos < len) {
    const lt = markup.indexOf("<", pos);
    if (lt === -1) {
      // Remaining text
      const txt = markup.slice(pos);
      if (txt) parent.appendChild(new ShimTextNode(decodeXMLEntities(txt)));
      break;
    }

    // Text before the tag
    if (lt > pos) {
      const txt = markup.slice(pos, lt);
      if (txt) parent.appendChild(new ShimTextNode(decodeXMLEntities(txt)));
    }

    // Check for closing tag
    if (markup[lt + 1] === "/") {
      // </tag> – find end
      const gt = markup.indexOf(">", lt);
      pos = gt + 1;
      return pos; // Return to parent
    }

    // Opening tag – parse tag name and attributes
    const tagEnd = findTagEnd(markup, lt + 1);
    const selfClosing = markup[tagEnd - 1] === "/";
    const tagContent = markup.slice(lt + 1, selfClosing ? tagEnd - 1 : tagEnd);

    const spaceIdx = tagContent.search(/[\s/]/);
    const tagName = spaceIdx === -1 ? tagContent : tagContent.slice(0, spaceIdx);
    const attrString = spaceIdx === -1 ? "" : tagContent.slice(spaceIdx);

    const el = new ShimElement(null, tagName);
    parseAttributes(el, attrString);
    parent.appendChild(el);

    pos = tagEnd + 1; // past '>'

    if (!selfClosing) {
      // Parse children until we find this tag's closing tag
      pos = parseChildrenUntilClose(el, markup, pos, tagName);
    }
  }
}

function parseChildrenUntilClose(parent, markup, pos, tagName) {
  const len = markup.length;
  while (pos < len) {
    const lt = markup.indexOf("<", pos);
    if (lt === -1) {
      // Remaining text
      const txt = markup.slice(pos);
      if (txt) parent.appendChild(new ShimTextNode(decodeXMLEntities(txt)));
      break;
    }

    // Text before tag
    if (lt > pos) {
      const txt = markup.slice(pos, lt);
      if (txt) parent.appendChild(new ShimTextNode(decodeXMLEntities(txt)));
    }

    // Closing tag?
    if (markup[lt + 1] === "/") {
      const gt = markup.indexOf(">", lt);
      pos = gt + 1;
      return pos;
    }

    // Nested opening tag
    const tagEnd = findTagEnd(markup, lt + 1);
    const selfClosing = markup[tagEnd - 1] === "/";
    const tagContent = markup.slice(lt + 1, selfClosing ? tagEnd - 1 : tagEnd);
    const spaceIdx = tagContent.search(/[\s/]/);
    const childTag = spaceIdx === -1 ? tagContent : tagContent.slice(0, spaceIdx);
    const attrString = spaceIdx === -1 ? "" : tagContent.slice(spaceIdx);

    const el = new ShimElement(null, childTag);
    parseAttributes(el, attrString);
    parent.appendChild(el);

    pos = tagEnd + 1;
    if (!selfClosing) {
      pos = parseChildrenUntilClose(el, markup, pos, childTag);
    }
  }
  return pos;
}

function findTagEnd(markup, start) {
  // Find the '>' that closes this tag, skipping over quoted attribute values
  let i = start;
  const len = markup.length;
  while (i < len) {
    const ch = markup[i];
    if (ch === ">" ) return i;
    if (ch === '"' || ch === "'") {
      // Skip quoted string
      const quote = ch;
      i++;
      while (i < len && markup[i] !== quote) i++;
    }
    i++;
  }
  return len - 1; // Shouldn't happen with well-formed markup
}

function parseAttributes(el, attrString) {
  // Match   name="value"   or   name='value'
  const re = /([\w-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
  let m;
  while ((m = re.exec(attrString)) !== null) {
    const name = m[1];
    const value = decodeXMLEntities(m[2] ?? m[3]);
    if (name === "style") {
      // Parse inline style into the style map
      for (const decl of value.split(";")) {
        const colon = decl.indexOf(":");
        if (colon !== -1) {
          el.style.setProperty(
            decl.slice(0, colon).trim(),
            decl.slice(colon + 1).trim(),
          );
        }
      }
    } else {
      el.setAttribute(name, value);
    }
  }
}

const ENTITY_MAP = { "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&#39;": "'" };
function decodeXMLEntities(s) {
  return s.replace(/&(amp|lt|gt|quot|#39);/g, (m) => ENTITY_MAP[m] ?? m);
}

// --------------------------------------------------------------------------
// ShimDocument – the fake `document` object
// --------------------------------------------------------------------------

class ShimDocument {
  constructor() {
    this.documentElement = new ShimElement(null, "html");
    this.body = new ShimElement(null, "body");
  }

  createElementNS(ns, tagName) {
    return new ShimElement(ns, tagName);
  }

  createElement(tagName) {
    return new ShimElement(null, tagName);
  }

  createTextNode(text) {
    return new ShimTextNode(text);
  }

  querySelector(selector) {
    return this.body._querySelectorOne(selector);
  }
}

// --------------------------------------------------------------------------
// installSvgDomShim – patch globalThis so circuit-vis code can use `document`
// --------------------------------------------------------------------------

/**
 * Install the minimal DOM shim on `globalThis`.
 *
 * Call this once before rendering circuits.  Returns a cleanup function
 * that restores the previous globals.
 */
export function installSvgDomShim() {
  const saved = {
    document: globalThis.document,
    window: globalThis.window,
    getComputedStyle: globalThis.getComputedStyle,
    DOMPoint: globalThis.DOMPoint,
    performance: globalThis.performance,
    requestAnimationFrame: globalThis.requestAnimationFrame,
  };

  const doc = new ShimDocument();

  globalThis.document = doc;
  globalThis.window = { document: doc, getComputedStyle: () => ({}) };
  globalThis.getComputedStyle = () => ({});
  globalThis.DOMPoint = class DOMPoint {
    constructor(x = 0, y = 0, z = 0, w = 1) {
      this.x = x; this.y = y; this.z = z; this.w = w;
    }
  };
  globalThis.performance = { now: () => Date.now() };
  globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 0);

  return function restoreGlobals() {
    for (const [k, v] of Object.entries(saved)) {
      if (v === undefined) {
        delete globalThis[k];
      } else {
        globalThis[k] = v;
      }
    }
  };
}
