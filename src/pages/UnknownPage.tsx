import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ToolRow } from "../components/ToolRow";
import { StatusChip } from "../components/StatusChip";
import { SourceNoticeLine } from "../components/SourceNotice";
import { Menu, type MenuItem } from "../components/ui/Menu";
import { elapsedText, HeaderAction, useMinuteClock, type ElapsedKeys } from "../components/PageHeader";
import { CheckCircleIcon, SpinnerIcon, TerminalIcon } from "../components/icons";
import { SHOWN_FOR_MS, useCopyCommand } from "../lib/clipboard";
import { elapsedSince, formatBytes } from "../lib/format";
import { useRevealInFinder, useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
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
 * The size over the date, a line each and nothing between them, in the
 * column where a tool's version goes: what a program has in place of one.
 * A broken link has neither -- there is no target to measure -- and its
 * chip says why. As wide as the widest date, 「2026年10月18日」, so the
 * chips before it line up down the list, a broken link's too.
 */
function SizeAndDate({ entry, language }: { entry: UnknownEntry; language: string }) {
  const size = entry.size_bytes === null ? null : formatBytes(entry.size_bytes);
  const date = entry.modified_at === null ? null : formatDate(entry.modified_at, language);
  return (
    <span className="block min-w-[6.25rem]">
      {size !== null ? <span className="block">{size}</span> : null}
      {date !== null ? <span className="block">{date}</span> : null}
    </span>
  );
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
 * The app a link leads into, by its folder's name -- "Docker.app" -- for
 * the row's line under its name: "Points into Docker.app", which its
 * chip's ⓘ says at more length. Only for a link, and only when the app the
 * scan found (`app_bundle`) is on the way the link leads -- where it
 * resolves, or what a broken link says -- not merely around the folder the
 * link is in. A program that is no link points nowhere: the ⓘ says whose
 * part it is.
 */
function linkedApp(entry: UnknownEntry): string | null {
  if (entry.kind === "File" || entry.app_bundle === null) return null;
  const app = `${entry.app_bundle}.app`;
  const leadsInto = [entry.resolved, entry.link_target].some(
    (path) => path !== null && path.split("/").includes(app),
  );
  return leadsInto ? app : null;
}

/**
 * What Show in Finder says of itself, by kind, as its hint: a link shows
 * the file it points to, where Finder opens, not the folder the link is
 * in; a broken link has no file to show, and the item is off. A program
 * that is no link shows itself, which needs no word. A `Record` over
 * `EntryKind`, as `KIND_KEYS` is.
 */
const SHOW_IN_FINDER_HINTS: Record<EntryKind, string | null> = {
  File: null,
  Symlink: "unknown.showsLinkTarget",
  BrokenSymlink: "unknown.targetGone",
};

/**
 * The avatar of a program no source accounts for: a prompt, in the
 * neutral colour of the Overview's Unknown tile. Decorative, as a
 * source's is: the name is beside it.
 */
function ProgramAvatar() {
  return (
    <span
      aria-hidden="true"
      className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-[7px] bg-neutral-avatar text-white"
    >
      <TerminalIcon size={18} />
    </span>
  );
}

/** "Scanned 3 min ago", 「上次扫描：3 分钟前」: when this page's scan last answered. */
const SCANNED_KEYS: ElapsedKeys = {
  justNow: "unknown.scannedJustNow",
  minutes: "unknown.scannedMinutesAgo",
  hours: "unknown.scannedHoursAgo",
  days: "unknown.scannedDaysAgo",
};

/**
 * The Unknown page's own way to look again, for its toolbar
 * (`PageHeader`'s `actions`) in place of the sources' Check again: Scan
 * again, and when its scan last answered in its tooltip. It re-runs only
 * this scan, never the sources' refresh -- so no ⌘R beside it, which is
 * Check again's -- and the page itself asks for one whenever the
 * sources' snapshot moves.
 *
 * The scan carries no time of its own, so the time is when its answer
 * arrived (`dataUpdatedAt`). A spinner while one runs, whoever asked for
 * it, with the button off, and 「正在扫描…」 as the page's subtitle
 * (`usePageSubtitle` in src/App.tsx); no time before the first answer,
 * nor after a scan that failed, whose reason the page says.
 */
export function ScanAgain() {
  const { t } = useTranslation();
  const scan = useUnknownScan();
  const scannedAt = scan.data === undefined || scan.dataUpdatedAt === 0 ? null : scan.dataUpdatedAt;
  const now = useMinuteClock(scannedAt);

  const label = t("unknown.scanAgain");
  let status: string | null = null;
  if (scan.isFetching) {
    status = t("unknown.scanning");
  } else if (!scan.isError && scannedAt !== null) {
    status = elapsedText(t, SCANNED_KEYS, elapsedSince(scannedAt / 1000, now));
  }

  return (
    <HeaderAction
      label={label}
      tooltip={status === null ? label : t("toolbar.scanAgainTip", { label, status })}
      onPress={() => void scan.refetch()}
      busy={scan.isFetching}
    />
  );
}

/**
 * The command-line programs on this Mac that no source accounts for, as
 * rows like every other list's: a neutral avatar, the name with the path
 * it was found at -- and, after it, the app a link points into -- a chip
 * for what kind of entry it is, whose ⓘ says the rest, where there is more
 * to say, its size and date where a tool's version would be, and a ⋯ menu
 * to show it in Finder or copy its path. Canager only lists them: nothing
 * here runs or removes anything, which the page says once, at its top,
 * above the folders it looked in; Scan again is in the page header
 * (`ScanAgain`).
 *
 * Its paths select (`select-text`), to be copied into Terminal or into
 * Finder's Go to Folder: each row's, the folders it looked in, and the
 * lines behind a chip's ⓘ, among them where a link leads.
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
  // snapshot (ruling 9). Scan again, in the page header, asks regardless
  // (`ScanAgain`). `refetch` is stable
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

  // A row's Copy path and Show in Finder, and a word about how the last
  // one went: "Copied" or "Couldn't copy" as the other pages say it
  // (`useCopyCommand`), or "Couldn't show it in Finder" -- for as long,
  // and started over by each new failure. Nothing needs saying when
  // Finder comes forward with the file. The newest word wins: a copy
  // takes a failure to show away.
  const { status: copyStatus, copy } = useCopyCommand();
  const reveal = useRevealInFinder();
  const { isError: revealFailed, submittedAt: revealedAt, reset: resetReveal } = reveal;
  useEffect(() => {
    if (!revealFailed) return;
    const timer = window.setTimeout(resetReveal, SHOWN_FOR_MS);
    return () => window.clearTimeout(timer);
  }, [revealFailed, revealedAt, resetReveal]);
  const notice = revealFailed
    ? t("unknown.showInFinderFailed")
    : copyStatus === "copied"
      ? t("common.copied")
      : copyStatus === "failed"
        ? t("common.copyFailed")
        : null;

  // The ⋯ menu. Show in Finder hands the plugin where the program is,
  // every link followed (`resolved`): what the plugin would make of the
  // row's own path anyway, as it resolves a path before it asks Finder,
  // and with no `~` in it, which the plugin would not read as the home
  // folder. A broken link resolves nowhere, and the item is off. Copy path
  // copies the path the row shows.
  const menuItems = (entry: UnknownEntry): MenuItem[] => {
    const { resolved } = entry;
    const hint = SHOW_IN_FINDER_HINTS[entry.kind];
    return [
      {
        id: "reveal",
        label: t("unknown.showInFinder"),
        hint: hint === null ? undefined : t(hint),
        disabled: resolved === null,
        onSelect: () => {
          if (resolved !== null) reveal.mutate(resolved);
        },
      },
      {
        id: "copyPath",
        label: t("unknown.copyPath"),
        onSelect: () => {
          resetReveal();
          copy(entry.path);
        },
      },
    ];
  };

  return (
    <div className="flex min-h-full flex-col">
      <div className="flex shrink-0 flex-col gap-1 px-5 pb-3">
        {/* Scan again is in the page header (`ScanAgain`), where the other
            pages have Check again. */}
        <p className="text-body text-muted">{t("unknown.intro")}</p>
        {result ? (
          <div className="flex items-baseline gap-4">
            <p className="min-w-0 flex-1 select-text break-words text-small text-muted">
              {t("unknown.lookedIn", {
                // 「~/.local/bin、/usr/local/bin」, "~/.local/bin, /usr/local/bin".
                folders: result.scanned.map((dir) => dir.path).join(t("common.listSeparator")),
              })}
            </p>
            {/* How a row's Copy path or Show in Finder went, at the top of
                the page as the other pages say how a Copy command went. */}
            <p role="status" className="shrink-0 text-small text-muted">
              {notice}
            </p>
          </div>
        ) : null}
      </div>
      {scan.isError ? (
        <p role="alert" className="px-5 pb-2 text-body text-danger">
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
            <div className="px-5 pb-2">
              <SourceNoticeLine
                variant="warning"
                title={stopped}
                detailsLabel={t("common.details")}
                detailsAriaLabel={t("common.detailsLabel", { title: stopped })}
              />
            </div>
          ) : null}
          {result.entries.length === 0 ? (
            // A scan that stopped early vouches only for what it checked:
            // no check mark over the rest.
            <div className="flex flex-1 flex-col items-center justify-center gap-3 px-5 pb-10 text-center">
              {stopped === null ? <CheckCircleIcon size={44} className="text-success" /> : null}
              <p className="text-section text-foreground">
                {t(stopped === null ? "unknown.empty" : "unknown.emptyChecked")}
              </p>
            </div>
          ) : (
            <div className="px-2 pb-2">
              {result.entries.map((entry) => {
                const name = fileName(entry.path);
                const app = linkedApp(entry);
                const facts = factsOf(entry, t, technical);
                const detail =
                  facts.length === 0
                    ? undefined
                    : facts.map((fact) => (
                        <span key={fact} className="block select-text break-words">
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
                      name={name}
                      // Home abbreviated as Rust sent it (`UnknownEntry.path`).
                      description={entry.path}
                      selectableDescription
                      descriptionNote={app === null ? undefined : t("unknown.pointsInto", { app })}
                      status={<StatusChip label={t(KIND_KEYS[entry.kind])} detail={detail} />}
                      version={<SizeAndDate entry={entry} language={i18n.language} />}
                      menu={<Menu label={t("common.moreActions", { name })} items={menuItems(entry)} />}
                    />
                  </div>
                );
              })}
            </div>
          )}
          {result.attributed > 0 ? (
            <p className="px-5 pb-6 pt-2 text-small text-muted">
              {t("unknown.attributed", { count: result.attributed })}
            </p>
          ) : null}
        </>
      )}
    </div>
  );
}
