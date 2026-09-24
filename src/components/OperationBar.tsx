import { useTranslation } from "react-i18next";
import { useOperations, useCancelOperation } from "../lib/queries";
import { outcomeArgs, outcomeKey } from "../lib/format";
import { useUiStore } from "../store/ui";
import type { OpStatus } from "../lib/types";

const ACTIVE_STATUSES: OpStatus[] = ["Queued", "Running", "CancelRequested", "Cancelling", "Verifying"];

export function OperationBar() {
  const { t } = useTranslation();
  const { data: operations } = useOperations();
  const cancelMutation = useCancelOperation();
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const logs = useUiStore((s) => s.logs);

  // The backend lists operations newest first (Task 3). Showing the newest
  // one — not only an *active* one — is what lets the user see the outcome
  // of an update that finished while they were looking elsewhere; the bar
  // only goes back to idle once the list itself is empty.
  const current = operations?.[0];

  if (!current) {
    return (
      <div className="flex h-full items-center px-4 text-sm text-[var(--color-muted)]">
        {t("operations.idle")}
      </div>
    );
  }

  const isActive = ACTIVE_STATUSES.includes(current.status);
  // `OperationManager::cancel` (ops/mod.rs) refuses a `NoCancel` op at
  // every status, so a Cancel button for one would promise something the
  // backend will not do. No adapter produces `NoCancel` yet.
  const cancellable = current.cancel_policy !== "NoCancel";

  // "Running" alone used to be the only thing on screen while an install,
  // upgrade or uninstall waits for a `brew update` a refresh left running
  // (`BrewAdapter::wait_for_update`, up to `OP_UPDATE_WAIT`): the log
  // drawer said so (`LogNote::WaitingForBrewUpdate`), but this bar did
  // not, and it is the only thing visible before the drawer is opened.
  // There is no separate `OpStatus` for this -- `set_status` in
  // `ops/mod.rs` leaves the record's status at `Running` for the whole of
  // `execute`, including any time it spends inside `wait_for_update` --
  // so this reads the same log the drawer already renders
  // (`useUiStore().logs`) instead of adding one: the note this
  // operation's log most recently carried is the wait starting, and
  // nothing (no further `Log` or `Note` event) has arrived since to say
  // it ended.
  //
  // Gated on `current.status === "Running"`, not on `isActive`: `cancel()`
  // (`ops/mod.rs`) sets the record's status straight to `CancelRequested`
  // as soon as the user presses Cancel, independent of and before
  // `execute`/`wait_for_update` notice the cancellation, so a cancel
  // pressed during the wait leaves this operation `CancelRequested` (then
  // briefly `Cancelling`) while the last log line is still the same
  // `WaitingForBrewUpdate` note. Without this guard that combination read
  // as "waiting for Homebrew to finish updating" even though the op was
  // no longer just waiting -- it had a cancel in flight -- which told the
  // user their Cancel click had not registered.
  const opLogs = logs.filter((l) => l.opId === current.id);
  const lastOpLog = opLogs[opLogs.length - 1];
  const waitingForBrewUpdate =
    current.status === "Running" &&
    lastOpLog !== undefined &&
    "note" in lastOpLog &&
    "WaitingForBrewUpdate" in lastOpLog.note;

  return (
    <div className="flex h-full items-center justify-between gap-4 px-4">
      <button
        type="button"
        onClick={() => {
          setFocusedOpId(current.id);
          setDrawerOpen(true);
        }}
        className="flex min-w-0 flex-1 gap-2 truncate text-left text-sm text-[var(--color-foreground)]"
      >
        <span>
          {t("operations.current", {
            kind: t(`operations.kind.${current.kind}`),
            name: current.name,
            status: waitingForBrewUpdate
              ? t("operations.status.waitingForBrewUpdate")
              : t(`operations.status.${current.status}`),
          })}
        </span>
        {current.status === "Done" && current.outcome ? (
          <span className="text-[var(--color-muted)]">
            {t(`operations.outcome.${outcomeKey(current.outcome)}`, outcomeArgs(current.outcome))}
          </span>
        ) : null}
      </button>
      {isActive && cancellable ? (
        <button
          type="button"
          onClick={() => cancelMutation.mutate(current.id)}
          disabled={current.status === "CancelRequested" || current.status === "Cancelling"}
          className="shrink-0 rounded-md border border-[var(--color-border)] px-3 py-1 text-sm disabled:opacity-50"
        >
          {t("operations.cancel")}
        </button>
      ) : null}
    </div>
  );
}
