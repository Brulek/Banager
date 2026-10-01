/**
 * "Remind Me in 30 Days" running out while Banager stays open. What a
 * snooze hides is worked out from the clock whenever the settings or the
 * snapshot change (`hidingRule`, src/lib/updateState.ts), and Rust drops
 * a snooze that has run out only as it loads the settings, at launch. So
 * without a change, a snoozed tool would stay out of the Updates page's
 * list and count, the sidebar and the Dock's badge after its date. This
 * drops it from the settings the page holds once its `until` has passed,
 * which has every count worked out again. Nothing is written: the file
 * keeps the entry until Rust next loads it, and drops it then.
 */
import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useSettings } from "./queries";
import { queryKeys } from "./queryKeys";
import type { Settings } from "./types";
import { activeSnoozes } from "./updateState";

/**
 * The longest wait before looking again. A timer can be held up while the
 * Mac sleeps, and one longer than about 24.8 days never runs as asked; a
 * snooze runs 30. So the page looks at least hourly, and a snooze that ran
 * out while the Mac slept is dropped within the hour after it wakes.
 */
export const LONGEST_WAIT_MS = 60 * 60 * 1000;

/**
 * How long from `nowMs` until the earliest snooze still running runs out,
 * at most `LONGEST_WAIT_MS`; null with none running.
 */
export function nextSnoozeLookMs(settings: Pick<Settings, "snoozed_updates">, nowMs: number): number | null {
  const running = activeSnoozes(settings, nowMs);
  if (running.length === 0) return null;
  const earliest = Math.min(...running.map((snoozed) => snoozed.until * 1000));
  return Math.min(Math.max(earliest - nowMs, 0), LONGEST_WAIT_MS);
}

/** Mounted once, by `App`'s `UpdateWatchers`. */
export function useSnoozeExpiry(): void {
  const queryClient = useQueryClient();
  const { data: settings } = useSettings();
  // Bumped when a look finds nothing run out yet, to wait again.
  const [looks, setLooks] = useState(0);

  useEffect(() => {
    if (settings === undefined) return;
    const wait = nextSnoozeLookMs(settings, Date.now());
    if (wait === null) return;
    const timer = setTimeout(() => {
      const held = queryClient.getQueryData<Settings>(queryKeys.settings);
      if (held === undefined) return;
      const running = activeSnoozes(held);
      if (running.length === (held.snoozed_updates ?? []).length) {
        setLooks((n) => n + 1);
        return;
      }
      queryClient.setQueryData<Settings>(queryKeys.settings, { ...held, snoozed_updates: running });
    }, wait);
    return () => clearTimeout(timer);
  }, [settings, looks, queryClient]);
}
