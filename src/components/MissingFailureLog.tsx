import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { OpSummary } from "../lib/types";
import { outcomeCause } from "../lib/failureCause";
import type { LogEntry } from "../store/ui";
import { DisclosureButton } from "./DisclosureButton";
import { TRY_AGAIN_KEYS } from "./FailureNextStep";

/**
 * The words a failure kept (`Failed.summary`: the last lines of the
 * tool's stderr, a login masked out of them) when this window's log has
 * none of its lines any more -- it keeps the newest 2,000, and a reloaded
 * web view keeps none -- or null. Only a summary that is not empty: its
 * words were lines the log once had, so "no longer available" is true; a
 * tool that wrote nothing to stderr may have written nothing at all.
 */
export function missingLogSummary(op: OpSummary, logs: readonly LogEntry[]): string | null {
  if (op.status !== "Done") return null;
  const outcome = op.outcome;
  if (outcome === null || typeof outcome === "string" || !("Failed" in outcome)) return null;
  const summary = outcome.Failed.summary.trim();
  if (summary === "") return null;
  return logs.some((line) => line.opId === op.id) ? null : summary;
}

/**
 * Over the empty log of a failure that kept its words
 * (`missingLogSummary`): that the log is no longer available; with
 * technical details off, how to try again where no cause's step is over
 * the log already, and the words themselves behind Show Error Details, the
 * app's own disclosure row -- reachable without the technical-details
 * setting, which nothing here would point to. With it on, the subtitle
 * has the words and `SubtitleStep` the step. Keyed by the operation where
 * it is used, so each log of a run opens with its details closed.
 */
export function MissingFailureLog({
  op,
  summary,
  id,
  technical,
}: {
  op: OpSummary;
  summary: string;
  id: string;
  technical: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const panelId = useId();
  return (
    <div className="mb-3 flex flex-col gap-1 text-body text-foreground">
      {/* What the dialog says it is about as it opens: both sentences. */}
      <div id={id} className="flex flex-col gap-1">
        <p className="break-words">{t("failureRecovery.logGone")}</p>
        {!technical && outcomeCause(op.outcome) === null ? (
          <p className="break-words">{t("failureRecovery.next", { again: t(TRY_AGAIN_KEYS[op.kind]) })}</p>
        ) : null}
      </div>
      {!technical ? (
        <div>
          <DisclosureButton open={open} panelId={panelId} onToggle={() => setOpen(!open)}>
            {t("failureRecovery.details")}
          </DisclosureButton>
          {open ? (
            <p
              id={panelId}
              className="mt-1 max-h-48 select-text overflow-y-auto whitespace-pre-wrap break-words rounded-control bg-group px-2.5 py-2 font-mono text-small text-foreground"
            >
              {summary}
            </p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
