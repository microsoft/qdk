// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

const vscodeThemeAttribute = "data-vscode-theme-kind";
const jupyterThemeAttribute = "data-jp-theme-light";
const commonThemeAttribute = "data-theme";

type ThemeChangeCallback = (isDark: boolean) => void;

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

// Example theme change callback that updates GitHub markdown styles
export function updateGitHubTheme(isDark: boolean) {
  // Update the stylesheet href based on the theme to apply
  document.head.querySelectorAll("link").forEach((el) => {
    const ref = el.getAttribute("href");
    if (ref && ref.includes("github-markdown")) {
      const newVal = ref.replace(
        /(dark\.css)|(light\.css)/,
        isDark ? "dark.css" : "light.css",
      );
      el.setAttribute("href", newVal);
    }
  });
}
