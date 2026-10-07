import { useMemo } from "react";
import { recentUpdates, useHistory } from "./history";
import { useOperations, useSnapshot } from "./queries";
import { NO_HISTORY } from "./types";
import { adapterIdOf } from "./sources";
import { artifactKeyId } from "../store/ui";

/**
 * The tools whose newest kept update stopped for the Mac's password, while
 * the last check still offers them one. A current operation supersedes
 * history; Clear does not: it marks every record dismissed
 * (`HistoryStore::clear`), which tidies 「最近的更新记录」 but does not resolve
 * the stop, so the records it dismissed count here too (`includeDismissed`).
 */
export function usePasswordRecoveryKeys(): ReadonlySet<string> {
  const { data: history = NO_HISTORY } = useHistory();
  const { data: operations = [] } = useOperations();
  const { data: snapshot } = useSnapshot();
  return useMemo(() => {
    const offered = new Set((snapshot?.updates ?? []).map((candidate) => artifactKeyId(candidate.key)));
    return new Set(recentUpdates(history, operations, Date.now(), offered, { includeDismissed: true })
      .filter((record) => adapterIdOf(record.key.instance_id) === "brew" &&
        typeof record.result === "object" && "Failed" in record.result && record.result.Failed.cause === "needsPassword")
      .map((record) => artifactKeyId(record.key)));
  }, [history, operations, snapshot]);
}
