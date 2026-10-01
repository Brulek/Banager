import { useTranslation } from "react-i18next";
import { useOperations, useSettings } from "../lib/queries";
import { outcomeWords } from "../lib/operations";
import { namesInSentence } from "../lib/sources";
import { artifactKeyId, useUiStore } from "../store/ui";
import { CloseIcon } from "./icons";
import { BUTTON, SMALL_ICON_BUTTON } from "./ui/controls";
import { SMALL_WRAPPING } from "./ui/group";

/**
 * What the last batch uninstall did not uninstall (spec §6.6), at the top
 * of the Installed list once every operation it started has finished: 「2
 * 个没有卸载」, then each of them with how it ended, in the operation bar's
 * words (`outcomeWords`: 「未能完成」, 「已取消」, a failure's cause), and its
 * 查看日志. Under a Homebrew formula whose dependent in the batch did not
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
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const { data: operations } = useOperations();
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;
  if (record === null || operations === undefined) return null;
  const byOp = new Map(operations.map((op) => [op.id, op]));
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
  const viewLog = (opId: number) => {
    setFocusedOpId(opId);
    setDrawerOpen(true);
  };
  return (
    <section aria-label={t("batchUninstall.result", { count: notUninstalled.length })} data-batch-result="" className="px-5 pb-2 pt-2">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p role="alert" className="text-body text-danger-text">
            {t("batchUninstall.result", { count: notUninstalled.length })}
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
                    <span className={`text-muted ${SMALL_WRAPPING}`}>{outcomeWords(t, op.outcome, technical)}</span>
                    <button
                      type="button"
                      aria-label={t("batchUninstall.viewLogOf", { name: item.name })}
                      onClick={() => viewLog(op.id)}
                      className={BUTTON.small.grey}
                    >
                      {t("common.viewLog")}
                    </button>
                  </div>
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
        </div>
        <button type="button" aria-label={t("batchUninstall.resultDismiss")} onClick={dismiss} className={SMALL_ICON_BUTTON}>
          <CloseIcon size={16} />
        </button>
      </div>
    </section>
  );
}
