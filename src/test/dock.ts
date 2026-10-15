import { getCurrentWindow } from "@tauri-apps/api/window";
import { vi } from "vitest";

/**
 * Banager's icon in the Dock, for a test: from the call on, what the page
 * asks it to show (`setDockBadge` in src/lib/api.ts, through the mocked
 * window in ./setup.ts). `badge` is what it shows now: the count set last,
 * or undefined for no badge, whether taken away or never set. `counts` is
 * every count set, in order, undefined where the badge was taken away.
 */
export function watchDock(): { badge(): number | undefined; counts(): Array<number | undefined> } {
  const setBadgeCount = vi.mocked(getCurrentWindow().setBadgeCount);
  setBadgeCount.mockReset();
  return {
    badge: () => setBadgeCount.mock.lastCall?.[0],
    counts: () => setBadgeCount.mock.calls.map(([count]) => count),
  };
}
