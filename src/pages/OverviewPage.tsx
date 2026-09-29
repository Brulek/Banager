import { Fragment, useId } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOpenOllamaApp, useSettings, useSnapshot } from "../lib/queries";
import { isStartupSnapshot } from "../lib/events";
import { elapsedSince } from "../lib/format";
import {
  ADAPTER_LABEL_KEYS,
  openOllamaErrorDetail,
  openOllamaErrorMessage,
  sourceNoticesFor,
} from "../lib/sources";
import type { SourceNoticeSpec } from "../lib/sources";
import { updatesSummary } from "../lib/updateState";
import type { UpdatesSummary } from "../lib/updateState";
import type { ManagerInstance, Settings } from "../lib/types";
import { useUiStore } from "../store/ui";
import { holdsRow, isUnderway, useUpdateOperationFor } from "../components/UpdateProgress";
import { CHECKED_KEYS, elapsedText, useMinuteClock } from "../components/PageHeader";
import { DETAILS_TRIGGER_CLASS } from "../components/SourceNotice";
import { FilledWarningIcon, StatusSymbol, type StatusSymbolKind } from "../components/StatusSymbol";
import { ChevronIcon, InfoIcon } from "../components/icons";
import { Popover } from "../components/ui/Popover";
import { BUTTON, LINK } from "../components/ui/controls";
import { FORM_COLUMN, GROUP, GROUP_ROW, GROUP_WITH_ICONS, SMALL_WRAPPING } from "../components/ui/group";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/** The status's title. A `switch` with no default, so a new summary without words here fails `tsc`. */
function headlineText(t: Translate, summary: UpdatesSummary): string {
  switch (summary.kind) {
    case "updates":
      return t("overview.updatesAvailable", { count: summary.actionable.length });
    case "upToDate":
      return t("overview.upToDate");
    case "updating":
      return t("overview.updating", { count: summary.count });
    case "nothingToUpdate":
      return summary.everyChecked ? t("overview.nothingToUpdate") : t("overview.nothingToUpdateChecked");
  }
}

/** The status's symbol for a verdict. A `switch` with no default, as `headlineText`. */
function symbolOf(summary: UpdatesSummary): StatusSymbolKind {
  switch (summary.kind) {
    case "updates":
      return "updates";
    case "upToDate":
      return "upToDate";
    case "updating":
      return "busy";
    case "nothingToUpdate":
      return "quiet";
  }
}

/**
 * The line under "Nothing to update": what there is instead, in the
 * Updates page's own numbers (`updatesSummary`) -- the updates the user
 * hid, the ones under its "Can't update here", the checks that did not
 * finish -- or null when there is none of that. A source not checked in
 * full says so in the problems group under the status instead.
 *
 * The Updates page lists no hidden update, and Settings lists them all,
 * under 「已跳过的版本」 and 「不再提醒的工具」: their count is the way
 * there (`showHidden`), a link in the line -- the one link on the page.
 * The rest of it is text.
 */
function nothingToUpdateLine(
  t: Translate,
  summary: Extract<UpdatesSummary, { kind: "nothingToUpdate" }>,
  showHidden: () => void,
): ReactNode {
  const parts: ReactNode[] = [];
  if (summary.hidden > 0) {
    parts.push(
      <button type="button" onClick={showHidden} className={LINK}>
        {t("overview.hiddenCount", { count: summary.hidden })}
      </button>,
    );
  }
  if (summary.cantUpdateHere > 0) {
    parts.push(t("overview.cantUpdateHereCount", { count: summary.cantUpdateHere }));
  }
  if (summary.checksUnfinished > 0) {
    parts.push(t("overview.checksUnfinished", { count: summary.checksUnfinished }));
  }
  if (parts.length === 0) return null;
  return parts.map((part, index) => (
    <Fragment key={index}>
      {index > 0 ? t("overview.listSeparator") : null}
      {part}
    </Fragment>
  ));
}

/**
 * One source's problem, a row of the group under the status: the notice
 * the Updates and Installed pages give it (`sourceNoticesFor`, in their
 * words) -- a filled orange ⚠︎ for a warning, a muted ⓘ for news -- its
 * title, its description under it, and on the right its own button where
 * it has one: Open Ollama, Check again, wired as the lists wire theirs
 * (`SourceNotices`), Check again off while a check runs. A press of Open
 * Ollama that failed says so under the description, with its Details.
 */
function ProblemRow({ notice }: { notice: SourceNoticeSpec }) {
  const { t } = useTranslation();
  const openOllamaApp = useOpenOllamaApp();
  const { checkAgain, checking } = useCheckAgain();
  const opensOllama = notice.action?.id === "openOllama";

  let error: ReactNode = null;
  if (opensOllama && openOllamaApp.error) {
    const message = openOllamaErrorMessage(t, openOllamaApp.error.message);
    const detail = openOllamaErrorDetail(t, openOllamaApp.error.message);
    error =
      detail === null ? (
        message
      ) : (
        <>
          {message}{" "}
          <Popover
            trigger={t("common.details")}
            triggerLabel={t("common.detailsLabel", { title: message })}
            triggerClassName={DETAILS_TRIGGER_CLASS}
          >
            {detail}
          </Popover>
        </>
      );
  }

  return (
    <li className="flex min-h-11.5 items-center gap-4 px-2.5 py-2">
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {notice.variant === "warning" ? (
          <FilledWarningIcon size={16} className="text-warning" />
        ) : (
          <InfoIcon size={16} className="shrink-0 text-muted" />
        )}
        <div className="min-w-0">
          <p className="text-body text-foreground">{t(notice.titleKey, notice.values)}</p>
          <p className={`${SMALL_WRAPPING} text-muted`}>{t(notice.descriptionKey, notice.values)}</p>
          {/* A <div>: the error's own "Details" panel is one. */}
          {error !== null ? (
            <div role="alert" className="text-small text-danger-text">
              {error}
            </div>
          ) : null}
        </div>
      </div>
      {notice.action ? (
        <button
          type="button"
          onClick={opensOllama ? () => openOllamaApp.mutate() : checkAgain}
          disabled={opensOllama ? false : checking}
          className={BUTTON.regular.grey}
        >
          {t(notice.action.labelKey)}
        </button>
      ) : null}
    </li>
  );
}

/**
 * The status row, the first group's one row: a 48 symbol, the title in
 * 13 bold with a line under it in 11 muted, and on the right the row's one
 * button. `alert`: the title and its line are read out as they appear --
 * mounted afresh, so that a screen reader says them.
 */
function StatusRow({
  symbol,
  title,
  line,
  button,
  alert = false,
}: {
  symbol: StatusSymbolKind;
  title: string;
  line: ReactNode;
  button: ReactNode;
  alert?: boolean;
}) {
  return (
    <div className={GROUP}>
      <div data-status={symbol} className="flex items-center gap-3 px-2.5 py-2.5">
        <StatusSymbol kind={symbol} />
        <div key={alert ? "alert" : "status"} role={alert ? "alert" : undefined} className="min-w-0 flex-1">
          <h2 className="text-title text-foreground">{title}</h2>
          {line !== null ? <p className={`${SMALL_WRAPPING} text-muted`}>{line}</p> : null}
        </div>
        {button}
      </div>
    </div>
  );
}

/**
 * The daily check, on or off, as Software Update shows its automatic
 * updates -- 「打开」/「关闭」, as System Settings words a switch's state --
 * a row that opens Settings, where it is changed.
 */
function AutoCheckRow({ settings }: { settings: Settings }) {
  const { t } = useTranslation();
  const setPage = useUiStore((s) => s.setPage);
  const labelId = useId();
  const valueId = useId();
  return (
    <div className={GROUP}>
      <button
        type="button"
        onClick={() => setPage("settings")}
        aria-labelledby={`${labelId} ${valueId}`}
        className={`${GROUP_ROW} w-full text-left`}
      >
        <span id={labelId} className="min-w-0 truncate text-body text-foreground">
          {t("settings.autoCheck.label")}
        </span>
        <span className="flex shrink-0 items-center gap-1 text-body text-muted">
          <span id={valueId}>{settings.auto_check ? t("overview.autoCheckOn") : t("overview.autoCheckOff")}</span>
          <ChevronIcon size={14} className="text-tertiary" />
        </span>
      </button>
    </div>
  );
}

/**
 * The first page, laid out as System Settings' Software Update (spec R1):
 * a column of groups, as wide as Settings' and at its top.
 *
 * First, one row: where the updates stand (`StatusRow`). The Updates
 * page's own verdict (`updatesSummary`) as its title -- how many updates
 * it offers to install, "Everything is up to date" only when that page
 * would say so, and a plain "Nothing to update" when there is nothing to
 * install but that is not the same thing ("No updates in the sources
 * checked" where a source was not checked in full) -- with a symbol for it
 * on the left (`StatusSymbol`) and a line under it: when the sources were
 * last checked, or what there is instead of updates
 * (`nothingToUpdateLine`). On the right, always one button, the one thing
 * to do: Review Updates, the default button, which opens the Updates page
 * with all of them selected; a grey one where that page lists only what
 * cannot be updated here; See Progress while they install; otherwise a
 * grey Check Again, as macOS's empty states offer, off while a check runs.
 * The number of updates is said once, in the title.
 *
 * While a check runs, the row keeps what the last one found -- its symbol,
 * its title, its button -- and only its line says 「正在检查…」: the
 * toolbar's ⟳ is already turning. When the last check failed --
 * `startupRefreshError`, which every refresh sets or clears -- the row
 * says so instead, as an alert, with its reason, and Check Again becomes
 * its default button: what is listed is from the check before it. Before
 * the first check has answered, the same row, turning, says 「正在检查…」
 * and why that takes a while -- the startup placeholder is not an answer
 * (`isStartupSnapshot`) -- so that nothing on the page moves when the
 * answer comes.
 *
 * Second, the daily check, on or off (`AutoCheckRow`).
 *
 * Last, only when a source has something to say, a group of one row for
 * each: its first notice (`sourceNoticesFor`: not running, not answering,
 * a list it could not download, another program that runs instead), with
 * its own button (`ProblemRow`). What a source lets Canager do at all,
 * pip being read-only, is not news here; both lists say it on each of its
 * rows. The sources themselves are in the sidebar.
 */
export function OverviewPage() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const setPage = useUiStore((s) => s.setPage);
  const showHiddenUpdates = useUiStore((s) => s.showHiddenUpdates);
  const selectUpdates = useUiStore((s) => s.selectUpdates);
  const checkFailure = useUiStore((s) => s.startupRefreshError);
  const { checkAgain, checking } = useCheckAgain();
  const operationFor = useUpdateOperationFor();
  const refreshedAt = snapshot?.refreshed_at ?? null;
  const now = useMinuteClock(refreshedAt);

  if (!snapshot || !settings || isStartupSnapshot(snapshot)) {
    return (
      <div className={FORM_COLUMN}>
        <StatusRow symbol="busy" title={t("common.checking")} line={t("common.firstCheckDetail")} button={null} />
        {settings ? <AutoCheckRow settings={settings} /> : null}
      </div>
    );
  }

  const labelOf = (instance: ManagerInstance): string => {
    const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
    return labelKey ? t(labelKey) : instance.adapter_id;
  };

  const installedByInstance = new Map<string, number>();
  for (const artifact of snapshot.artifacts) {
    const id = artifact.key.instance_id;
    installedByInstance.set(id, (installedByInstance.get(id) ?? 0) + 1);
  }

  const summary = updatesSummary(
    snapshot,
    settings,
    (candidate) => holdsRow(operationFor(candidate)),
    (candidate) => isUnderway(operationFor(candidate)),
  );

  const problems: SourceNoticeSpec[] = snapshot.instances.flatMap((instance) => {
    const [notice] = sourceNoticesFor(instance, labelOf(instance), installedByInstance.get(instance.id) ?? 0);
    return notice === undefined ? [] : [notice];
  });

  // A check that failed says so until one works, but not while the next
  // one runs.
  const failed = !checking && checkFailure !== null;
  const lastChecked = refreshedAt === null ? null : elapsedText(t, CHECKED_KEYS, elapsedSince(refreshedAt, now));
  let line: ReactNode = null;
  if (failed) {
    line = t("emptyStates.loadFailed.description", { message: checkFailure });
  } else if (checking) {
    line = t("common.checking");
  } else if (summary.kind === "nothingToUpdate") {
    line = nothingToUpdateLine(t, summary, showHiddenUpdates) ?? lastChecked;
  } else if (summary.kind !== "updating") {
    line = lastChecked;
  }

  const checkAgainButton = (kind: "grey" | "default") => (
    <button type="button" onClick={checkAgain} disabled={checking} className={BUTTON.regular[kind]}>
      {t("header.checkAgain")}
    </button>
  );
  let button: ReactNode;
  if (failed) {
    // The one thing to do now: check again.
    button = checkAgainButton("default");
  } else if (summary.kind === "updates") {
    button = (
      <button
        type="button"
        onClick={() => {
          // Every row the Updates page would tick with Select all, and
          // no other; any row selected earlier stays as it was.
          selectUpdates(summary.actionable.map((candidate) => candidate.key));
          setPage("updates");
        }}
        className={BUTTON.regular.default}
      >
        {t("overview.reviewUpdates")}
      </button>
    );
  } else if (summary.kind === "updating") {
    // Where each one's progress is: in its own row.
    button = (
      <button type="button" onClick={() => setPage("updates")} className={BUTTON.regular.grey}>
        {t("overview.seeProgress")}
      </button>
    );
  } else if (summary.kind === "nothingToUpdate" && summary.cantUpdateHere > 0) {
    // The Updates page has rows to show, every one under "Can't update
    // here": nothing to select, only a page to open.
    button = (
      <button type="button" onClick={() => setPage("updates")} className={BUTTON.regular.grey}>
        {t("overview.reviewUpdates")}
      </button>
    );
  } else {
    button = checkAgainButton("grey");
  }

  return (
    <div className={FORM_COLUMN}>
      <StatusRow
        symbol={failed ? "failed" : symbolOf(summary)}
        title={failed ? t("header.checkFailed") : headlineText(t, summary)}
        line={line}
        button={button}
        alert={failed}
      />

      <AutoCheckRow settings={settings} />

      {problems.length > 0 ? (
        <ul aria-label={t("overview.attentionLabel")} className={GROUP_WITH_ICONS}>
          {problems.map((notice) => (
            <ProblemRow key={notice.id} notice={notice} />
          ))}
        </ul>
      ) : null}
    </div>
  );
}
