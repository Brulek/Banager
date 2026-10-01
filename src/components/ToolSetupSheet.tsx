import { useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useSystemFacts } from "../lib/diagnostics";
import { elapsedSince } from "../lib/format";
import { previewSnapshot, useInventoryPreview } from "../lib/inventoryPreview";
import { isStartupSnapshot } from "../lib/events";
import { useSettings, useSizes, useSnapshot } from "../lib/queries";
import {
  toolSetupCheck,
  useToolSetupSheet,
  type SetupLine,
  type SetupSection,
  type SetupSymbol,
  type SetupView,
} from "../lib/toolSetupCheck";
import { useUiStore } from "../store/ui";
import { CheckCircleIcon, SpinnerIcon } from "./icons";
import { TextWithInfo } from "./InfoDetail";
import { CHECKED_KEYS, elapsedText, useMinuteClock } from "./PageHeader";
import { FilledWarningIcon } from "./StatusSymbol";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { GROUP_TITLE, GROUP_WITH_ICONS, SMALL_WRAPPING } from "./ui/group";

/** A line's symbol at 16, in the column its words start 8 after: the Overview's problems' own. */
function LineSymbol({ kind }: { kind: SetupSymbol }) {
  switch (kind) {
    case "warning":
      return <FilledWarningIcon size={16} className="text-warning" />;
    case "note":
      // No glyph: an ⓘ in this app is a button that explains, and a line's
      // own ⓘ is one -- two on a line read as two things to press.
      return <span aria-hidden="true" className="w-4 shrink-0" />;
    case "busy":
      return <SpinnerIcon size={16} className="shrink-0 text-muted" />;
    case "fine":
      return <CheckCircleIcon size={16} className="shrink-0 text-muted" />;
    default: {
      const unhandled: never = kind;
      return unhandled;
    }
  }
}

/**
 * One line: its symbol, its words -- with an ⓘ at their end where it has a
 * longer why -- and under them, 11 muted, the sources it means or the
 * folders it names; on the right its 查看, which closes the sheet and
 * opens the list the line counted.
 */
function Line({ line, onView }: { line: SetupLine; onView: (view: SetupView) => void }) {
  const { t } = useTranslation();
  const view = line.view;
  return (
    <li data-setup-line={line.id} data-symbol={line.symbol} className="flex min-h-9 items-center gap-4 px-2.5 py-1.5">
      {/* The symbol by the first line of the words, however many there are. */}
      <div className="flex min-w-0 flex-1 items-start gap-2">
        <LineSymbol kind={line.symbol} />
        <div className="min-w-0">
          <p className="break-words text-body text-foreground">
            {line.detail === null ? (
              line.text
            ) : (
              <TextWithInfo text={line.text} label={t("common.detailsLabel", { title: line.text })}>
                {line.detail}
              </TextWithInfo>
            )}
          </p>
          {line.secondary !== null ? (
            <p className={`${SMALL_WRAPPING} select-text break-words text-muted`}>{line.secondary}</p>
          ) : null}
        </div>
      </div>
      {view !== null ? (
        <button
          type="button"
          onClick={() => onView(view)}
          aria-label={t("setupCheck.viewLabel", { line: line.text })}
          className={BUTTON.regular.grey}
        >
          {t("families.view")}
        </button>
      ) : null}
    </li>
  );
}

/** One section: its title over a group of its lines, as Settings' groups are drawn. */
function Section({ section, onView }: { section: SetupSection; onView: (view: SetupView) => void }) {
  const titleId = useId();
  return (
    <section aria-labelledby={titleId} data-setup-section={section.id} className="mt-4">
      <h3 id={titleId} className={GROUP_TITLE}>
        {section.title}
      </h3>
      <ul className={GROUP_WITH_ICONS}>
        {section.lines.map((line) => (
          <Line key={line.id} line={line} onView={onView} />
        ))}
      </ul>
    </section>
  );
}

/**
 * 「工具环境」, the sheet Help's 「检查工具环境…」 and Settings' 「检查…」 open
 * (`useToolSetupSheet`): how this Mac's command-line tools are set up, in
 * sections of short lines (`toolSetupCheck`, src/lib/toolSetupCheck.ts),
 * built from what the window already holds -- nothing is run or read for
 * it, and it changes nothing. Under the title, when the last check was;
 * while the first check since launch has not finished, a sentence that
 * says so, over what is known so far. A line's 查看 closes the sheet and
 * opens the list it counted: the Installed page with that 「显示」 choice
 * picked, a source's own page, or Other Programs. Done, the default
 * button, closes it.
 */
export function ToolSetupSheet() {
  const { t } = useTranslation();
  const open = useToolSetupSheet((s) => s.open);
  const close = () => useToolSetupSheet.setState({ open: false });
  const doneRef = useRef<HTMLButtonElement>(null);
  const { data: committed } = useSnapshot();
  const preview = useInventoryPreview();
  const { data: sizes } = useSizes();
  const { data: settings } = useSettings();
  const { data: facts } = useSystemFacts();
  const openInstalled = useUiStore((s) => s.openInstalled);
  const setInstalledShow = useUiStore((s) => s.setInstalledShow);
  const openPage = useUiStore((s) => s.openPage);

  const answered = committed !== undefined && !isStartupSnapshot(committed);
  const snapshot = answered ? committed : preview === null ? null : previewSnapshot(preview);
  const refreshedAt = answered ? committed.refreshed_at : null;
  const now = useMinuteClock(refreshedAt);
  // Built only while it is open: it reads every tool, and the snapshot
  // and the sizes change while it is closed.
  const check = open
    ? toolSetupCheck(t, {
        snapshot,
        pending: !answered,
        facts,
        sizes: sizes ?? null,
        technicalDetails: settings?.show_technical_details ?? false,
      })
    : null;

  const onView = (view: SetupView) => {
    close();
    switch (view.kind) {
      case "installed":
        openInstalled(null);
        setInstalledShow(view.show);
        return;
      case "source":
        openInstalled(view.instanceId);
        return;
      case "unknown":
        openPage("unknown");
        return;
      default: {
        const unhandled: never = view;
        return unhandled;
      }
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => useToolSetupSheet.setState({ open: next })}
      title={t("setupCheck.title")}
      subtitle={
        refreshedAt === null
          ? answered
            ? t("setupCheck.neverChecked")
            : t("common.checking")
          : elapsedText(t, CHECKED_KEYS, elapsedSince(refreshedAt, now))
      }
      width="several"
      initialFocus={doneRef}
      footer={
        <button ref={doneRef} type="button" onClick={close} className={BUTTON.large.default}>
          {t("common.done")}
        </button>
      }
    >
      {check?.pending ? (
        <p role="status" className="mt-1 text-body text-foreground">
          {t("setupCheck.pending")}
        </p>
      ) : null}
      {check !== null && !check.pending && check.attention > 0 ? (
        <p data-setup-summary="" className="mt-1 flex items-center gap-2 text-body text-foreground">
          <FilledWarningIcon size={16} className="text-warning" />
          {t("reviewFixes.setupAttention", { count: check.attention })}
        </p>
      ) : null}
      {check?.sections.map((section) => (
        <Section key={section.id} section={section} onView={onView} />
      ))}
    </Dialog>
  );
}
