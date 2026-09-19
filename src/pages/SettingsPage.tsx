import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings, useSaveSettings } from "../lib/queries";
import type { Settings, Language } from "../lib/types";
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

  const unignore = (key: Settings["ignored_updates"][number]) => {
    persist({
      ...current,
      ignored_updates: current.ignored_updates.filter(
        (k) => artifactKeyId(k) !== artifactKeyId(key),
      ),
    });
  };

  return (
    <div className="flex flex-col gap-6 p-6">
      <h1 className="text-lg font-semibold">{t("settings.title")}</h1>

      {saveMutation.isError && (
        <p role="alert">{t("settings.saveError", { message: saveMutation.error.message })}</p>
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

      <div>
        <p className="mb-2">{t("settings.language.label")}</p>
        <div role="radiogroup" aria-label={t("settings.language.label")} className="flex gap-2">
          {LANGUAGES.map((lang) => (
            <button
              key={lang}
              type="button"
              role="radio"
              aria-checked={current.language === lang}
              onClick={() => persist({ ...current, language: lang })}
            >
              {t(languageLabelKey(lang))}
            </button>
          ))}
        </div>
      </div>

      <div>
        <p className="mb-2 font-medium">{t("settings.ignoredUpdates.title")}</p>
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
                >
                  {t("settings.ignoredUpdates.unignore")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
