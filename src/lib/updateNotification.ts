/**
 * The page's part in the update notification, Settings' 「有更新时通知我」
 * (src-tauri/src/notify.rs): it tells Rust, after each snapshot, which
 * updates Update all would take, and Rust decides whether a notification
 * goes out; and it opens the Updates page when Rust has brought the window
 * back for one -- on a Mac, as Canager comes to the front with its window
 * closed or minimized while a notification waits on it (`on_activate` in
 * src-tauri/src/window.rs), which is how a click on it arrives.
 */
import { useEffect, useRef } from "react";
import { onOpenUpdates, reportUpdateSet } from "./api";
import { useOperations, useSnapshot } from "./queries";
import { useStartableUpdates } from "../components/UpdateProgress";
import { artifactKeyId, useUiStore } from "../store/ui";
import type { UpdateCandidate, UpdatePair } from "./types";

/** An update as the report names it: its row's key, and the version the row offers. */
export function updatePairOf(candidate: UpdateCandidate): UpdatePair {
  return { key_id: artifactKeyId(candidate.key), target: candidate.target };
}

/**
 * Mounted once, by `App`'s `UpdateWatchers`, next to `useDockBadge`:
 *
 * - Once for each snapshot, as soon as the settings and the operations
 *   are in too, it reports the updates Update all would take
 *   (`useStartableUpdates`) with the snapshot's `round`
 *   (`reportUpdateSet`) -- not the backend's startup snapshot, round 0,
 *   which no round committed. Rust posts nothing unless the round was the
 *   daily check's, notifications are on and another app is in front, not
 *   Canager, and posts only news: a (row, version) pair neither told nor
 *   seen before.
 * - When Rust has brought the window back for the notification
 *   (`OPEN_UPDATES_EVENT`), it opens the Updates page, as the sidebar's
 *   Updates does. On a Mac, Rust hears no click on the notification itself
 *   (`post` in src-tauri/src/notify.rs): it sends this when Canager comes
 *   to the front with its window closed or minimized while a notification
 *   waits on it -- after a click, or ⌘-Tab or the Dock icon then
 *   (`on_activate` in src-tauri/src/window.rs).
 */
export function useUpdateNotification(): void {
  const { data: snapshot } = useSnapshot();
  // Waited for as well, so that a row an update takes is left out of the
  // report as it is of Update all, and not counted while the operations
  // are still on their way.
  const { data: operations } = useOperations();
  const startable = useStartableUpdates();
  const setPage = useUiStore((s) => s.setPage);
  // The last round reported: a round is reported once, whatever changes
  // after it -- the next snapshot is the next report. The snapshot cache
  // orders snapshots by round (`isNewerSnapshot` in src/lib/events.ts),
  // so a later round is reported however its clock read, and an earlier
  // one landing after it never takes the cache back to be sent again.
  const reported = useRef<number | null>(null);
  const round = snapshot !== undefined && snapshot.round > 0 ? snapshot.round : null;

  useEffect(() => {
    if (round === null || startable === undefined || operations === undefined) return;
    if (reported.current === round) return;
    reported.current = round;
    // A report that fails is not sent again: the next snapshot's is the
    // next report.
    reportUpdateSet(round, startable.map(updatePairOf)).catch((e: unknown) => {
      console.error("report_update_set failed", e);
    });
  }, [round, startable, operations]);

  // Listened for once: `setPage` is the same function from one render to
  // the next.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    onOpenUpdates(() => {
      // Inert once unmounted, while the listening may still be under way
      // (StrictMode's first mount), as `useMenuCommands` is.
      if (!cancelled) setPage("updates");
    })
      .then((stopListening) => {
        if (cancelled) {
          stopListening();
        } else {
          stop = stopListening;
        }
      })
      .catch((e: unknown) => {
        // A click then only brings the window back, on whatever page it
        // was left on.
        console.error("listening for the update notification's click failed", e);
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [setPage]);
}
