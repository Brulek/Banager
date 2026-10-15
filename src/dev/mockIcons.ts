/**
 * The browser preview's app icons (docs/ui-preview.md): what
 * `artifact_icon` answers in `pnpm dev:mock`. The real command
 * (`Session::artifact_icon`) draws macOS's own icon for a cask's app as a
 * PNG; there is no Mac here, so each app gets a small generated SVG
 * instead -- a rounded square in a colour of its own with its initial --
 * so the rows have an icon to show. Dev-only, like everything in src/dev:
 * ./mockBackend.ts is its one caller.
 */
import type { InstalledArtifact } from "../lib/types";

/**
 * The icon `row` shows in the preview, as a `data:image/svg+xml` URL, or
 * null for every row the real command draws nothing for: anything but a
 * cask, and a cask whose path is not an absolute `.app` (a font, a `pkg`,
 * one with no app). The real command also draws nothing when that folder
 * is missing or a link; every path in the pretend Mac is there.
 */
export function appIcon(row: InstalledArtifact): string | null {
  if (row.key.kind !== "Cask" || row.path === null) return null;
  if (!row.path.startsWith("/") || !row.path.endsWith(".app")) return null;
  const hue = hueOf(row.key.name);
  const initial = escapeXml(row.display_name.trim().charAt(0).toUpperCase() || "?");
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">` +
    `<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">` +
    `<stop offset="0" stop-color="hsl(${hue}, 70%, 62%)"/>` +
    `<stop offset="1" stop-color="hsl(${hue}, 70%, 40%)"/>` +
    `</linearGradient></defs>` +
    `<rect x="3" y="3" width="58" height="58" rx="14" fill="url(#g)"/>` +
    `<rect x="3.5" y="3.5" width="57" height="57" rx="13.5" fill="none" stroke="#000" stroke-opacity="0.12"/>` +
    `<text x="32" y="43" text-anchor="middle" fill="#fff" font-size="30" font-weight="700" ` +
    `font-family="-apple-system, BlinkMacSystemFont, 'Helvetica Neue', sans-serif">${initial}</text>` +
    `</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** A hue of the cask's own, the same on every run. */
function hueOf(name: string): number {
  let hash = 0;
  for (const char of name) {
    hash = (hash * 31 + (char.codePointAt(0) ?? 0)) % 360;
  }
  return hash;
}

function escapeXml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
