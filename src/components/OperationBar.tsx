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
            status: t(`operations.status.${current.status}`),
          })}
        </span>
        {current.status === "Done" && current.outcome ? (
          <span className="text-[var(--color-muted)]">
            {t(`operations.outcome.${outcomeKey(current.outcome)}`, outcomeArgs(current.outcome))}
          </span>
        ) : null}
      </button>
      {isActive ? (
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
