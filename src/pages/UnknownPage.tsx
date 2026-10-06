import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { rowFitFor, ToolRow } from "../components/ToolRow";
import { ListWidthProvider, StatusColumnProvider, useElementWidth } from "../components/VirtualList";
import { StatusChip } from "../components/StatusChip";
import { SourceNoticeLine } from "../components/SourceNotice";
import { EmptyState } from "../components/EmptyState";
import { TextWithInfo } from "../components/InfoDetail";
import { Menu, type MenuItem } from "../components/ui/Menu";
import { elapsedText, HeaderAction, useMinuteClock, type ElapsedKeys } from "../components/PageHeader";
import { SpinnerIcon, TerminalIcon } from "../components/icons";
import { SHOWN_FOR_MS, copyStatusText, useCopyCommand } from "../lib/clipboard";
import { revealFailureKey } from "../lib/revealFailure";
import { elapsedSince, formatBytes } from "../lib/format";
import { mediumDateText } from "../lib/shortDate";
import { useRevealInFinder, useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
import type { EntryKind, ScanStop, UnknownEntry } from "../lib/types";

/**
 * The status word each kind of entry has, or null: a broken link's
 * 「找不到原文件」, with an orange ⚠︎ -- something is wrong with it -- a
 * link into a protected place's 「指向受保护的位置」, plain -- nothing is
 * wrong, Banager just did not look -- and nothing for a program or a link
 * that works, which are the normal states a row does not put in words
 * (spec §3.3, §3.4). The word stands where the size and the date would,
 * which neither link has (`SizeAndDate`). A `Record` over `EntryKind`, so
 * a variant added to the mirror without an answer here fails `tsc` -- this
 * project's signature defect is a variant that is defined, mirrored and
 * never rendered.
 */
const STATUS_KEYS: Record<EntryKind, string | null> = {
  File: null,
  Symlink: null,
  BrokenSymlink: "unknown.kind.BrokenSymlink",
  ProtectedSymlink: "unknown.protectedLink",
};

/** The status word's tone, by kind (`StatusChip`): only a broken link has something wrong with it. */
const STATUS_TONES: Record<EntryKind, "neutral" | "warning"> = {
  File: "neutral",
  Symlink: "neutral",
  BrokenSymlink: "warning",
  ProtectedSymlink: "neutral",
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

/**
 * The size and the date, two columns of one line each, 72 and 104 wide
 * and 16 apart, in the place of a tool's version: what a program has in
 * place of one (spec §3.3), as Finder's list shows Size and Date
 * Modified. 104 holds the widest date, 「2026年10月18日」, so each column
 * lines up down the list. A broken link has neither -- there is no target
 * to measure -- and its status word stands in their place, at their right
 * (`status`), where it moves no row's name or path at any width: a status
 * column of its own would go to the start of the path's line in a narrow
 * window (`ToolRow`'s `narrow` fit), and push that one path out of line
 * with the rest.
 */
function SizeAndDate({ entry, language, status }: { entry: UnknownEntry; language: string; status?: ReactNode }) {
  if (status !== undefined) {
    return (
      <span data-status="" className="flex w-48 justify-end">
        {status}
      </span>
    );
  }
  const size = entry.size_bytes === null ? null : formatBytes(entry.size_bytes);
  const date = entry.modified_at === null ? null : mediumDateText(new Date(entry.modified_at * 1000), language);
  return (
    <span className="flex">
      <span data-size="" className="w-18 truncate">
        {size}
      </span>
      <span data-date="" className="ml-4 w-26 truncate">
        {date}
      </span>
    </span>
  );
}

/**
 * The size's and the date's headings, over their columns as Finder's list
 * heads Size and Date Modified: 「大小」 and 「修改日期」, at 11 in the
 * secondary colour, each at its column's right as the values under it are
 * (`SizeAndDate`, whose columns take the version column's `text-right`),
 * with the ⋯'s room after them -- the row's own spacing, so they line up at
 * any width the columns are drawn at. The date is the program's last
 * modification (`UnknownEntry.modified_at`, the target's for a link).
 * Words, not controls: nothing in it takes the focus, so Tab and the arrow
 * keys go from the page to the rows as before, and it is no row.
 */
function ColumnHeads() {
  const { t } = useTranslation();
  return (
    <div data-column-heads="" className="flex px-5 pb-1 text-small text-muted">
      <span className="min-w-0 flex-1" />
      <span className="ml-4 flex shrink-0 whitespace-nowrap">
        <span data-size-head="" className="w-18 truncate text-right">
          {t("unknown.columns.size")}
        </span>
        <span data-date-head="" className="ml-4 w-26 truncate text-right">
          {t("unknown.columns.modified")}
        </span>
      </span>
      {/* The ⋯'s column, as wide and as far off as on a row. */}
      <span aria-hidden="true" className="ml-4 w-6 shrink-0" />
    </div>
  );
}

/**
 * The protected places a person knows by name, by the folder each is
 * (crates/banager-core/src/protected.rs): Finder's own names for them.
 */
const PLACE_KEYS: ReadonlyArray<[prefix: string, key: string]> = [
  ["~/Documents", "reviewFixes.places.documents"],
  ["~/Desktop", "reviewFixes.places.desktop"],
  ["~/Downloads", "reviewFixes.places.downloads"],
  ["~/Pictures", "reviewFixes.places.pictures"],
  ["~/Movies", "reviewFixes.places.movies"],
  ["~/Music", "reviewFixes.places.music"],
  ["~/Library/Mobile Documents", "reviewFixes.places.icloud"],
  ["/Volumes", "reviewFixes.places.volumes"],
];

/** The place a protected folder is in, by its name in Finder, or null for one not named here. */
export function placeOf(dir: string): string | null {
  const lower = dir.toLowerCase();
  for (const [prefix, key] of PLACE_KEYS) {
    const p = prefix.toLowerCase();
    if (lower === p || lower.startsWith(`${p}/`)) return key;
  }
  return null;
}

/**
 * 「有2个文件夹在受保护的位置（文稿、桌面），没有读取。」: how many folders the scan
 * left unread, and where, in Finder's names -- with technical details off
 * too, the paths themselves being behind the ⓘ only with them on. Where a
 * folder is in no place named here, the count alone.
 */
function protectedFoldersText(t: Translate, dirs: readonly string[]): string {
  const keys = dirs.map(placeOf);
  if (keys.some((key) => key === null)) return t("unknown.protectedFolders", { count: dirs.length });
  const places = [...new Set(keys as string[])].map((key) => t(key));
  return t("reviewFixes.protectedFoldersIn", {
    count: dirs.length,
    places: places.join(t("common.listSeparator")),
  });
}

/**
 * What there is to say about a program, a line each: what a broken link
 * pointed at, why a link into a protected place was not followed (never
 * where it leads), the app it runs inside, that another account owns it, and
 * -- with technical details on -- where a link leads. A plain file
 * resolves to itself, so that last one is for links only. Behind a broken
 * link's word's ⓘ; for any other row, which has no word to hang an ⓘ on
 * (spec §3.4: no bare ⓘ), its tooltip and what a screen reader says of it.
 * Empty for a program of the user's own that is none of these.
 */
function factsOf(entry: UnknownEntry, t: Translate, technical: boolean): string[] {
  const facts: string[] = [];
  if (entry.kind === "BrokenSymlink") {
    facts.push(t("unknown.brokenLink", { target: entry.link_target ?? "" }));
  }
  // Where it leads was never looked at: no path, only why.
  if (entry.kind === "ProtectedSymlink") facts.push(t("unknown.protectedLinkWhy"));
  if (entry.app_bundle !== null) facts.push(t("unknown.partOfApp", { app: entry.app_bundle }));
  if (!entry.owned_by_me) facts.push(t("unknown.adminOwned"));
  if (technical && entry.kind === "Symlink" && entry.resolved !== null) {
    facts.push(t("unknown.linksTo", { path: entry.resolved }));
  }
  return facts;
}

/**
 * What the row's line says after its path, in sight, in the secondary
 * text a link's 「指向Docker.app」 already had (`ToolRow`'s
 * `descriptionNote`): the app it belongs to -- a link by where it leads
 * (`linkedApp`), a program inside an app by whose part it is -- and that
 * another account owns it, 「属于系统或其他账户」, a dot between them.
 * Nothing for a program of the user's own that is none of these. The
 * longer facts -- what a broken link pointed at, why a protected one was
 * not followed, where a link leads -- stay behind the word's ⓘ or in the
 * tooltip (`factsOf`).
 */
function noteFactsOf(entry: UnknownEntry, t: Translate): string[] {
  const notes: string[] = [];
  const app = linkedApp(entry);
  if (app !== null) notes.push(t("unknown.pointsInto", { app }));
  else if (entry.app_bundle !== null) notes.push(t("unknown.partOfApp", { app: entry.app_bundle }));
  if (!entry.owned_by_me) notes.push(t("unknown.adminOwned"));
  return notes;
}

/**
 * The app a link leads into, by its folder's name -- "Docker.app" -- for
 * the row's line under its name: "Points into Docker.app", which its
 * tooltip (a broken link's ⓘ) says at more length. Only for a link, and
 * only when the app the scan found (`app_bundle`) is on the way the link
 * leads -- where it resolves, or what a broken link says -- not merely
 * around the folder the link is in. A program that is no link points
 * nowhere: its tooltip says whose part it is.
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
 * `EntryKind`, as `STATUS_KEYS` is.
 */
const SHOW_IN_FINDER_HINTS: Record<EntryKind, string | null> = {
  File: null,
  Symlink: "unknown.showsLinkTarget",
  BrokenSymlink: "unknown.targetGone",
  ProtectedSymlink: "unknown.protectedLinkHint",
};

/**
 * The avatar of a program no source accounts for: a prompt, in the
 * neutral colour (`neutral-avatar`) -- systemGray, and in the dark
 * systemGray3's #48484A, so that the tiles down the list are not the
 * brightest thing on a dark page -- with, in dark mode, the 12% white
 * edge that keeps it outlined on the selected row's fill: the tile a tool
 * with no logo has on the Installed page (`ToolAvatar`) and the sidebar's
 * Other Programs mark. Decorative, as a source's is: the name
 * is beside it. `facts`, what the row's tooltip says (`factsOf`), are said
 * to a screen reader here, the row having no ⓘ for them.
 */
function ProgramAvatar({ facts }: { facts: string }) {
  return (
    <>
      <span
        aria-hidden="true"
        className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-[7px] bg-neutral-avatar text-white dark:inset-ring dark:inset-ring-white/12"
      >
        <TerminalIcon size={18} />
      </span>
      {facts !== "" ? <span className="sr-only">{facts}</span> : null}
    </>
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
 * rows like every other list's (spec §3.3): a neutral avatar, the name
 * with the path it was found at -- and, after it, the app it belongs to and
 * whether another account owns it (`noteFactsOf`) -- its size and its date in two columns where a tool's version
 * would be, headed 「大小」 and 「修改日期」 as Finder heads them
 * (`ColumnHeads`), or, for a broken link, its status word there with an ⓘ; what
 * more there is to say of any other row in its tooltip (`factsOf`); and a
 * ⋯ menu to show it in Finder or copy its path. No row has a status
 * column, so every name and every path starts at one x at any width. One
 * line over the list says what these are; under it, quieter, the folders
 * it looked in and how many it recognised. Scan again is in the page
 * header (`ScanAgain`). With nothing to list, the empty state in the
 * list's place (`EmptyState`).
 *
 * Its paths select (`select-text`), to be copied into Terminal or into
 * Finder's Go to Folder: each row's, the folders it looked in, and the
 * lines behind a broken link's ⓘ, among them what it pointed at.
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
  // How wide the list is, for its rows to give way column by column as
  // the window narrows (`ToolRow`'s `rowFitFor`, spec R9), as a
  // virtualized list tells its own rows.
  const [listBox, setListBox] = useState<HTMLDivElement | null>(null);
  const listWidth = useElementWidth(listBox);
  // Too narrow for a path and the app a link points into on one line, the
  // note gives way to the path, cut short with its words in its own
  // tooltip (`ToolRow`'s `descriptionNote`; walk-3 W3-8), down to the
  // narrowest window's list; only beside the inspector is the note left
  // out, the row's tooltip saying what it can at more length.
  const fit = rowFitFor(listWidth);
  const roomForNote = fit === "full" || fit === "compact" || fit === "narrow";
  // The headings over the size and the date where the rows draw those
  // columns (`ToolRow`'s version column, down to its `narrow` fit) and a
  // row has either: over broken links alone they would head nothing.
  const headed =
    roomForNote &&
    (result?.entries.some((entry) => entry.size_bytes !== null || entry.modified_at !== null) ?? false);
  const stopped = result === undefined || result.stopped === null ? null : stoppedText(t, result.stopped);
  // The folders it did not read, being in protected places: a quiet line
  // under the list, naming them behind an ⓘ only with technical details on.
  const unread = result?.protected_dirs ?? [];
  const unreadText = unread.length === 0 ? null : protectedFoldersText(t, unread);

  // A row's Copy path and Show in Finder, and a word about how the last
  // one went: "Copied" or "Couldn't copy" as the other pages say it
  // (`useCopyCommand`), or "Couldn't show it in Finder" -- or, for a
  // program that changed since the scan, to scan again (`revealFailureKey`)
  // -- for as long, and started over by each new failure. Nothing needs saying when
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
  const notice = revealFailed ? t(revealFailureKey(reveal.error)) : copyStatusText(t, copyStatus);

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
      {/* One line over the list: what these are -- 「以下程序」, so not
          over a list with nothing in it, nor over a scan that failed before
          it listed anything, where only its failure stands (walk-5 W5-19).
          A failed scan again keeps the list it had, and the line over it.
          Scan again is in the page header (`ScanAgain`), where the other
          pages have Check again. At its right, how a row's Copy path or
          Show in Finder went, as the other pages say how a Copy command
          went. */}
      <div className="flex shrink-0 items-baseline gap-4 px-5 pb-2">
        <p className="min-w-0 flex-1 text-body text-muted">
          {(result === undefined ? scan.isError : result.entries.length === 0) ? null : t("unknown.intro")}
        </p>
        <p role="status" className="shrink-0 text-small text-muted">
          {notice}
        </p>
      </div>
      {scan.isError ? (
        <p role="alert" className="px-5 pb-2 text-body text-danger-text">
          {/* The scan's own words only with "Show technical details" on, as
              every other raw error; without it, what to do. */}
          {settings?.show_technical_details
            ? t("unknown.scanFailed", { message: scan.error.message })
            : t("unknown.scanFailedPlain")}
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
            <div className="px-5">
              <SourceNoticeLine
                variant="warning"
                title={stopped}
                detailsLabel={t("common.details")}
                detailsAriaLabel={t("common.detailsLabel", { title: stopped })}
              />
            </div>
          ) : null}
          {result.entries.length === 0 ? (
            // Nothing unexplained, as a Mac list says it has nothing to
            // show: a ✓ in a circle, no button. A scan that stopped early
            // vouches only for what it checked: no check mark over the rest,
            // nor one over folders it left unread.
            stopped === null && unread.length === 0 ? (
              <EmptyState symbol="check" title={t("unknown.empty")} />
            ) : (
              <EmptyState symbol="info" title={t("unknown.emptyChecked")} />
            )
          ) : (
            <div ref={setListBox}>
              {headed ? <ColumnHeads /> : null}
              <ListWidthProvider value={listWidth}>
                {/* A broken link's word stands where its size and date would
                    (`SizeAndDate`), never in a status column: the rows
                    have none, and their paths take its room. */}
                <StatusColumnProvider value={false}>
                  {result.entries.map((entry) => {
                    const name = fileName(entry.path);
                    // In sight after the path where the row has room; else
                    // its tooltip says them with the rest.
                    const notes = roomForNote ? noteFactsOf(entry, t) : [];
                    const facts = factsOf(entry, t, technical);
                    const statusKey = STATUS_KEYS[entry.kind];
                    // A broken link's word, its facts behind its ⓘ. Any other
                    // row has no word, and no ⓘ standing alone in its place:
                    // its facts are its tooltip.
                    const status =
                      statusKey === null ? undefined : (
                        <StatusChip
                          label={t(statusKey)}
                          tone={STATUS_TONES[entry.kind]}
                          detail={
                            facts.length === 0
                              ? undefined
                              : facts.map((fact) => (
                                  <span key={fact} className="block select-text break-words">
                                    {fact}
                                  </span>
                                ))
                          }
                        />
                      );
                    // What the line says in sight is not said again to a
                    // screen reader by the avatar (`ProgramAvatar`); the
                    // pointer's tooltip keeps every fact.
                    const tooltip = status === undefined ? facts : [];
                    const unseen = tooltip.filter((fact) => !notes.includes(fact));
                    return (
                      // A slot of its own, as a virtualized list's rows have:
                      // a stacking context each, and the one with an open ⓘ
                      // lifted over the rows after it (`data-list-slot` in
                      // index.css), whose words would otherwise cover it.
                      <div
                        key={entry.path}
                        data-list-slot=""
                        title={tooltip.length === 0 ? undefined : tooltip.join("\n")}
                        className="relative z-0"
                      >
                        <ToolRow
                          avatar={<ProgramAvatar facts={unseen.join(t("common.listSeparator"))} />}
                          name={name}
                          // Home abbreviated as Rust sent it (`UnknownEntry.path`).
                          description={entry.path}
                          selectableDescription
                          descriptionNote={notes.length === 0 ? undefined : notes.join(" · ")}
                          version={<SizeAndDate entry={entry} language={i18n.language} status={status} />}
                          menu={<Menu label={t("common.moreActions", { name })} items={menuItems(entry)} />}
                        />
                      </div>
                    );
                  })}
                </StatusColumnProvider>
              </ListWidthProvider>
            </div>
          )}
          {/* Under the list, quieter: where it looked, and how many
              programs it found a source for. */}
          <div className="flex flex-col gap-1 px-5 pb-6 pt-3 text-small text-muted">
            <p className="select-text break-words">
              {t("unknown.lookedIn", {
                // 「~/.local/bin、/usr/local/bin」, "~/.local/bin, /usr/local/bin".
                folders: result.scanned.map((dir) => dir.path).join(t("common.listSeparator")),
              })}
            </p>
            {result.attributed > 0 ? <p>{t("unknown.attributed", { count: result.attributed })}</p> : null}
            {unreadText === null ? null : (
              <p data-protected-dirs="">
                {technical ? (
                  <TextWithInfo text={unreadText} label={t("common.detailsLabel", { title: unreadText })}>
                    {unread.map((dir) => (
                      <span key={dir} className="block select-text break-words">
                        {dir}
                      </span>
                    ))}
                  </TextWithInfo>
                ) : (
                  unreadText
                )}
              </p>
            )}
          </div>
        </>
      )}
    </div>
  );
}
