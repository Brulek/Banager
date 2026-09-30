/**
 * The badge on Canager's icon in the Dock: how many updates the Updates
 * page offers to start now, the number beside the sidebar's Updates. Both read it from
 * one hook (`useUpdateCount`), so the Dock cannot promise an update the
 * window does not offer.
 */
import { useEffect } from "react";
import { setDockBadge } from "./api";
import { useUpdateCount } from "../components/UpdateProgress";

/**
 * Mounted once, by `App`'s `UpdateWatchers`. No badge until the snapshot
 * and the settings have both arrived, and none while the count is 0 --
 * which it is until the first check comes back, since the snapshot the
 * backend starts from lists no update (`Snapshot::empty()` in
 * crates/banager-core/src/session/mod.rs). After that, the badge is set
 * again each time the count changes, and only then: a check that finds
 * the same number leaves it alone.
 *
 * Set at mount too, whatever the count: the badge is the app's, not the
 * page's, so a page loaded again in a running app would otherwise go on
 * showing the one the last page set until its own count changed.
 */
export function useDockBadge(): void {
  const count = useUpdateCount() ?? 0;

  useEffect(() => {
    // A failure leaves the badge as it was, and nothing on the page
    // depends on it.
    setDockBadge(count).catch((e: unknown) => {
      console.error("set_badge_count failed", e);
    });
  }, [count]);
}
