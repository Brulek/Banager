import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ToolRow } from "../components/ToolRow";
import { StatusChip } from "../components/StatusChip";
import { SourceNoticeLine } from "../components/SourceNotice";
import { CheckCircleIcon, RefreshIcon, SpinnerIcon, TerminalIcon } from "../components/icons";
import { formatBytes } from "../lib/format";
import { useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
import type { EntryKind, ScanStop, UnknownEntry } from "../lib/types";

/**
 * The chip for each kind of entry. A `Record` over `EntryKind`, so a
 * variant added to the mirror without a chip here fails `tsc` -- this
 * project's signature defect is a variant that is defined, mirrored and
 * never rendered.
 */
const KIND_KEYS: Record<EntryKind, string> = {
  File: "unknown.kind.File",
  Symlink: "unknown.kind.Symlink",
  BrokenSymlink: "unknown.kind.BrokenSymlink",
};

/** Whatever `useTranslation()`'s `t` needs to look a key up; same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * The notice for a scan that stopped early, carrying the number it
 * stopped at -- the one Rust enforced, never a copy in the locale files.
 * `in` branches with a `never` default, as `faultKey` in src/lib/format.ts.
 */
function stoppedText(t: Translate, stopped: ScanStop): string {
  if ("FileLimit" in stopped) {
    return t("unknown.stopped.FileLimit", { count: stopped.FileLimit.max_entries });
  }
  if ("TimeLimit" in stopped) {
    return t("unknown.stopped.TimeLimit", { seconds: stopped.TimeLimit.max_secs });
  }
  const unhandled: never = stopped;
  return unhandled;
}

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** An absolute date in the user's language: when the file last changed, which "3 days ago" would blur. */
function formatDate(seconds: number, language: string): string {
  return new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(
    new Date(seconds * 1000),
  );
}

/**
 * The size and the date, in the column where a tool's version goes: what
 * a program has in place of one. A broken link has neither -- there is no
 * target to measure -- and its chip says why.
 */
function sizeAndDate(entry: UnknownEntry, t: Translate, language: string): string | null {
  const size = entry.size_bytes === null ? null : formatBytes(entry.size_bytes);
  const date = entry.modified_at === null ? null : formatDate(entry.modified_at, language);
  if (size !== null && date !== null) return t("unknown.sizeAndDate", { size, date });
  return size ?? date;
}

/**
 * What the kind chip's ⓘ says about a program, a line each: what a broken
 * link pointed at, the app it runs inside, that another account owns it,
 * and -- with technical details on -- where a link leads. A plain file
 * resolves to itself, so that last one is for links only. Empty for a
 * program of the user's own that is none of these, whose chip is then a
 * plain label.
 */
function factsOf(entry: UnknownEntry, t: Translate, technical: boolean): string[] {
  const facts: string[] = [];
  if (entry.kind === "BrokenSymlink") {
    facts.push(t("unknown.brokenLink", { target: entry.link_target ?? "" }));
  }
  if (entry.app_bundle !== null) facts.push(t("unknown.partOfApp", { app: entry.app_bundle }));
  if (!entry.owned_by_me) facts.push(t("unknown.adminOwned"));
  if (technical && entry.kind === "Symlink" && entry.resolved !== null) {
    facts.push(t("unknown.linksTo", { path: entry.resolved }));
  }
  return facts;
}

/**
 * The avatar of a program no source accounts for: a prompt, in the
 * neutral colour of the Overview's Unknown tile. Decorative, as a
 * source's is: the name is beside it.
 */
function ProgramAvatar() {
  return (
    <span
      aria-hidden="true"
      className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-[9px] bg-muted text-white"
    >
      <TerminalIcon size={18} />
    </span>
  );
}

/**
 * The command-line programs on this Mac that no source accounts for, as
 * rows like every other list's: a neutral avatar, the name with the path
 * it was found at, a chip for what kind of entry it is -- whose ⓘ says
 * the rest, where there is more to say -- and its size and date where a
 * tool's version would be. Canager only lists them: nothing here runs or
 * removes anything, which the page says once, at its top, beside Scan
 * again and above the folders it looked in.
 */
export function UnknownPage() {
  const { t, i18n } = useTranslation();
  const { data: settings } = useSettings();
  const { data: snapshot } = useSnapshot();
  const scan = useUnknownScan();
  const { refetch } = scan;
  const generation = snapshot?.generation;

  // One scan per snapshot generation while the page is open. The query is
  // `enabled: false` (src/lib/queries.ts), so nothing runs until asked:
  // the first defined `generation` -- the snapshot query's answer, an
  // in-memory read -- asks once, and every later change to it asks again,
  // which is how a refresh landing after the page opened (the startup
  // refresh, most often) corrects a list judged against an empty
  // snapshot (ruling 9). The button asks regardless. `refetch` is stable
  // across renders. In development, StrictMode (src/main.tsx) runs this
  // effect twice on mount and the second `refetch` restarts the first
  // scan: a read that is thrown away, accepted over `cancelRefetch:
  // false`, which would make a generation change join a scan still
  // judging against the old snapshot.
  useEffect(() => {
    if (generation === undefined) return;
    void refetch();
  }, [refetch, generation]);

  const result = scan.data;
  const technical = settings?.show_technical_details ?? false;
  const stopped = result === undefined || result.stopped === null ? null : stoppedText(t, result.stopped);

  return (
    <div className="flex min-h-full flex-col">
      <div className="flex shrink-0 flex-col gap-1 px-6 pb-3">
        <div className="flex items-center justify-between gap-4">
          <p className="min-w-0 text-body text-muted">{t("unknown.intro")}</p>
          {/* This page's own: it re-runs only this scan, never the sources'
              refresh, which is the page header's Check again. */}
          <button
            type="button"
            onClick={() => void refetch()}
            disabled={scan.isFetching}
            className="flex h-8 shrink-0 items-center gap-1.5 rounded-button border border-border bg-surface px-3 text-body font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-60 disabled:hover:bg-surface"
          >
            {scan.isFetching ? (
              <SpinnerIcon size={15} className="shrink-0 text-accent-text" />
            ) : (
              <RefreshIcon size={15} className="shrink-0" />
            )}
            {scan.isFetching ? t("unknown.scanning") : t("unknown.scanAgain")}
          </button>
        </div>
        {result ? (
          <p className="break-words text-small text-muted">
            {t("unknown.lookedIn", {
              // 「~/.local/bin、/usr/local/bin」, "~/.local/bin, /usr/local/bin".
              folders: result.scanned.map((dir) => dir.path).join(t("common.listSeparator")),
            })}
          </p>
        ) : null}
      </div>
      {scan.isError ? (
        <p role="alert" className="px-6 pb-2 text-body text-danger">
          {t("unknown.scanFailed", { message: scan.error.message })}
        </p>
      ) : null}
      {result === undefined ? (
        scan.isFetching ? (
          <div className="flex flex-1 items-center justify-center pb-10">
            <SpinnerIcon size={22} className="text-muted" />
          </div>
        ) : null
      ) : (
        <>
          {stopped !== null ? (
            <div className="px-6 pb-2">
              <SourceNoticeLine
                variant="warning"
                title={stopped}
                detailsLabel={t("common.details")}
                detailsAriaLabel={t("common.detailsLabel", { title: stopped })}
              />
            </div>
          ) : null}
          {result.entries.length === 0 ? (
            <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 pb-10 text-center">
              <CheckCircleIcon size={44} className="text-success" />
              <p className="text-section text-foreground">{t("unknown.empty")}</p>
            </div>
          ) : (
            <div className="px-3 pb-2">
              {result.entries.map((entry) => {
                const facts = factsOf(entry, t, technical);
                const detail =
                  facts.length === 0
                    ? undefined
                    : facts.map((fact) => (
                        <span key={fact} className="block break-words">
                          {fact}
                        </span>
                      ));
                return (
                  // A slot of its own, as a virtualized list's rows have:
                  // a stacking context each, and the one with an open ⓘ
                  // lifted over the rows after it (`data-list-slot` in
                  // index.css), whose chips would otherwise cover it.
                  <div key={entry.path} data-list-slot="" className="relative z-0">
                    <ToolRow
                      avatar={<ProgramAvatar />}
                      name={fileName(entry.path)}
                      // Home abbreviated as Rust sent it (`UnknownEntry.path`).
                      description={entry.path}
                      status={<StatusChip label={t(KIND_KEYS[entry.kind])} detail={detail} />}
                      // As wide as a size and a date, so the chips before it
                      // line up down the list, a broken link's too.
                      version={
                        <span className="inline-block min-w-[9.5rem]">{sizeAndDate(entry, t, i18n.language)}</span>
                      }
                    />
                  </div>
                );
              })}
            </div>
          )}
          {result.attributed > 0 ? (
            <p className="px-6 pb-6 pt-2 text-small text-muted">
              {t("unknown.attributed", { count: result.attributed })}
            </p>
          ) : null}
        </>
      )}
    </div>
  );
}
