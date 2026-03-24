// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

const vscodeThemeAttribute = "data-vscode-theme-kind";
const jupyterThemeAttribute = "data-jp-theme-light";
const commonThemeAttribute = "data-theme";

type ThemeChangeCallback = (isDark: boolean) => void;

/**
 * Detects the current theme based on body attributes, url params, or OS preference.
 * If no theme attribute is set on the body or url, sets the data-theme attribute
 * based on the OS dark mode preference.
 *
 * Should be called after the DOM is ready. Returns undefined if document.body
 * is not available (e.g., in non-browser environments or before DOM is ready).
 *
 * @returns true if dark theme, false if light theme, undefined if not in browser
 */
export function ensureTheme(): boolean | undefined {
  if (typeof document === "undefined" || !document.body) {
    return undefined;
  }
  const el = document.body;

  // Check if any theme attribute is already set
  const vscodeTheme = el.getAttribute(vscodeThemeAttribute);
  const jupyterTheme = el.getAttribute(jupyterThemeAttribute);
  const commonTheme = el.getAttribute(commonThemeAttribute);

  if (vscodeTheme) {
    return (
      vscodeTheme !== "vscode-light" &&
      vscodeTheme !== "vscode-high-contrast-light"
    );
  }

  if (jupyterTheme) {
    return jupyterTheme !== "true"; // true means light theme
  }

  if (commonTheme) {
    return commonTheme === "dark";
  }

  // Check for theme specified in URL parameters
  const urlParams = new URLSearchParams(window.location.search);
  const urlTheme = urlParams.get("theme");
  if (urlTheme === "dark" || urlTheme === "light") {
    el.setAttribute(commonThemeAttribute, urlTheme);
    return urlTheme === "dark";
  }

  // No theme attribute set, detect OS preference and set it, and observe for changes
  const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  el.setAttribute(commonThemeAttribute, mediaQuery.matches ? "dark" : "light");

  mediaQuery.addEventListener("change", (e) => {
    el.setAttribute(commonThemeAttribute, e.matches ? "dark" : "light");
  });

  return mediaQuery.matches;
}

/**
 * Wire up a callback to invoke when the theme changes.
 *
 * @param el The element to observe for theme attribute changes (usually document.body)
 * @param callback The callback to invoke when the theme changes
 */
export function detectThemeChange(el: Element, callback: ThemeChangeCallback) {
  const observer = new MutationObserver((mutations: MutationRecord[]) => {
    let isDark = false; // Default to light

    for (const mutation of mutations) {
      if (mutation.attributeName === vscodeThemeAttribute) {
        const themeAttr = el.getAttribute(vscodeThemeAttribute);
        isDark =
          themeAttr !== "vscode-light" &&
          themeAttr !== "vscode-high-contrast-light";
      } else if (mutation.attributeName === jupyterThemeAttribute) {
        const themeAttr = el.getAttribute(jupyterThemeAttribute);
        isDark = themeAttr !== "true"; // true means light theme
      } else if (mutation.attributeName === commonThemeAttribute) {
        const themeAttr = el.getAttribute(commonThemeAttribute);
        isDark = themeAttr === "dark";
      }
    }
    callback(isDark);
  });

  observer.observe(el, {
    attributeFilter: [
      vscodeThemeAttribute,
      jupyterThemeAttribute,
      commonThemeAttribute,
    ],
  });
}

/**
 * Helper for updating specific stylesheets based on theme
 * @param isDark true to set dark theme, false to set light theme
 * @param searchText The text to search for in the stylesheet href to identify it
 * @param replaceText The regex to use to find the part of the href to replace
 * @param lightText The replacement text for light theme
 * @param darkText The replacement text for dark theme
 */
export function updateStyleSheetTheme(
  isDark: boolean,
  searchText: string,
  replaceText: RegExp,
  lightText: string,
  darkText: string,
) {
  // Update the stylesheet href based on the theme to apply
  document.head.querySelectorAll("link").forEach((el) => {
    const ref = el.getAttribute("href");
    if (ref && ref.includes(searchText)) {
      const newVal = ref.replace(replaceText, isDark ? darkText : lightText);
      el.setAttribute("href", newVal);
    }
  });
}
