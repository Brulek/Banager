import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { CopyButton } from "../components/CopyButton";
import { CheckIcon, SpinnerIcon } from "../components/icons";
import { EndedWithout } from "../components/EndedWithout";
import { TextWithInfo } from "../components/InfoDetail";
import { DiscoverResults, DiscoverSearchField } from "../components/DiscoverSearch";
import { HomebrewSetupSheet } from "../components/HomebrewSetupSheet";
import { InstallSheet } from "../components/InstallSheet";
import { SourceAvatar } from "../components/SourceAvatar";
import { ToolAvatar } from "../components/ToolAvatar";
import { BUTTON } from "../components/ui/controls";
import { FORM_COLUMN, GROUP, GROUP_FOOTNOTE, GROUP_ROW_TWO_LINES, GROUP_TITLE, SMALL_WRAPPING } from "../components/ui/group";
import {
  commandOf,
  endedAfterCheck,
  entryName,
  iconKeyOf,
  installTargetOf,
  installOperationOf,
  installProgressOf,
  isUnderway,
  ITEM_KEYS,
  stateText,
  wordsOf,
  type InstallProgress,
} from "../lib/catalog";
import { useCatalog, useOpenOllamaApp, useOperations, usePendingOperations, useSettings, useSnapshot } from "../lib/queries";
import { openOllamaErrorMessage } from "../lib/sources";
import { useUiStore } from "../store/ui";
import type { CatalogCategory, CatalogEntry } from "../lib/types";

/** The groups, in the order the page lists them, and each one's title. */
const CATEGORIES: [CatalogCategory, string][] = [
  ["Basics", "discover.categories.basics"],
  ["AiAssistants", "discover.categories.aiAssistants"],
  ["LocalModels", "discover.categories.localModels"],
  ["CommonTools", "discover.categories.commonTools"],
];

/**
 * The Discover page (phase 5): the catalog Banager suggests, by group,
 * each entry with its logo, its name, what it is -- with who needs it and
 * how it is used behind the ⓘ -- and on the right whether this Mac has it,
 * by any way the catalog knows of (`get_catalog`). Until a check has
 * finished, nothing is said of any entry: an empty snapshot would say
 * everything needs Homebrew.
 *
 * A "Not installed" entry the catalog installs from here has Install in
 * its state's place (step B), which opens the install sheet
 * (`InstallSheet`). Once started, the row says how it goes -- waiting,
 * 「正在安装…」 with a spinner -- and how it ended: installed, the check
 * after it having found it, with how to use it in place of its line and
 * Copy for what to type; or not, in a word that opens its log, with
 * Install to try again. Homebrew's own Install, where it is not there,
 * opens the Terminal hand-off (`HomebrewSetupSheet`, step C), and the
 * line over the list says to start with it. The starter model (step D)
 * installs with Ollama: where Ollama's app is there and not running, its
 * row, and Ollama's own once installed from here, have Open Ollama.
 */
export function DiscoverPage() {
  const { t } = useTranslation();
  const { data: catalogView } = useCatalog();
  const catalog = catalogView?.entries;
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const { data: operations } = useOperations();
  const pending = usePendingOperations();
  const opFinishedAt = useUiStore((s) => s.opFinishedAt);
  const [installing, setInstalling] = useState<CatalogEntry | null>(null);
  const [settingUpHomebrew, setSettingUpHomebrew] = useState(false);
  // What the search field has (step E): while it has a text, and the field
  // is there, the results are in the list's place.
  const [query, setQuery] = useState("");
  // Checked: the snapshot, and the one the catalog shown was made from --
  // not the empty one of before the first check, shown while the first
  // check's catalog is asked for (from the independent review of the
  // branch).
  const checked = snapshot?.refreshed_at != null && catalogView?.madeFrom?.refreshed_at != null;
  const brew = snapshot?.instances.find((instance) => instance.adapter_id === "brew");
  const brewCanInstall = snapshot?.instances.some(
    (instance) =>
      instance.adapter_id === "brew" && instance.status.unavailable === null && instance.read_only_reason === null,
  );
  // A field that went -- Homebrew stopped answering -- takes its results
  // with it: the suggestions are back.
  const searching = checked && brewCanInstall === true && query.trim().length > 0;
  // Homebrew is there, and cannot install: the page says where to see why.
  const homebrewNotReady = checked && brew !== undefined && brewCanInstall !== true;
  // No Homebrew: it comes first, and the page says so -- or, on a Mac its
  // installer no longer supports, that the tools below that install with
  // Homebrew cannot be installed from here, though each row says it needs
  // Homebrew. Not the model, which Ollama downloads, an Intel Mac's
  // included (from the review of the fix batch).
  const homebrewState = checked ? catalog?.find((entry) => entry.id === "homebrew")?.state : undefined;
  const homebrewFirst = homebrewState === "NotInstalled";
  const homebrewUnsupported = homebrewState === "HomebrewUnsupported";
  const sourceLabel = t("adapters.brew");
  // Ollama's app is here and not running: Open Ollama, as its source's
  // notice has it.
  const openOllama = useOpenOllamaApp();
  // Or no Ollama source at all while the model waits for Ollama
  // (`OllamaNotReady`): its app is there with no `ollama` command yet,
  // which its first run offers to put in place.
  const noOllamaSource = !(snapshot?.instances.some((instance) => instance.adapter_id === "ollama") ?? true);
  const ollamaStopped =
    snapshot?.instances.some(
      (instance) => instance.adapter_id === "ollama" && instance.status.unavailable === "NotRunning",
    ) || noOllamaSource;
  // Where `open -a Ollama` failed, why, beside the button, as Ollama's
  // notice says it: a press that does nothing would say nothing. Nor one
  // that opened it and found no `ollama` command still -- the app was
  // running, or its offer to put the command in place was turned down
  // (from the review of 8f7a680b). On the row's right (`end`), the words
  // wrap within a column of their own, so the row's name and what it is
  // keep their room; under its name, in the line's own flow.
  const openOllamaButton = (where: "end" | "line") => {
    // A word too long for the column, or for the row under the name -- a
    // raw error with technical details on -- breaks rather than spilling
    // over the button or off the row (from the reviews of 206ada35 and
    // 9baf7a74).
    const width = where === "end" ? "max-w-56 text-right wrap-anywhere" : "min-w-0 wrap-anywhere";
    return (
      <span className="flex items-center gap-2">
        {openOllama.isError ? (
          <span role="alert" className={`text-danger-text ${SMALL_WRAPPING} ${width}`}>
            {openOllamaErrorMessage(t, openOllama.error.message, settings?.show_technical_details ?? false)}
          </span>
        ) : openOllama.isSuccess && noOllamaSource ? (
          <span role="status" className={`text-muted ${SMALL_WRAPPING} ${width}`}>
            {t("discover.ollamaNoCommand")}
          </span>
        ) : null}
        <button
          type="button"
          onClick={() => openOllama.mutate()}
          disabled={openOllama.isPending}
          className={BUTTON.small.grey}
        >
          {t("sourceNotice.openOllama")}
        </button>
      </span>
    );
  };

  const right = (entry: CatalogEntry, name: string, progress: InstallProgress | null, installed: boolean): ReactNode => {
    if (progress !== null && isUnderway(progress)) {
      const word =
        progress.kind === "queued"
          ? t("discover.progress.queued")
          : progress.kind === "running"
            ? t("discover.progress.running")
            : t("discover.progress.cancelling");
      return (
        <span data-catalog-state="" className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap text-small text-muted">
          {progress.kind === "queued" ? null : <SpinnerIcon size={16} className="shrink-0" />}
          {word}
        </span>
      );
    }
    // Installed by any way the catalog knows of, or by the install that
    // just ended, before the check after it has said so.
    if (installed) {
      return (
        <span data-catalog-state="" className="flex shrink-0 items-center gap-1 text-muted">
          <CheckIcon size={14} />
          {stateText(t, "Installed")}
        </span>
      );
    }
    // Homebrew itself is installed in Terminal (`HomebrewSetupSheet`);
    // the rest from here (`InstallSheet`).
    const install =
      entry.state === "NotInstalled" && (entry.installs_here || entry.install === "HomebrewItself") ? (
        <button
          type="button"
          onClick={() => (entry.install === "HomebrewItself" ? setSettingUpHomebrew(true) : setInstalling(entry))}
          aria-label={t("discover.installLabel", { name })}
          className={BUTTON.small.grey}
        >
          {t("discover.install")}
        </button>
      ) : null;
    // How the last install ended, beside Install to try again -- or beside
    // why this Mac cannot install it now, which a check since may say.
    const ended =
      progress === null || progress.kind === "installed" ? null : progress.kind === "cancelled" ? (
        <span className="whitespace-nowrap text-small text-muted">{t("discover.progress.cancelled")}</span>
      ) : (
        <EndedWithout progress={progress} name={name} />
      );
    const state = install ?? <span className="text-muted">{stateText(t, entry.state)}</span>;
    // The model, waiting for an Ollama that is installed and not running --
    // after a download that failed or was stopped too, beside how it ended
    // (from the review of the areas earlier reviews left).
    if (entry.state === "OllamaNotReady" && ollamaStopped) {
      return (
        <span data-catalog-state="" className="flex shrink-0 items-center gap-2">
          {ended}
          {openOllamaButton("end")}
        </span>
      );
    }
    if (ended === null && install === null) {
      return (
        <span data-catalog-state="" className="flex shrink-0 items-center gap-1 text-muted">
          {stateText(t, entry.state)}
        </span>
      );
    }
    return (
      <span data-catalog-state="" className="flex shrink-0 items-center gap-2">
        {ended}
        {state}
      </span>
    );
  };

  return (
    <div className={FORM_COLUMN}>
      <p className="px-2.5 text-muted">
        {t(
          homebrewFirst
            ? "discover.introNoHomebrew"
            : homebrewUnsupported
              ? "discover.introHomebrewUnsupported"
              : homebrewNotReady
                ? "discover.introHomebrewNotReady"
                : "discover.intro",
        )}
      </p>
      {/* Searching takes a Homebrew to search with, and to install into:
          one that answered and can be written to. */}
      {checked && brewCanInstall ? <DiscoverSearchField query={query} onChange={setQuery} /> : null}
      {searching ? <DiscoverResults query={query} /> : null}
      {searching ? null : CATEGORIES.map(([category, titleKey]) => {
        const entries = (catalog ?? []).filter((entry) => entry.category === category);
        if (entries.length === 0) return null;
        const titleId = `discover-${category}`;
        return (
          <section key={category} aria-labelledby={titleId}>
            <h2 id={titleId} className={GROUP_TITLE}>
              {t(titleKey)}
            </h2>
            <ul className={GROUP}>
              {entries.map((entry) => {
                const keys = ITEM_KEYS[entry.id];
                const name = entryName(t, entry.id);
                const op = installOperationOf(entry, operations ?? [], pending);
                const progress = op === null ? null : installProgressOf(op);
                // What the install that just ended says is newer than the
                // catalog until the check after it comes; after that, the
                // catalog says what there is. Measured against the
                // snapshot the catalog was made from, which is the last
                // one while the new one's is asked for (from the review of
                // 44674745).
                const justInstalled =
                  progress?.kind === "installed" &&
                  op !== null &&
                  endedAfterCheck(op, opFinishedAt, catalogView?.madeFrom ?? null);
                const installed = entry.state === "Installed" || justInstalled;
                // Installed from here: how to use it, in place of what it is.
                const use = installed && progress?.kind === "installed" ? keys?.use : undefined;
                const command = commandOf(entry);
                const words = wordsOf(entry);
                const target = installTargetOf(entry);
                return (
                  <li key={entry.id} data-catalog-entry={entry.id} className={GROUP_ROW_TWO_LINES}>
                    <span className="flex min-w-0 items-center gap-2.5">
                      {entry.install === "HomebrewItself" ? (
                        // Homebrew is its own source: its mark alone, as the
                        // sidebar's row has it.
                        <SourceAvatar adapterId="brew" label={sourceLabel} size="sm" />
                      ) : (
                        <ToolAvatar
                          size="sm"
                          adapterId={target?.adapterId ?? "brew"}
                          sourceLabel={target?.adapterId === "ollama" ? t("adapters.ollama") : sourceLabel}
                          iconKey={iconKeyOf(entry, brew)}
                        />
                      )}
                      <span className="min-w-0">
                        <span className="block truncate text-foreground">{name}</span>
                        {keys === undefined ? null : use !== undefined && (command !== undefined || keys.opens) ? (
                          <span
                            data-catalog-use=""
                            className={`flex flex-wrap items-center gap-x-2 gap-y-1 text-muted ${SMALL_WRAPPING}`}
                          >
                            <span>{t(use, words)}</span>
                            {command !== undefined ? (
                              <CopyButton text={command} label={t("discover.copyCommand", { command })} />
                            ) : ollamaStopped ? (
                              // Ollama's app, until it runs.
                              openOllamaButton("line")
                            ) : null}
                          </span>
                        ) : (
                          <span className={`block text-muted ${SMALL_WRAPPING}`}>
                            <TextWithInfo text={t(keys.what, words)} label={t("common.detailsLabel", { title: name })}>
                              {t(keys.more, words)}
                            </TextWithInfo>
                          </span>
                        )}
                      </span>
                    </span>
                    {checked ? right(entry, name, progress, installed) : null}
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
      {searching ? null : <p className={GROUP_FOOTNOTE}>{t("discover.footnote")}</p>}
      <HomebrewSetupSheet open={settingUpHomebrew} onClose={() => setSettingUpHomebrew(false)} />
      <InstallSheet
        entry={installing}
        iconKey={installing === null ? undefined : iconKeyOf(installing, brew)}
        onClose={() => setInstalling(null)}
      />
    </div>
  );
}
