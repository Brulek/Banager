import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode, Ref } from "react";
import { useTranslation } from "react-i18next";
import { useSettings, useSaveSettings, useSnapshot } from "../lib/queries";
import { ADAPTER_LABEL_KEYS, adapterIdOf, adapterLabel, settingsSaveErrorMessage } from "../lib/sources";
import { shownSkippedVersion, skippedVersionId } from "../lib/updateState";
import type { ArtifactKey, Settings, Language, SkippedVersion } from "../lib/types";
import { artifactKeyId, useUiStore } from "../store/ui";
import { Switch } from "../components/ui/Switch";
import { IconCreditsDrawer } from "../components/IconCreditsDrawer";

const LANGUAGES: Language[] = ["System", "En", "ZhCn"];

function languageLabelKey(lang: Language): string {
  if (lang === "System") return "settings.language.system";
  if (lang === "En") return "settings.language.english";
  return "settings.language.chinese";
}

/** A row's own button: Stop skipping, Remind me again, View. */
const ROW_BUTTON =
  "h-7 shrink-0 rounded-button border border-border bg-surface px-3 text-small font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent";

/**
 * One group of settings: its title in the section style, over a card that
 * holds its rows, a hairline between each two -- the Overview's panels'
 * look. A region, named by its title. With `headingRef`, a group the page
 * can be opened at: its title can then take the focus from a script
 * (`tabIndex` -1), and draws no ring for it, as the page's title does.
 */
function SettingsGroup({
  title,
  headingRef,
  children,
}: {
  title: string;
  headingRef?: Ref<HTMLHeadingElement>;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId}>
      <h2
        id={headingId}
        ref={headingRef}
        tabIndex={headingRef === undefined ? undefined : -1}
        className="mb-2 px-1 text-section text-foreground outline-none"
      >
        {title}
      </h2>
      <div className="divide-y divide-border rounded-panel border border-border bg-surface">{children}</div>
    </section>
  );
}

/**
 * One setting in a group: what it is and, in a line under it, what it
 * does, with its control on the right. `label` is the element that names
 * the control -- a `<label>` for a switch.
 */
function SettingRow({
  label,
  description,
  control,
}: {
  label: ReactNode;
  description?: ReactNode;
  control: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        {label}
        {description}
      </div>
      {control}
    </div>
  );
}

const ROW_LABEL = "block text-body font-medium text-foreground";
const ROW_DESCRIPTION = "mt-0.5 text-small text-muted";

/**
 * A hidden update's software, the way the Updates and Installed rows name
 * it: its name, then its source's in small muted text -- none for a tool
 * with its own installer, which is its own source.
 */
function EntryName({ name, source }: { name: string; source: string | undefined }) {
  return (
    <>
      <span className="truncate text-body font-medium text-foreground">{name}</span>
      {source !== undefined ? <span className="shrink-0 text-small text-muted">{source}</span> : null}
    </>
  );
}

/**
 * Settings, in four cards: 「通用」 -- the language, and whether to show
 * technical details -- 「更新」 -- whether Homebrew's self-updating apps
 * are listed -- 「已隐藏的更新」, the versions skipped and the software
 * never to be reminded about, each with the button that takes it back,
 * where the Overview's count of hidden updates opens the page -- and
 * 「关于」, whose 「图标来源」 row opens the credits for the logos
 * built into the app (`IconCreditsDrawer`). Every change is saved at
 * once; one that cannot be saved is undone on screen and said at the top.
 */
export function SettingsPage() {
  const { t } = useTranslation();
  const settingsQuery = useSettings();
  const saveMutation = useSaveSettings();
  const { data: snapshot } = useSnapshot();
  const [draft, setDraft] = useState<Settings | null>(null);
  const [creditsOpen, setCreditsOpen] = useState(false);
  const languageRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const languageLabelId = useId();
  const skippedTitleId = useId();
  const ignoredTitleId = useId();

  useEffect(() => {
    if (settingsQuery.data && draft === null) {
      setDraft(settingsQuery.data);
    }
  }, [settingsQuery.data, draft]);

  // What the Updates and Installed rows call each package they list: its
  // `display_name` ("Claude Code", where the package is "claude").
  const displayNames = useMemo(() => {
    const byId = new Map<string, string>();
    for (const artifact of snapshot?.artifacts ?? []) {
      byId.set(artifactKeyId(artifact.key), artifact.display_name);
    }
    return byId;
  }, [snapshot]);

  const current = draft ?? settingsQuery.data;
  const loaded = !settingsQuery.isLoading && current !== undefined;

  // Opened from the Overview's 「2 个已隐藏」 (`showHiddenUpdates`): the
  // group of hidden updates -- its title and the card under it -- in view,
  // and the focus on its title, as soon as it is on screen.
  const hiddenHeading = useRef<HTMLHeadingElement>(null);
  const hiddenUpdatesRequested = useUiStore((s) => s.hiddenUpdatesRequested);
  const hiddenUpdatesShown = useUiStore((s) => s.hiddenUpdatesShown);
  useEffect(() => {
    const heading = hiddenHeading.current;
    if (!hiddenUpdatesRequested || heading === null) return;
    heading.parentElement?.scrollIntoView?.({ block: "nearest" });
    heading.focus({ preventScroll: true });
    hiddenUpdatesShown();
  }, [hiddenUpdatesRequested, loaded, hiddenUpdatesShown]);

  if (settingsQuery.isLoading || !current) {
    return <p className="px-6 text-body text-muted">{t("common.loading")}</p>;
  }

  // Arrow functions declared *after* the early return, not hoisted function
  // declarations: TypeScript only carries the `!current` narrowing into
  // function expressions, so a `function persist() {}` here would see
  // `current` as `Settings | undefined` and fail `pnpm build` (TS18048 /
  // TS2345) two tasks later, at Task 17's type-check.
  const persist = (next: Settings) => {
    const previous = current;
    setDraft(next);
    saveMutation.mutate(next, {
      onError: () => setDraft(previous),
    });
  };

  // Arrow keys walk the language group and wrap at both ends, the way a
  // native radio group does. Selection follows focus (WAI-ARIA's radio
  // pattern), so the arrow that moves the highlight also saves the choice.
  const moveLanguage = (event: KeyboardEvent, index: number) => {
    const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
    const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
    if (!back && !forward) return;
    event.preventDefault();
    const next = (index + (forward ? 1 : -1) + LANGUAGES.length) % LANGUAGES.length;
    persist({ ...current, language: LANGUAGES[next] });
    languageRefs.current[next]?.focus();
  };

  // An entry's name and source. Where the snapshot no longer lists the
  // package, a tool with its own installer is still named by its product
  // name, which is its source's too (its `display_name` is that name, from
  // its recipe in crates/canager-core/src/adapters/standalone/), and
  // anything else by the package's own name. The source is left out where
  // it would only say the name again, as on the Updates page's rows. It
  // goes by the key alone: an instance id starts with its adapter's.
  const entryOf = (key: ArtifactKey): { name: string; source: string | undefined } => {
    const adapterId = adapterIdOf(key.instance_id);
    const source = adapterLabel(t, adapterId);
    const standalone =
      adapterId.startsWith("standalone-") &&
      Object.prototype.hasOwnProperty.call(ADAPTER_LABEL_KEYS, adapterId);
    const name = displayNames.get(artifactKeyId(key)) || (standalone ? source : key.name);
    return { name, source: source === name ? undefined : source };
  };

  const unignore = (key: Settings["ignored_updates"][number]) => {
    persist({
      ...current,
      ignored_updates: current.ignored_updates.filter(
        (k) => artifactKeyId(k) !== artifactKeyId(key),
      ),
    });
  };

  const unskip = (skipped: SkippedVersion) => {
    persist({
      ...current,
      skipped_versions: current.skipped_versions.filter(
        (s) => skippedVersionId(s) !== skippedVersionId(skipped),
      ),
    });
  };

  return (
    // No title of its own: the page header over it says "Settings".
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-6 px-6 pb-8">
      {saveMutation.isError && (
        <p role="alert" className="text-body text-danger">
          {t("settings.saveError", {
            message: settingsSaveErrorMessage(t, saveMutation.error.message),
          })}
        </p>
      )}

      <SettingsGroup title={t("settings.groups.general")}>
        <SettingRow
          label={
            <span id={languageLabelId} className={ROW_LABEL}>
              {t("settings.language.label")}
            </span>
          }
          control={
            // A segmented control, the way the Installed page's sort is
            // drawn, with a radio group's keyboard.
            <div role="radiogroup" aria-labelledby={languageLabelId} className="flex shrink-0 rounded-button bg-hover p-0.5">
              {LANGUAGES.map((lang, index) => {
                const selected = current.language === lang;
                return (
                  <button
                    key={lang}
                    ref={(node) => {
                      languageRefs.current[index] = node;
                    }}
                    type="button"
                    role="radio"
                    aria-checked={selected}
                    // Roving tabindex: one Tab stop for the whole group, landing
                    // on the current choice, which is what a radio group is
                    // supposed to feel like from the keyboard. Without it Tab
                    // stopped at all three buttons and the arrow keys did nothing.
                    tabIndex={selected ? 0 : -1}
                    onKeyDown={(event) => moveLanguage(event, index)}
                    onClick={() => persist({ ...current, language: lang })}
                    // The choice is marked for the eye, not only for a
                    // screen reader: a raised segment of its own.
                    className={`rounded-[6px] px-3 py-1 text-small font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-accent ${
                      selected ? "bg-surface text-foreground shadow-sm" : "text-muted hover:text-foreground"
                    }`}
                  >
                    {t(languageLabelKey(lang))}
                  </button>
                );
              })}
            </div>
          }
        />
        <SettingRow
          label={
            <label htmlFor="settings-show-technical" className={ROW_LABEL}>
              {t("settings.showTechnicalDetails.label")}
            </label>
          }
          description={
            <p id="settings-show-technical-desc" className={ROW_DESCRIPTION}>
              {t("settings.showTechnicalDetails.description")}
            </p>
          }
          control={
            <Switch
              id="settings-show-technical"
              aria-describedby="settings-show-technical-desc"
              checked={current.show_technical_details}
              onCheckedChange={(checked) => persist({ ...current, show_technical_details: checked })}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title={t("settings.groups.updates")}>
        <SettingRow
          label={
            <label htmlFor="settings-include-self-updating" className={ROW_LABEL}>
              {t("settings.includeSelfUpdating.label")}
            </label>
          }
          description={
            <p id="settings-include-self-updating-desc" className={ROW_DESCRIPTION}>
              {t("settings.includeSelfUpdating.description")}
            </p>
          }
          control={
            <Switch
              id="settings-include-self-updating"
              aria-describedby="settings-include-self-updating-desc"
              checked={current.include_self_updating}
              onCheckedChange={(checked) => persist({ ...current, include_self_updating: checked })}
            />
          }
        />
      </SettingsGroup>

      {/* The two ways the Updates page hides an update, each in its own
          list, because they end differently: a skip stops hiding anything
          by itself once its source offers another version, while a package
          never to be reminded about stays hidden until it is removed here.

          Every stored skip is listed, including one whose version its
          source has since moved past: such an entry hides nothing any more
          (`hidingRule` matches only the version a row offers), and it is
          shown rather than dropped. This page reads the snapshot for names
          only (`entryOf`), and every other change saved here writes the
          list back as it is, so an entry leaves it only when the user
          presses Stop skipping on it, or skips that package's next version
          on the Updates page, which replaces it (`withSkippedVersion`). */}
      <SettingsGroup title={t("settings.groups.hidden")} headingRef={hiddenHeading}>
        <section aria-labelledby={skippedTitleId} className="px-4 py-3">
          <h3 id={skippedTitleId} className="text-small font-semibold text-muted">
            {t("settings.skippedVersions.title")}
          </h3>
          {current.skipped_versions.length === 0 ? (
            <p className="mt-1.5 text-body text-muted">{t("settings.skippedVersions.empty")}</p>
          ) : (
            <ul className="mt-1 flex flex-col">
              {current.skipped_versions.map((skipped) => {
                // An Ollama model's skipped version is a digest, never shown.
                const version = shownSkippedVersion(skipped);
                const { name, source } = entryOf(skipped.key);
                return (
                  <li key={skippedVersionId(skipped)} className="flex items-center justify-between gap-4 py-1.5">
                    <span className="flex min-w-0 items-baseline gap-2">
                      <EntryName name={name} source={source} />
                      <span className="shrink-0 text-small tabular-nums text-muted">
                        {version ?? t("settings.skippedVersions.newBuild")}
                      </span>
                    </span>
                    <button
                      type="button"
                      aria-label={
                        version === null
                          ? t("settings.skippedVersions.unskipNewBuildAriaLabel", { name })
                          : t("settings.skippedVersions.unskipAriaLabel", { name, version })
                      }
                      onClick={() => unskip(skipped)}
                      className={ROW_BUTTON}
                    >
                      {t("settings.skippedVersions.unskip")}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <section aria-labelledby={ignoredTitleId} className="px-4 py-3">
          <h3 id={ignoredTitleId} className="text-small font-semibold text-muted">
            {t("settings.ignoredUpdates.title")}
          </h3>
          {current.ignored_updates.length === 0 ? (
            <p className="mt-1.5 text-body text-muted">{t("settings.ignoredUpdates.empty")}</p>
          ) : (
            <ul className="mt-1 flex flex-col">
              {current.ignored_updates.map((key) => {
                const { name, source } = entryOf(key);
                return (
                  <li key={artifactKeyId(key)} className="flex items-center justify-between gap-4 py-1.5">
                    <span className="flex min-w-0 items-baseline gap-2">
                      <EntryName name={name} source={source} />
                    </span>
                    <button
                      type="button"
                      aria-label={t("settings.ignoredUpdates.unignoreAriaLabel", { name })}
                      onClick={() => unignore(key)}
                      className={ROW_BUTTON}
                    >
                      {t("settings.ignoredUpdates.unignore")}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </SettingsGroup>

      <SettingsGroup title={t("settings.groups.about")}>
        <SettingRow
          label={<span className={ROW_LABEL}>{t("settings.iconCredits.label")}</span>}
          description={<p className={ROW_DESCRIPTION}>{t("settings.iconCredits.description")}</p>}
          control={
            <button
              type="button"
              aria-label={t("settings.iconCredits.openAriaLabel")}
              onClick={() => setCreditsOpen(true)}
              className={ROW_BUTTON}
            >
              {t("settings.iconCredits.open")}
            </button>
          }
        />
      </SettingsGroup>
      <IconCreditsDrawer open={creditsOpen} onOpenChange={setCreditsOpen} />
    </div>
  );
}
