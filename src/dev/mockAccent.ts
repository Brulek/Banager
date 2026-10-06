/**
 * `?accent=`: the accent colour the browser preview pretends the user
 * picked in System Settings, for a look at the words drawn on it
 * (decision I21b, src/lib/accentInk.ts). A headless or desktop browser
 * keeps index.css's default blue, which is what the window shows without
 * `AccentColor`; this stands in for `AccentColor` with macOS 27's own
 * values (AppKit's `controlAccentColor` in sRGB, in the light and the dark
 * appearance), darkened by 15% under Increase Contrast as index.css
 * darkens the real one. Dev-only, like everything in src/dev.
 */
export const MOCK_ACCENTS: Record<string, { light: string; dark: string }> = {
  blue: { light: "#007aff", dark: "#007aff" },
  purple: { light: "#953d96", dark: "#a550a7" },
  pink: { light: "#f74f9e", dark: "#f74f9e" },
  red: { light: "#e0383e", dark: "#ff5257" },
  orange: { light: "#f7821b", dark: "#f7821b" },
  yellow: { light: "#ffc726", dark: "#ffc600" },
  green: { light: "#62ba46", dark: "#62ba46" },
  graphite: { light: "#989898", dark: "#8c8c8c" },
};

/**
 * The style sheet for `?accent=<name>`, or null for none or an unknown
 * name (with the line to log for the latter). `:root:root`: index.css's
 * own `:root` comes into the page after this module has run.
 */
export function mockAccentStyle(search: string): { css: string | null; problem: string | null } {
  const raw = new URLSearchParams(search).get("accent");
  if (raw === null || raw === "") return { css: null, problem: null };
  const accent = MOCK_ACCENTS[raw.toLowerCase()];
  if (accent === undefined) {
    return { css: null, problem: `?accent=${raw} is not one of: ${Object.keys(MOCK_ACCENTS).join(", ")}` };
  }
  const darker = (colour: string) => `color-mix(in srgb, ${colour} 85%, black)`;
  return {
    css: [
      `:root:root { --color-accent: ${accent.light}; }`,
      `@media (prefers-color-scheme: dark) { :root:root { --color-accent: ${accent.dark}; } }`,
      `@media (prefers-contrast: more) { :root:root { --color-accent: ${darker(accent.light)}; } }`,
      `@media (prefers-contrast: more) and (prefers-color-scheme: dark) { :root:root { --color-accent: ${darker(accent.dark)}; } }`,
    ].join("\n"),
    problem: null,
  };
}

/** Puts `?accent=`'s style sheet in the page; returns the line to log, if any. */
export function applyMockAccent(search: string): string | null {
  const { css, problem } = mockAccentStyle(search);
  if (css !== null) {
    const style = document.createElement("style");
    style.dataset.mockAccent = "";
    style.textContent = css;
    document.head.append(style);
  }
  return problem;
}
