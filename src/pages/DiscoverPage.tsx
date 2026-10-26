import { useTranslation } from "react-i18next";
import { CheckIcon } from "../components/icons";
import { TextWithInfo } from "../components/InfoDetail";
import { SourceAvatar } from "../components/SourceAvatar";
import { ToolAvatar } from "../components/ToolAvatar";
import { FORM_COLUMN, GROUP, GROUP_FOOTNOTE, GROUP_ROW_TWO_LINES, GROUP_TITLE, SMALL_WRAPPING } from "../components/ui/group";
import { useCatalog, useSnapshot } from "../lib/queries";
import type { ArtifactKey, CatalogCategory, CatalogEntry, CatalogState, ManagerInstance } from "../lib/types";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/** The groups, in the order the page lists them, and each one's title. */
const CATEGORIES: [CatalogCategory, string][] = [
  ["Basics", "discover.categories.basics"],
  ["AiAssistants", "discover.categories.aiAssistants"],
  ["LocalModels", "discover.categories.localModels"],
  ["CommonTools", "discover.categories.commonTools"],
];

/**
 * Each entry's words, by its id (`CatalogItem::id` in
 * crates/banager-core/src/catalog.rs): its name, what it is -- the row's
 * line -- and, behind the line's ⓘ, who needs it and how it is used once
 * installed. Literal keys, so `completeness.test.ts` finds each one; a
 * catalog id with none here shows its id and no line.
 */
const ITEM_KEYS: Record<string, { name: string; what: string; more: string }> = {
  homebrew: {
    name: "discover.items.homebrew.name",
    what: "discover.items.homebrew.what",
    more: "discover.items.homebrew.more",
  },
  "claude-code": {
    name: "discover.items.claude-code.name",
    what: "discover.items.claude-code.what",
    more: "discover.items.claude-code.more",
  },
  codex: {
    name: "discover.items.codex.name",
    what: "discover.items.codex.what",
    more: "discover.items.codex.more",
  },
  "gemini-cli": {
    name: "discover.items.gemini-cli.name",
    what: "discover.items.gemini-cli.what",
    more: "discover.items.gemini-cli.more",
  },
  ollama: {
    name: "discover.items.ollama.name",
    what: "discover.items.ollama.what",
    more: "discover.items.ollama.more",
  },
  node: {
    name: "discover.items.node.name",
    what: "discover.items.node.what",
    more: "discover.items.node.more",
  },
  uv: {
    name: "discover.items.uv.name",
    what: "discover.items.uv.what",
    more: "discover.items.uv.more",
  },
  ffmpeg: {
    name: "discover.items.ffmpeg.name",
    what: "discover.items.ffmpeg.what",
    more: "discover.items.ffmpeg.more",
  },
};

/**
 * What the row says on its right of `state`. A `switch` with no default,
 * so a state added to `CatalogState` without words here fails `tsc`.
 */
function stateText(t: Translate, state: CatalogState): string {
  if (typeof state === "object") return t("discover.state.needsMacOS", { major: state.NeedsMacOS.major });
  switch (state) {
    case "Installed":
      return t("discover.state.installed");
    case "NotInstalled":
      return t("discover.state.notInstalled");
    case "NeedsHomebrew":
      return t("discover.state.needsHomebrew");
    case "HomebrewUnsupported":
      return t("discover.state.homebrewUnsupported");
  }
}

/**
 * What finds an entry's logo: a Homebrew formula or cask by its name, as
 * the Installed page finds that package's, under this Mac's first
 * Homebrew -- whose row, where the cask is installed there, also has its
 * app's icon. Homebrew itself has none: its row draws Homebrew's mark.
 */
function iconKeyOf(entry: CatalogEntry, brew: ManagerInstance | undefined): ArtifactKey | undefined {
  if (entry.install === "HomebrewItself") return undefined;
  const { kind, name } = entry.install.Homebrew;
  return { instance_id: brew?.id ?? "brew:/opt/homebrew", kind, name };
}

/**
 * The Discover page (phase 5 step A): the catalog Banager suggests, by
 * group, each entry with its logo, its name, what it is -- with who needs
 * it and how it is used behind the ⓘ -- and on the right whether this Mac
 * has it, by any way the catalog knows of (`get_catalog`). Until a check
 * has finished, nothing is said of any entry: an empty snapshot would say
 * everything needs Homebrew. Installing from here is the next step's; the
 * footnote says so.
 */
export function DiscoverPage() {
  const { t } = useTranslation();
  const { data: catalog } = useCatalog();
  const { data: snapshot } = useSnapshot();
  const checked = snapshot?.refreshed_at != null;
  const brew = snapshot?.instances.find((instance) => instance.adapter_id === "brew");
  const sourceLabel = t("adapters.brew");
  return (
    <div className={FORM_COLUMN}>
      <p className="px-2.5 text-muted">{t("discover.intro")}</p>
      {CATEGORIES.map(([category, titleKey]) => {
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
                const name = keys === undefined ? entry.id : t(keys.name);
                return (
                  <li key={entry.id} data-catalog-entry={entry.id} className={GROUP_ROW_TWO_LINES}>
                    <span className="flex min-w-0 items-center gap-2.5">
                      {entry.install === "HomebrewItself" ? (
                        // Homebrew is its own source: its mark alone, as the
                        // sidebar's row has it.
                        <SourceAvatar adapterId="brew" label={sourceLabel} size="sm" />
                      ) : (
                        <ToolAvatar size="sm" adapterId="brew" sourceLabel={sourceLabel} iconKey={iconKeyOf(entry, brew)} />
                      )}
                      <span className="min-w-0">
                        <span className="block truncate text-foreground">{name}</span>
                        {keys === undefined ? null : (
                          <span className={`block text-muted ${SMALL_WRAPPING}`}>
                            <TextWithInfo text={t(keys.what)} label={t("common.detailsLabel", { title: name })}>
                              {t(keys.more)}
                            </TextWithInfo>
                          </span>
                        )}
                      </span>
                    </span>
                    {checked ? (
                      <span data-catalog-state="" className="flex shrink-0 items-center gap-1 text-muted">
                        {entry.state === "Installed" ? <CheckIcon size={14} /> : null}
                        {stateText(t, entry.state)}
                      </span>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
      <p className={GROUP_FOOTNOTE}>{t("discover.footnote")}</p>
    </div>
  );
}
