import { useMemo } from "react";
import { recentUpdates, useHistory } from "./history";
import { useOperations, useSnapshot } from "./queries";
import { NO_HISTORY } from "./types";
import { adapterIdOf } from "./sources";
import { artifactKeyId } from "../store/ui";

/** A current operation supersedes history. Clearing its display is not resolving a password stop. */
export function usePasswordRecoveryKeys(): ReadonlySet<string> {
  const { data: history = NO_HISTORY } = useHistory();
  const { data: operations = [] } = useOperations();
  const { data: snapshot } = useSnapshot();
  return useMemo(() => {
    const offered = new Set((snapshot?.updates ?? []).map((candidate) => artifactKeyId(candidate.key)));
    return new Set(recentUpdates({ ...history, cleared_before: null }, operations, Date.now(), offered)
      .filter((record) => adapterIdOf(record.key.instance_id) === "brew" &&
        typeof record.result === "object" && "Failed" in record.result && record.result.Failed.cause === "needsPassword")
      .map((record) => artifactKeyId(record.key)));
  }, [history, operations, snapshot]);
}
