/**
 * The Installed list on screen while the first check since launch is still
 * checking for updates. That check's snapshot comes only once every source
 * has answered every question -- the update checks ask every registry over
 * the network, and Homebrew's `brew update` alone can take two minutes --
 * but what each source has installed is known within seconds. The backend
 * sends that much first (`UiEvent.InventoryPreview`, kept by
 * `writeInventoryPreview` in ./events.ts), and the Installed page lists it,
 * every Update and Uninstall off, until the snapshot arrives and takes its
 * place.
 *
 * Only the Installed page, its toolbar's count and the sidebar's count of
 * what is installed read it. What reads updates -- the Updates page, the
 * Overview's verdict, the Dock's badge, the update notification -- reads
 * the snapshot alone, which is still the startup placeholder: a list with
 * no updates in it is no answer that there are none.
 */
import { useEffect, useMemo } from "react";
import { skipToken, useQuery, useQueryClient } from "@tanstack/react-query";
import { isStartupSnapshot } from "./events";
import { useSnapshot } from "./queries";
import { queryKeys } from "./queryKeys";
import type { InventoryPreview, Snapshot } from "./types";

/**
 * The first check's list (`InventoryPreview`) while the snapshot cache has
 * nothing but the startup placeholder -- or nothing yet -- and the list has
 * something in it; otherwise null. A list with nothing in it shows nothing
 * early: "Checking…" says as much until the snapshot comes. Once a real
 * snapshot is in, the list held is dropped for good, wherever this is
 * mounted.
 */
export function useInventoryPreview(): InventoryPreview | null {
  const queryClient = useQueryClient();
  const { data: snapshot } = useSnapshot();
  // Never fetched: only `writeInventoryPreview` writes it.
  const { data: held } = useQuery<InventoryPreview | null>({
    queryKey: queryKeys.inventoryPreview,
    queryFn: skipToken,
  });
  const answered = snapshot !== undefined && !isStartupSnapshot(snapshot);
  useEffect(() => {
    if (answered && held) queryClient.setQueryData(queryKeys.inventoryPreview, null);
  }, [answered, held, queryClient]);
  if (answered || !held || held.artifacts.length === 0) return null;
  return held;
}

/**
 * `preview` as the Installed page reads a snapshot: its sources and its
 * rows, no update, no error, nothing stale. Never a round's answer -- round
 * 0, no timestamp -- and never written to the snapshot cache: it is built
 * for the page that lists it and handed to nothing else.
 */
export function previewSnapshot(preview: InventoryPreview): Snapshot {
  return {
    generation: 0,
    round: 0,
    detect: "Found",
    instances: preview.instances,
    artifacts: preview.artifacts,
    updates: [],
    refreshed_at: null,
    stale: false,
    errors: [],
  };
}

/**
 * What the Installed page lists: the snapshot (`useSnapshot`), or, while
 * the first check is still checking for updates and its list is in
 * (`useInventoryPreview`), that list (`previewSnapshot`) with `preview`
 * true -- the page then offers nothing to do: `Session::issue_plan` would
 * refuse it anyway, since nothing it could plan against is committed.
 */
export function useInstalledSnapshot(): {
  data: Snapshot | undefined;
  isLoading: boolean;
  preview: boolean;
} {
  const query = useSnapshot();
  const preview = useInventoryPreview();
  // One object per list held: the page memoizes on its snapshot.
  const previewed = useMemo(() => (preview === null ? null : previewSnapshot(preview)), [preview]);
  if (previewed !== null) return { data: previewed, isLoading: false, preview: true };
  return { data: query.data, isLoading: query.isLoading, preview: false };
}
