import { useTranslation } from "react-i18next";
import { useOperations, useSettings } from "../lib/queries";
import { outcomeWords } from "../lib/operations";
import { FAILURE_CAUSE_KEYS, outcomeCause } from "../lib/failureCause";
import { namesInSentence } from "../lib/sources";
import { artifactKeyId, useUiStore } from "../store/ui";
import { CloseIcon, WarningFilledIcon } from "./icons";
import { ResultRowStep } from "./FailureNextStep";
import { BUTTON, SMALL_ICON_BUTTON } from "./ui/controls";
import { focusOrFallback } from "./ui/focus";
import { SMALL_WRAPPING } from "./ui/group";

/**
 * What the last batch uninstall did not uninstall (spec §6.6), at the top
 * of the Installed list once every operation it started has finished: 「2
 * 个没有卸载」, then each of them with how it ended, in the operation bar's
 * words (`outcomeWords`: 「未能卸载」, 「已取消」, a failure's cause), and its
 * 查看日志 -- and, with technical details on, where that is the tool's own
 * words, whose they are and what to do next (`ResultRowStep`). Under a
 * Homebrew formula whose dependent in the batch did not
 * uninstall, the two facts that explain it, and no cause claimed beyond
 * them: that one did not uninstall, and Homebrew does not uninstall what
 * is still needed. Nothing while any of it still runs, nothing when all of
 * it succeeded, and nothing for a tool that never started (the sheet said
 * why). Its × puts it away; the next batch replaces it. The log drawer is
 * opened only by its buttons, never by itself.
 */
export function BatchUninstallResult() {
  const { t } = useTranslation();
  const record = useUiStore((s) => s.uninstallBatch);
  const dismiss = useUiStore((s) => s.dismissUninstallBatch);
  const selectUninstalls = useUiStore((s) => s.selectUninstalls);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const { data: operations } = useOperations();
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;
  if (record === null || operations === undefined) return null;
  const byOp = new Map(operations.map((op) => [op.id, op]));
  // One newer than every operation listed is not evicted but not listed
  // yet: the list is fetched a moment after a start (`refetchOperations`),
  // and until then the batch is still under way as far as this block knows.
  const newestListed = Math.max(0, ...operations.map((op) => op.id));
  if (record.items.some((item) => item.opId !== null && item.opId > newestListed)) return null;
  const started = record.items.flatMap((item) => {
    const op = item.opId === null ? undefined : byOp.get(item.opId);
    // An operation the backend no longer lists (it keeps the newest 200)
    // has nothing left to say.
    return op === undefined ? [] : [{ item, op }];
  });
  if (started.some(({ op }) => op.status !== "Done")) return null;
  const notUninstalled = started.filter(({ op }) => op.outcome !== "Succeeded");
  if (notUninstalled.length === 0) return null;
  const nameOf = new Map(record.items.map((item) => [artifactKeyId(item.key), item.name]));
  const succeeded = new Set(
    started.filter(({ op }) => op.outcome === "Succeeded").map(({ item }) => artifactKeyId(item.key)),
  );
  // 「已卸载2个，1个没有卸载」 when some did: the block says what became
  // of the whole batch, not only of what went wrong.
  const heading =
    succeeded.size > 0
      ? t("batchUninstallMore.mixed", { done: succeeded.size, count: notUninstalled.length })
      : t("batchUninstall.result", { count: notUninstalled.length });
  // A known cause in a sentence that says what to do (`failure.line`), as
  // the row's ⓘ says it; the tool's own words with technical details on.
  const causeLine = (outcome: (typeof notUninstalled)[number]["op"]["outcome"]): string | null => {
    if (technical) return null;
    const cause = outcomeCause(outcome);
    return cause === null ? null : t(FAILURE_CAUSE_KEYS[cause].line);
  };
  const viewLog = (opId: number) => {
    setFocusedOpId(opId);
    setDrawerOpen(true);
  };
  return (
    <section aria-label={heading} data-batch-result="" className="px-5 pb-2 pt-2">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          {/* In the label colour with a ⚠︎, as System Settings says a partial
              result: what did not happen is not an error of the page's. */}
          <p role="alert" className="flex items-center gap-1.5 text-body text-foreground">
            <WarningFilledIcon size={14} className="shrink-0 text-warning" />
            {heading}
          </p>
          <ul className="mt-1 flex flex-col gap-1.5">
            {notUninstalled.map(({ item, op }) => {
              // Its dependents in the batch that did not uninstall: Homebrew
              // keeps what is still needed.
              const waitedFor = item.after.filter((id) => !succeeded.has(id)).map((id) => nameOf.get(id) ?? id);
              return (
                <li key={op.id} data-batch-result-item="">
                  <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                    <span className={`text-foreground ${SMALL_WRAPPING}`}>{item.name}</span>
                    <span className={`text-muted ${SMALL_WRAPPING}`}>{causeLine(op.outcome) ?? outcomeWords(t, op.outcome, op.kind, technical)}</span>
                    <button
                      type="button"
                      aria-label={t("batchUninstall.viewLogOf", { name: item.name })}
                      onClick={() => viewLog(op.id)}
                      className={BUTTON.small.grey}
                    >
                      {t("common.viewLog")}
                    </button>
                  </div>
                  {/* With technical details on, the tool's own words are
                      the row's: whose they are, and what to do next. */}
                  <ResultRowStep op={op} technical={technical} className={`mt-0.5 text-muted ${SMALL_WRAPPING}`} />
                  {waitedFor.length > 0 ? (
                    <p className={`mt-0.5 text-muted ${SMALL_WRAPPING}`}>
                      {t("batchUninstall.resultStillNeeded", {
                        names: namesInSentence(
                          t,
                          waitedFor.map((name) => t("batchUninstall.quoted", { name })),
                        ),
                        count: waitedFor.length,
                      })}
                    </p>
                  ) : null}
                </li>
              );
            })}
          </ul>
          {/* The way to try again: they are ticked once more, for 「卸载所选」
              to preview them again -- nothing starts from here. */}
          <button
            type="button"
            onClick={() => selectUninstalls(notUninstalled.map(({ item }) => item.key))}
            className={`mt-2 ${BUTTON.small.grey}`}
          >
            {t("reviewFixes.selectAgain", { count: notUninstalled.length })}
          </button>
        </div>
        <button
          type="button"
          aria-label={t("batchUninstall.resultDismiss")}
          onClick={(event) => {
            // The × goes with the block: the focus it had goes to the
            // page's title (`focusOrFallback`) rather than the window's
            // body, as Settings' Stop Skipping does when its row goes. A
            // click that left the focus elsewhere leaves it there.
            const had = document.activeElement;
            dismiss();
            if (had === event.currentTarget || had === null || had === document.body) focusOrFallback(null);
          }}
          className={SMALL_ICON_BUTTON}
        >
          <CloseIcon size={16} />
        </button>
      </div>
    </section>
  );
}
