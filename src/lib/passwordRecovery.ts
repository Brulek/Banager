import { useMemo } from "react";
import { recentUpdates, useHistory } from "./history";
import { useSnapshot } from "./queries";
import { NO_HISTORY, type OpSummary, type UpdateCandidate } from "./types";
import { adapterIdOf } from "./sources";
import { artifactKeyId } from "../store/ui";

/**
 * The tools whose newest kept update stopped for the Mac's password, while
 * the last check still offers them one. An operation the tool's row still
 * shows (`operationFor`, the page's `useUpdateOperationFor`) supersedes
 * history: that row says how it stands, View Steps of a stop of its own
 * included. One the row no longer shows does not: after the page reloads
 * (the error screen's Reload) the window forgets which version each
 * update was for, and the backend still lists this launch's stop -- its
 * record then gives the row View Steps as after a restart, not a checkbox
 * and a plain Update counted again (r35 U3). Clear does not supersede it
 * either: it marks every record dismissed
 * (`HistoryStore::clear`), which tidies 「最近的更新记录」 but does not resolve
 * the stop, so the records it dismissed count here too (`includeDismissed`).
 * Per tool, not per version: a stop kept on one version still counts while
 * the source offers a newer one, within `RECENT_DAYS`. The step that asked
 * for the password is the cask's own, which a newer version most likely
 * runs too, and View Steps plans whichever update is offered.
 */
export function usePasswordRecoveryKeys(
  operationFor: (candidate: UpdateCandidate) => OpSummary | null,
): ReadonlySet<string> {
  const { data: history = NO_HISTORY } = useHistory();
  const { data: snapshot } = useSnapshot();
  return useMemo(() => {
    const updates = snapshot?.updates ?? [];
    const offered = new Set(updates.map((candidate) => artifactKeyId(candidate.key)));
    const shownInRows = new Set(
      updates.flatMap((candidate) => (operationFor(candidate) === null ? [] : [artifactKeyId(candidate.key)])),
    );
    return new Set(recentUpdates(history, shownInRows, Date.now(), offered, { includeDismissed: true })
      .filter((record) => adapterIdOf(record.key.instance_id) === "brew" &&
        typeof record.result === "object" && "Failed" in record.result && record.result.Failed.cause === "needsPassword")
      .map((record) => artifactKeyId(record.key)));
  }, [history, operationFor, snapshot]);
}
