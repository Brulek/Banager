import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { OpSummary } from "../lib/types";
import { outcomeCause } from "../lib/failureCause";
import type { LogEntry } from "../store/ui";
import { CopyButton } from "./CopyButton";
import { DisclosureButton } from "./DisclosureButton";
import { TRY_AGAIN_KEYS, subtitleStep } from "./FailureNextStep";

/**
 * The words a failure kept (`Failed.summary`: the last lines of the
 * tool's stderr, a login masked out of them) when this window's log has
 * none of its lines any more -- it keeps the newest 2,000, and a reloaded
 * web view keeps none -- or null. Only a summary that is not empty: its
 * words were lines the log once had, so "no longer available" is true; a
 * tool that wrote nothing to stderr may have written nothing at all. The
 * core puts a failure's words in the log before its outcome: a command's
 * stderr as it runs (`run_plan`), the stderr of the read npm and uv take
 * before a command when it ends the operation (`read_before_run`, both in
 * crates/banager-core/src/adapters/mod.rs), and macOS's refusal to move a
 * path to the Trash as a note (`removal::execute_removal`).
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
 * setting, which nothing here would point to -- with Copy Error Details
 * under them, as a source's kept words have (`SourceDiagnostic`): Copy Log
 * has no line left to copy, and the step says to send them (r21 C9). With
 * it on, the subtitle has the words, `SubtitleStep` the step and
 * `SubtitleWordsCopy` their Copy Error Details. Keyed by the operation
 * where it is used, so each log of a run opens with its details closed.
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
            <>
              <p
                id={panelId}
                className="mt-1 max-h-48 select-text overflow-y-auto whitespace-pre-wrap break-words rounded-control bg-group px-2.5 py-2 font-mono text-small text-foreground"
              >
                {summary}
              </p>
              {/* At the left under the words, as the saved warnings' Copy Command. */}
              <div className="mt-2 flex items-center justify-start">
                <CopyButton text={summary} label={t("failureRecovery.copy")} size="regular" />
              </div>
            </>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/**
 * With "Show technical details" on, the words a failure kept where the
 * log's subtitle is the only place left that shows them, or null: this
 * window's log has none of the operation's lines (`missingLogSummary`),
 * or none of its stderr lines, which the step under the subtitle says
 * (`subtitleStep`). The subtitle does not select, and Copy Log has none
 * of those words to copy.
 */
export function subtitleWordsToCopy(op: OpSummary, logs: readonly LogEntry[], technical: boolean): string | null {
  if (!technical) return null;
  const gone = missingLogSummary(op, logs);
  if (gone !== null) return gone;
  if (subtitleStep(op, logs, technical) === null) return null;
  const outcome = op.outcome;
  if (outcome === null || typeof outcome === "string" || !("Failed" in outcome)) return null;
  return outcome.Failed.summary.trim();
}

/**
 * Copy Error Details for the words in the log's subtitle
 * (`subtitleWordsToCopy`), at the left under the step that names it, as
 * it is under the unfolded words with technical details off
 * (`MissingFailureLog`): it copies those words alone.
 */
export function SubtitleWordsCopy({
  op,
  logs,
  technical,
}: {
  op: OpSummary;
  logs: readonly LogEntry[];
  technical: boolean;
}) {
  const { t } = useTranslation();
  const words = subtitleWordsToCopy(op, logs, technical);
  if (words === null) return null;
  return (
    <div className="mb-3 flex items-center justify-start">
      <CopyButton text={words} label={t("failureRecovery.copy")} size="regular" />
    </div>
  );
}
