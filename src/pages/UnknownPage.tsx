import { useEffect } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ArtifactRow } from "../components/ArtifactRow";
import { SourceNotice } from "../components/SourceNotice";
import { formatBytes } from "../lib/format";
import { useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
import type { EntryKind, ScanStop, UnknownEntry } from "../lib/types";

/**
 * The badge for each kind of entry. A `Record` over `EntryKind`, so a
 * variant added to the mirror without a badge here fails `tsc` -- this
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
 * The banner for a scan that stopped early, carrying the number it
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

/** An absolute date in the user's language. This repository deliberately has no relative-time formatter. */
function formatDate(seconds: number, language: string): string {
  return new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(
    new Date(seconds * 1000),
  );
}

/**
 * Everything the row says under its name, one line each: the path as
 * found, what a broken link pointed at, the app it runs inside, size and
 * date, who put it there, and -- with technical details on -- where a
 * link resolves. A plain file resolves to itself, so that last line is
 * for links only.
 */
function describeEntry(
  entry: UnknownEntry,
  t: Translate,
  language: string,
  technical: boolean,
): ReactNode {
  const lines: string[] = [entry.path];
  if (entry.kind === "BrokenSymlink") {
    lines.push(t("unknown.brokenLink", { target: entry.link_target ?? "" }));
  }
  if (entry.app_bundle !== null) {
    lines.push(t("unknown.partOfApp", { app: entry.app_bundle }));
  }
  const size = entry.size_bytes === null ? null : formatBytes(entry.size_bytes);
  const date = entry.modified_at === null ? null : formatDate(entry.modified_at, language);
  if (size !== null && date !== null) {
    lines.push(t("unknown.sizeAndDate", { size, date }));
  } else if (size !== null) {
    lines.push(size);
  } else if (date !== null) {
    lines.push(date);
  }
  if (!entry.owned_by_me) {
    lines.push(t("unknown.adminOwned"));
  }
  if (technical && entry.kind === "Symlink" && entry.resolved !== null) {
    lines.push(t("unknown.linksTo", { path: entry.resolved }));
  }
  // Keyed by position: the list is rebuilt from the entry on every render
  // and two lines can read the same (a size with no date is one bare
  // string), so the text itself is not a safe key.
  return lines.map((line, index) => (
    <span key={index} className="block">
      {line}
    </span>
  ));
}

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

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="flex items-start justify-between gap-4 p-4">
        <div className="min-w-0">
          <h1 className="text-lg font-semibold">{t("unknown.title")}</h1>
          <p className="mt-1 text-sm text-[var(--color-muted)]">{t("unknown.intro")}</p>
        </div>
        {/* The app's first standing refresh control, scoped to this page:
            it re-runs only this scan, never the sources' refresh. */}
        <button
          type="button"
          onClick={() => void refetch()}
          disabled={scan.isFetching}
          className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
        >
          {scan.isFetching ? t("unknown.scanning") : t("unknown.scanAgain")}
        </button>
      </div>
      {scan.isError ? (
        <p role="alert" className="px-4 pb-2 text-sm text-[var(--color-danger)]">
          {t("unknown.scanFailed", { message: scan.error.message })}
        </p>
      ) : null}
      {result ? (
        <>
          {result.stopped !== null ? (
            <div className="px-4">
              <SourceNotice variant="warning" title={stoppedText(t, result.stopped)} />
            </div>
          ) : null}
          {result.entries.length === 0 ? (
            <p className="p-12 text-center text-sm text-[var(--color-muted)]">
              {t("unknown.empty")}
            </p>
          ) : (
            result.entries.map((entry) => (
              <ArtifactRow
                key={entry.path}
                name={fileName(entry.path)}
                description={describeEntry(
                  entry,
                  t,
                  i18n.language,
                  settings?.show_technical_details ?? false,
                )}
                wrapDescription
                badgeText={t(KIND_KEYS[entry.kind])}
                badgeVariant="neutral"
              />
            ))
          )}
          <div className="p-4 text-xs text-[var(--color-muted)]">
            {result.attributed > 0 ? (
              <p>{t("unknown.attributed", { count: result.attributed })}</p>
            ) : null}
            <p className="mt-2">{t("unknown.lookedIn")}</p>
            <ul>
              {result.scanned.map((dir) => (
                <li key={dir.path}>
                  {t("unknown.dirCount", { path: dir.path, count: dir.entries })}
                </li>
              ))}
            </ul>
          </div>
        </>
      ) : null}
    </div>
  );
}
