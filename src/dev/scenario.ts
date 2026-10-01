/**
 * The browser preview's URL switches (docs/ui-preview.md): which machine
 * the mock backend pretends to be, read once from `location.search` when
 * the page loads. Dev-only, like everything in src/dev: nothing outside
 * this folder imports it, and only `vite --mode mock` puts it in a page.
 */
import type { Language } from "../lib/types";
import type { Page } from "../store/ui";

/**
 * `?state=`: the machine as a whole.
 *
 * - `full` (the default): every source, with every kind of update row.
 * - `loading`: the first refresh never answers, as at a real launch.
 * - `error`: `get_snapshot` and `refresh` reject. The page shows the
 *   failure once TanStack Query stops retrying (about 7 s, and only while
 *   the tab is visible).
 * - `refresh-error`: the same failure screen at once: the startup
 *   snapshot loads, then the first refresh rejects.
 * - `empty`: no source at all is set up on this Mac.
 * - `nothing`: Homebrew is set up, with nothing installed.
 * - `uptodate`: every source answered, nothing to update.
 * - `hidden`: the only updates are ones the user skipped or silenced.
 * - `stale`: the last refresh could not finish for two sources.
 * - `notices`: every source notice that has a look of its own.
 * - `offline`: no registry answered; every lookup is "could not check".
 * - `many`: about 800 things installed, about one in seven with an
 *   update, for how the long lists feel (`addMany` in ./mockData.ts).
 * - `preview`: the first refresh lists what is installed
 *   (`InventoryPreview`) and then never finishes checking for updates, as
 *   a real launch looks while `brew update` runs: the Installed page lists
 *   the Mac above, every Uninstall off.
 */
export const SCENARIO_STATES = [
  "full",
  "loading",
  "error",
  "refresh-error",
  "empty",
  "nothing",
  "uptodate",
  "hidden",
  "stale",
  "notices",
  "offline",
  "many",
  "preview",
] as const;
export type ScenarioState = (typeof SCENARIO_STATES)[number];

/**
 * `?outcome=`: how every operation submitted in the preview ends.
 * `password`: the command stops where `sudo` wanted the Mac's password
 * and had no terminal to ask in -- what a cask's installer or uninstaller
 * does under Banager (`needsPassword` in src/lib/failureCause.ts).
 */
export const SCENARIO_OUTCOMES = [
  "succeeded",
  "failed",
  "cancelled",
  "unconfirmed",
  "attention",
  "banager",
  "password",
] as const;
export type ScenarioOutcome = (typeof SCENARIO_OUTCOMES)[number];

/** `?scan=`: what the Unknown page's scan comes back with. */
export const SCENARIO_SCANS = ["found", "stopped", "empty", "error"] as const;
export type ScenarioScan = (typeof SCENARIO_SCANS)[number];

/**
 * `?sizes=`: how measuring disk use goes after each refresh. `measured`
 * (the default): 「正在计算…」 for a moment, then every size; `pending`:
 * it never finishes, for a look at the details while it runs.
 */
export const SCENARIO_SIZES = ["measured", "pending"] as const;
export type ScenarioSizes = (typeof SCENARIO_SIZES)[number];

const PAGES: readonly Page[] = ["overview", "updates", "installed", "unknown", "settings"];

/** `?lang=`: the Settings language the preview starts with. */
const LANGUAGES: Record<string, Language> = {
  system: "System",
  en: "En",
  "zh-cn": "ZhCn",
};

export interface Scenario {
  state: ScenarioState;
  /** Settings' language at startup; the app follows it (`useLanguageSync`). */
  language: Language;
  /** Settings' Show technical details at startup (`?tech=1`). */
  technicalDetails: boolean;
  /** The page the window opens on, or `null` for the app's own default. */
  page: Page | null;
  outcome: ScenarioOutcome;
  scan: ScenarioScan;
  sizes: ScenarioSizes;
}

export const DEFAULT_SCENARIO: Scenario = {
  state: "full",
  language: "System",
  technicalDetails: false,
  page: null,
  outcome: "succeeded",
  scan: "found",
  sizes: "measured",
};

function pick<T extends string>(
  params: URLSearchParams,
  name: string,
  allowed: readonly T[],
  fallback: T,
  problems: string[],
): T {
  const raw = params.get(name);
  if (raw === null || raw === "") return fallback;
  const value = raw.toLowerCase();
  const found = allowed.find((candidate) => candidate === value);
  if (found !== undefined) return found;
  problems.push(`?${name}=${raw} is not one of: ${allowed.join(", ")}`);
  return fallback;
}

/**
 * The scenario `search` asks for, and a line for every switch it could not
 * read -- an unknown value falls back to the default rather than failing
 * the page, and the preview logs the line so a typo is not silent.
 */
export function parseScenario(search: string): { scenario: Scenario; problems: string[] } {
  const params = new URLSearchParams(search);
  const problems: string[] = [];
  const langRaw = params.get("lang");
  let language = DEFAULT_SCENARIO.language;
  if (langRaw !== null && langRaw !== "") {
    const found = LANGUAGES[langRaw.toLowerCase()];
    if (found === undefined) {
      problems.push(`?lang=${langRaw} is not one of: ${Object.keys(LANGUAGES).join(", ")}`);
    } else {
      language = found;
    }
  }
  const tech = params.get("tech");
  const page = pick<Page | "">(params, "page", PAGES, "", problems);
  return {
    scenario: {
      state: pick(params, "state", SCENARIO_STATES, DEFAULT_SCENARIO.state, problems),
      language,
      technicalDetails: tech === "1" || tech === "true",
      page: page === "" ? null : page,
      outcome: pick(params, "outcome", SCENARIO_OUTCOMES, DEFAULT_SCENARIO.outcome, problems),
      scan: pick(params, "scan", SCENARIO_SCANS, DEFAULT_SCENARIO.scan, problems),
      sizes: pick(params, "sizes", SCENARIO_SIZES, DEFAULT_SCENARIO.sizes, problems),
    },
    problems,
  };
}
