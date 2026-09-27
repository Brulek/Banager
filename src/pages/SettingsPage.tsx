import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { useSettings, useSaveSettings } from "../lib/queries";
import { settingsSaveErrorMessage } from "../lib/sources";
import { shownSkippedVersion, skippedVersionId } from "../lib/updateState";
import type { Settings, Language, SkippedVersion } from "../lib/types";
import { artifactKeyId } from "../store/ui";
import { Switch } from "../components/ui/Switch";

const LANGUAGES: Language[] = ["System", "En", "ZhCn"];

function languageLabelKey(lang: Language): string {
  if (lang === "System") return "settings.language.system";
  if (lang === "En") return "settings.language.english";
  return "settings.language.chinese";
}

export function SettingsPage() {
  const { t } = useTranslation();
  const settingsQuery = useSettings();
  const saveMutation = useSaveSettings();
  const [draft, setDraft] = useState<Settings | null>(null);
  const languageRefs = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    if (settingsQuery.data && draft === null) {
      setDraft(settingsQuery.data);
    }
  }, [settingsQuery.data, draft]);

  const current = draft ?? settingsQuery.data;

  if (settingsQuery.isLoading || !current) {
    return <p>{t("settings.loading")}</p>;
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
    <div className="flex flex-col gap-6 p-6">
      {saveMutation.isError && (
        <p role="alert">
          {t("settings.saveError", {
            message: settingsSaveErrorMessage(t, saveMutation.error.message),
          })}
        </p>
      )}

      <div className="flex items-center justify-between gap-4">
        <div className="flex flex-col">
          <label htmlFor="settings-show-technical">
            {t("settings.showTechnicalDetails.label")}
          </label>
          <p
            id="settings-show-technical-desc"
            className="text-sm text-[var(--color-muted-foreground)]"
          >
            {t("settings.showTechnicalDetails.description")}
          </p>
        </div>
        <Switch
          id="settings-show-technical"
          aria-describedby="settings-show-technical-desc"
          checked={current.show_technical_details}
          onCheckedChange={(checked) =>
            persist({ ...current, show_technical_details: checked })
          }
        />
      </div>

      <div className="flex items-center justify-between gap-4">
        <div className="flex flex-col">
          <label htmlFor="settings-include-self-updating">
            {t("settings.includeSelfUpdating.label")}
          </label>
          <p
            id="settings-include-self-updating-desc"
            className="text-sm text-[var(--color-muted-foreground)]"
          >
            {t("settings.includeSelfUpdating.description")}
          </p>
        </div>
        <Switch
          id="settings-include-self-updating"
          aria-describedby="settings-include-self-updating-desc"
          checked={current.include_self_updating}
          onCheckedChange={(checked) =>
            persist({ ...current, include_self_updating: checked })
          }
        />
      </div>

      <div>
        <p className="mb-2">{t("settings.language.label")}</p>
        <div role="radiogroup" aria-label={t("settings.language.label")} className="flex gap-2">
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
                className={
                  selected
                    ? "rounded-md border border-[var(--color-accent)] bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)]"
                    : "rounded-md border border-[var(--color-border)] px-3 py-1 text-sm hover:bg-[var(--color-hover)]"
                }
              >
                {t(languageLabelKey(lang))}
              </button>
            );
          })}
        </div>
      </div>

      {/* The two ways the Updates page hides an update, each in its own
          list, because they end differently: a skip stops hiding anything
          by itself once its source offers another version, while a package
          never to be reminded about stays hidden until it is removed here.

          Every stored skip is listed, including one whose version its
          source has since moved past: such an entry hides nothing any more
          (`hidingRule` matches only the version a row offers), and it is
          shown rather than dropped. This page does not read the snapshot,
          and every other change saved here writes the list back as it is,
          so an entry leaves it only when the user presses Stop skipping on
          it, or skips that package's next version on the Updates page,
          which replaces it (`withSkippedVersion`). */}
      <section aria-labelledby="settings-skipped-versions-title">
        <h2 id="settings-skipped-versions-title" className="mb-2 font-medium">
          {t("settings.skippedVersions.title")}
        </h2>
        {current.skipped_versions.length === 0 ? (
          <p>{t("settings.skippedVersions.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {current.skipped_versions.map((skipped) => {
              // An Ollama model's skipped version is a digest, never shown.
              const version = shownSkippedVersion(skipped);
              return (
                <li
                  key={skippedVersionId(skipped)}
                  className="flex items-center justify-between gap-4"
                >
                  <span className="flex items-baseline gap-2">
                    <span>{skipped.key.name}</span>
                    <span className="text-sm text-[var(--color-muted-foreground)]">
                      {version ?? t("settings.skippedVersions.newBuild")}
                    </span>
                  </span>
                  <button
                    type="button"
                    aria-label={
                      version === null
                        ? t("settings.skippedVersions.unskipNewBuildAriaLabel", {
                            name: skipped.key.name,
                          })
                        : t("settings.skippedVersions.unskipAriaLabel", {
                            name: skipped.key.name,
                            version,
                          })
                    }
                    onClick={() => unskip(skipped)}
                    className="shrink-0 rounded-md border border-[var(--color-border)] px-3 py-1 text-sm hover:bg-[var(--color-hover)]"
                  >
                    {t("settings.skippedVersions.unskip")}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section aria-labelledby="settings-ignored-updates-title">
        <h2 id="settings-ignored-updates-title" className="mb-2 font-medium">
          {t("settings.ignoredUpdates.title")}
        </h2>
        {current.ignored_updates.length === 0 ? (
          <p>{t("settings.ignoredUpdates.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {current.ignored_updates.map((key) => (
              <li key={artifactKeyId(key)} className="flex items-center justify-between gap-4">
                <span>{key.name}</span>
                <button
                  type="button"
                  aria-label={t("settings.ignoredUpdates.unignoreAriaLabel", {
                    name: key.name,
                  })}
                  onClick={() => unignore(key)}
                  className="shrink-0 rounded-md border border-[var(--color-border)] px-3 py-1 text-sm hover:bg-[var(--color-hover)]"
                >
                  {t("settings.ignoredUpdates.unignore")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
