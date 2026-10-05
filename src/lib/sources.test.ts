import { describe, expect, it } from "vitest";
import {
  ADAPTER_LABEL_KEYS,
  adapterIdOf,
  adapterLabel,
  canWrite,
  failedSourceAdapters,
  failedSourceNames,
  hasSourceNotice,
  instanceLabels,
  instanceNames,
  isAvailable,
  namesInSentence,
  notActionableMessage,
  NOTHING_FOUND_KEYS,
  nothingFound,
  openOllamaErrorDetail,
  openOllamaErrorMessage,
  parseNotActionable,
  parseOpenOllamaFailure,
  parseUninstallBlocked,
  parseUninstallUnsafe,
  planErrorDetail,
  planErrorMessage,
  READ_ONLY_DETAIL_KEYS,
  refusalSentence,
  settingsSaveErrorMessage,
  settingsSaveSentence,
  sourceNoticesFor,
  sourceWarningOf,
  standaloneSummaryKey,
  toolDescription,
  UNAVAILABLE_DETAIL_KEYS,
  UNINSTALL_BLOCKED_KEYS,
  uninstallBlockedCopy,
  uninstallHoldKey,
  unfinishedChecksNotice,
  UPDATE_BLOCKED_KEYS,
  typedLauncherName,
  installedCountByInstance,
} from "./sources";
import type { DescribedTool } from "./sources";
import type { ArtifactKey, InstalledArtifact, InstanceNote, ManagerInstance, SourceError } from "./types";
import { NO_FACTS } from "./types";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import i18n from "../i18n";
import { noticeValues } from "./answeredWhen";

/** A stub `t`: returns the key with its interpolations inlined -- same
 *  convention as warnings.test.ts's `fakeT`, enough to prove the right key
 *  and values were looked up without coupling this test to English copy. */
function fakeT(key: string, options?: Record<string, unknown>): string {
  return options && Object.keys(options).length > 0 ? `${key}(${JSON.stringify(options)})` : key;
}

function instance(over: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id: "brew:/opt/homebrew",
    adapter_id: "brew",
    exe_path: "/opt/homebrew/bin/brew",
    prefix: "/opt/homebrew",
    scope: "User",
    version: "7.0.3",
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...over,
  };
}

describe("sourceNoticesFor", () => {
  it("says nothing about a source that answered and can be changed", () => {
    expect(sourceNoticesFor(instance(), "Homebrew")).toEqual([]);
    expect(hasSourceNotice(instance())).toBe(false);
    expect(canWrite(instance())).toBe(true);
    expect(isAvailable(instance())).toBe(true);
  });

  it("has no notice for what a source lets Banager do: a read-only source's rows say it themselves", () => {
    // pip, or an npm whose folder the account cannot write, being
    // read-only is what it always is, not something Banager found out this
    // time: each of its rows carries a "Read-only" chip, on both lists,
    // with its own way out (`READ_ONLY_DETAIL_KEYS`). A notice line for it
    // at the top of a list that mixes sources would not say which rows it
    // is about.
    for (const reason of ["ByDesign", "PrefixNotWritable"] as const) {
      const readOnly = instance({ adapter_id: reason === "ByDesign" ? "pip" : "npm", read_only_reason: reason });
      expect(sourceNoticesFor(readOnly, "pip")).toEqual([]);
      expect(hasSourceNotice(readOnly)).toBe(false);
      expect(canWrite(readOnly)).toBe(false);
    }
  });

  it("offers to start Ollama, and only Ollama, when a source is not running", () => {
    // One state, one sentence: every source that is not running gets the
    // same copy, named in the user's language. Ollama is the only one
    // Banager can start, so it is the only one whose notice also carries a
    // button -- a second wording for the same state is what let a stopped
    // Ollama read one way in its notice and another in its plan refusal.
    const [ollama] = sourceNoticesFor(
      instance({ adapter_id: "ollama", status: { unavailable: "NotRunning", notes: [] } }),
      "Ollama",
    );
    expect(ollama.variant).toBe("warning");
    expect(ollama.titleKey).toBe("sourceNotice.notRunning.title");
    expect(ollama.descriptionKey).toBe("sourceNotice.notRunning.description");
    expect(ollama.values).toEqual({ source: "Ollama" });
    expect(ollama.action).toEqual({ id: "openOllama", labelKey: "sourceNotice.openOllama" });

    // Any other source that reports NotRunning gets the same copy and no
    // button: Banager has no way to start it.
    const [other] = sourceNoticesFor(
      instance({ adapter_id: "brew", status: { unavailable: "NotRunning", notes: [] } }),
      "Homebrew",
    );
    expect(other.titleKey).toBe("sourceNotice.notRunning.title");
    expect(other.values).toEqual({ source: "Homebrew" });
    expect(other.action).toBeUndefined();
  });

  it("gives a stopped source the same sentence in its notice and in a refusal", () => {
    // A stopped Ollama used to be described two ways: `ollamaNotRunning`
    // in its notice, `notRunning` in `issue_plan`'s refusal. One state,
    // one sentence.
    const [notice] = sourceNoticesFor(
      instance({ adapter_id: "ollama", status: { unavailable: "NotRunning", notes: [] } }),
      "Ollama",
    );
    expect(
      notActionableMessage(fakeT, { read_only: null, unavailable: "NotRunning" }, "Ollama"),
    ).toBe(fakeT(notice.descriptionKey, notice.values));
  });

  it("tells a root user to reopen Banager rather than that Homebrew is missing", () => {
    // Launched with `sudo`, Homebrew refuses to run, so `BrewAdapter::detect`
    // reports the install it found as `RefusesAsRoot` instead of reporting
    // nothing. Its own copy, because the action is its own: not "start it"
    // and not "reinstall it", but quit and open Banager again normally.
    const [notice] = sourceNoticesFor(
      instance({ status: { unavailable: "RefusesAsRoot", notes: [] } }),
      "Homebrew",
    );
    expect(notice.variant).toBe("warning");
    expect(notice.titleKey).toBe("sourceNotice.refusesAsRoot.title");
    expect(notice.descriptionKey).toBe("sourceNotice.refusesAsRoot.description");
    expect(notice.values).toEqual({ source: "Homebrew" });
    // Banager cannot relaunch itself out from under sudo, so no button
    // pretends it can.
    expect(notice.action).toBeUndefined();
  });

  it("says an https OLLAMA_HOST is Banager's own refusal, with no button that would meet it again", () => {
    // `OllamaAdapter::detect` never sends a request the http client would
    // refuse; it used to pass that refusal off as a daemon that did not
    // answer -- or, here with Ollama.app installed, as one not running,
    // over an Open Ollama button that could not help.
    const [notice, ...rest] = sourceNoticesFor(
      instance({
        id: "ollama:https://ollama.home.lan",
        adapter_id: "ollama",
        exe_path: "/opt/homebrew/bin/ollama",
        status: { unavailable: "HttpsHostRefused", notes: [] },
      }),
      "Ollama",
      3,
    );
    expect(rest).toEqual([]);
    expect(notice).toEqual({
      id: "ollama:https://ollama.home.lan:https-host-refused",
      variant: "warning",
      titleKey: "sourceNotice.httpsHostRefused.title",
      descriptionKey: "sourceNotice.httpsHostRefused.description",
      values: { source: "Ollama" },
    });
    // Says it is what this app supports, names the variable, and is not
    // "not responding".
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.httpsHostRefused.title).toMatch(/isn't supported|不支持/);
      expect(locale.sourceNotice.httpsHostRefused.description).toMatch(/OLLAMA_HOST/);
      expect(locale.sourceNotice.httpsHostRefused.description).toContain("{{source}}");
      expect(locale.sourceNotice.httpsHostRefused.description).not.toMatch(/respond|响应|later|稍后/);
    }
    expect(
      notActionableMessage(fakeT, { read_only: null, unavailable: "HttpsHostRefused" }, "Ollama"),
    ).toBe(fakeT(notice.descriptionKey, notice.values));
    expect(UNAVAILABLE_DETAIL_KEYS.HttpsHostRefused).toBe(notice.descriptionKey);
  });

  it("says a Python with no pip has none, by the name it is typed by, as news and not a warning", () => {
    // Python's own "No module named pip" (`PipAdapter::detect`): nothing
    // is broken and checking again changes nothing, so no button and no
    // ⚠︎ in the sidebar.
    const noPip = instance({
      id: "pip:/opt/local/bin/python3.13",
      adapter_id: "pip",
      exe_path: "/opt/local/bin/python3.13",
      prefix: "/opt/local/bin",
      read_only_reason: "ByDesign",
      status: { unavailable: "NoPip", notes: [] },
    });
    const [notice, ...rest] = sourceNoticesFor(noPip, "pip（local）");
    expect(rest).toEqual([]);
    expect(notice).toEqual({
      id: "pip:/opt/local/bin/python3.13:no-pip",
      variant: "info",
      titleKey: "sourceNotice.noPip.title",
      descriptionKey: "sourceNotice.noPip.description",
      values: { command: "python3.13" },
    });
    expect(sourceWarningOf(noPip, "pip（local）", 0)).toBeNull();
    expect(i18n.getFixedT("en")(notice.titleKey, notice.values)).toBe("python3.13 doesn't include pip");
    expect(i18n.getFixedT("zh-CN")(notice.titleKey, notice.values)).toBe("“python3.13”没有附带pip");
    // Read-only comes first, as for any pip; the no-pip half names no source.
    expect(notActionableMessage(fakeT, { read_only: "ByDesign", unavailable: "NoPip" }, "pip")).toBe(
      'sourceNotice.pipReadOnly.description sourceNotice.noPip.description',
    );
    expect(UNAVAILABLE_DETAIL_KEYS.NoPip).toBe(notice.descriptionKey);
  });

  it("says a source that would not answer is showing last time's data", () => {
    const [notice] = sourceNoticesFor(
      instance({ status: { unavailable: "NotResponding", notes: [] } }),
      "Homebrew",
      4,
    );
    expect(notice.titleKey).toBe("sourceNotice.unreachable.title");
    expect(notice.descriptionKey).toBe("sourceNotice.unreachable.descriptionWithRows");
    expect(notice.values).toEqual({ source: "Homebrew", count: 4 });
    // Its next step is to check again: the button that does it, beside it,
    // in the header's words -- not a sentence sending the user to find it.
    expect(notice.action).toEqual({ id: "checkAgain", labelKey: "header.checkAgain" });
  });

  it("says what a silent source is and how many of its tools are its last answer, by what it lists (W2-10)", () => {
    // 「uv没有响应」 on every page left the person asking what uv is and
    // which of their tools it touches. The sentence says: the program that
    // installed this many tools; Ollama, whose rows are models; a tool with
    // its own installer, whose one row is itself. A count of what it
    // installed, never "of those listed": a search, the 「显示」 popup or
    // the Updates page's list shows fewer, and the Overview none.
    const silent = (adapter_id: string) =>
      instance({ id: adapter_id, adapter_id, status: { unavailable: "NotResponding", notes: [] } });
    const say = (adapterId: string, label: string, rows: number, language: "en" | "zh-CN") => {
      const [notice] = sourceNoticesFor(silent(adapterId), label, rows);
      return i18n.getFixedT(language)(notice.descriptionKey, notice.values);
    };
    expect(say("uv", "uv", 1, "zh-CN")).toBe(
      "有1个工具是用uv安装的。uv这次没有响应，显示的是它上次响应时的结果，请稍后重新检查。",
    );
    expect(say("uv", "uv", 1, "en")).toBe(
      "One tool was installed with uv. It didn't respond this time, so that tool is shown as it was when uv last responded. Check again later.",
    );
    expect(say("brew", "Homebrew", 12, "en")).toBe(
      "12 tools were installed with Homebrew. It didn't respond this time, so they're shown as they were when it last responded. Check again later.",
    );
    expect(say("ollama", "Ollama", 3, "zh-CN")).toBe(
      "Ollama中有3个模型。Ollama这次没有响应，显示的是它上次响应时的结果，请稍后重新检查。",
    );
    expect(say("ollama", "Ollama", 3, "en")).toBe(
      "Ollama has 3 models. It didn't respond this time, so they're shown as they were when it last responded. Check again later.",
    );
    expect(say("ollama", "Ollama", 1, "en")).toBe(
      "Ollama has one model. It didn't respond this time, so that model is shown as it was when Ollama last responded. Check again later.",
    );
    expect(say("standalone-claude", "Claude Code", 1, "zh-CN")).toBe(
      "Claude Code这次没有响应，显示的是它上次响应时的结果，请稍后重新检查。",
    );
    expect(say("standalone-claude", "Claude Code", 1, "en")).toBe(
      "Claude Code didn't respond this time, so what's shown is from when it last responded. Check again later.",
    );
    // With nothing of theirs on screen: that it cannot be listed this
    // time, in the same three ways.
    expect(say("uv", "uv", 0, "zh-CN")).toBe("这次无法列出用它安装的工具。请稍后重新检查。");
    expect(say("ollama", "Ollama", 0, "zh-CN")).toBe("这次无法列出它的模型。请稍后重新检查。");
    expect(say("standalone-claude", "Claude Code", 0, "zh-CN")).toBe("这次无法列出它。请稍后重新检查。");
    // Nothing that soothes: whether it needs anything is not known. And
    // nothing about what a page lists, which the count is not.
    for (const language of ["en", "zh-CN"] as const) {
      for (const [id, rows] of [["uv", 2], ["ollama", 2], ["standalone-claude", 1]] as const) {
        expect(say(id, id, rows, language)).not.toMatch(/通常|不用管|usually|nothing to worry|列出的|listed/i);
      }
      expect(say("uv", "uv", 0, language)).not.toMatch(/通常|不用管|usually|nothing to worry/i);
    }
  });

  it("counts what each source has installed, every row of the snapshot, for every page alike", () => {
    const row = (instance_id: string, name: string, reason = "Requested") => ({
      key: { instance_id, kind: "Formula" as const, name },
      reason,
    });
    const counts = installedCountByInstance(
      [
        row("uv", "ruff"),
        row("brew", "jq"),
        row("uv", "llm"),
        row("brew", "openssl@3", "Dependency"),
        row("brew", "oniguruma", "Unknown"),
      ] as InstalledArtifact[],
    );
    // Components other software brought in count too: the Sidebar,
    // Overview, Installed and Updates pages all read this one number.
    expect(counts.get("uv")).toBe(2);
    expect(counts.get("brew")).toBe(3);
    expect(counts.get("npm")).toBeUndefined();
    expect(installedCountByInstance([]).size).toBe(0);
  });

  it("does not promise carried-forward rows when the page has none to show", () => {
    // The snapshot is in memory only -- `Session::new` starts from
    // `Snapshot::empty()` and nothing is persisted -- so on the first
    // refresh after every launch there is nothing to carry forward. A
    // source whose CLI simply fails (cargo, when `cargo --version` does)
    // hits this on every single launch, and the notice used to say
    // "Below is what Banager saw last time" over an empty group.
    const [notice] = sourceNoticesFor(
      instance({ status: { unavailable: "NotResponding", notes: [] } }),
      "Homebrew",
      0,
    );
    expect(notice.descriptionKey).toBe("sourceNotice.unreachable.detail");
    // And the default is the copy that claims nothing: a caller that does
    // not know how many rows it is about to draw must not promise any.
    expect(
      sourceNoticesFor(instance({ status: { unavailable: "NotResponding", notes: [] } }), "x")[0]
        .descriptionKey,
    ).toBe("sourceNotice.unreachable.detail");
  });

  it("says when a silent source last answered, in each of its sentences, once this session has heard it (R12)", () => {
    // How old the rows of a source that did not answer are: the one thing
    // 「显示的是它上次响应时的结果」 left out. Said where Banager has the
    // time (`answered_at`), in the sentence the notice already says.
    const at = new Date(2026, 9, 2, 9, 12).getTime() / 1000;
    const now = new Date(2026, 9, 2, 10, 0).getTime();
    const nine = new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(new Date(at * 1000));
    const silent = (adapter_id: string, answered_at: number | null) =>
      instance({ id: adapter_id, adapter_id, answered_at, status: { unavailable: "NotResponding", notes: [] } });
    const say = (adapterId: string, label: string, rows: number, language: "en" | "zh-CN", answeredAt: number | null = at) => {
      const [notice] = sourceNoticesFor(silent(adapterId, answeredAt), label, rows);
      const t = i18n.getFixedT(language);
      return t(notice.descriptionKey, noticeValues(t, notice, now, language));
    };
    expect(say("uv", "uv", 2, "zh-CN")).toBe(
      "有2个工具是用uv安装的。uv这次没有响应，显示的是它今天09:12响应时的结果，请稍后重新检查。",
    );
    expect(say("uv", "uv", 1, "en")).toBe(
      `One tool was installed with uv. It didn't respond this time, so that tool is shown as it was when uv last responded at ${nine} today. Check again later.`,
    );
    expect(say("brew", "Homebrew", 12, "en")).toBe(
      `12 tools were installed with Homebrew. It didn't respond this time, so they're shown as they were when it last responded at ${nine} today. Check again later.`,
    );
    expect(say("ollama", "Ollama", 3, "zh-CN")).toBe(
      "Ollama中有3个模型。Ollama这次没有响应，显示的是它今天09:12响应时的结果，请稍后重新检查。",
    );
    expect(say("ollama", "Ollama", 1, "en")).toBe(
      `Ollama has one model. It didn't respond this time, so that model is shown as it was when Ollama last responded at ${nine} today. Check again later.`,
    );
    expect(say("ollama", "Ollama", 3, "en")).toBe(
      `Ollama has 3 models. It didn't respond this time, so they're shown as they were when it last responded at ${nine} today. Check again later.`,
    );
    expect(say("standalone-claude", "Claude Code", 1, "zh-CN")).toBe(
      "Claude Code这次没有响应，显示的是它今天09:12响应时的结果，请稍后重新检查。",
    );
    expect(say("standalone-claude", "Claude Code", 1, "en")).toBe(
      `Claude Code didn't respond this time, so what's shown is from when it last responded at ${nine} today. Check again later.`,
    );
    // With none of its rows to show: what that answer had.
    expect(say("uv", "uv", 0, "zh-CN")).toBe("这次无法列出用它安装的工具。它今天09:12响应时，没有任何工具。请稍后重新检查。");
    expect(say("uv", "uv", 0, "en")).toBe(
      `The tools installed with it can't be listed this time. When it last responded at ${nine} today, it had no tools. Check again later.`,
    );
    expect(say("ollama", "Ollama", 0, "zh-CN")).toBe("这次无法列出它的模型。它今天09:12响应时，没有任何模型。请稍后重新检查。");
    expect(say("ollama", "Ollama", 0, "en")).toBe(
      `Its models can't be listed this time. When it last responded at ${nine} today, it had no models. Check again later.`,
    );
    // A tool with its own installer lists itself whenever it answers: with
    // no row of it, there is no answer of its to date.
    expect(say("standalone-claude", "Claude Code", 0, "zh-CN")).toBe("这次无法列出它。请稍后重新检查。");
    expect(sourceNoticesFor(silent("standalone-claude", at), "Claude Code", 0)[0].answeredAt).toBeUndefined();
    // Not heard this session -- the first check after launch, or a source
    // that has not answered since -- and a payload from before the field:
    // the same sentences as ever, and no time.
    for (const answeredAt of [null, undefined]) {
      for (const [id, rows] of [["uv", 0], ["uv", 2], ["ollama", 0], ["ollama", 2], ["standalone-claude", 1]] as const) {
        const bare = instance({ id, adapter_id: id, status: { unavailable: "NotResponding", notes: [] } });
        const [notice] = sourceNoticesFor({ ...bare, answered_at: answeredAt as number | null }, id, rows);
        expect(notice.answeredAt).toBeUndefined();
        expect(notice.descriptionKey).toBe(sourceNoticesFor(bare, id, rows)[0].descriptionKey);
        expect(notice.descriptionKey).not.toMatch(/At$/);
      }
    }
    expect(say("uv", "uv", 2, "zh-CN", null)).toBe(
      "有2个工具是用uv安装的。uv这次没有响应，显示的是它上次响应时的结果，请稍后重新检查。",
    );
  });

  it("selects timed notices for a Unix-zero response, with and without carried-forward rows", () => {
    const silent = instance({ answered_at: 0, status: { unavailable: "NotResponding", notes: [] } });
    expect(sourceNoticesFor(silent, "Homebrew", 2)[0]).toMatchObject({
      descriptionKey: "sourceNotice.unreachable.descriptionWithRowsAt",
      answeredAt: 0,
      values: { source: "Homebrew", count: 2 },
    });
    expect(sourceNoticesFor(silent, "Homebrew", 0)[0]).toMatchObject({
      descriptionKey: "sourceNotice.unreachable.detailAt",
      answeredAt: 0,
    });
  });

  it("says a time only for a source that did not answer: what it said, and how it is, are this round's", () => {
    const at = new Date(2026, 9, 2, 9, 12).getTime() / 1000;
    const states: ManagerInstance["status"][] = [
      { unavailable: "NotRunning", notes: [] },
      { unavailable: "RefusesAsRoot", notes: [] },
      { unavailable: "HttpsHostRefused", notes: [] },
      { unavailable: "NoPip", notes: [] },
      {
        unavailable: null,
        notes: ["IndexUpdating", "IndexMayBeStale", "NotOnPath", "ShadowedByHomebrew", "ShadowedByNpm", "ShadowedByOther", "LauncherOnly"],
      },
    ];
    for (const status of states) {
      for (const notice of sourceNoticesFor(instance({ answered_at: at, status }), "Homebrew", 3)) {
        expect(notice.answeredAt, notice.id).toBeUndefined();
        for (const language of ["en", "zh-CN"]) {
          expect(i18n.getFixedT(language)(notice.descriptionKey, notice.values), notice.id).not.toContain("{{when}}");
        }
      }
    }
  });

  it("writes each timed sentence as its timeless twin, the time in it and the same next step at its end", () => {
    const timed = (locale: typeof en | typeof zhCN) =>
      Object.entries(locale.sourceNotice.unreachable).filter(([key]) => /At(_one|_other)?$/.test(key));
    expect(timed(en).map(([key]) => key)).toEqual([
      "detailAt",
      "descriptionWithRowsAt_one",
      "descriptionWithRowsAt_other",
      "descriptionWithModelsAt_one",
      "descriptionWithModelsAt_other",
      "descriptionWithOwnRowAt",
      "detailModelsAt",
    ]);
    for (const [key, copy] of [...timed(en), ...timed(zhCN)]) {
      expect(copy, key).toContain("{{when}}");
      expect(copy.endsWith("Check again later.") || copy.endsWith("请稍后重新检查。"), key).toBe(true);
      // Never what a page lists (W2-10): the count is what it installed.
      if (key.startsWith("description")) expect(copy, key).not.toMatch(/列出的|listed/);
    }
    // The rows' sentences keep their words; only 「上次」 gives way to the time.
    expect(zhCN.sourceNotice.unreachable.descriptionWithRowsAt_other).toBe(
      zhCN.sourceNotice.unreachable.descriptionWithRows_other.replace("它上次响应时", "它{{when}}响应时"),
    );
    expect(en.sourceNotice.unreachable.descriptionWithRowsAt_other).toBe(
      en.sourceNotice.unreachable.descriptionWithRows_other.replace("last responded.", "last responded {{when}}."),
    );
  });

  it("warns that a stale index makes up-to-date unreliable, with the header's Check again", () => {
    const [note] = sourceNoticesFor(
      instance({ status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
      "Homebrew",
    );
    expect(note.titleKey).toBe("sourceNotice.indexMayBeStale.title");
    // One name for every button that runs the check: the header's.
    expect(note.action).toEqual({ id: "checkAgain", labelKey: "header.checkAgain" });
    // Why, and the next step; the button beside it says which.
    expect(zhCN.sourceNotice.indexMayBeStale.description).toBe("此次检查使用了旧清单。请检查网络连接后重试。");
    expect(en.sourceNotice.indexMayBeStale.description).toBe(
      "This check used the old list. Check your internet connection, then try again.",
    );
  });

  it("says a still-running download is still running: no failure, no button", () => {
    // The download has not failed, so the copy must not say it did or send
    // the user off to check their connection, and there is nothing to
    // retry: the core refreshes by itself when the download ends.
    const [note] = sourceNoticesFor(
      instance({ status: { unavailable: null, notes: ["IndexUpdating"] } }),
      "Homebrew",
    );
    expect(note.variant).toBe("info");
    expect(note.titleKey).toBe("sourceNotice.indexUpdating.title");
    expect(note.descriptionKey).toBe("sourceNotice.indexUpdating.description");
    expect(note.action).toBeUndefined();
  });

  it("says what a read-only source found out this time, as for any source", () => {
    // Read-only and silent at once: the silence is news, and a note about
    // its list is too; being read-only is its rows' chip.
    const notices = sourceNoticesFor(
      instance({
        adapter_id: "pip",
        read_only_reason: "ByDesign",
        status: { unavailable: "NotResponding", notes: ["IndexMayBeStale"] },
      }),
      "pip",
    );
    expect(notices.map((n) => n.titleKey)).toEqual([
      "sourceNotice.unreachable.title",
      "sourceNotice.indexMayBeStale.title",
    ]);
    expect(new Set(notices.map((n) => n.id)).size).toBe(2);
  });

  it("is the one rule hasSourceNotice answers from", () => {
    // The "No installed tools found" gate (`nothingFound`) and the
    // pages' notice lines must never disagree about which sources have something to
    // say: a Mac whose only source is a stopped Ollama would otherwise see
    // the empty state and no way to start it.
    for (const inst of [
      instance(),
      instance({ read_only_reason: "ByDesign" }),
      instance({ status: { unavailable: "NotRunning", notes: [] } }),
      instance({ status: { unavailable: "RefusesAsRoot", notes: [] } }),
      instance({ status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
      instance({ status: { unavailable: null, notes: ["IndexUpdating"] } }),
      instance({ status: { unavailable: null, notes: ["NotOnPath"] } }),
      instance({ status: { unavailable: null, notes: ["ShadowedByHomebrew"] } }),
      instance({ status: { unavailable: null, notes: ["ShadowedByNpm"] } }),
      instance({ status: { unavailable: null, notes: ["ShadowedByOther"] } }),
      instance({ status: { unavailable: null, notes: ["LauncherOnly"] } }),
    ]) {
      expect(hasSourceNotice(inst)).toBe(sourceNoticesFor(inst, "Homebrew").length > 0);
    }
  });

  // A standalone tool's instance: the launcher is its `exe_path`, the tool
  // root its `prefix`, and the command the user types is the launcher's
  // file name.
  const claude = instance({
    id: "standalone-claude",
    adapter_id: "standalone-claude",
    exe_path: "/Users/someone/.local/bin/claude",
    prefix: "/Users/someone/.local/share/claude",
    version: "2.1.281",
  });

  it("tells a standalone tool's user when typing its name may not run this copy", () => {
    // Four payload-free notes, four actionable sentences (spec §七): the
    // path of what PATH finds first is not in the notice -- the user this
    // app is for would not recognise it -- but the command name is, so the
    // sentence can say "when you type claude".
    for (const [note, id, key] of [
      ["NotOnPath", "not-on-path", "sourceNotice.notOnPath"],
      ["ShadowedByHomebrew", "shadowed-by-homebrew", "sourceNotice.shadowedByHomebrew"],
      ["ShadowedByNpm", "shadowed-by-npm", "sourceNotice.shadowedByNpm"],
      ["ShadowedByOther", "shadowed-by-other", "sourceNotice.shadowedByOther"],
    ] as const) {
      const notices = sourceNoticesFor(
        { ...claude, status: { unavailable: null, notes: [note] } },
        "Claude Code",
      );
      expect(notices).toEqual([
        {
          id: `standalone-claude:${id}`,
          variant: "info",
          titleKey: `${key}.title`,
          descriptionKey: `${key}.description`,
          values: { source: "Claude Code", command: "claude" },
          // Another program answers to the name: Show searches the
          // Installed page for it, which lists both (W2-9). Not found at
          // all, there is nothing to show beside the tool.
          ...(note === "NotOnPath"
            ? {}
            : { action: { id: "searchCommand", labelKey: "sourceNotice.showCommand", command: "claude" } }),
        },
      ]);
    }
  });

  it("warns, and names the link, when a standalone tool's launcher is left without its program", () => {
    // The half-uninstalled state (program files gone, launcher dangling):
    // a warning because this launcher is broken; another PATH copy may
    // work. The row's own Uninstall moves the link, which the sentence
    // says, and is the one way out done in the app: the notice's button
    // shows the tool on the Installed page, where its Uninstall… is. Check
    // again, after a reinstall or the files back from the Trash, is the
    // toolbar's.
    const notices = sourceNoticesFor(
      { ...claude, status: { unavailable: null, notes: ["LauncherOnly"] } },
      "Claude Code",
    );
    expect(notices).toEqual([
      {
        id: "standalone-claude:launcher-only",
        variant: "warning",
        titleKey: "sourceNotice.launcherOnly.title",
        descriptionKey: "sourceNotice.launcherOnly.description",
        values: { source: "Claude Code", command: "claude" },
        action: { id: "showTool", labelKey: "sourceNotice.showTool", instanceId: "standalone-claude" },
      },
    ]);
    // Named for what it shows, never a bare "Show" (walk-3 W3-5).
    expect(en.sourceNotice.showTool).toBe("Show Tool");
    expect(zhCN.sourceNotice.showTool).toBe("查看");
  });

  it("falls back to the whole exe_path as the command when it has no file name", () => {
    const notices = sourceNoticesFor(
      { ...claude, exe_path: "/", status: { unavailable: null, notes: ["NotOnPath"] } },
      "Claude Code",
    );
    expect(notices[0].values).toEqual({ source: "Claude Code", command: "/" });
  });

  it("names the command in every standalone notice about typing it, and the source where the title stands alone, in both locales", () => {
    // The titles are what the Overview's "Needs attention" shows, with no
    // group or row around them, so each one says which tool it is about.
    for (const locale of [en, zhCN]) {
      for (const key of ["notOnPath", "shadowedByHomebrew", "shadowedByNpm", "shadowedByOther"] as const) {
        expect(locale.sourceNotice[key].title).toContain("{{command}}");
      }
      expect(locale.sourceNotice.notOnPath.title).toContain("{{source}}");
      expect(locale.sourceNotice.launcherOnly.title).toContain("{{source}}");
      expect(locale.sourceNotice.launcherOnly.description).toContain("{{command}}");
      expect(locale.sourceNotice.launcherOnly.description).toContain("{{source}}");
      // Each "another program runs first" title says whose it is, where
      // Banager can tell: the three used to share one title.
      expect(locale.sourceNotice.shadowedByHomebrew.title).toContain("Homebrew");
      expect(locale.sourceNotice.shadowedByNpm.title).toContain("npm");
      expect(locale.sourceNotice.shadowedByOther.title).not.toMatch(/Homebrew|npm/);
      expect(locale.sourceNotice.shadowedByHomebrew.description).toContain("Homebrew");
      expect(locale.sourceNotice.shadowedByNpm.description).toContain("npm");
      for (const key of ["shadowedByHomebrew", "shadowedByNpm"] as const) {
        expect(locale.sourceNotice[key].description).not.toMatch(/both are listed on this page|两份在这一页上都能找到/);
      }
    }
  });

  it("says typing its name doesn't run the one installed, never that nothing runs, in both locales (T6)", () => {
    // NotOnPath is also the note when another executable with the tool's
    // name is on PATH and this copy is not (route::shadow_note): typing the
    // name then runs that other program, which may even be another copy
    // of the tool -- so "Terminal won't find Claude Code" would be false
    // there. The title says it is the one installed -- "it" -- that typing
    // the name does not run, in plain words: 「找不到这一份」 meant nothing
    // to someone who does not know there can be several copies of a tool.
    // What happens in Terminal is judged by the PATH Banager sees, which it
    // takes from a login shell when opened from Finder
    // (src-tauri/src/lib.rs); the detail's first step, a new Terminal
    // window, covers a shell whose PATH has not caught up.
    expect(en.sourceNotice.notOnPath.title).toBe(
      "{{source}} is installed, but typing {{command}} in Terminal doesn't run it",
    );
    expect(zhCN.sourceNotice.notOnPath.title).toBe("{{source}}已安装，但在终端输入“{{command}}”打不开它");
    expect(en.sourceNotice.notOnPath.description).toContain("Open a new Terminal window and try again;");
    expect(zhCN.sourceNotice.notOnPath.description).toContain("请新开一个终端窗口再试；");
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.notOnPath.title).not.toMatch(/nothing|什么也/);
    }
  });

  it("says what was checked for not-on-PATH, and no cause it did not check, in both locales", () => {
    // route::shadow_note answers NotOnPath whenever no executable
    // `command` on PATH resolves to this copy -- also when the launcher's
    // folder is on PATH but the file it links to has no executable bit
    // (route.rs, test_shadow_note_says_not_on_path_when_the_launcher_is_on_path_but_its_target_is_not_executable).
    // So the detail says that this copy -- "it", the one the title says
    // is installed -- is not in the places Terminal looks for commands,
    // which a launcher there that cannot run does not change (the
    // program it links to is elsewhere, and Terminal runs nothing by it):
    // not that its folder is missing from them (step-B review finding
    // B-5), not that the command can't be found (another program with
    // its name may well be found, and run, first), nor "probably", "most
    // likely" or PATH.
    expect(en.sourceNotice.notOnPath.description).toContain(
      "It isn't in any of the places Terminal looks for commands.",
    );
    expect(zhCN.sourceNotice.notOnPath.description).toContain("终端查找命令的位置里没有它。");
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.notOnPath.description).not.toMatch(
        /folder|PATH|probably|likely|文件夹|多半|可能/,
      );
      expect(locale.sourceNotice.notOnPath.description).not.toMatch(/找不到|not found|can't find|couldn't find/i);
    }
  });

  it("calls what PATH finds first a program with the tool's name, never another copy, and says Banager can't tell which it is, in both locales (T6)", () => {
    // Step D's review: route::shadow_note compares the name the user types
    // and where the first executable of that name resolves -- a Homebrew
    // directory, an npm one, or anywhere else -- never what program it is.
    // Homebrew's formula `grok`, a regular-expression tool, gets the very
    // note that the `grok` of Homebrew's cask `grok-build`, which is Grok
    // Build, gets; npm's package `grok-cli`, a third-party wrapper, puts a
    // `grok` on PATH that is not Grok Build either (route.rs,
    // test_shadow_note_classifies_by_where_the_first_one_resolves_not_by_what_program_it_is).
    // So each title names a program with the same name, its detail says
    // Banager can't tell whether it is the tool, and none says "probably".
    expect(en.sourceNotice.shadowedByHomebrew.title).toBe(
      "Typing {{command}} in Terminal runs a program with that name from Homebrew",
    );
    expect(zhCN.sourceNotice.shadowedByHomebrew.title).toBe("在终端输入“{{command}}”，运行的是Homebrew装的同名程序");
    expect(zhCN.sourceNotice.shadowedByNpm.title).toBe("在终端输入“{{command}}”，运行的是npm装的同名程序");
    expect(zhCN.sourceNotice.shadowedByOther.title).toBe("在终端输入“{{command}}”，运行的是另一个同名程序");
    for (const key of ["shadowedByHomebrew", "shadowedByNpm"] as const) {
      expect(en.sourceNotice[key].description).toContain("Couldn't confirm whether it's another copy of {{source}}.");
      expect(zhCN.sourceNotice[key].description).toContain("无法确认它是不是另一份{{source}}。");
    }
    expect(en.sourceNotice.shadowedByOther.description).toContain("a program that couldn't be identified");
    expect(zhCN.sourceNotice.shadowedByOther.description).toContain("另一个无法识别的程序");
    // What it means for the person (W2-9): the copy the tool's own
    // installer put there is not the one that runs -- said of that
    // installer, the words the details' 「装了两份」 use for it
    // (`commands.twinOtherStandalone`), never of 「这一份」.
    for (const key of ["shadowedByHomebrew", "shadowedByNpm", "shadowedByOther"] as const) {
      expect(en.sourceNotice[key].description).toMatch(/^Terminal (finds|first finds) .*, not the one \{\{source\}\}'s own installer installed\./);
      expect(zhCN.sourceNotice[key].description).toMatch(/^终端先找到的是.*，而不是用\{\{source\}\}自带的安装程序装的那个。/);
      expect(zhCN.sourceNotice[key].description).not.toContain("这一份");
    }
    // Where to look is the page by the name its row in the sidebar has.
    expect(en.sourceNotice.shadowedByOther.description).toContain(`If it's in ${en.nav.unknown},`);
    expect(zhCN.sourceNotice.shadowedByOther.description).toContain(`如果它在“${zhCN.nav.unknown}”中`);
    expect(en.nav.unknown).toBe("Other Programs");
    expect(zhCN.nav.unknown).toBe("其他程序");
    for (const key of ["notOnPath", "shadowedByHomebrew", "shadowedByNpm", "shadowedByOther"] as const) {
      for (const locale of [en, zhCN]) {
        expect(locale.sourceNotice[key].title).not.toMatch(/another copy|other copy|另一份|多半|probably|likely/i);
        expect(locale.sourceNotice[key].description).not.toMatch(/runs? (that other|another) copy|运行的是另一份|多半/);
      }
    }
  });

  it("says it is this copy that can no longer run when the launcher is left without its program, in both locales (T7)", () => {
    // Another installation -- a Homebrew or npm command of the same name --
    // may run in Terminal as before, so the detail speaks of this copy
    // only: the source's own command, never 「这一份」, which means nothing
    // to someone who does not know there can be several. To keep the
    // tool, reinstall it, or put its files back if an earlier uninstall
    // that stopped partway moved them to the Trash -- "if", as spec §9.2
    // says: the Trash can have been emptied since. Not needed, the row
    // on the Installed page offers Uninstall (its artifact carries no
    // `uninstall_blocked` since step C), which cleans it up -- named by
    // its page, not as a button to press: the notice also stands on the
    // Overview and the Updates page, where its one button is Show, which
    // opens that page on the tool.
    expect(en.sourceNotice.launcherOnly.description).toBe(
      "The {{command}} command can't run anymore, so reinstall {{source}} or put its files back from the Trash if they're there, then check again. If you no longer need {{source}}, uninstall it from the Installed page.",
    );
    expect(zhCN.sourceNotice.launcherOnly.description).toBe(
      "“{{command}}”已无法运行，请重新安装{{source}}；如果程序文件在废纸篓中，也可以拖回原处后重新检查。不再需要时，可在“已安装”中卸载{{source}}。",
    );
    expect(JSON.stringify(zhCN.sourceNotice)).not.toContain("这一份");
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(
        /typing .* in Terminal fails|输入 .* 会失败|website|same page|网站|同一页/,
      );
    }
  });

  it("does not call the launcher all that is left, in both locales: grok's agent link can dangle beside it", () => {
    // The whole-step review of step D: in Grok Build's launcher-only state
    // `~/.grok/bin/agent` dangles beside `~/.grok/bin/grok`, and a stopped
    // uninstall can leave other listed paths as well, so "Only the grok
    // link is left" was untrue. The title says what holds for every
    // launcher-only row: the program files are gone.
    expect(en.sourceNotice.launcherOnly.title).toBe("{{source}}'s program files are missing");
    expect(zhCN.sourceNotice.launcherOnly.title).toBe("找不到{{source}}的程序文件");
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.launcherOnly.title).not.toMatch(/only|只剩/i);
    }
  });

  it("promises a Node from Homebrew manages only the npm packages installed with it, in both locales (T5)", () => {
    // After Node is reinstalled with Homebrew, npm's global folder is a
    // different one: the packages in the old folder do not move over, and
    // the new npm does not list them. So the sentence promises to manage
    // what is installed with the new Node, never "them".
    expect(en.sourceNotice.prefixNotWritable.description).toContain(
      "After you install Node with Homebrew, you can manage the npm packages you install with it here.",
    );
    expect(zhCN.sourceNotice.prefixNotWritable.description).toContain(
      "用Homebrew安装Node后，再用它安装的npm包就能在这里管理。",
    );
    expect(en.sourceNotice.prefixNotWritable.description).not.toMatch(/manage them|usually/);
    expect(zhCN.sourceNotice.prefixNotWritable.description).not.toMatch(/就能管理它们了|通常/);
  });

  it("keeps every notice's title short enough for one line, and its detail to two sentences, in both locales", () => {
    const notices = (locale: typeof en) =>
      Object.values(locale.sourceNotice).filter(
        (entry): entry is { title: string; description: string } =>
          typeof entry === "object" && "title" in entry && "description" in entry,
      );
    for (const notice of notices(en)) {
      expect(notice.title.length, notice.title).toBeLessThanOrEqual(80);
      expect(notice.description.match(/[.!?](\s|$)/g)?.length ?? 0, notice.description).toBeLessThanOrEqual(2);
    }
    for (const notice of notices(zhCN as unknown as typeof en)) {
      expect(notice.title.length, notice.title).toBeLessThanOrEqual(40);
      expect(notice.title).not.toMatch(/[（(]|多半|可能/);
      expect(notice.description.match(/[。！？]/g)?.length ?? 0, notice.description).toBeLessThanOrEqual(2);
    }
  });
});

describe("uninstallHoldKey", () => {
  it("holds an uninstall while Homebrew updates its list, the one note Rust refuses an uninstall's preview for", () => {
    const key = uninstallHoldKey(instance({ status: { unavailable: null, notes: ["IndexUpdating"] } }));
    expect(key).toBe("installed.uninstallHold.IndexUpdating");
    expect(typeof en.installed.uninstallHold.IndexUpdating).toBe("string");
    expect(typeof zhCN.installed.uninstallHold.IndexUpdating).toBe("string");
  });

  it("holds nothing for any other note, none, or one this build does not know", () => {
    const others: InstanceNote[] = [
      "IndexMayBeStale",
      "NotOnPath",
      "ShadowedByHomebrew",
      "ShadowedByNpm",
      "ShadowedByOther",
      "LauncherOnly",
    ];
    for (const note of others) {
      expect(uninstallHoldKey(instance({ status: { unavailable: null, notes: [note] } })), note).toBeNull();
    }
    expect(uninstallHoldKey(instance())).toBeNull();
    const unknown = "SomeFutureNote" as unknown as InstanceNote;
    expect(uninstallHoldKey(instance({ status: { unavailable: null, notes: [unknown] } }))).toBeNull();
  });
});

describe("parseNotActionable", () => {
  it("reads the JSON plan_operation_error puts on the wire for the gate's refusal", () => {
    expect(
      parseNotActionable('{"kind":"not_actionable","read_only":"PrefixNotWritable","unavailable":null}'),
    ).toEqual({ read_only: "PrefixNotWritable", unavailable: null });
    expect(
      parseNotActionable('{"kind":"not_actionable","read_only":null,"unavailable":"NotRunning"}'),
    ).toEqual({ read_only: null, unavailable: "NotRunning" });
  });

  it("is null for every other backend error, which stays plain text", () => {
    expect(parseNotActionable("unknown instance fake:1")).toBeNull();
    // Valid JSON, and other structured kinds `submit_operation_error` sends
    // -- must not be mistaken for `not_actionable`'s shape.
    expect(parseNotActionable('{"kind":"source_gone"}')).toBeNull();
    expect(parseNotActionable('{"kind":"expired"}')).toBeNull();
    expect(parseNotActionable('{"kind":"unknown"}')).toBeNull();
    expect(parseNotActionable('{"kind":"something_else"}')).toBeNull();
    expect(parseNotActionable("")).toBeNull();
  });
});

describe("notActionableMessage", () => {
  it("uses the same copy as the read-only notice, needing no source name", () => {
    expect(
      notActionableMessage(fakeT, { read_only: "PrefixNotWritable", unavailable: null }, "npm"),
    ).toBe("sourceNotice.prefixNotWritable.description");
    expect(
      notActionableMessage(fakeT, { read_only: "ByDesign", unavailable: null }, "pip"),
    ).toBe("sourceNotice.pipReadOnly.description");
  });

  it("uses the same copy as the state notice, naming the source", () => {
    expect(
      notActionableMessage(fakeT, { read_only: null, unavailable: "NotRunning" }, "Ollama"),
    ).toBe('sourceNotice.notRunning.description({"source":"Ollama"})');
    expect(
      notActionableMessage(fakeT, { read_only: null, unavailable: "NotResponding" }, "Homebrew"),
    ).toBe('sourceNotice.unreachable.description({"source":"Homebrew"})');
    expect(
      notActionableMessage(fakeT, { read_only: null, unavailable: "RefusesAsRoot" }, "Homebrew"),
    ).toBe('sourceNotice.refusesAsRoot.description({"source":"Homebrew"})');
  });

  it("joins both when a source is read-only and silent at once", () => {
    expect(
      notActionableMessage(
        fakeT,
        { read_only: "PrefixNotWritable", unavailable: "NotResponding" },
        "npm",
      ),
    ).toBe(
      'sourceNotice.prefixNotWritable.description sourceNotice.unreachable.description({"source":"npm"})',
    );
  });
});

describe("planErrorMessage", () => {
  it("localises a NotActionable refusal instead of showing it verbatim", () => {
    expect(
      planErrorMessage(
        fakeT,
        '{"kind":"not_actionable","read_only":"PrefixNotWritable","unavailable":null}',
        "npm",
        false,
      ),
    ).toBe("sourceNotice.prefixNotWritable.description");
  });

  it("shows a string that is not a structured payload verbatim with technical details on, rather than swallowing it", () => {
    expect(planErrorMessage(fakeT, "unknown instance fake:1", "npm", true)).toBe(
      "unknown instance fake:1",
    );
    // A kind this build does not know is not guessed at either.
    expect(planErrorMessage(fakeT, '{"kind":"toString"}', "npm", true)).toBe('{"kind":"toString"}');
  });

  it("gives nothing of the backend's own words with technical details off, but a cause it can read", () => {
    // The caller's sentence says what happened without them
    // (`refusalSentence`).
    expect(planErrorMessage(fakeT, "unknown instance fake:1", "npm", false)).toBeNull();
    expect(planErrorMessage(fakeT, '{"kind":"toString"}', "npm", false)).toBeNull();
    expect(planErrorMessage(fakeT, '{"kind":"update_blocked","reason":"Held"}', "Homebrew", false)).toBeNull();
    // What `failureCause` reads is said in a person's words.
    expect(planErrorMessage(fakeT, "curl: (6) Could not resolve host: ghcr.io", "Homebrew", false)).toBe(
      "failure.line.network",
    );
    // With them on, the words themselves, as they came.
    expect(planErrorMessage(fakeT, "curl: (6) Could not resolve host: ghcr.io", "Homebrew", true)).toBe(
      "curl: (6) Could not resolve host: ghcr.io",
    );
  });

  it("words each of Banager's own planning failures itself, naming the source", () => {
    for (const [kind, key] of [
      ["output_too_large", "planRefused.outputTooLarge"],
      ["index_updating", "planRefused.indexUpdating"],
      ["not_listed", "planRefused.notListed"],
      ["refused", "planRefused.refused"],
    ]) {
      expect(planErrorMessage(fakeT, JSON.stringify({ kind }), "Homebrew", false)).toBe(
        `${key}({"source":"Homebrew"})`,
      );
    }
  });

  it("interpolates the data an invalid name or a missing program carries", () => {
    expect(planErrorMessage(fakeT, '{"kind":"invalid_name","name":"-rf"}', "npm", false)).toBe(
      'planRefused.invalidName({"name":"-rf","source":"npm"})',
    );
    expect(
      planErrorMessage(
        fakeT,
        '{"kind":"program_missing","program":"/opt/homebrew/bin/brew"}',
        "Homebrew",
        false,
      ),
    ).toBe('planRefused.programMissing({"program":"/opt/homebrew/bin/brew"})');
  });

  it("quotes the system's reason a tool could not start inside a translated sentence, with technical details on", () => {
    expect(
      planErrorMessage(fakeT, '{"kind":"spawn_failed","detail":"Permission denied (os error 13)"}', "npm", true),
    ).toBe('planRefused.spawnFailed({"source":"npm","detail":"Permission denied (os error 13)"})');
  });

  it("says only that a tool could not start with technical details off", () => {
    expect(
      planErrorMessage(fakeT, '{"kind":"spawn_failed","detail":"Permission denied (os error 13)"}', "npm", false),
    ).toBe('planRefused.spawnFailedPlain({"source":"npm"})');
  });

  it("words each reason a path-list uninstall preview was refused, naming the path", () => {
    // `plan_operation_error` (src-tauri/src/ipc.rs) spells the reason in
    // snake_case by hand; these six are the whole set, and each has its
    // own sentence. The path arrives with `$HOME` already abbreviated.
    for (const [reason, key] of [
      ["outside_home", "planRefused.uninstallUnsafe.outsideHome"],
      ["shared_folder", "planRefused.uninstallUnsafe.sharedFolder"],
      ["missing", "planRefused.uninstallUnsafe.missing"],
      ["not_owned_by_you", "planRefused.uninstallUnsafe.notOwnedByYou"],
      ["not_what_instructions_expect", "planRefused.uninstallUnsafe.notWhatInstructionsExpect"],
      ["overlaps_kept", "planRefused.uninstallUnsafe.overlapsKept"],
    ]) {
      expect(
        planErrorMessage(
          fakeT,
          JSON.stringify({ kind: "uninstall_unsafe", path: "~/.local/bin/claude", reason }),
          "Claude Code",
          false,
        ),
      ).toBe(`${key}({"path":"~/.local/bin/claude"})`);
    }
    // A reason this build has no copy for, or a payload without its path,
    // is shown verbatim rather than guessed at -- with technical details
    // on; without them, not at all.
    const unknown = '{"kind":"uninstall_unsafe","path":"~/x","reason":"cursed"}';
    expect(planErrorMessage(fakeT, unknown, "Claude Code", true)).toBe(unknown);
    expect(planErrorMessage(fakeT, unknown, "Claude Code", false)).toBeNull();
    const pathless = '{"kind":"uninstall_unsafe","reason":"missing"}';
    expect(planErrorMessage(fakeT, pathless, "Claude Code", true)).toBe(pathless);
    expect(planErrorMessage(fakeT, pathless, "Claude Code", false)).toBeNull();
  });

  it("refuses a path it can't confirm without citing official instructions, which Antigravity CLI and Grok Build don't publish, in both locales", () => {
    // `not_what_instructions_expect` is what check 4 and the ancestry rule
    // (`removal::check_item`) answer on every path-list uninstall. Claude
    // Code's list is built from Anthropic's removal steps, but Antigravity
    // CLI and Grok Build publish none: their lists are Banager's own
    // reading of how each was installed (`recipes::AGY`, `recipes::GROK`
    // and their fixture READMEs). An agy launcher in a `~/.local/bin` that
    // is a link to a dotfiles folder inside the home folder, or a grok
    // `~/.grok/downloads` that is a link to another disk, gets this
    // sentence, so it says what Banager expects rather than what "the
    // official instructions" describe.
    const refusal = {
      en: en.planRefused.uninstallUnsafe.notWhatInstructionsExpect,
      zhCN: zhCN.planRefused.uninstallUnsafe.notWhatInstructionsExpect,
    };
    const detail = {
      en: en.planRefused.uninstallUnsafe.notWhatInstructionsExpectDetail,
      zhCN: zhCN.planRefused.uninstallUnsafe.notWhatInstructionsExpectDetail,
    };
    expect(refusal.en).toContain("isn't what was expected");
    expect(refusal.zhCN).toContain("和预期的不一样");
    for (const sentence of [refusal.en, refusal.zhCN, detail.en, detail.zhCN]) {
      expect(sentence).not.toMatch(/official|instructions|官方|说明/);
    }
    // What may be wrong is still said, behind the sentence's ⓘ
    // (`planErrorDetail`; docs/superpowers/backlog.md quotes the Chinese).
    expect(detail.en).toContain("It, or a folder it's in, links somewhere else");
    expect(detail.zhCN).toContain("它或它所在的文件夹链接到了别处");
    expect(
      planErrorDetail(
        fakeT,
        '{"kind":"uninstall_unsafe","path":"~/.local/bin/agy","reason":"not_what_instructions_expect"}',
      ),
    ).toBe("planRefused.uninstallUnsafe.notWhatInstructionsExpectDetail");
  });

  it("localises the submit-time refusal for a source that stopped answering", () => {
    // `submit_operation_error` sends the same payload `plan_operation`
    // does, because `Session::submit` now re-runs the actionability gate
    // against the current snapshot -- a preview the user was reading when
    // Ollama stopped is exactly the case that reaches a real person.
    expect(
      planErrorMessage(
        fakeT,
        '{"kind":"not_actionable","read_only":null,"unavailable":"NotRunning"}',
        "Ollama",
        false,
      ),
    ).toBe('sourceNotice.notRunning.description({"source":"Ollama"})');
  });

  it("localises the submit-time refusal for a source that is gone, without naming it", () => {
    // The instance is not in the snapshot any more, so the caller's
    // `sourceLabel` has already fallen back to the raw instance id. The
    // copy must not interpolate it.
    expect(planErrorMessage(fakeT, '{"kind":"source_gone"}', "brew:/opt/homebrew", false)).toBe(
      "planRefused.sourceGone",
    );
  });

  it("localises an expired plan instead of showing SubmitError::Expired's own English", () => {
    // `submit_operation_error` used to send `SubmitError::Expired`'s
    // `Display` verbatim -- this project's own English, unlocalised. It
    // now sends `{"kind":"expired"}` like every other structured refusal.
    expect(planErrorMessage(fakeT, '{"kind":"expired"}', "Homebrew", false)).toBe("planRefused.expired");
  });

  it("says a pinned package is being kept where it is, instead of showing the backend's JSON", () => {
    // `update_blocked` is the per-package refusal of the gate
    // (`AdapterError::UpdateBlocked` / `SubmitError::UpdateBlocked`,
    // `update_blocked_json` in src-tauri/src/ipc.rs). The Updates page
    // hides the button for such a row, so this is the stale-page path.
    expect(
      planErrorMessage(fakeT, '{"kind":"update_blocked","reason":"Pinned"}', "Homebrew", false),
    ).toBe('updates.blocked.Pinned.refused({"source":"Homebrew"})');
    // A reason this build does not know is shown verbatim, not guessed at.
    expect(
      planErrorMessage(fakeT, '{"kind":"update_blocked","reason":"Held"}', "Homebrew", true),
    ).toBe('{"kind":"update_blocked","reason":"Held"}');
  });

  it("localises an unknown/already-submitted plan instead of showing SubmitError::Unknown's own English", () => {
    expect(planErrorMessage(fakeT, '{"kind":"unknown"}', "Homebrew", false)).toBe("planRefused.unknown");
  });
});

describe("refusalSentence", () => {
  it("says a refusal in its sentence, its own words whatever the setting", () => {
    for (const technical of [false, true]) {
      expect(refusalSentence(fakeT, "updates.planFailed", '{"kind":"expired"}', "Homebrew", technical)).toBe(
        'updates.planFailed({"message":"planRefused.expired"})',
      );
    }
  });

  it("says the backend's own words only with technical details on, and each sentence's plain one without", () => {
    for (const [frame, plain] of [
      ["updates.planFailed", "updates.planFailedPlain"],
      ["updates.submitFailed", "updates.submitFailedPlain"],
      ["uninstall.planError", "uninstall.planErrorPlain"],
      ["uninstall.submitError", "uninstall.submitErrorPlain"],
    ] as const) {
      expect(refusalSentence(fakeT, frame, "glib is pinned", "Homebrew", true)).toBe(
        `${frame}({"message":"glib is pinned"})`,
      );
      expect(refusalSentence(fakeT, frame, "glib is pinned", "Homebrew", false)).toBe(plain);
    }
  });
});

describe("adapterIdOf and adapterLabel", () => {
  it("names a source by the adapter its instance id names, even when the snapshot has lost the instance", () => {
    expect(adapterIdOf("brew:/opt/homebrew")).toBe("brew");
    expect(adapterIdOf("ollama:http://127.0.0.1:11434")).toBe("ollama");
    // A tool with its own installer has one instance, named by its adapter alone.
    expect(adapterIdOf("standalone-claude")).toBe("standalone-claude");
    expect(adapterLabel(fakeT, "brew")).toBe("adapters.brew");
    expect(adapterLabel(fakeT, "standalone-claude")).toBe("adapters.standalone-claude");
  });

  it("gives an adapter this build has no name for its id, and never a key off the prototype", () => {
    expect(adapterLabel(fakeT, "winget")).toBe("winget");
    expect(adapterLabel(fakeT, "toString")).toBe("toString");
  });
});

describe("instanceLabels", () => {
  it("names a source by its kind alone while it is the only one of its kind", () => {
    const pip = instance({ id: "pip:/usr/bin/python3", adapter_id: "pip", prefix: "/usr" });
    expect([...instanceLabels(fakeT, [instance(), pip])]).toEqual([
      ["brew:/opt/homebrew", "adapters.brew"],
      ["pip:/usr/bin/python3", "adapters.pip"],
    ]);
  });

  it("names two Homebrews by the Mac each is for: Apple silicon's /opt/homebrew, an Intel Mac's /usr/local", () => {
    const intel = instance({ id: "brew:/usr/local", prefix: "/usr/local", exe_path: "/usr/local/bin/brew" });
    const labels = instanceLabels(fakeT, [instance(), intel]);
    expect(labels.get("brew:/opt/homebrew")).toBe(
      'common.sourceWithPlace({"source":"adapters.brew","place":"common.place.appleSilicon"})',
    );
    expect(labels.get("brew:/usr/local")).toBe(
      'common.sourceWithPlace({"source":"adapters.brew","place":"common.place.intel"})',
    );
    // A trailing slash is the same place.
    const slashed = instance({ id: "brew:/usr/local/", prefix: "/usr/local/" });
    expect(instanceLabels(fakeT, [instance(), slashed]).get("brew:/usr/local/")).toBe(
      'common.sourceWithPlace({"source":"adapters.brew","place":"common.place.intel"})',
    );
  });

  it("names any other place by the last part of its path that tells it apart, without a hidden folder's dot", () => {
    const home = instance({ id: "brew:/Users/you/homebrew", prefix: "/Users/you/homebrew" });
    const linux = instance({ id: "brew:/home/linuxbrew/.linuxbrew", prefix: "/home/linuxbrew/.linuxbrew" });
    const labels = instanceLabels(fakeT, [instance(), home, linux]);
    expect(labels.get("brew:/Users/you/homebrew")).toBe(
      'common.sourceWithPlace({"source":"adapters.brew","place":"homebrew"})',
    );
    expect(labels.get("brew:/home/linuxbrew/.linuxbrew")).toBe(
      'common.sourceWithPlace({"source":"adapters.brew","place":"linuxbrew"})',
    );
    // Not Homebrew: /opt/homebrew and /usr/local are only paths. The
    // first's last part tells it apart; the second has no telling part, so
    // it is its whole path.
    const npm = instance({ id: "npm:/opt/homebrew", adapter_id: "npm", prefix: "/opt/homebrew" });
    const npmSystem = instance({ id: "npm:/usr/local", adapter_id: "npm", prefix: "/usr/local" });
    expect([...instanceLabels(fakeT, [npm, npmSystem]).values()]).toEqual([
      'common.sourceWithPlace({"source":"adapters.npm","place":"homebrew"})',
      'common.sourceWithPlace({"source":"adapters.npm","place":"/usr/local"})',
    ]);
  });

  it.each(["en", "zh-CN"])("uses Homebrew for pip under its known prefix in %s", (lang) => {
    const t = i18n.getFixedT(lang);
    const brewPip = instance({ id: "pip:brew", adapter_id: "pip", prefix: "/opt/homebrew/bin/" });
    const other = instance({ id: "pip:other", adapter_id: "pip", prefix: "/opt/local/bin" });
    expect(instanceNames(t, [brewPip, other]).get(brewPip.id)?.place).toBe("Homebrew");
    expect(instanceLabels(t, [brewPip, other]).get(brewPip.id)).toBe(
      lang === "en" ? "pip (Homebrew)" : "pip（Homebrew）",
    );
    expect(instanceNames(t, [brewPip, other]).get(other.id)?.place).toBe("/opt/local/bin");
    // A similarly named custom folder is not identified as Homebrew.
    const custom = { ...brewPip, prefix: "/opt/homebrew-custom" };
    expect(instanceNames(t, [custom, other]).get(custom.id)?.place).toBe("homebrew-custom");
  });

  it("falls back to whole prefixes where two places' names would read the same", () => {
    const a = instance({ id: "brew:/Users/you/homebrew", prefix: "/Users/you/homebrew" });
    const b = instance({ id: "brew:/Volumes/Work/homebrew", prefix: "/Volumes/Work/homebrew" });
    expect([...instanceLabels(fakeT, [a, b]).values()]).toEqual([
      'common.sourceWithPlace({"source":"adapters.brew","place":"~/homebrew"})',
      'common.sourceWithPlace({"source":"adapters.brew","place":"/Volumes/Work/homebrew"})',
    ]);
  });

  it("tells two of one prefix apart by their programs, and else by the rest of their ids", () => {
    const system = instance({ id: "pip:/usr/bin/python3", adapter_id: "pip", prefix: "/usr", exe_path: "/usr/bin/python3" });
    const other = instance({ id: "pip:/usr/bin/python3.12", adapter_id: "pip", prefix: "/usr", exe_path: "/usr/bin/python3.12" });
    expect([...instanceLabels(fakeT, [system, other]).values()]).toEqual([
      'common.sourceWithPlace({"source":"adapters.pip","place":"/usr/bin/python3"})',
      'common.sourceWithPlace({"source":"adapters.pip","place":"/usr/bin/python3.12"})',
    ]);
    const a = instance({ id: "ollama:http://127.0.0.1:11434", adapter_id: "ollama", prefix: "/usr/local", exe_path: "/usr/local/bin/ollama" });
    const b = instance({ id: "ollama:http://127.0.0.1:11435", adapter_id: "ollama", prefix: "/usr/local", exe_path: "/usr/local/bin/ollama" });
    expect([...instanceLabels(fakeT, [a, b]).values()]).toEqual([
      'common.sourceWithPlace({"source":"adapters.ollama","place":"http://127.0.0.1:11434"})',
      'common.sourceWithPlace({"source":"adapters.ollama","place":"http://127.0.0.1:11435"})',
    ]);
  });

  it("reads 「Homebrew（Intel）」 in Chinese and \"Homebrew (Apple silicon)\" in English", () => {
    expect(zhCN.common.sourceWithPlace).toBe("{{source}}（{{place}}）");
    expect(en.common.sourceWithPlace).toBe("{{source}} ({{place}})");
    expect(zhCN.common.place).toEqual({ appleSilicon: "Apple芯片", intel: "Intel" });
    expect(en.common.place).toEqual({ appleSilicon: "Apple silicon", intel: "Intel" });
  });
});

describe("instanceNames", () => {
  it("is instanceLabels' two parts: the kind's name, and where it is only where another of its kind is", () => {
    const intel = instance({ id: "brew:/usr/local", prefix: "/usr/local" });
    const pip = instance({ id: "pip:/usr/bin/python3", adapter_id: "pip", prefix: "/usr" });
    expect([...instanceNames(fakeT, [instance(), intel, pip])]).toEqual([
      ["brew:/opt/homebrew", { source: "adapters.brew", place: "common.place.appleSilicon" }],
      ["brew:/usr/local", { source: "adapters.brew", place: "common.place.intel" }],
      ["pip:/usr/bin/python3", { source: "adapters.pip", place: null }],
    ]);
  });
});

describe("sourceWarningOf", () => {
  it("is the source's first warning notice, and nothing for news that is no problem", () => {
    expect(sourceWarningOf(instance(), "Homebrew", 0)).toBeNull();
    expect(sourceWarningOf(instance({ status: { unavailable: null, notes: ["IndexUpdating"] } }), "Homebrew", 3)).toBeNull();
    expect(
      sourceWarningOf(instance({ status: { unavailable: "NotResponding", notes: ["IndexMayBeStale"] } }), "Homebrew", 3)
        ?.id,
    ).toBe("brew:/opt/homebrew:unreachable");
    expect(
      sourceWarningOf(instance({ status: { unavailable: null, notes: ["IndexUpdating", "IndexMayBeStale"] } }), "Homebrew", 3)
        ?.id,
    ).toBe("brew:/opt/homebrew:index-may-be-stale");
  });
});

describe("planErrorDetail", () => {
  it("says Banager's own refusal in one sentence, with nothing behind an ⓘ about whose problem it was", () => {
    // The polish-3 copy rules (规则 3): 「问题出在 Banager，不在你的 Mac」
    // was reassurance, not a next step, and is gone.
    expect(planErrorMessage(fakeT, '{"kind":"refused"}', "Homebrew", false)).toBe(
      'planRefused.refused({"source":"Homebrew"})',
    );
    expect(planErrorDetail(fakeT, '{"kind":"refused"}')).toBeNull();
    expect(en.planRefused.refused).toBe("Couldn't continue because of an internal error.");
    expect(zhCN.planRefused.refused).toBe("发生内部错误，无法继续。");
    expect("banagerFaultDetail" in en.common).toBe(false);
    expect("banagerFaultDetail" in zhCN.common).toBe(false);
  });

  it("has nothing more to say about every other refusal, or about text that is not one", () => {
    for (const raw of [
      '{"kind":"expired"}',
      '{"kind":"source_gone"}',
      '{"kind":"not_actionable","read_only":"ByDesign","unavailable":null}',
      '{"kind":"uninstall_unsafe","path":"~/.local/bin/claude","reason":"shared_folder"}',
      '{"kind":"uninstall_blocked","reason":"Pinned"}',
      "unknown instance fake:1",
      '{"kind":"toString"}',
    ]) {
      expect(planErrorDetail(fakeT, raw), raw).toBeNull();
    }
  });
});

describe("settingsSaveErrorMessage", () => {
  it("words the reasons a person can act on itself, whatever the setting", () => {
    for (const [reason, key] of [
      ["permission_denied", "settingsSaveFailed.permissionDenied"],
      ["disk_full", "settingsSaveFailed.diskFull"],
      ["read_only", "settingsSaveFailed.readOnly"],
    ]) {
      for (const technical of [false, true]) {
        expect(
          settingsSaveErrorMessage(fakeT, JSON.stringify({ kind: "settings_save_failed", reason }), technical),
        ).toBe(key);
      }
    }
  });

  it("quotes the system's own text for any other reason, inside a translated phrase, with technical details on", () => {
    const other = '{"kind":"settings_save_failed","reason":"other","detail":"Input/output error (os error 5)"}';
    expect(settingsSaveErrorMessage(fakeT, other, true)).toBe(
      'settingsSaveFailed.other({"detail":"Input/output error (os error 5)"})',
    );
    // Without them there is nothing to say of it: the caller's sentence
    // says the save failed (`settingsSaveSentence`).
    expect(settingsSaveErrorMessage(fakeT, other, false)).toBeNull();
  });

  it("shows anything that is not the payload verbatim with technical details on, and nothing of it without", () => {
    expect(settingsSaveErrorMessage(fakeT, "boom", true)).toBe("boom");
    expect(settingsSaveErrorMessage(fakeT, '{"kind":"expired"}', true)).toBe('{"kind":"expired"}');
    expect(settingsSaveErrorMessage(fakeT, "boom", false)).toBeNull();
    expect(settingsSaveErrorMessage(fakeT, '{"kind":"expired"}', false)).toBeNull();
  });
});

describe("settingsSaveSentence", () => {
  it("says a reason a person can act on in its sentence, whatever the setting", () => {
    const full = JSON.stringify({ kind: "settings_save_failed", reason: "disk_full" });
    for (const technical of [false, true]) {
      expect(settingsSaveSentence(fakeT, "settings.saveError", full, technical)).toBe(
        'settings.saveError({"message":"settingsSaveFailed.diskFull"})',
      );
    }
  });

  it("says the system's own words only with technical details on, and each sentence's plain one without", () => {
    for (const [frame, plain] of [
      ["settings.saveError", "settings.saveErrorPlain"],
      ["updates.saveChoiceFailed", "updates.saveChoiceFailedPlain"],
    ] as const) {
      expect(settingsSaveSentence(fakeT, frame, "boom", true)).toBe(`${frame}({"message":"boom"})`);
      expect(settingsSaveSentence(fakeT, frame, "boom", false)).toBe(plain);
    }
  });
});

describe("parseOpenOllamaFailure", () => {
  it("reads the two payloads open_ollama_app rejects with", () => {
    // The same strings src-tauri/src/ipc.rs locks in
    // `test_open_ollama_failure_payloads_are_the_two_the_front_end_decodes`.
    expect(parseOpenOllamaFailure('{"kind":"ollama_open_failed","reason":"not_installed"}')).toBe(
      "not_installed",
    );
    expect(parseOpenOllamaFailure('{"kind":"ollama_open_failed","reason":"launch_failed"}')).toBe(
      "launch_failed",
    );
  });

  it("is null for anything else, including a reason this build does not know", () => {
    expect(parseOpenOllamaFailure('{"kind":"ollama_open_failed","reason":"something_new"}')).toBeNull();
    expect(
      parseOpenOllamaFailure('{"kind":"not_actionable","read_only":null,"unavailable":"NotRunning"}'),
    ).toBeNull();
    expect(parseOpenOllamaFailure("No such file or directory (os error 2)")).toBeNull();
    expect(parseOpenOllamaFailure("null")).toBeNull();
    expect(parseOpenOllamaFailure("")).toBeNull();
  });
});

describe("openOllamaErrorMessage", () => {
  it("gives each failure its own copy, never the raw JSON", () => {
    for (const technical of [false, true]) {
      expect(
        openOllamaErrorMessage(fakeT, '{"kind":"ollama_open_failed","reason":"not_installed"}', technical),
      ).toBe("sourceNotice.openOllamaFailed.notInstalled");
      expect(
        openOllamaErrorMessage(fakeT, '{"kind":"ollama_open_failed","reason":"launch_failed"}', technical),
      ).toBe("sourceNotice.openOllamaFailed.launchFailed");
    }
  });

  it("shows anything it does not recognise verbatim with technical details on, and plain words without", () => {
    expect(openOllamaErrorMessage(fakeT, "command open_ollama_app not found", true)).toBe(
      "command open_ollama_app not found",
    );
    expect(openOllamaErrorMessage(fakeT, "command open_ollama_app not found", false)).toBe(
      "sourceNotice.openOllamaFailed.other",
    );
  });

  it("keeps why Homebrew's ollama has no app for the failure's Details, and gives the others none", () => {
    // The sentence says what to do -- download the app -- and the Details
    // why it is missing when Ollama was installed with Homebrew.
    expect(
      openOllamaErrorDetail(fakeT, '{"kind":"ollama_open_failed","reason":"not_installed"}'),
    ).toBe("sourceNotice.openOllamaFailed.notInstalledDetail");
    expect(openOllamaErrorDetail(fakeT, '{"kind":"ollama_open_failed","reason":"launch_failed"}')).toBeNull();
    expect(openOllamaErrorDetail(fakeT, "command open_ollama_app not found")).toBeNull();
    expect(en.sourceNotice.openOllamaFailed.notInstalled).toBe(
      "There's no Ollama app in Applications. Download it from ollama.com, install it, then click Open Ollama again.",
    );
    expect(zhCN.sourceNotice.openOllamaFailed.notInstalledDetail).toBe("用Homebrew安装的ollama命令不包含此App。");
  });
});

describe("failedSourceNames and namesInSentence", () => {
  // What 「部分检查未完成」 says behind its ⓘ: which sources did not
  // finish, by name.
  const t = (key: string, options?: Record<string, string>): string => {
    const english: Record<string, string> = {
      "adapters.brew": "Homebrew",
      "adapters.npm": "npm",
      "adapters.uv": "uv",
      "common.listSeparator": ", ",
    };
    if (key === "common.listAnd") return `${options?.list} and ${options?.last}`;
    return english[key] ?? key;
  };

  it("names each source once, in the order the errors first name it, by its adapter", () => {
    const names = failedSourceNames(
      t,
      [
        { instance_id: "brew:/opt/homebrew", message: "brew list failed" },
        { instance_id: "npm", message: "internal error detecting this source" },
        { instance_id: "brew:/opt/homebrew", message: "brew outdated failed" },
        // A second Homebrew is Homebrew too.
        { instance_id: "brew:/usr/local", message: "brew list failed" },
        { instance_id: "uv:/Users/you/.local/share/uv/tools", message: "uv tool list exited 2" },
      ],
      [instance()],
    );
    expect(names).toEqual(["Homebrew", "npm", "uv"]);
  });

  it("lists names the way a sentence does", () => {
    expect(namesInSentence(t, [])).toBe("");
    expect(namesInSentence(t, ["Homebrew"])).toBe("Homebrew");
    expect(namesInSentence(t, ["Homebrew", "npm"])).toBe("Homebrew and npm");
    expect(namesInSentence(t, ["Homebrew", "npm", "uv"])).toBe("Homebrew, npm and uv");
  });
});

describe("failedSourceAdapters", () => {
  function err(instance_id: string, message = "boom"): SourceError {
    return { instance_id, message };
  }

  it("is empty for no errors", () => {
    expect(failedSourceAdapters([], [instance()])).toEqual([]);
  });

  it("puts one source down once even when inventory and check-updates both failed for it", () => {
    // The exact shape session/refresh.rs produces for one broken Homebrew:
    // one SourceError from the inventory fetch, one from check_updates,
    // same instance_id. The notice names sources, not calls.
    const errors = [
      err("brew:/opt/homebrew", "brew list failed"),
      err("brew:/opt/homebrew", "brew outdated failed"),
    ];
    expect(failedSourceAdapters(errors, [instance()])).toEqual(["brew"]);
  });

  it("puts two instances of one source down as that one source, as the notice names it", () => {
    // An Apple-silicon Mac with a second Homebrew in /usr/local, offline:
    // the banner said "Homebrew didn't finish this check" over "2 checks
    // didn't finish".
    const intel = instance({ id: "brew:/usr/local", prefix: "/usr/local", exe_path: "/usr/local/bin/brew" });
    expect(
      failedSourceAdapters(
        [err("brew:/opt/homebrew", "brew update failed"), err("brew:/usr/local", "brew update failed")],
        [instance(), intel],
      ),
    ).toEqual(["brew"]);
    // Two npm prefixes, each failing both calls: one npm, not four.
    const errors = [
      err("npm:/opt/homebrew/lib", "npm ls failed"),
      err("npm:/opt/homebrew/lib", "npm outdated failed"),
      err("npm:/usr/local/lib", "npm ls failed"),
      err("npm:/usr/local/lib", "npm outdated failed"),
    ];
    expect(failedSourceAdapters(errors, [])).toEqual(["npm"]);
  });

  it("puts a detect-stage failure (bare adapter id) down to its adapter, in the order the errors come", () => {
    // refresh.rs pushes the detect-panic error with instance_id set to the
    // adapter id alone (e.g. "cargo"), not a "<adapter_id>:<path>"
    // instance id.
    const errors = [err("cargo", "internal error detecting this source"), err("brew:/opt/homebrew")];
    expect(failedSourceAdapters(errors, [instance()])).toEqual(["cargo", "brew"]);
  });

  it("is the list the notice's names come from, one name each", () => {
    const errors = [
      err("brew:/opt/homebrew"),
      err("brew:/usr/local"),
      err("npm"),
      err("uv:/Users/you/.local/share/uv/tools"),
    ];
    const adapters = failedSourceAdapters(errors, [instance()]);
    expect(failedSourceNames(fakeT, errors, [instance()])).toEqual(
      adapters.map((adapterId) => fakeT(ADAPTER_LABEL_KEYS[adapterId])),
    );
    // No two sources share a name in either language, so the notice names
    // each source once.
    for (const locale of [en, zhCN]) {
      const names = Object.values(locale.adapters);
      expect(new Set(names).size).toBe(names.length);
      expect(Object.keys(locale.adapters).sort()).toEqual(Object.keys(ADAPTER_LABEL_KEYS).sort());
    }
  });
});

describe("unfinishedChecksNotice", () => {
  const enT = i18n.getFixedT("en");
  const zhT = i18n.getFixedT("zh-CN");
  const say = (t: typeof enT, notice: ReturnType<typeof unfinishedChecksNotice>) =>
    notice === null ? null : [t(notice.titleKey, notice.values), t(notice.descriptionKey, notice.values)];

  it("is nothing when every check finished", () => {
    expect(unfinishedChecksNotice(enT, [], [instance()])).toBeNull();
  });

  it("is one warning with Check again, the sources behind its ⓘ, each named once", () => {
    // Homebrew's inventory and update check both failed: one source. A
    // detection that failed names its adapter; so does an instance the
    // snapshot no longer lists.
    const errors: SourceError[] = [
      { instance_id: "brew:/opt/homebrew", message: "brew list failed" },
      { instance_id: "brew:/opt/homebrew", message: "brew outdated failed" },
      { instance_id: "npm", message: "internal error detecting this source" },
      { instance_id: "uv:/Users/you/.local/share/uv/tools", message: "uv tool list exited 2" },
    ];
    const notice = unfinishedChecksNotice(enT, errors, [instance()]);
    expect(notice).toMatchObject({
      id: "checks-unfinished",
      variant: "warning",
      // The toolbar's own words for the same check, as a silent source's notice has.
      action: { id: "checkAgain", labelKey: "header.checkAgain" },
    });
    expect(say(enT, notice)).toEqual([
      "Some checks didn't finish",
      "Homebrew, npm and uv didn't finish checking this time; some updates may not be listed yet.",
    ]);
    // Never 更新 for a check: it is this app's word for installing a newer version.
    expect(say(zhT, unfinishedChecksNotice(zhT, errors, [instance()]))).toEqual([
      "部分检查未完成",
      "Homebrew、npm和uv这次未检查完，更新可能还没全部列出。",
    ]);
    // Two Homebrews that both failed are the one Homebrew it names.
    const intel = instance({ id: "brew:/usr/local", prefix: "/usr/local", exe_path: "/usr/local/bin/brew" });
    expect(
      say(
        enT,
        unfinishedChecksNotice(
          enT,
          [
            { instance_id: "brew:/opt/homebrew", message: "brew update failed" },
            { instance_id: intel.id, message: "brew update failed" },
          ],
          [instance(), intel],
        ),
      ),
    ).toEqual(["Some checks didn't finish", "Homebrew didn't finish checking this time; some updates may not be listed yet."]);
  });

  it("leaves out a source that did not answer: its own notice says so", () => {
    // `refresh` carries a silent source's errors forward while an
    // operation holds it; its "isn't responding" line already says the
    // check did not reach it.
    const silent = instance({ status: { unavailable: "NotResponding", notes: [] } });
    const stopped = instance({
      id: "ollama:http://127.0.0.1:11434",
      adapter_id: "ollama",
      status: { unavailable: "NotRunning", notes: [] },
    });
    const pipx = instance({ id: "pipx:/Users/you/.local/pipx", adapter_id: "pipx" });
    const errors: SourceError[] = [
      { instance_id: silent.id, message: "brew list timed out" },
      { instance_id: stopped.id, message: "connection refused" },
    ];
    expect(unfinishedChecksNotice(enT, errors, [silent, stopped, pipx])).toBeNull();
    expect(
      say(enT, unfinishedChecksNotice(enT, [...errors, { instance_id: pipx.id, message: "timed out" }], [silent, stopped, pipx])),
    ).toEqual(["Some checks didn't finish", "pipx didn't finish checking this time; some updates may not be listed yet."]);
    // One source or several, the same words: no 它们 for one.
    expect(
      say(zhT, unfinishedChecksNotice(zhT, [...errors, { instance_id: pipx.id, message: "timed out" }], [silent, stopped, pipx])),
    ).toEqual(["部分检查未完成", "pipx这次未检查完，更新可能还没全部列出。"]);
  });

  it("names only the sources in view, and a failed detection by its adapter", () => {
    const pipx = instance({ id: "pipx:/Users/you/.local/pipx", adapter_id: "pipx" });
    const errors: SourceError[] = [
      { instance_id: pipx.id, message: "pipx list timed out" },
      { instance_id: "brew", message: "internal error detecting this source" },
    ];
    // The Installed page on Homebrew: its detection failed, pipx is not in view.
    expect(say(enT, unfinishedChecksNotice(enT, errors, [instance(), pipx], [instance()]))).toEqual([
      "Some checks didn't finish",
      "Homebrew didn't finish checking this time; some updates may not be listed yet.",
    ]);
    // On pipx: pipx alone.
    expect(say(enT, unfinishedChecksNotice(enT, errors, [instance(), pipx], [pipx]))).toEqual([
      "Some checks didn't finish",
      "pipx didn't finish checking this time; some updates may not be listed yet.",
    ]);
    // On a source whose checks all finished: nothing.
    const cargo = instance({ id: "cargo:/Users/you/.cargo", adapter_id: "cargo" });
    expect(unfinishedChecksNotice(enT, errors, [instance(), pipx, cargo], [cargo])).toBeNull();
  });
});

describe("nothingFound", () => {
  const enT = i18n.getFixedT("en");
  const jq: InstalledArtifact = {
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
    display_name: "jq",
    version: "1.7",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
  const found = (over: Partial<Parameters<typeof nothingFound>[1]> = {}) =>
    nothingFound(enT, { detect: "Found", instances: [instance()], artifacts: [], errors: [], ...over });

  it("is no source when every source's detection found nothing, whatever else", () => {
    expect(found({ detect: "Missing", instances: [] })).toBe("noSources");
    expect(enT(NOTHING_FOUND_KEYS.noSources.title)).toBe("No tools to manage");
    expect(enT(NOTHING_FOUND_KEYS.noSources.description)).toBe("Install Homebrew first.");
  });

  it("is nothing installed only when no source, and no check that did not finish, has something to say", () => {
    expect(found()).toBe("nothingInstalled");
    expect(enT(NOTHING_FOUND_KEYS.nothingInstalled.title)).toBe("No installed tools found");
    // Something installed.
    expect(found({ artifacts: [jq] })).toBeNull();
    // A stopped Ollama, whose Open Ollama is on the page.
    expect(found({ instances: [instance({ status: { unavailable: "NotRunning", notes: [] } })] })).toBeNull();
    // A check that did not finish: nothing listed may be only what it did not get to.
    expect(found({ errors: [{ instance_id: "brew:/opt/homebrew", message: "brew list timed out" }] })).toBeNull();
    // A read-only source says so on its rows, and has none.
    expect(found({ instances: [instance({ read_only_reason: "ByDesign" })] })).toBe("nothingInstalled");
  });
});

describe("UPDATE_BLOCKED_KEYS", () => {
  it("quotes a brew path with a space in the unpin command, so it pastes as one argument", () => {
    const instance = {
      id: "brew:/Users/Alice Smith/homebrew",
      adapter_id: "brew",
      exe_path: "/Users/Alice Smith/homebrew/bin/brew",
      prefix: "/Users/Alice Smith/homebrew",
      scope: "User",
      version: "7.0.6",
      answered_at: null,
      unverified_version: null,
      read_only_reason: null,
      status: { unavailable: null, notes: [] },
    } satisfies ManagerInstance;
    const key = { instance_id: instance.id, kind: "Formula", name: "glib" } satisfies ArtifactKey;
    expect(UPDATE_BLOCKED_KEYS.Pinned.command(key, instance)).toBe(
      "'/Users/Alice Smith/homebrew/bin/brew' unpin glib",
    );
  });

  it("builds pipx's own unpin command for a pinned pipx tool, from the pipx Banager found", () => {
    // pipx spells it `pipx unpin <name>` (its `commands/upgrade.py:473`);
    // a brew-shaped `brew unpin cowsay` would answer "No available formula".
    const pipx = {
      id: "pipx",
      adapter_id: "pipx",
      exe_path: "/Users/Alice Smith/.local/bin/pipx",
      prefix: "/Users/Alice Smith/.local/bin",
      scope: "User",
      version: "1.17.3",
      answered_at: null,
      unverified_version: null,
      read_only_reason: null,
      status: { unavailable: null, notes: [] },
    } satisfies ManagerInstance;
    const key = { instance_id: "pipx", kind: "Tool", name: "cowsay" } satisfies ArtifactKey;
    expect(UPDATE_BLOCKED_KEYS.Pinned.command(key, pipx)).toBe(
      "'/Users/Alice Smith/.local/bin/pipx' unpin cowsay",
    );
    // An instance missing from the snapshot still gets the right tool,
    // read off the key's own `instance_id`.
    expect(UPDATE_BLOCKED_KEYS.Pinned.command(key, undefined)).toBe("pipx unpin cowsay");
  });

  it("names the pinned package's own source in its detail, not always Homebrew, and the command once", () => {
    // pipx produces `Pinned` too, so "It's pinned in Homebrew" would be
    // false on a pipx row.
    for (const copy of [en.updates.blocked.Pinned.detail, zhCN.updates.blocked.Pinned.detail]) {
      expect(copy).toContain("{{source}}");
      expect(copy).not.toContain("Homebrew");
      expect(copy.split("{{command}}")).toHaveLength(2);
    }
    expect(UPDATE_BLOCKED_KEYS.Pinned.commandInDetail).toBe(true);
  });

  it("promises nothing in a pin's detail about when the update comes, or that the package stays put", () => {
    // The one sentence serves a row whose source did not answer the last
    // check, which gets no Update button until it does, and a pinned app
    // that updates itself, which `brew pin` warns may move anyway: "the
    // next time Banager checks" and "keeping it at the version it has"
    // would each be false of one of them.
    expect(en.updates.blocked.Pinned.detail).not.toMatch(/next time|version it has now|keeping|current version/);
    expect(zhCN.updates.blocked.Pinned.detail).not.toMatch(/下次|现在的版本|当前版本/);
  });

  it("does not promise, when refusing, that a pinned package stays at its version", () => {
    // `refused` is given only the source's label, never the package, so it
    // is also what a pinned app that updates itself would get -- and `brew
    // pin` warns such an app may update despite the pin.
    expect(en.updates.blocked.Pinned.refused).not.toMatch(/version/);
    expect(zhCN.updates.blocked.Pinned.refused).not.toMatch(/版本/);
  });

  it("does not say in Chinese that Homebrew is the one who pinned it", () => {
    // Someone ran `brew pin` -- the user, or a script. "Homebrew 把它固定"
    // made Homebrew the one who did it; the English never says who. The
    // detail names the source as where the pin is (「在 {{source}} 里」),
    // not as who put it there.
    for (const copy of [zhCN.updates.blocked.Pinned.detail, zhCN.updates.blocked.Pinned.refused]) {
      expect(copy).not.toMatch(/把[^，。]*固定/);
      expect(copy).not.toMatch(/\{\{source\}\}\s*固定/);
    }
    expect(zhCN.updates.blocked.Pinned.detail).toContain("在{{source}}中固定");
    expect(zhCN.updates.blocked.Pinned.refused).toMatch(/被固定/);
  });

  it("names the tool's own launcher, quoted when its path has a space, as what a self-updating tool is opened with", () => {
    // Spec §4.4 / §十三 #31: the detail says to open the tool once (not
    // `<launcher> --version`, which on agy 1.2.10 never reaches its
    // updater), so the command is the launcher itself, bare. A missing
    // instance gives the bare name, which `refresh` never produces.
    const agy = instance({
      id: "standalone-agy",
      adapter_id: "standalone-agy",
      exe_path: "/Users/Alice Smith/.local/bin/agy",
      prefix: "/Users/Alice Smith/.gemini/antigravity-cli",
    });
    const key = { instance_id: "standalone-agy", kind: "Binary", name: "agy" } satisfies ArtifactKey;
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.command(key, agy)).toBe(
      "'/Users/Alice Smith/.local/bin/agy'",
    );
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.command(key, undefined)).toBe("agy");
    // A path is a technical detail: it is a line of its own under the
    // sentence, with "Show technical details" on, not part of it.
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.commandInDetail).toBe(false);
  });

  it("tells a self-updating tool's user to open it once, in both locales, with no command or versions in the sentence", () => {
    // The versions are the row's own version column; the launcher is shown
    // under the sentence only with technical details on.
    expect(en.updates.blocked.SelfUpdatesOnly.detail).toBe(
      "It updates itself and can't be updated here. Open it once and it checks for a new version.",
    );
    expect(zhCN.updates.blocked.SelfUpdatesOnly.detail).toBe("它会自行更新，无法在这里更新。打开它一次就会检查新版本。");
    for (const copy of [en.updates.blocked.SelfUpdatesOnly.detail, zhCN.updates.blocked.SelfUpdatesOnly.detail]) {
      expect(copy).not.toContain("{{command}}");
      expect(copy).not.toContain("{{target}}");
    }
    // `refused` gets only the source's label.
    expect(en.updates.blocked.SelfUpdatesOnly.refused).not.toContain("{{command}}");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.refused).not.toContain("{{command}}");
    expect(en.updates.blocked.SelfUpdatesOnly.badge).toBe("Updates when run");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.badge).toBe("打开它来更新");
  });
});

describe("UPDATE_BLOCKED_KEYS.Disabled", () => {
  const key = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "quickjot" } satisfies ArtifactKey;
  const artifact = (replacement: string | null): InstalledArtifact => ({
    key,
    display_name: "QuickJot",
    version: "2.3.1",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: {
      ...NO_FACTS,
      homebrew: {
        deprecated: null,
        disabled: { date: "2026-09-01", reason: "fails_gatekeeper_check", replacement },
        caveats: null,
        other_versions: [],
      },
    },
  });

  it("has nothing to run: Homebrew will not update it, and Banager does not force it", () => {
    expect(UPDATE_BLOCKED_KEYS.Disabled.command(key, instance())).toBe("");
    expect(UPDATE_BLOCKED_KEYS.Disabled.commandInDetail).toBe(false);
    expect(UPDATE_BLOCKED_KEYS.Disabled.typed).toBeNull();
  });

  it("uses Homebrew's own word for the mark, as the Installed page does", () => {
    expect(UPDATE_BLOCKED_KEYS.Disabled.badge).toBe("brewStatus.disabledWord");
    expect(en.brewStatus.disabledWord).toBe("Disabled");
    expect(zhCN.brewStatus.disabledWord).toBe("已停用");
  });

  it("says why there is no button, in both locales, and that the copy installed stays, not whether it works", () => {
    // And what the person may do about it, as the Installed page's
    // Homebrew line says it (`brewStatus.disabled`; walk-2 W2-13).
    expect(en.updates.disabledBlocked.detail).toBe(
      "{{source}} has disabled it and won't provide more updates. The installed copy isn't removed; uninstall it when you no longer need it.",
    );
    expect(zhCN.updates.disabledBlocked.detail).toBe(
      "{{source}}已停用它，以后不再提供更新。已安装的这一份不会被删除；不再需要时可以卸载它。",
    );
    expect(en.updates.disabledBlocked.refused).toBe("Couldn't update it because {{source}} has disabled it.");
    expect(zhCN.updates.disabledBlocked.refused).toBe("无法更新，因为{{source}}已停用它。");
    for (const copy of [en.updates.disabledBlocked.detail, zhCN.updates.disabledBlocked.detail]) {
      expect(copy).not.toMatch(/still works|继续使用|仍可使用|{{command}}/);
    }
  });

  it("adds Homebrew's suggested replacement where it gives one", () => {
    expect(UPDATE_BLOCKED_KEYS.Disabled.note(key, instance(), artifact("quickjot-ng"))).toEqual({
      key: "brewStatus.replacement",
      options: { name: "quickjot-ng" },
    });
    expect(UPDATE_BLOCKED_KEYS.Disabled.note(key, instance(), artifact(null))).toBeNull();
    expect(UPDATE_BLOCKED_KEYS.Disabled.note(key, instance(), undefined)).toBeNull();
  });

  it("is what a stale page's refusal says", () => {
    const raw = JSON.stringify({ kind: "update_blocked", reason: "Disabled" });
    expect(planErrorMessage(i18n.getFixedT("en"), raw, "Homebrew", false)).toBe(
      "Couldn't update it because Homebrew has disabled it.",
    );
  });
});

describe("UPDATE_BLOCKED_KEYS.Pinned's note", () => {
  it("says on a pinned pipx tool that pipx's unpin also unpins what was injected, and nothing on Homebrew's", () => {
    const pipxKey = { instance_id: "pipx", kind: "Tool", name: "poetry" } satisfies ArtifactKey;
    const pipx = instance({ id: "pipx", adapter_id: "pipx", exe_path: "/opt/homebrew/bin/pipx" });
    expect(UPDATE_BLOCKED_KEYS.Pinned.note(pipxKey, pipx, undefined)).toEqual({ key: "updates.pinnedPipxInjected" });
    // An instance the snapshot lacks: the adapter from the key.
    expect(UPDATE_BLOCKED_KEYS.Pinned.note(pipxKey, undefined, undefined)).toEqual({
      key: "updates.pinnedPipxInjected",
    });
    const brewKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" } satisfies ArtifactKey;
    expect(UPDATE_BLOCKED_KEYS.Pinned.note(brewKey, instance(), undefined)).toBeNull();
    // What the command does, not how many: Banager does not list injected
    // packages.
    // Without pipx's own word for them, 「注入」, which says nothing to a
    // person who never ran `pipx inject` (walk-2 W2-13).
    expect(en.updates.pinnedPipxInjected).toBe("This command also unpins the other packages installed alongside it.");
    expect(zhCN.updates.pinnedPipxInjected).toBe("这条命令也会让和它装在一起的其他包不再固定版本。");
    expect(zhCN.updates.pinnedPipxInjected).not.toContain("注入");
    expect(en.updates.pinnedPipxInjected).not.toMatch(/inject/i);
    expect(zhCN.updates.pinnedPipxInjected).not.toMatch(/\d|{{count}}/);
  });
});

describe("typedLauncherName", () => {
  const agy = instance({
    id: "standalone-agy",
    adapter_id: "standalone-agy",
    exe_path: "/Users/Alice Smith/.local/bin/agy",
    prefix: "/Users/Alice Smith/.gemini/antigravity-cli",
  });
  const agyArtifact = (commands: InstalledArtifact["facts"]["commands"]): InstalledArtifact => ({
    key: { instance_id: "standalone-agy", kind: "Binary", name: "agy" },
    display_name: "Antigravity CLI",
    version: "1.2.11",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: "/Users/Alice Smith/.local/bin/agy",
    auto_updates: true,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, commands },
  });

  it("is the launcher's own name, not its path, while typing it runs this copy", () => {
    expect(typedLauncherName(agyArtifact([{ name: "agy", state: "Runs" }]), agy)).toBe("agy");
  });

  it("is null where typing it would not open this copy, or Banager did not judge it", () => {
    expect(typedLauncherName(agyArtifact([{ name: "agy", state: { NotOnPath: { dir: "~/.local/bin" } } }]), agy)).toBeNull();
    expect(typedLauncherName(agyArtifact([{ name: "agy", state: { ShadowedBy: { by: null } } }]), agy)).toBeNull();
    expect(typedLauncherName(agyArtifact([{ name: "agy", state: null }]), agy)).toBeNull();
    expect(typedLauncherName(agyArtifact([]), agy)).toBeNull();
    // Another command of the artifact's does not stand in for the launcher.
    expect(typedLauncherName(agyArtifact([{ name: "agent", state: "Runs" }]), agy)).toBeNull();
    expect(typedLauncherName(undefined, agy)).toBeNull();
    expect(typedLauncherName(agyArtifact([{ name: "agy", state: "Runs" }]), undefined)).toBeNull();
  });

  it("is said in a sentence that names the Terminal and sets the name as {{command}}, in both locales", () => {
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.typed?.detail).toBe("updates.selfUpdatesTyped");
    expect(en.updates.selfUpdatesTyped).toBe(
      "It updates itself and can't be updated here. Type {{command}} in Terminal to open it once, and it checks for a new version.",
    );
    expect(zhCN.updates.selfUpdatesTyped).toBe(
      "它会自行更新，无法在这里更新。在终端里输入{{command}}打开它一次，就会检查新版本。",
    );
  });
});

describe("the Updates page's chip details", () => {
  // Every sentence behind a status chip on the Updates page, in both
  // locales: the redesign's rule is at most two short sentences.
  interface ChipCopy {
    updates: {
      blocked: { Pinned: { detail: string }; SelfUpdatesOnly: { detail: string } };
      selfUpdatesTyped: string;
      disabledBlocked: { detail: string };
      cannotCheckShort: string;
      unavailableDetail: Record<string, string>;
    };
    sourceNotice: {
      pipReadOnly: { description: string };
      prefixNotWritable: { description: string };
    };
  }
  const details = (locale: ChipCopy) => [
    locale.updates.blocked.Pinned.detail,
    locale.updates.blocked.SelfUpdatesOnly.detail,
    locale.updates.selfUpdatesTyped,
    locale.updates.disabledBlocked.detail,
    locale.updates.cannotCheckShort,
    locale.sourceNotice.pipReadOnly.description,
    locale.sourceNotice.prefixNotWritable.description,
    ...Object.values(locale.updates.unavailableDetail),
  ];

  it("keeps every one to at most two sentences, in both locales", () => {
    for (const copy of details(en)) {
      expect(copy.match(/[.!?](\s|$)/g)?.length ?? 0, copy).toBeLessThanOrEqual(2);
    }
    for (const copy of details(zhCN)) {
      expect(copy.match(/[。！？]/g)?.length ?? 0, copy).toBeLessThanOrEqual(2);
    }
  });

  it("gives pip's rows pipx or uv, and npm's rows Homebrew, each only its own advice, in the words a refusal uses", () => {
    // The two read-only reasons need different ways out: pipx or uv for
    // pip, and a Node installed with Homebrew for an npm whose folder the
    // account cannot change. The chip's detail is the sentence a refusal
    // for that source says (`notActionableMessage`), so the two cannot
    // disagree -- npm's used to promise, on the chip, that a Node from
    // Homebrew lets Banager manage the packages already there (T5).
    expect(READ_ONLY_DETAIL_KEYS).toEqual({
      ByDesign: "sourceNotice.pipReadOnly.description",
      PrefixNotWritable: "sourceNotice.prefixNotWritable.description",
    });
    expect(notActionableMessage(fakeT, { read_only: "ByDesign", unavailable: null }, "pip")).toBe(
      READ_ONLY_DETAIL_KEYS.ByDesign,
    );
    expect(notActionableMessage(fakeT, { read_only: "PrefixNotWritable", unavailable: null }, "npm")).toBe(
      READ_ONLY_DETAIL_KEYS.PrefixNotWritable,
    );
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.pipReadOnly.description).toMatch(/pipx或uv|pipx or uv/);
      expect(locale.sourceNotice.pipReadOnly.description).not.toContain("Homebrew");
      expect(locale.sourceNotice.prefixNotWritable.description).toContain("Homebrew");
      expect(locale.sourceNotice.prefixNotWritable.description).not.toMatch(/pipx|uv/);
    }
    // What to do, and of which: reinstall, and only a tool the person uses
    // in Terminal (walk-2 W2-13).
    expect(zhCN.sourceNotice.pipReadOnly.description).toBe(
      "pip安装的内容只能在这里查看。如果其中有你在终端里使用的命令行工具，可以用pipx或uv重新安装它，之后就能在这里更新和卸载。",
    );
    expect(en.sourceNotice.pipReadOnly.description).toBe(
      "You can only view pip installs here. If one of them is a command-line tool you use in Terminal, reinstall it with pipx or uv to update and uninstall it here.",
    );
  });

  it("calls a source that did not answer one thing on every page, and says what to do next", () => {
    // ruff read 暂时不可用 on the Updates page and 暂时不能更新 on the
    // Installed page, and its drawer said 没有响应 under the chip over
    // 没有应答 in the notice under it: one fact, in two words each time.
    expect(zhCN.updates.sourceUnavailable).toBe("暂时不能更新");
    expect(en.updates.sourceUnavailable).toBe("Can't update now");
    expect(JSON.stringify(zhCN)).not.toContain("应答");
    expect(zhCN.sourceNotice.unreachable.title).toBe("{{source}}没有响应");
    // Each ends with the next step, which the notice's own Check again
    // carries out: said, not pointed at 「点按“重新检查”」 -- the only other
    // one on screen is the toolbar's ⟳, which has no words.
    for (const copy of [
      zhCN.sourceNotice.unreachable.description,
      zhCN.sourceNotice.unreachable.detail,
      zhCN.sourceNotice.unreachable.detailModels,
      zhCN.sourceNotice.unreachable.detailOwn,
      zhCN.sourceNotice.unreachable.descriptionWithRows_other,
      zhCN.sourceNotice.unreachable.descriptionWithModels_other,
      zhCN.sourceNotice.unreachable.descriptionWithOwnRow,
      zhCN.installed.sourceEmpty.unreachable,
    ]) {
      expect(copy.endsWith("请稍后重新检查。"), copy).toBe(true);
    }
    for (const copy of [
      en.sourceNotice.unreachable.description,
      en.sourceNotice.unreachable.detail,
      en.sourceNotice.unreachable.detailModels,
      en.sourceNotice.unreachable.detailOwn,
      en.sourceNotice.unreachable.descriptionWithRows_one,
      en.sourceNotice.unreachable.descriptionWithRows_other,
      en.sourceNotice.unreachable.descriptionWithModels_one,
      en.sourceNotice.unreachable.descriptionWithModels_other,
      en.sourceNotice.unreachable.descriptionWithOwnRow,
      en.installed.sourceEmpty.unreachable,
    ]) {
      expect(copy.endsWith("Check again later."), copy).toBe(true);
    }
  });

  it("never sends the user to press a Check Again from a notice that has one of its own", () => {
    for (const locale of [en, zhCN]) {
      for (const [key, value] of Object.entries(locale.sourceNotice)) {
        expect(JSON.stringify(value), key).not.toMatch(/点按“重新检查”|click Check Again|Click Check Again/);
      }
      expect(locale.installed.sourceEmpty.unreachable).not.toMatch(/点按“重新检查”|Click Check Again/);
    }
  });

  it("says what to do about a source that did not answer by why it did not, naming the source", () => {
    // "Check again later" is no help for an Ollama that is not running or
    // a Banager started with sudo.
    expect(UNAVAILABLE_DETAIL_KEYS).toEqual({
      NotRunning: "updates.unavailableDetail.NotRunning",
      NotResponding: "updates.unavailableDetail.NotResponding",
      RefusesAsRoot: "updates.unavailableDetail.RefusesAsRoot",
      HttpsHostRefused: "sourceNotice.httpsHostRefused.description",
      NoPip: "sourceNotice.noPip.description",
    });
    // Each says what to do with the button that does it: Check again.
    expect(en.updates.unavailableDetail.NotResponding).toBe(
      "{{source}} isn't responding. Click Check Again later.",
    );
    expect(zhCN.updates.unavailableDetail.NotResponding).toBe("{{source}}没有响应。请稍后点按“重新检查”。");
    for (const locale of [en, zhCN]) {
      for (const copy of Object.values(locale.updates.unavailableDetail)) {
        expect(copy).toContain("{{source}}");
      }
      expect(locale.updates.unavailableDetail.NotRunning).not.toMatch(/later|稍后/);
      expect(locale.updates.unavailableDetail.RefusesAsRoot).not.toMatch(/later|稍后/);
    }
    expect(en.updates.unavailableDetail.NotRunning).toBe("{{source}} isn't running. Open it, then click Check Again.");
    expect(zhCN.updates.unavailableDetail.NotRunning).toBe("{{source}}没有运行。请打开它，然后点按“重新检查”。");
    expect(en.updates.unavailableDetail.RefusesAsRoot).toMatch(/Quit, then open Banager again/);
    expect(zhCN.updates.unavailableDetail.RefusesAsRoot).toMatch(/请退出，然后连按两次重新打开/);
  });
});

describe("UNINSTALL_BLOCKED_KEYS", () => {
  it("builds the same unpin command the Updates page gives, from the owning brew", () => {
    // One builder for both refusals (`unpinCommand`), so a pinned
    // package's two rows cannot give two different commands.
    const brew = instance({
      id: "brew:/usr/local",
      exe_path: "/usr/local/bin/brew",
      prefix: "/usr/local",
    });
    for (const kind of ["Formula", "Cask"] as const) {
      const key = { instance_id: brew.id, kind, name: "onyx" } satisfies ArtifactKey;
      expect(UNINSTALL_BLOCKED_KEYS.Pinned.command(key, brew)).toBe(
        UPDATE_BLOCKED_KEYS.Pinned.command(key, brew),
      );
    }
    expect(
      UNINSTALL_BLOCKED_KEYS.Pinned.command(
        { instance_id: brew.id, kind: "Cask", name: "onyx" },
        brew,
      ),
    ).toBe("/usr/local/bin/brew unpin --cask onyx");
  });

  it("names the source and the command in both locales' sentences", () => {
    for (const copy of [
      en.installed.blocked.Pinned.description,
      en.installed.blocked.Pinned.refused,
      zhCN.installed.blocked.Pinned.description,
      zhCN.installed.blocked.Pinned.refused,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy.split("{{command}}")).toHaveLength(2);
    }
  });

  it("promises nothing in a pin's detail about when Uninstall comes back, so one sentence holds for a source that did not answer", () => {
    // A row carried forward from a Homebrew that did not answer has no
    // Uninstall button until Homebrew answers a check again, pinned or
    // not, so "the next time it checks, at the latest the next time you
    // start Banager" did not hold there, and needed a second sentence.
    // The detail says what stands in the way and what removes it.
    expect(en.installed.blocked.Pinned.description).toBe(
      "It's pinned in {{source}}. To uninstall it, first run {{command}} in Terminal.",
    );
    expect(zhCN.installed.blocked.Pinned.description).toBe(
      "它在{{source}}中固定了版本。要卸载，请先在终端运行{{command}}。",
    );
    for (const locale of [en, zhCN]) {
      expect(locale.installed.blocked.Pinned.description).not.toMatch(/next time|at the latest|下次|最晚|pin\)/);
    }
  });

  it("carries no command for a tool with no safe uninstall method: there is nothing to run first", () => {
    // Unlike a pin, nothing the user runs can make Banager able to
    // uninstall it; the sentence points at the tool's own instructions
    // and has no `{{command}}` slot, so `withCommand` renders it as plain
    // text and `InstalledPage` sets no `<code>`.
    const claude = instance({
      id: "standalone-claude",
      adapter_id: "standalone-claude",
      exe_path: "/Users/someone/.local/bin/claude",
    });
    const key: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.command(key, claude)).toBe("");
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.command(key, undefined)).toBe("");
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.badge).toBe("installed.blocked.NoSafeMethod.badge");
    expect(en.installed.blocked.NoSafeMethod.badge).toBe("Manual uninstall");
    expect(zhCN.installed.blocked.NoSafeMethod.badge).toBe("需手动卸载");
  });

  it("names the source and never a command in the no-safe-method sentences, in both locales", () => {
    for (const copy of [
      en.installed.blocked.NoSafeMethod.description,
      en.installed.blocked.NoSafeMethod.refused,
      zhCN.installed.blocked.NoSafeMethod.description,
      zhCN.installed.blocked.NoSafeMethod.refused,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy).not.toContain("{{command}}");
    }
    expect(en.adapters["standalone-claude"]).toBe("Claude Code");
    expect(zhCN.adapters["standalone-claude"]).toBe("Claude Code");
  });

  it("points at the tool's official documentation, not at a website Banager doesn't show, in the no-safe-method sentence", () => {
    // Banager shows no homepage and opens no link (the Tauri opener
    // paragraph in docs/what-we-run.md), so "its website" named nothing
    // the user could find from the row. The documentation, called by the
    // tool's own name, is something they can look up.
    expect(en.installed.blocked.NoSafeMethod.description).toContain("{{source}}'s official documentation");
    expect(zhCN.installed.blocked.NoSafeMethod.description).toContain("{{source}}的官方文档");
    for (const locale of [en, zhCN]) {
      expect(locale.installed.blocked.NoSafeMethod.description).not.toMatch(/website|网站/);
    }
  });

  it("says why a uv tool has no Uninstall while UV_TOOL_DIR is set, and hands over no command to copy", () => {
    // `UninstallBlocked::UvToolDirSet`: `uv tool uninstall` of the last
    // tool deletes the folder above the one UV_TOOL_DIR names when that
    // holds no other folder. The command that would do it is not set
    // apart to be copied. Every uv tool carries the reason
    // (`UvAdapter::uninstall_blocked`), the last or not, so the copy says
    // Banager uninstalls none, not that this one is the last.
    const uv = instance({ id: "uv", adapter_id: "uv", exe_path: "/opt/homebrew/bin/uv" });
    const key: ArtifactKey = { instance_id: "uv", kind: "Tool", name: "ruff" };
    expect(UNINSTALL_BLOCKED_KEYS.UvToolDirSet.command(key, uv)).toBe("");
    expect(UNINSTALL_BLOCKED_KEYS.UvToolDirSet.badge).toBe("installed.blocked.UvToolDirSet.badge");
    expect(uninstallBlockedCopy("UvToolDirSet", "uv")).toBe(UNINSTALL_BLOCKED_KEYS.UvToolDirSet);
    expect(en.installed.blocked.UvToolDirSet.badge).toBe("Can't uninstall here");
    expect(zhCN.installed.blocked.UvToolDirSet.badge).toBe("无法在此卸载");
    expect([...zhCN.installed.blocked.UvToolDirSet.badge].length).toBeLessThanOrEqual(6);
    for (const locale of [en, zhCN]) {
      for (const copy of [locale.installed.blocked.UvToolDirSet.description, locale.installed.blocked.UvToolDirSet.refused]) {
        expect(copy).toContain("UV_TOOL_DIR");
        expect(copy).not.toContain("{{command}}");
        // Running the same `uv tool uninstall` in Terminal carries the same
        // risk, so the copy sends no one there; and it states uv's rule,
        // read in uv 0.12.17's source, without hedging.
        expect(copy).not.toMatch(/Terminal|终端/);
        expect(copy).not.toMatch(/\bmay\b|\bmight\b|可能/);
      }
    }
    // The refusal stops before anything runs, so the polish-3 copy rules
    // (规则 2) say what did not happen once, as "Couldn't uninstall" and
    // 无法卸载, with no "nothing changed" or 没有改动.
    expect(en.installed.blocked.UvToolDirSet.refused).toBe("Couldn't uninstall because UV_TOOL_DIR is set.");
    expect(zhCN.installed.blocked.UvToolDirSet.refused).toBe("无法卸载，因为设置了UV_TOOL_DIR。");
    // uv's rule, said once: uv checks the tools folder for another tool's
    // folder first (`crates/uv/src/commands/tool/uninstall.rs:40-52`), so
    // "its last tool" is the whole condition. Then that Banager uninstalls
    // none. The refusal says what happened in one sentence, as the other
    // reasons' refusals do.
    expect(en.installed.blocked.UvToolDirSet.description).toContain("when uv uninstalls its last tool");
    expect(en.installed.blocked.UvToolDirSet.description).toContain("and everything in it");
    expect(en.installed.blocked.UvToolDirSet.description).toContain("So no uv tool can be uninstalled here while it's set.");
    expect(zhCN.installed.blocked.UvToolDirSet.description).toContain("uv卸载最后一个工具");
    expect(zhCN.installed.blocked.UvToolDirSet.description).toContain("及其中的所有文件");
    expect(zhCN.installed.blocked.UvToolDirSet.description).toContain("无法在这里卸载任何uv工具");
    for (const copy of [en.installed.blocked.UvToolDirSet.description, en.installed.blocked.UvToolDirSet.refused]) {
      expect(copy).not.toContain("uninstalling the last uv tool");
    }
    for (const copy of [zhCN.installed.blocked.UvToolDirSet.description, zhCN.installed.blocked.UvToolDirSet.refused]) {
      expect(copy).not.toMatch(/卸载最后一个\s*uv\s*工具/);
    }
  });
});

describe("parseUninstallBlocked", () => {
  it("reads the reason out of the uninstall gate's payload and nothing else", () => {
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"Pinned"}')).toBe("Pinned");
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"NoSafeMethod"}')).toBe(
      "NoSafeMethod",
    );
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"UvToolDirSet"}')).toBe(
      "UvToolDirSet",
    );
    // The upgrade gate's payload is a different refusal with different copy.
    expect(parseUninstallBlocked('{"kind":"update_blocked","reason":"Pinned"}')).toBeNull();
    // A reason this build has no copy for is not guessed at.
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"Held"}')).toBeNull();
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"toString"}')).toBeNull();
    expect(parseUninstallBlocked("not json")).toBeNull();
  });
});

describe("parseUninstallUnsafe", () => {
  it("reads the path and the reason out of the preview's refusal and nothing else", () => {
    expect(
      parseUninstallUnsafe(
        '{"kind":"uninstall_unsafe","path":"~/.claude/downloads","reason":"not_owned_by_you"}',
      ),
    ).toEqual({ path: "~/.claude/downloads", reason: "not_owned_by_you" });
    // The gate's own refusal is a different payload with different copy.
    expect(parseUninstallUnsafe('{"kind":"uninstall_blocked","reason":"Pinned"}')).toBeNull();
    // A reason this build has no copy for is not guessed at; nor is a
    // prototype property, nor a path that is not a string.
    expect(parseUninstallUnsafe('{"kind":"uninstall_unsafe","path":"~/x","reason":"toString"}')).toBeNull();
    expect(parseUninstallUnsafe('{"kind":"uninstall_unsafe","path":7,"reason":"missing"}')).toBeNull();
    expect(parseUninstallUnsafe("not json")).toBeNull();
  });
});

describe("STANDALONE_SUMMARY_KEYS", () => {
  it("gives each standalone tool a sentence and every other source none", () => {
    // A standalone artifact's `description` is `null` on the wire (a bare
    // string could not be localised), so both pages find the line's key
    // here by adapter id; every package manager's row with no description
    // gets its source's line instead (`toolDescription`).
    expect(standaloneSummaryKey("standalone-claude")).toBe("standalone.summary.standalone-claude");
    expect(standaloneSummaryKey("standalone-agy")).toBe("standalone.summary.standalone-agy");
    expect(standaloneSummaryKey("standalone-grok")).toBe("standalone.summary.standalone-grok");
    for (const id of ["brew", "npm", "pipx", "uv", "pip", "cargo", "ollama", "toString", ""]) {
      expect(standaloneSummaryKey(id)).toBeNull();
    }
  });

  it("says in one line what each tool is, in both locales, and nothing about how it was installed", () => {
    // The row's avatar and name are the tool's own -- a tool with its own
    // installer is its own source -- so "installed with its own
    // installer, not with Homebrew or npm" said again what the row shows,
    // and took a second sentence to do it
    // (docs/superpowers/2026-09-27-ui-redesign.md, 原则 1).
    expect(en.standalone.summary["standalone-claude"]).toBe("Anthropic's AI coding assistant");
    expect(zhCN.standalone.summary["standalone-claude"]).toBe("Anthropic的AI编程助手");
    for (const summary of [...Object.values(en.standalone.summary), ...Object.values(zhCN.standalone.summary)]) {
      expect(summary).not.toMatch(/installer|Homebrew|npm|安装器|[.。]/);
    }
  });

  it("gives rustup its sentence and its label, in both locales", () => {
    expect(standaloneSummaryKey("standalone-rustup")).toBe("standalone.summary.standalone-rustup");
    expect(ADAPTER_LABEL_KEYS["standalone-rustup"]).toBe("adapters.standalone-rustup");
    // What it does, without "toolchain manager".
    expect(en.standalone.summary["standalone-rustup"]).toBe("Installs and updates Rust");
    expect(zhCN.standalone.summary["standalone-rustup"]).toBe("安装和更新Rust的工具");
    expect(en.adapters["standalone-rustup"]).toBe("rustup");
    expect(zhCN.adapters["standalone-rustup"]).toBe("rustup");
  });

  it("labels Antigravity CLI and Grok Build by their product names alone, in both locales", () => {
    // Spec §9.2 put the command in parentheses after the name ("Grok Build
    // (grok)"). The redesign drops parenthetical asides from every page
    // (docs/superpowers/2026-09-27-ui-redesign.md, 原则 3), and the label is
    // a name -- on the Overview's tiles, a row's source chip, a notice's
    // title -- so it is the product's alone. The command is said where
    // typing it matters: the PATH and launcher notices' `{{command}}`, and
    // under a self-updating tool's detail with technical details on.
    expect(en.adapters["standalone-agy"]).toBe("Antigravity CLI");
    expect(zhCN.adapters["standalone-agy"]).toBe("Antigravity CLI");
    expect(en.adapters["standalone-grok"]).toBe("Grok Build");
    expect(zhCN.adapters["standalone-grok"]).toBe("Grok Build");
    for (const label of [...Object.values(en.adapters), ...Object.values(zhCN.adapters)]) {
      expect(label).not.toMatch(/[()（）]/);
    }
    expect(ADAPTER_LABEL_KEYS["standalone-agy"]).toBe("adapters.standalone-agy");
    expect(ADAPTER_LABEL_KEYS["standalone-grok"]).toBe("adapters.standalone-grok");
  });

  it("has the two AI CLIs' lines in both locales, naming the publisher", () => {
    expect(en.standalone.summary["standalone-agy"]).toBe("Google's AI coding assistant");
    expect(zhCN.standalone.summary["standalone-agy"]).toBe("Google的AI编程助手");
    expect(en.standalone.summary["standalone-grok"]).toBe("xAI's AI coding assistant");
    expect(zhCN.standalone.summary["standalone-grok"]).toBe("xAI的AI编程助手");
  });
});

describe("toolDescription", () => {
  const tool = (over: Partial<DescribedTool> = {}): DescribedTool => ({
    description: null,
    kind: "Formula",
    path: null,
    ...over,
  });

  it("gives the source's own description first, then a standalone tool's summary", () => {
    // The one lookup both pages' rows read, so the Updates row of a tool
    // with no description of its own says what its Installed row says.
    expect(toolDescription(fakeT, tool({ description: "Verify system files structure" }), "brew", "Homebrew")).toBe(
      "Verify system files structure",
    );
    expect(toolDescription(fakeT, tool({ kind: "Binary" }), "standalone-grok", "Grok Build")).toBe(
      "standalone.summary.standalone-grok",
    );
    expect(
      toolDescription(fakeT, tool({ kind: "Binary", description: undefined }), "standalone-rustup", "rustup"),
    ).toBe("standalone.summary.standalone-rustup");
  });

  it("says what the source says a tool is when the source gave no description, never that there is none", () => {
    // npm's, pip's, pipx's, uv's, Cargo's and Ollama's inventories never
    // carry a description, and some of Homebrew's casks have none: every
    // one of those rows used to read 「暂无简介」/"No description".
    const cases: Array<[string, DescribedTool, string]> = [
      ["brew", tool({ kind: "Formula" }), "toolRow.fallback.homebrewPackage"],
      // A cask with an app is an app...
      ["brew", tool({ kind: "Cask", path: "/Applications/iTerm.app" }), "toolRow.fallback.homebrewApp"],
      // ...and one without (a font, a driver) is not called one.
      ["brew", tool({ kind: "Cask", path: null }), "toolRow.fallback.homebrewPackage"],
      ["npm", tool({ kind: "Package" }), "toolRow.fallback.npmPackage"],
      ["pip", tool({ kind: "Package" }), "toolRow.fallback.pythonPackage"],
      ["pipx", tool({ kind: "Tool" }), "toolRow.fallback.pipxTool"],
      ["uv", tool({ kind: "Tool" }), "toolRow.fallback.uvTool"],
      ["cargo", tool({ kind: "Binary", path: "/Users/you/.cargo/bin/tokei" }), "toolRow.fallback.cargoProgram"],
      ["ollama", tool({ kind: "Model" }), "toolRow.fallback.ollamaModel"],
      // An empty description is none.
      ["npm", tool({ kind: "Package", description: "" }), "toolRow.fallback.npmPackage"],
    ];
    for (const [adapterId, described, key] of cases) {
      expect(toolDescription(fakeT, described, adapterId, "label"), adapterId).toBe(key);
    }
    // A source this build has no line for says who installed it; a
    // prototype property is not a key.
    expect(toolDescription(fakeT, tool({ kind: "Package" }), "gem", "RubyGems")).toBe(
      'toolRow.fallback.other({"source":"RubyGems"})',
    );
    expect(toolDescription(fakeT, tool(), "toString", "toString")).toBe(
      'toolRow.fallback.other({"source":"toString"})',
    );
  });

  it("only says of a source what holds for everything it lists, in both locales", () => {
    // npm lists npm and corepack, which come with Node, and packages with
    // no command at all; pip lists what came with Python; Ollama lists
    // cloud models. So none of those lines says "installed with", "command
    // line" or "local"; the three whose sources list only what they
    // installed themselves say so.
    for (const locale of [en, zhCN]) {
      const lines = locale.toolRow.fallback;
      for (const key of ["homebrewPackage", "npmPackage", "pythonPackage", "ollamaModel"] as const) {
        expect(lines[key]).not.toMatch(/installed with|command|local|安装|命令行|本地/i);
      }
      for (const key of ["pipxTool", "uvTool", "cargoProgram", "homebrewApp"] as const) {
        expect(lines[key]).toMatch(/installed with|用.+安装/);
      }
      expect(lines.other).toContain("{{source}}");
    }
    expect(en.toolRow.fallback.ollamaModel).toBe("Ollama model");
    expect(zhCN.toolRow.fallback.ollamaModel).toBe("Ollama模型");
    expect(zhCN.toolRow.fallback.homebrewApp).toBe("用Homebrew安装的App");
    expect(zhCN.toolRow.fallback.cargoProgram).toBe("用Cargo安装的程序");
  });

  // A Homebrew formula with a line in the table, as a window in Chinese
  // hands it to a row (`useTranslatedDescription`).
  const GIT = "Distributed revision control system";
  const GIT_ZH = "分布式版本控制系统";

  it("gives the tool's line in the window's language before its source's words, and a fallback's", () => {
    expect(toolDescription(fakeT, tool({ description: GIT, translated: GIT_ZH }), "brew", "Homebrew")).toBe(GIT_ZH);
    // npm's inventory gives no description: the line, not "npm package".
    const prettier = tool({ kind: "Package", translated: "代码格式化工具" });
    expect(toolDescription(fakeT, prettier, "npm", "npm")).toBe("代码格式化工具");
    // None, or an empty one: what the row said without it.
    for (const translated of [null, undefined, ""]) {
      expect(toolDescription(fakeT, tool({ description: GIT, translated }), "brew", "Homebrew")).toBe(GIT);
      expect(toolDescription(fakeT, tool({ kind: "Package", translated }), "npm", "npm")).toBe(
        "toolRow.fallback.npmPackage",
      );
    }
  });

  it("keeps a standalone tool's summary, which is in the window's language already", () => {
    const claude = tool({ kind: "Binary", translated: "某个翻译" });
    expect(toolDescription(fakeT, claude, "standalone-claude", "Claude Code")).toBe(
      "standalone.summary.standalone-claude",
    );
  });
});

describe("toolDescription, with a line in the window's language", () => {
  const tool = (over: Partial<DescribedTool> = {}): DescribedTool => ({
    description: null,
    kind: "Formula",
    path: null,
    ...over,
  });
  const GIT = "Distributed revision control system";
  const GIT_ZH = "分布式版本控制系统";

  it("gives the window's line in place of the source's own words, and nothing of those beside it", () => {
    // One line: a Chinese window that says a tool's line in Chinese and
    // then the source's English under it reads as half translated.
    expect(toolDescription(fakeT, tool({ description: GIT, translated: GIT_ZH }), "brew", "Homebrew")).toBe(GIT_ZH);
  });

  it("gives the source's own words where there is no line in the window's language", () => {
    // In English, or with no line in the table: the source's words alone.
    expect(toolDescription(fakeT, tool({ description: GIT }), "brew", "Homebrew")).toBe(GIT);
    expect(toolDescription(fakeT, tool({ description: GIT, translated: "" }), "brew", "Homebrew")).toBe(GIT);
    // A line from a registry's description, where the inventory gave none.
    const prettier = tool({ kind: "Package", translated: "代码格式化工具" });
    expect(toolDescription(fakeT, prettier, "npm", "npm")).toBe("代码格式化工具");
    // A fallback, and a standalone tool's summary, which no line replaces.
    expect(toolDescription(fakeT, tool({ kind: "Package" }), "npm", "npm")).toBe("toolRow.fallback.npmPackage");
    const grok = tool({ kind: "Binary", translated: "某个翻译" });
    expect(toolDescription(fakeT, grok, "standalone-grok", "Grok Build")).toBe("standalone.summary.standalone-grok");
  });
});

describe("uninstallBlockedCopy", () => {
  it("gives rustup's row its own reason for NoSafeMethod, and every other row B's", () => {
    // The rustup recipe's gate puts `NoSafeMethod` on the artifact when
    // Rust is not in its standard folders (crates/banager-core/src/
    // adapters/standalone/rustup.rs, `uninstall_blocked`); B's sentence
    // for that variant says the tool has no uninstall command, which is
    // false for rustup. The badge stays; the two sentences are rustup's.
    const rustup = uninstallBlockedCopy("NoSafeMethod", "standalone-rustup");
    expect(rustup.badge).toBe(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.badge);
    expect(rustup.description).toBe("installed.blocked.NoSafeMethod.standalone-rustup.description");
    expect(rustup.refused).toBe("installed.blocked.NoSafeMethod.standalone-rustup.refused");
    expect(
      rustup.command({ instance_id: "standalone-rustup", kind: "Binary", name: "rustup" }, undefined),
    ).toBe("");
    // "Isn't entirely in", not "keeps it somewhere else": the gate also
    // refuses when only a part is elsewhere -- a link at the top of either
    // folder -- and the variables' names are not this audience's words.
    expect(en.installed.blocked.NoSafeMethod["standalone-rustup"].description).toBe(
      "Rust on this Mac isn't entirely in ~/.cargo and ~/.rustup, and only Rust in those folders can be uninstalled here. Follow rustup's official documentation and run “rustup self uninstall”.",
    );
    expect(zhCN.installed.blocked.NoSafeMethod["standalone-rustup"].description).toBe(
      "这台Mac上的Rust不全在~/.cargo和~/.rustup中，而这里只卸载这两个位置的Rust。请按照rustup的官方文档运行“rustup self uninstall”。",
    );
    // Everyone else: B's copy, whatever the adapter.
    expect(uninstallBlockedCopy("NoSafeMethod", "standalone-claude")).toBe(
      UNINSTALL_BLOCKED_KEYS.NoSafeMethod,
    );
    expect(uninstallBlockedCopy("NoSafeMethod", undefined)).toBe(UNINSTALL_BLOCKED_KEYS.NoSafeMethod);
    expect(uninstallBlockedCopy("Pinned", "standalone-rustup")).toBe(UNINSTALL_BLOCKED_KEYS.Pinned);
    expect(uninstallBlockedCopy("Pinned", "brew")).toBe(UNINSTALL_BLOCKED_KEYS.Pinned);
  });
});
