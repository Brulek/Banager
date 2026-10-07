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
 * - `huge`: about 5,000 things installed -- `many`'s, and about 4,200
 *   more made from their names -- as on a Mac with several thousand
 *   formulae and casks (`addHuge` in ./mockData.ts).
 * - `preview`: the first refresh lists what is installed
 *   (`InventoryPreview`) and then never finishes checking for updates, as
 *   a real launch looks while `brew update` runs: the Installed page lists
 *   the Mac above, every Uninstall off.
 * - `refused`: two sources Banager did not ask, each saying why: an
 *   Ollama whose `OLLAMA_HOST` is an `https://` address, and a second
 *   Python, with no pip.
 * - `unchecked`: nothing to update, and uv not answering: the Overview
 *   names it, 「uv这次没检查，其余能在这里更新的都已是最新」 (decision I22;
 *   Codex's own install, which Banager never checks, is there too).
 * - `nonode`: the author's Mac on 2026-10-07: npm could not start, for want
 *   of the `node` that `brew upgrade node@22` could not link again (npm's
 *   own self-update had taken its place in `bin`), and Homebrew lists
 *   `node@22` and `node@20`, keg-only and not linked. npm's notice says why
 *   and offers Fix…, whose sheet previews `brew link --formula --force`: for
 *   `node@22`, with npm's own `npm` and `npx` in the way, it says so and
 *   gives the Terminal command that would link it anyway; for `node@20`, a
 *   Link that puts npm back, after which npm's own update is shown as
 *   updating with Node (`withNoNode` in ./mockData.ts).
 * - `nonode-intel`: `nonode`, on a Mac that also has an Intel Homebrew in
 *   /usr/local with the same `node@20` at the same version: Fix… offers
 *   three choices, each saying where its Homebrew is (「Apple芯片」,
 *   「Intel」), and the Intel one is linked by `/usr/local/bin/brew` (r7 F1).
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
  "huge",
  "preview",
  "refused",
  "unchecked",
  "nonode",
  "nonode-intel",
  "nonode-partial",
  "startup-error",
] as const;
export type ScenarioState = (typeof SCENARIO_STATES)[number];

/**
 * `?outcome=`: how every operation submitted in the preview ends.
 * `password`: the command stops where `sudo` wanted the Mac's password
 * and had no terminal to ask in -- what a cask's installer or uninstaller
 * does under Banager (`needsPassword` in src/lib/failureCause.ts).
 * `mixed`: the 2nd, 4th, … operation of the session fails, every other one
 * succeeds -- a batch with some of it left to look at.
 * `already`: every update finds its package already at its new version when
 * its turn comes, as an earlier update of an Update all that upgraded it as a
 * dependency leaves it (r6 y3-batch): done, the first of a source
 * 「轮到它时已是新版本」, the rest 「已由前面的更新一并完成」
 * (`OpSummary.already_updated`). Anything else ends as `succeeded`.
 */
export const SCENARIO_OUTCOMES = [
  "succeeded",
  "failed",
  "cancelled",
  "unconfirmed",
  "attention",
  "banager",
  "password",
  "mixed",
  "already",
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

/**
 * `?path=`: what the last refresh made of the login shell's `PATH`, which
 * Check Tool Setup says (`get_system_facts`'s `login_path` and
 * `path_folders`). `read` (the default): restored, every folder read;
 * `unread`: restored, with a folder in Documents that could not be read;
 * `default`: never restored -- an app opened from Finder with the system's
 * few folders -- so no command has a verdict.
 */
export const SCENARIO_PATHS = ["read", "unread", "default"] as const;
export type ScenarioPath = (typeof SCENARIO_PATHS)[number];

const PAGES: readonly Page[] = ["overview", "updates", "installed", "unknown", "settings"];

/** `?lang=`: the Settings language the preview starts with. */
const LANGUAGES: Record<string, Language> = {
  system: "System",
  en: "En",
  "zh-cn": "ZhCn",
  "zh-hant": "ZhHant",
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
  path: ScenarioPath;
  /**
   * `?welcome=1`: the settings say the welcome sheet has not been shown,
   * so it opens over the first page, as at a first launch. Off by default,
   * so that every other look at the preview is as it was.
   */
  welcome: boolean;
}

export const DEFAULT_SCENARIO: Scenario = {
  state: "full",
  language: "System",
  technicalDetails: false,
  page: null,
  outcome: "succeeded",
  scan: "found",
  sizes: "measured",
  path: "read",
  welcome: false,
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
  const welcome = params.get("welcome");
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
      path: pick(params, "path", SCENARIO_PATHS, DEFAULT_SCENARIO.path, problems),
      welcome: welcome === "1" || welcome === "true",
    },
    problems,
  };
}
