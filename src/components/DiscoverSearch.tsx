import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  commandOf,
  endedAfterCheck,
  installOperationFor,
  installProgressOf,
  isUnderway,
  ITEM_KEYS,
  searchable,
  stateText,
  wordsOf,
} from "../lib/catalog";
import { useCatalog, useOperations, usePendingOperations, useSearchHomebrew, useSettings, useSnapshot } from "../lib/queries";
import { useSettled } from "../lib/settled";
import { planErrorMessage } from "../lib/sources";
import { useTranslatedDescription } from "../lib/toolDescriptions";
import { useUiStore } from "../store/ui";
import type { ArtifactKey, Found } from "../lib/types";
import { EndedWithout } from "./EndedWithout";
import { CopyButton } from "./CopyButton";
import { CheckIcon, SearchIcon, SpinnerIcon } from "./icons";
import { InstallSheet } from "./InstallSheet";
import { SheetPending } from "./SheetParts";
import { ToolAvatar } from "./ToolAvatar";
import { BUTTON } from "./ui/controls";
import { GROUP, GROUP_FOOTNOTE, GROUP_ROW_TWO_LINES, GROUP_TITLE, SMALL_WRAPPING } from "./ui/group";

/** How long the text has to stay the same before it is searched for, in ms: Homebrew takes a second or more a search. */
export const SEARCH_AFTER_MS = 600;

/** How long a search's results can be installed by their ids (Rust `FOUND_LIFETIME`), in ms. */
const FOUND_LIFETIME_MS = 10 * 60 * 1000;

/**
 * Why a search listed nothing, in the page's words -- with technical
 * details on too, where the refusal is one of the window's own kinds: not
 * its JSON (from the independent review of the branch).
 */
function searchError(
  t: (key: string, options?: Record<string, string | number>) => string,
  raw: string,
  technical: boolean,
): string {
  try {
    const parsed = JSON.parse(raw) as { kind?: unknown; state?: unknown };
    if (parsed.kind === "invalid_query") return t("discover.search.invalid");
    if (parsed.kind === "catalog_state") return t("discover.search.needsHomebrew");
  } catch {
    // Not one of the window's own refusals: said below.
  }
  const message = planErrorMessage(t, raw, t("adapters.brew"), technical);
  return message === null ? t("discover.search.failedPlain") : t("discover.search.failed", { message });
}

/**
 * The search field over Discover's list (phase 5 step E): what is typed in
 * it, for the page to show the results in the list's place.
 */
export function DiscoverSearchField({ query, onChange }: { query: string; onChange: (query: string) => void }) {
  const { t } = useTranslation();
  return (
    <span className="relative mx-2.5 flex h-7 items-center">
      <SearchIcon size={14} className="pointer-events-none absolute left-2 text-muted" />
      <input
        type="search"
        value={query}
        onChange={(e) => onChange(e.target.value)}
        placeholder={t("discover.search.placeholder")}
        aria-label={t("discover.search.label")}
        maxLength={64}
        // A tool's name is no word: nothing underlined or corrected.
        spellCheck={false}
        className="h-7 w-full appearance-none rounded-control bg-fill-subtle pl-7 pr-2 text-body text-foreground placeholder:text-muted [&::-webkit-search-decoration]:appearance-none"
      />
    </span>
  );
}

/**
 * What Homebrew has for `query` (step E), in the list's place while there
 * is a text: what Homebrew's own search lists for it among its formulae
 * and casks -- names with it in them, aliases, close spellings
 * (`search_homebrew`; a description only fills in what each is, since
 * `--desc` names a third-party tap's package by its short name), each
 * with what it is, in the window's
 * words where it has them, whether it is one of the suggestions, and
 * Install -- which opens the install sheet for that result, by its id --
 * or that it is installed. An install under way or ended says so in its
 * row, as a suggestion's does.
 */
export function DiscoverResults({ query }: { query: string }) {
  const { t } = useTranslation();
  const settled = useSettled(query.trim(), SEARCH_AFTER_MS);
  const asked = searchable(settled) ? settled : "";
  const search = useSearchHomebrew(asked);
  const { data: snapshot } = useSnapshot();
  const catalog = useCatalog().data?.entries;
  const { data: operations } = useOperations();
  const { data: settings } = useSettings();
  const pending = usePendingOperations();
  const opFinishedAt = useUiStore((s) => s.opFinishedAt);
  const describe = useTranslatedDescription();
  const [installing, setInstalling] = useState<Found | null>(null);
  const brew = snapshot?.instances.find((instance) => instance.adapter_id === "brew");
  const keyOf = (found: Found): ArtifactKey => ({
    instance_id: brew?.id ?? "brew:/opt/homebrew",
    kind: found.kind,
    name: found.name,
  });
  const descriptionOf = (found: Found) => describe(keyOf(found), "brew") ?? found.description;

  // A list kept through a failed search again while its ids are kept, and
  // no longer: by the failure's own time, which re-renders the page with
  // each failure, and by the clock, looked at once more when the ten
  // minutes are up (the timer below) -- a failure a minute before then
  // must not keep the list past them (from the reviews of the fix batch).
  const [, lookAgain] = useState(0);
  const expiresIn = search.isError && search.data !== undefined ? search.dataUpdatedAt + FOUND_LIFETIME_MS - Date.now() : null;
  useEffect(() => {
    if (expiresIn === null || expiresIn <= 0) return;
    const timer = setTimeout(() => lookAgain((n) => n + 1), expiresIn);
    return () => clearTimeout(timer);
  }, [expiresIn === null, search.dataUpdatedAt]);
  const expired =
    search.isError &&
    (search.errorUpdatedAt - search.dataUpdatedAt >= FOUND_LIFETIME_MS ||
      Date.now() - search.dataUpdatedAt >= FOUND_LIFETIME_MS);
  // Nor while the text is one it does not take: the list is gone at once.
  const listShown =
    search.data !== undefined && !expired && !(query.trim().length > 0 && !searchable(query.trim()));
  let body: ReactNode;
  if (query.trim().length > 0 && !searchable(query.trim())) {
    body = <p className="px-2.5 text-muted">{t("discover.search.invalid")}</p>;
  } else if (asked === "" || search.isPending) {
    body = <SheetPending text={t("discover.search.searching")} />;
  } else if (!listShown || search.data === undefined) {
    // Why it could not search -- where it has nothing to show, or what it
    // has is past its ids' ten minutes: a search asked again in the
    // background (every five minutes) that fails leaves the list it had
    // while its ids are kept, and no longer (from the reviews of the fix
    // batch).
    body = search.isError ? (
      <p role="alert" className="px-2.5 text-danger-text">
        {searchError(t, search.error.message, settings?.show_technical_details ?? false)}
      </p>
    ) : (
      <SheetPending text={t("discover.search.searching")} />
    );
  } else if (search.data.length === 0) {
    body = <p className="px-2.5 text-muted">{t("discover.search.none", { query: asked })}</p>;
  } else {
    body = (
      <section aria-labelledby="discover-results">
        <h2 id="discover-results" className={GROUP_TITLE}>
          {t("discover.search.title")}
        </h2>
        <ul className={GROUP}>
          {search.data.map((found) => {
            const op = installOperationFor({ adapterId: "brew", kind: found.kind, name: found.name }, operations ?? [], pending);
            const progress = op === null ? null : installProgressOf(op);
            const listed = snapshot?.artifacts.some(
              (a) => a.key.instance_id.startsWith("brew:") && a.key.kind === found.kind && a.key.name === found.name,
            );
            // What the last check lists, or the search said where there is
            // no snapshot; or the install that just ended, before the check
            // after it.
            // A suggestion goes by its entry, as its row on the list does:
            // installed another way (Claude Code from npm), or one this Mac
            // cannot install (Ollama before macOS 14) -- `plan_found`
            // refuses either.
            const entry = found.catalog_id === null ? undefined : catalog?.find((e) => e.id === found.catalog_id);
            const installed =
              (listed ?? found.installed) ||
              entry?.state === "Installed" ||
              (progress?.kind === "installed" && op !== null && endedAfterCheck(op, opFinishedAt, snapshot ?? null));
            const notHere = entry !== undefined && entry.state !== "NotInstalled" && entry.state !== "Installed";
            const description = descriptionOf(found);
            // A suggestion installed from here: how to use it, with Copy, in
            // place of what it is, as its row on the list says it. Not one
            // with nothing to type -- Ollama's app, whose row on the list
            // has Open Ollama while it is not running (from the review of
            // b5f7fca5).
            const command = entry === undefined ? undefined : commandOf(entry);
            const use =
              entry !== undefined && command !== undefined && installed && progress?.kind === "installed"
                ? ITEM_KEYS[entry.id]?.use
                : undefined;
            let right: ReactNode;
            if (progress !== null && isUnderway(progress)) {
              right = (
                <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-small text-muted">
                  {progress.kind === "queued" ? null : <SpinnerIcon size={16} className="shrink-0" />}
                  {t(progress.kind === "queued" ? "discover.progress.queued" : progress.kind === "running" ? "discover.progress.running" : "discover.progress.cancelling")}
                </span>
              );
            } else if (installed) {
              right = (
                <span className="flex items-center gap-1 text-muted">
                  <CheckIcon size={14} />
                  {t("discover.state.installed")}
                </span>
              );
            } else if (notHere) {
              right = <span className="text-muted">{stateText(t, entry.state)}</span>;
            } else {
              right = (
                <span className="flex items-center gap-2">
                  {progress === null || progress.kind === "installed" ? null : progress.kind === "cancelled" ? (
                    <span className="whitespace-nowrap text-small text-muted">{t("discover.progress.cancelled")}</span>
                  ) : (
                    <EndedWithout progress={progress} name={found.name} />
                  )}
                  <button
                    type="button"
                    onClick={() => setInstalling(found)}
                    aria-label={t("discover.installLabel", { name: found.name })}
                    className={BUTTON.small.grey}
                  >
                    {t("discover.install")}
                  </button>
                </span>
              );
            }
            return (
              // By what it is, not its id, which every search renews: a row
              // searched again keeps its place and the focus in it.
              <li key={`${found.kind}:${found.name}`} data-found={found.name} className={GROUP_ROW_TWO_LINES}>
                <span className="flex min-w-0 items-center gap-2.5">
                  <ToolAvatar size="sm" adapterId="brew" sourceLabel={t("adapters.brew")} iconKey={keyOf(found)} />
                  <span className="min-w-0">
                    <span className="flex min-w-0 items-center gap-1.5">
                      <span className="truncate text-foreground">{found.name}</span>
                      {/* One of the suggestions, or not: said beside the name. */}
                      <span data-found-mark="" className="shrink-0 rounded-sm bg-fill-subtle px-1 text-small text-muted">
                        {t(found.catalog_id === null ? "discover.search.notSuggestedMark" : "discover.search.suggestedMark")}
                      </span>
                    </span>
                    {use !== undefined && entry !== undefined ? (
                      <span
                        data-found-use=""
                        className={`flex flex-wrap items-center gap-x-2 gap-y-1 text-muted ${SMALL_WRAPPING}`}
                      >
                        <span>{t(use, wordsOf(entry))}</span>
                        {command === undefined ? null : <CopyButton text={command} label={t("discover.copyCommand", { command })} />}
                      </span>
                    ) : description === null ? null : (
                      <span className={`block truncate text-muted ${SMALL_WRAPPING}`}>{description}</span>
                    )}
                  </span>
                </span>
                <span data-found-state="" className="flex shrink-0 items-center">
                  {right}
                </span>
              </li>
            );
          })}
        </ul>
        <p className={GROUP_FOOTNOTE}>{t("discover.search.footnote")}</p>
      </section>
    );
  }

  // What a screen reader hears as the results come: how many, none
  // included. Always there, empty meanwhile: a status put in the page with
  // its words may never be read.
  const said =
    asked === "" || !listShown || search.data === undefined ? "" : t("discover.search.count", { count: search.data.length });
  return (
    <>
      <p role="status" className="sr-only">
        {said}
      </p>
      {body}
      <InstallSheet
        entry={null}
        found={installing}
        iconKey={installing === null ? undefined : keyOf(installing)}
        description={installing === null ? null : descriptionOf(installing)}
        onClose={() => setInstalling(null)}
      />
    </>
  );
}
