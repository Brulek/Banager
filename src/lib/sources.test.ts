import { describe, expect, it } from "vitest";
import {
  ADAPTER_LABEL_KEYS,
  artifactBlurb,
  canWrite,
  failedSourceCount,
  hasSourceNotice,
  isAvailable,
  notActionableMessage,
  openOllamaErrorMessage,
  parseNotActionable,
  parseOpenOllamaFailure,
  parseUninstallBlocked,
  parseUninstallUnsafe,
  planErrorMessage,
  settingsSaveErrorMessage,
  sourceNoticesFor,
  standaloneSummaryKey,
  UNINSTALL_BLOCKED_KEYS,
  uninstallBlockedCopy,
  UPDATE_BLOCKED_KEYS,
} from "./sources";
import type { ArtifactKey, ManagerInstance, SourceError } from "./types";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";

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

  it("gives each read-only reason its own copy", () => {
    const [pip] = sourceNoticesFor(
      instance({ adapter_id: "pip", read_only_reason: "ByDesign" }),
      "pip",
    );
    expect(pip.axis).toBe("capability");
    expect(pip.variant).toBe("info");
    expect(pip.titleKey).toBe("sourceNotice.pipReadOnly.title");

    const [npm] = sourceNoticesFor(
      instance({ adapter_id: "npm", read_only_reason: "PrefixNotWritable" }),
      "npm",
    );
    // Not pip's advice. Telling someone whose npm prefix is root-owned to
    // install a Python tool is worse than saying nothing.
    expect(npm.titleKey).toBe("sourceNotice.prefixNotWritable.title");
    expect(npm.descriptionKey).toBe("sourceNotice.prefixNotWritable.description");
  });

  it("offers to start Ollama, and only Ollama, when a source is not running", () => {
    // One state, one sentence: every source that is not running gets the
    // same copy, named in the user's language. Ollama is the only one
    // Canager can start, so it is the only one whose notice also carries a
    // button -- a second wording for the same state is what let a stopped
    // Ollama read one way in its notice and another in its plan refusal.
    const [ollama] = sourceNoticesFor(
      instance({ adapter_id: "ollama", status: { unavailable: "NotRunning", notes: [] } }),
      "Ollama",
    );
    expect(ollama.axis).toBe("state");
    expect(ollama.variant).toBe("warning");
    expect(ollama.titleKey).toBe("sourceNotice.notRunning.title");
    expect(ollama.descriptionKey).toBe("sourceNotice.notRunning.description");
    expect(ollama.values).toEqual({ source: "Ollama" });
    expect(ollama.action).toEqual({ id: "openOllama", labelKey: "sourceNotice.openOllama" });

    // Any other source that reports NotRunning gets the same copy and no
    // button: Canager has no way to start it.
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

  it("tells a root user to reopen Canager rather than that Homebrew is missing", () => {
    // Launched with `sudo`, Homebrew refuses to run, so `BrewAdapter::detect`
    // reports the install it found as `RefusesAsRoot` instead of reporting
    // nothing. Its own copy, because the action is its own: not "start it"
    // and not "reinstall it", but quit and open Canager again normally.
    const [notice] = sourceNoticesFor(
      instance({ status: { unavailable: "RefusesAsRoot", notes: [] } }),
      "Homebrew",
    );
    expect(notice.axis).toBe("state");
    expect(notice.variant).toBe("warning");
    expect(notice.titleKey).toBe("sourceNotice.refusesAsRoot.title");
    expect(notice.descriptionKey).toBe("sourceNotice.refusesAsRoot.description");
    expect(notice.values).toEqual({ source: "Homebrew" });
    // Canager cannot relaunch itself out from under sudo, so no button
    // pretends it can.
    expect(notice.action).toBeUndefined();
  });

  it("says a source that would not answer is showing last time's data", () => {
    const [notice] = sourceNoticesFor(
      instance({ status: { unavailable: "NotResponding", notes: [] } }),
      "Homebrew",
      4,
    );
    expect(notice.axis).toBe("state");
    expect(notice.titleKey).toBe("sourceNotice.unreachable.title");
    expect(notice.descriptionKey).toBe("sourceNotice.unreachable.descriptionWithRows");
    expect(notice.values).toEqual({ source: "Homebrew" });
  });

  it("does not promise carried-forward rows when the page has none to show", () => {
    // The snapshot is in memory only -- `Session::new` starts from
    // `Snapshot::empty()` and nothing is persisted -- so on the first
    // refresh after every launch there is nothing to carry forward. A
    // source whose CLI simply fails (cargo, when `cargo --version` does)
    // hits this on every single launch, and the notice used to say
    // "Below is what Canager saw last time" over an empty group.
    const [notice] = sourceNoticesFor(
      instance({ status: { unavailable: "NotResponding", notes: [] } }),
      "Homebrew",
      0,
    );
    expect(notice.descriptionKey).toBe("sourceNotice.unreachable.description");
    // And the default is the copy that claims nothing: a caller that does
    // not know how many rows it is about to draw must not promise any.
    expect(
      sourceNoticesFor(instance({ status: { unavailable: "NotResponding", notes: [] } }), "x")[0]
        .descriptionKey,
    ).toBe("sourceNotice.unreachable.description");
  });

  it("warns that a stale index makes up-to-date unreliable, with a way to retry", () => {
    const [note] = sourceNoticesFor(
      instance({ status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
      "Homebrew",
    );
    expect(note.axis).toBe("state");
    expect(note.titleKey).toBe("sourceNotice.indexMayBeStale.title");
    expect(note.action?.id).toBe("retry");
  });

  it("says a still-running download is still running: no failure, no button", () => {
    // The download has not failed, so the copy must not say it did or send
    // the user off to check their connection, and there is nothing to
    // retry: the core refreshes by itself when the download ends.
    const [note] = sourceNoticesFor(
      instance({ status: { unavailable: null, notes: ["IndexUpdating"] } }),
      "Homebrew",
    );
    expect(note.axis).toBe("state");
    expect(note.variant).toBe("info");
    expect(note.titleKey).toBe("sourceNotice.indexUpdating.title");
    expect(note.descriptionKey).toBe("sourceNotice.indexUpdating.description");
    expect(note.action).toBeUndefined();
  });

  it("carries both axes at once, capability first", () => {
    // Independent axes: a source can be read-only *and* silent, and each
    // half is something different for the user to do.
    const notices = sourceNoticesFor(
      instance({
        adapter_id: "pip",
        read_only_reason: "ByDesign",
        status: { unavailable: "NotResponding", notes: ["IndexMayBeStale"] },
      }),
      "pip",
    );
    expect(notices.map((n) => n.axis)).toEqual(["capability", "state", "state"]);
    expect(new Set(notices.map((n) => n.id)).size).toBe(3);
  });

  it("is the one rule hasSourceNotice answers from", () => {
    // SnapshotStatus's "Nothing installed yet" gate and the pages' group
    // headers must never disagree about which sources have something to
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
          axis: "state",
          variant: "info",
          titleKey: `${key}.title`,
          descriptionKey: `${key}.description`,
          values: { source: "Claude Code", command: "claude" },
        },
      ]);
    }
  });

  it("warns, and names the link, when a standalone tool's launcher is left without its program", () => {
    // The half-uninstalled state (program files gone, launcher dangling):
    // a warning because this launcher is broken; another PATH copy may
    // work. No button on the notice: the row's own Uninstall moves the
    // link, which the sentence says.
    const notices = sourceNoticesFor(
      { ...claude, status: { unavailable: null, notes: ["LauncherOnly"] } },
      "Claude Code",
    );
    expect(notices).toEqual([
      {
        id: "standalone-claude:launcher-only",
        axis: "state",
        variant: "warning",
        titleKey: "sourceNotice.launcherOnly.title",
        descriptionKey: "sourceNotice.launcherOnly.description",
        values: { source: "Claude Code", command: "claude" },
      },
    ]);
  });

  it("falls back to the whole exe_path as the command when it has no file name", () => {
    const notices = sourceNoticesFor(
      { ...claude, exe_path: "/", status: { unavailable: null, notes: ["NotOnPath"] } },
      "Claude Code",
    );
    expect(notices[0].values).toEqual({ source: "Claude Code", command: "/" });
  });

  it("puts the command and the source into every standalone notice's copy, in both locales", () => {
    for (const locale of [en, zhCN]) {
      for (const key of [
        "notOnPath",
        "shadowedByHomebrew",
        "shadowedByNpm",
        "shadowedByOther",
        "launcherOnly",
      ] as const) {
        expect(locale.sourceNotice[key].description).toContain("{{command}}");
      }
      // The three "another program with that name runs" notices share one
      // title, and two of the descriptions say whose directory it is in.
      expect(locale.sourceNotice.shadowedByNpm.title).toBe(locale.sourceNotice.shadowedByHomebrew.title);
      expect(locale.sourceNotice.shadowedByOther.title).toBe(locale.sourceNotice.shadowedByHomebrew.title);
      expect(locale.sourceNotice.shadowedByHomebrew.description).toContain("Homebrew");
      expect(locale.sourceNotice.shadowedByNpm.description).toContain("npm");
      expect(locale.sourceNotice.launcherOnly.description).toContain("{{source}}");
      // The LauncherOnly row offers Uninstall (its artifact carries no
      // `uninstall_blocked` since step C), so spec §9.2's promises are
      // back: the link goes to the Trash too, and a folder an earlier
      // stopped uninstall moved may be in the Trash -- "may", as spec §9.2
      // says: the Trash can have been emptied since. It still does not claim
      // typing the command fails -- another copy on PATH may run (B's
      // review finding 9).
      expect(locale.sourceNotice.launcherOnly.description).toMatch(
        /Uninstall moves the link to the Trash|卸载会把这个链接也移到废纸篓/,
      );
      expect(locale.sourceNotice.launcherOnly.description).toMatch(/may be in the Trash|可能在废纸篓里/);
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(
        /typing .* in Terminal fails|输入 .* 会失败/,
      );
      for (const key of ["shadowedByHomebrew", "shadowedByNpm"] as const) {
        expect(locale.sourceNotice[key].description).not.toMatch(/both are listed on this page|两份在这一页上都能找到/);
      }
    }
  });

  it("has the not-on-PATH notice say typing the name won't find this copy, and that Terminal then finds nothing or runs another program with that name, in both locales", () => {
    // NotOnPath is also the note when another executable with the tool's
    // name is on PATH and this copy is not (route::shadow_note): typing the
    // name then runs that other program, which may or may not be another
    // copy of the tool. So the sentence says it is this copy that won't be
    // found, and names both outcomes, instead of reading as "nothing runs".
    expect(en.sourceNotice.notOnPath.description).toContain("probably won't find this copy");
    expect(en.sourceNotice.notOnPath.description).toContain(
      "either finds nothing or runs another program named {{command}}",
    );
    expect(zhCN.sourceNotice.notOnPath.description).toContain("多半找不到这一份");
    expect(zhCN.sourceNotice.notOnPath.description).toContain("要么什么也找不到，要么运行的是另一个同名程序");
  });

  it("has the not-on-PATH notice say no executable entry on PATH reaches this copy, and give the missing folder as the likely cause, in both locales", () => {
    // route::shadow_note answers NotOnPath whenever no executable
    // `command` on PATH resolves to this copy -- also when the launcher's
    // folder is on PATH but the file it links to has no executable bit
    // (route.rs, test_shadow_note_says_not_on_path_when_the_launcher_is_on_path_but_its_target_is_not_executable).
    // So the sentence says what was checked, and gives the folder's absence
    // as the likely cause rather than stating it as the cause (step-B
    // review finding B-5).
    expect(en.sourceNotice.notOnPath.description).toContain(
      "no executable {{command}} in your shell's search path (PATH) leads to it",
    );
    expect(en.sourceNotice.notOnPath.description).toContain("Most likely the folder it lives in isn't in PATH");
    expect(en.sourceNotice.notOnPath.description).not.toMatch(/this copy: the folder/);
    expect(zhCN.sourceNotice.notOnPath.description).toContain("没有一个可执行的 {{command}} 通向这一份");
    expect(zhCN.sourceNotice.notOnPath.description).toContain("最可能的原因是它所在的文件夹不在 PATH 里");
    expect(zhCN.sourceNotice.notOnPath.description).not.toMatch(/这一份：它所在的文件夹/);
  });

  it("calls what PATH finds instead another program with the tool's name, never another copy, in both locales: it may only share the name", () => {
    // Step D's review: route::shadow_note compares the name the user types
    // and where the first executable of that name resolves -- a Homebrew
    // directory, an npm one, or anywhere else -- never what program it is.
    // Homebrew's formula `grok`, a regular-expression tool, gets the very
    // note that the `grok` of Homebrew's cask `grok-build`, which is Grok
    // Build, gets; npm's package `grok-cli`, a third-party wrapper, puts a
    // `grok` on PATH that is not Grok Build either (route.rs,
    // test_shadow_note_classifies_by_where_the_first_one_resolves_not_by_what_program_it_is).
    // So the three notices name another program with the same name, say it
    // may be another copy or a different program, and none of the four
    // PATH notices calls it a copy.
    for (const key of ["shadowedByHomebrew", "shadowedByNpm", "shadowedByOther"] as const) {
      expect(en.sourceNotice[key].title).toBe("Another program named {{command}} runs when you type {{command}}");
      expect(zhCN.sourceNotice[key].title).toBe("输入 {{command}} 时运行的是另一个同名程序");
      expect(en.sourceNotice[key].description).toContain(
        "It may be another copy of {{source}}, or a different program that happens to have the same name.",
      );
      expect(zhCN.sourceNotice[key].description).toContain(
        "它可能是 {{source}} 的另一份安装，也可能只是碰巧同名的另一个程序。",
      );
    }
    for (const key of ["notOnPath", "shadowedByHomebrew", "shadowedByNpm", "shadowedByOther"] as const) {
      expect(en.sourceNotice[key].title).not.toMatch(/copy/i);
      expect(en.sourceNotice[key].description).not.toMatch(/runs? (that other|another) copy/);
      expect(zhCN.sourceNotice[key].title).not.toMatch(/另一份/);
      expect(zhCN.sourceNotice[key].description).not.toMatch(/运行的是另一份/);
    }
  });

  it("points at the tool's official documentation, not at a website Canager doesn't show, when the launcher is left without its program", () => {
    // The same rule as the no-safe-method sentence (the row of a recipe
    // without an uninstall method; this row's own before step C): Canager shows
    // no homepage and opens no link, so "its website" and "the same page"
    // named nothing the user could find from here.
    expect(en.sourceNotice.launcherOnly.description).toContain("{{source}}'s official documentation");
    expect(zhCN.sourceNotice.launcherOnly.description).toContain("{{source}} 官方文档");
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(/website|same page|网站|同一页/);
    }
  });

  it("does not call the launcher all that is left, in both locales: grok's agent link can dangle beside it", () => {
    // The whole-step review of step D: in Grok Build's launcher-only state
    // `~/.grok/bin/agent` dangles beside `~/.grok/bin/grok`, and a stopped
    // uninstall can leave other listed paths as well, so "Only the grok
    // link is left" was untrue. The title says what holds for every
    // launcher-only row: this link is still there, and what it points to
    // is not.
    expect(en.sourceNotice.launcherOnly.title).toBe(
      "The {{command}} link is still there, but its program is gone",
    );
    expect(zhCN.sourceNotice.launcherOnly.title).toBe("{{command}} 这个链接还在，但它指向的程序已经不在了");
    for (const locale of [en, zhCN]) {
      expect(locale.sourceNotice.launcherOnly.title).not.toMatch(/only|只剩/i);
    }
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
      ),
    ).toBe("sourceNotice.prefixNotWritable.description");
  });

  it("shows a string that is not a structured payload verbatim, rather than swallowing it", () => {
    expect(planErrorMessage(fakeT, "unknown instance fake:1", "npm")).toBe(
      "unknown instance fake:1",
    );
    // A kind this build does not know is not guessed at either.
    expect(planErrorMessage(fakeT, '{"kind":"toString"}', "npm")).toBe('{"kind":"toString"}');
  });

  it("words each of Canager's own planning failures itself, naming the source", () => {
    for (const [kind, key] of [
      ["output_too_large", "planRefused.outputTooLarge"],
      ["index_updating", "planRefused.indexUpdating"],
      ["refused", "planRefused.refused"],
    ]) {
      expect(planErrorMessage(fakeT, JSON.stringify({ kind }), "Homebrew")).toBe(
        `${key}({"source":"Homebrew"})`,
      );
    }
  });

  it("interpolates the data an invalid name or a missing program carries", () => {
    expect(planErrorMessage(fakeT, '{"kind":"invalid_name","name":"-rf"}', "npm")).toBe(
      'planRefused.invalidName({"name":"-rf","source":"npm"})',
    );
    expect(
      planErrorMessage(
        fakeT,
        '{"kind":"program_missing","program":"/opt/homebrew/bin/brew"}',
        "Homebrew",
      ),
    ).toBe('planRefused.programMissing({"program":"/opt/homebrew/bin/brew"})');
  });

  it("quotes the system's reason a tool could not start inside a translated sentence", () => {
    expect(
      planErrorMessage(fakeT, '{"kind":"spawn_failed","detail":"Permission denied (os error 13)"}', "npm"),
    ).toBe('planRefused.spawnFailed({"source":"npm","detail":"Permission denied (os error 13)"})');
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
        ),
      ).toBe(`${key}({"path":"~/.local/bin/claude"})`);
    }
    // A reason this build has no copy for, or a payload without its path,
    // is shown verbatim rather than guessed at.
    const unknown = '{"kind":"uninstall_unsafe","path":"~/x","reason":"cursed"}';
    expect(planErrorMessage(fakeT, unknown, "Claude Code")).toBe(unknown);
    const pathless = '{"kind":"uninstall_unsafe","reason":"missing"}';
    expect(planErrorMessage(fakeT, pathless, "Claude Code")).toBe(pathless);
  });

  it("refuses a path it can't confirm without citing official instructions, which Antigravity CLI and Grok Build don't publish, in both locales", () => {
    // `not_what_instructions_expect` is what check 4 and the ancestry rule
    // (`removal::check_item`) answer on every path-list uninstall. Claude
    // Code's list is built from Anthropic's removal steps, but Antigravity
    // CLI and Grok Build publish none: their lists are Canager's own
    // reading of how each was installed (`recipes::AGY`, `recipes::GROK`
    // and their fixture READMEs). An agy launcher in a `~/.local/bin` that
    // is a link to a dotfiles folder inside the home folder, or a grok
    // `~/.grok/downloads` that is a link to another disk, gets this
    // sentence, so it says what Canager expects rather than what "the
    // official instructions" describe.
    const refusal = {
      en: en.planRefused.uninstallUnsafe.notWhatInstructionsExpect,
      zhCN: zhCN.planRefused.uninstallUnsafe.notWhatInstructionsExpect,
    };
    expect(refusal.en).toContain("it couldn't confirm this is what it expects to find there for this tool");
    expect(refusal.zhCN).toContain("它无法确认这符合它对这个工具的预期");
    for (const sentence of [refusal.en, refusal.zhCN]) {
      expect(sentence).not.toMatch(/official|instructions|官方|说明/);
    }
    // What may be wrong is still said (docs/superpowers/backlog.md quotes
    // the Chinese clause).
    expect(refusal.en).toContain("it, or a folder it is in, may be a link to somewhere else");
    expect(refusal.zhCN).toContain("它本身或它所在的某个文件夹可能链到了别处");
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
      ),
    ).toBe('sourceNotice.notRunning.description({"source":"Ollama"})');
  });

  it("localises the submit-time refusal for a source that is gone, without naming it", () => {
    // The instance is not in the snapshot any more, so the caller's
    // `sourceLabel` has already fallen back to the raw instance id. The
    // copy must not interpolate it.
    expect(planErrorMessage(fakeT, '{"kind":"source_gone"}', "brew:/opt/homebrew")).toBe(
      "planRefused.sourceGone",
    );
  });

  it("localises an expired plan instead of showing SubmitError::Expired's own English", () => {
    // `submit_operation_error` used to send `SubmitError::Expired`'s
    // `Display` verbatim -- this project's own English, unlocalised. It
    // now sends `{"kind":"expired"}` like every other structured refusal.
    expect(planErrorMessage(fakeT, '{"kind":"expired"}', "Homebrew")).toBe("planRefused.expired");
  });

  it("says a pinned package is being kept where it is, instead of showing the backend's JSON", () => {
    // `update_blocked` is the per-package refusal of the gate
    // (`AdapterError::UpdateBlocked` / `SubmitError::UpdateBlocked`,
    // `update_blocked_json` in src-tauri/src/ipc.rs). The Updates page
    // hides the button for such a row, so this is the stale-page path.
    expect(
      planErrorMessage(fakeT, '{"kind":"update_blocked","reason":"Pinned"}', "Homebrew"),
    ).toBe('updates.blocked.Pinned.refused({"source":"Homebrew"})');
    // A reason this build does not know is shown verbatim, not guessed at.
    expect(
      planErrorMessage(fakeT, '{"kind":"update_blocked","reason":"Held"}', "Homebrew"),
    ).toBe('{"kind":"update_blocked","reason":"Held"}');
  });

  it("localises an unknown/already-submitted plan instead of showing SubmitError::Unknown's own English", () => {
    expect(planErrorMessage(fakeT, '{"kind":"unknown"}', "Homebrew")).toBe("planRefused.unknown");
  });
});

describe("settingsSaveErrorMessage", () => {
  it("words the reasons a person can act on itself", () => {
    for (const [reason, key] of [
      ["permission_denied", "settingsSaveFailed.permissionDenied"],
      ["disk_full", "settingsSaveFailed.diskFull"],
      ["read_only", "settingsSaveFailed.readOnly"],
    ]) {
      expect(
        settingsSaveErrorMessage(fakeT, JSON.stringify({ kind: "settings_save_failed", reason })),
      ).toBe(key);
    }
  });

  it("quotes the system's own text for any other reason, inside a translated phrase", () => {
    expect(
      settingsSaveErrorMessage(
        fakeT,
        '{"kind":"settings_save_failed","reason":"other","detail":"Input/output error (os error 5)"}',
      ),
    ).toBe('settingsSaveFailed.other({"detail":"Input/output error (os error 5)"})');
  });

  it("shows anything that is not the payload verbatim", () => {
    expect(settingsSaveErrorMessage(fakeT, "boom")).toBe("boom");
    expect(settingsSaveErrorMessage(fakeT, '{"kind":"expired"}')).toBe('{"kind":"expired"}');
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
    expect(
      openOllamaErrorMessage(fakeT, '{"kind":"ollama_open_failed","reason":"not_installed"}'),
    ).toBe("sourceNotice.openOllamaFailed.notInstalled");
    expect(
      openOllamaErrorMessage(fakeT, '{"kind":"ollama_open_failed","reason":"launch_failed"}'),
    ).toBe("sourceNotice.openOllamaFailed.launchFailed");
  });

  it("shows anything it does not recognise verbatim rather than hiding it", () => {
    expect(openOllamaErrorMessage(fakeT, "command open_ollama_app not found")).toBe(
      "command open_ollama_app not found",
    );
  });
});

describe("failedSourceCount", () => {
  function err(instance_id: string, message = "boom"): SourceError {
    return { instance_id, message };
  }

  it("is 0 for no errors", () => {
    expect(failedSourceCount([])).toBe(0);
  });

  it("counts one source once even when inventory and check-updates both failed for it", () => {
    // The exact shape session/refresh.rs produces for one broken Homebrew:
    // one SourceError from the inventory fetch, one from check_updates,
    // same instance_id. The banner says "sources", not "calls".
    const errors = [
      err("brew:/opt/homebrew", "brew list failed"),
      err("brew:/opt/homebrew", "brew outdated failed"),
    ];
    expect(failedSourceCount(errors)).toBe(1);
  });

  it("counts two broken prefixes as two, not four, when each fails both calls", () => {
    const errors = [
      err("npm:/opt/homebrew/lib", "npm ls failed"),
      err("npm:/opt/homebrew/lib", "npm outdated failed"),
      err("npm:/usr/local/lib", "npm ls failed"),
      err("npm:/usr/local/lib", "npm outdated failed"),
    ];
    expect(failedSourceCount(errors)).toBe(2);
  });

  it("counts a detect-stage failure (bare adapter id) as its own distinct source", () => {
    // refresh.rs pushes the detect-panic error with instance_id set to the
    // adapter id alone (e.g. "cargo"), not a "<adapter_id>:<path>"
    // instance id -- real instance ids can never collide with it.
    const errors = [err("cargo", "internal error detecting this source"), err("brew:/opt/homebrew")];
    expect(failedSourceCount(errors)).toBe(2);
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
      unverified_version: null,
      read_only_reason: null,
      status: { unavailable: null, notes: [] },
    } satisfies ManagerInstance;
    const key = { instance_id: instance.id, kind: "Formula", name: "glib" } satisfies ArtifactKey;
    expect(UPDATE_BLOCKED_KEYS.Pinned.command(key, instance)).toBe(
      "'/Users/Alice Smith/homebrew/bin/brew' unpin glib",
    );
  });

  it("builds pipx's own unpin command for a pinned pipx tool, from the pipx Canager found", () => {
    // pipx spells it `pipx unpin <name>` (its `commands/upgrade.py:473`);
    // a brew-shaped `brew unpin cowsay` would answer "No available formula".
    const pipx = {
      id: "pipx",
      adapter_id: "pipx",
      exe_path: "/Users/Alice Smith/.local/bin/pipx",
      prefix: "/Users/Alice Smith/.local/bin",
      scope: "User",
      version: "1.17.3",
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

  it("names the pinned package's own source in the description, not always Homebrew", () => {
    // pipx produces `Pinned` too, so "Homebrew is keeping this" would be
    // false on a pipx row.
    for (const copy of [
      en.updates.blocked.Pinned.description,
      zhCN.updates.blocked.Pinned.description,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy).not.toContain("Homebrew");
    }
  });

  it("says the pin's release shows up when Canager next starts, not when 'it' is next opened", () => {
    // "it" has just meant the package, and for a pinned cask that is an
    // app, "open it" reads as "open that app". What refreshes is Canager's
    // start (`refreshIntoCache(queryClient, "initial")`), so name Canager.
    for (const copy of [
      en.updates.blocked.Pinned.description,
      en.updates.blocked.Pinned.descriptionSelfUpdating,
    ]) {
      expect(copy).toMatch(/the next time you start Canager\.$/);
    }
    for (const copy of [
      zhCN.updates.blocked.Pinned.description,
      zhCN.updates.blocked.Pinned.descriptionSelfUpdating,
    ]) {
      expect(copy).toMatch(/下次启动 Canager 的时候。$/);
    }
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
    // made Homebrew the one who did it; the English never says who.
    for (const copy of [
      zhCN.updates.blocked.Pinned.description,
      zhCN.updates.blocked.Pinned.descriptionSelfUpdating,
      zhCN.updates.blocked.Pinned.refused,
    ]) {
      expect(copy).not.toMatch(/把[^，。]*固定/);
      expect(copy).toMatch(/被固定/);
    }
  });

  it("names the tool's own launcher, quoted when its path has a space, as what a self-updating tool is opened with", () => {
    // Spec §4.4 / §十三 #31: the sentence says to open the tool once (not
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
    // The reason itself is "it updates itself": no separate sentence for a
    // self-updating package.
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.selfUpdatingDescription).toBeNull();
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.selfUpdatingDescriptionSourceUnavailable).toBeNull();
  });

  it("tells a self-updating tool's user to open it once, that it checks at most every 15 minutes, and names the versions", () => {
    // agy.md §4 (VERIFIED): a 15-minute debounce on its background check --
    // without that number, "I opened it and nothing happened" is certain.
    for (const copy of [
      en.updates.blocked.SelfUpdatesOnly.description,
      en.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy).toContain("{{command}}");
      expect(copy).toMatch(/Open it once/);
      expect(copy).toMatch(/15 minutes/);
    }
    for (const copy of [
      zhCN.updates.blocked.SelfUpdatesOnly.description,
      zhCN.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy).toContain("{{command}}");
      expect(copy).toMatch(/打开它一次/);
      expect(copy).toMatch(/15 分钟/);
    }
    // The available-source sentence names the two versions (spec §9.2);
    // the unavailable one cannot promise a current target and does not.
    expect(en.updates.blocked.SelfUpdatesOnly.description).toContain("{{current}} → {{target}}");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.description).toContain("{{current}} → {{target}}");
    expect(en.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable).not.toContain("{{target}}");
    // `refused` gets only the source's label.
    expect(en.updates.blocked.SelfUpdatesOnly.refused).not.toContain("{{command}}");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.refused).not.toContain("{{command}}");
    expect(en.updates.blocked.SelfUpdatesOnly.badge).toBe("Updates itself");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.badge).toBe("自己更新");
  });

  it("promises the self-installed update only while the tool's automatic updates are on", () => {
    // Google documents `AGY_CLI_DISABLE_AUTO_UPDATE=true` as turning
    // Antigravity CLI's background updater off (agy.md §4). Set in a
    // shell's startup file, it stops the very run the sentence asks for
    // from installing anything, and Canager does not look for it, so the
    // promise says "unless" in both locales.
    for (const copy of [
      en.updates.blocked.SelfUpdatesOnly.description,
      en.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable,
    ]) {
      expect(copy).toContain(
        "unless its automatic updates have been turned off, it checks for updates when it starts",
      );
    }
    for (const copy of [
      zhCN.updates.blocked.SelfUpdatesOnly.description,
      zhCN.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable,
    ]) {
      expect(copy).toContain("只要它的自动更新没有被关掉，它启动时就会检查更新");
    }
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
      en.installed.blocked.Pinned.descriptionSourceUnavailable,
      en.installed.blocked.Pinned.refused,
      zhCN.installed.blocked.Pinned.description,
      zhCN.installed.blocked.Pinned.descriptionSourceUnavailable,
      zhCN.installed.blocked.Pinned.refused,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy.split("{{command}}")).toHaveLength(2);
    }
  });

  it("carries no command for a tool with no safe uninstall method: there is nothing to run first", () => {
    // Unlike a pin, nothing the user runs can make Canager able to
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
    // Nothing about "when the button comes back" to say differently for a
    // silent source, so one sentence serves both.
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.descriptionSourceUnavailable).toBe(
      UNINSTALL_BLOCKED_KEYS.NoSafeMethod.description,
    );
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

  it("points at the tool's official documentation, not at a website Canager doesn't show, in the no-safe-method sentence", () => {
    // Canager shows no homepage and opens no link (the Tauri opener
    // paragraph in docs/what-we-run.md), so "its website" named nothing
    // the user could find from the row. The documentation, called by the
    // tool's own name, is something they can look up.
    expect(en.installed.blocked.NoSafeMethod.description).toContain("{{source}}'s official documentation");
    expect(zhCN.installed.blocked.NoSafeMethod.description).toContain("{{source}} 官方文档");
    for (const locale of [en, zhCN]) {
      expect(locale.installed.blocked.NoSafeMethod.description).not.toMatch(/website|网站/);
    }
  });
});

describe("parseUninstallBlocked", () => {
  it("reads the reason out of the uninstall gate's payload and nothing else", () => {
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"Pinned"}')).toBe("Pinned");
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"NoSafeMethod"}')).toBe(
      "NoSafeMethod",
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
    // string could not be localised), so the Installed page finds the
    // sentence's key here by adapter id, and every package manager keeps
    // `installed.noDescription` for an unblocked package with no blurb.
    expect(standaloneSummaryKey("standalone-claude")).toBe("standalone.summary.standalone-claude");
    expect(standaloneSummaryKey("standalone-agy")).toBe("standalone.summary.standalone-agy");
    expect(standaloneSummaryKey("standalone-grok")).toBe("standalone.summary.standalone-grok");
    for (const id of ["brew", "npm", "pipx", "uv", "pip", "cargo", "ollama", "toString", ""]) {
      expect(standaloneSummaryKey(id)).toBeNull();
    }
  });

  it("has the sentence in both locales, naming the installer route", () => {
    expect(en.standalone.summary["standalone-claude"]).toBe(
      "Anthropic's coding assistant for the terminal. Installed with its own installer, not with Homebrew or npm.",
    );
    expect(zhCN.standalone.summary["standalone-claude"]).toBe(
      "Anthropic 的终端编程助手。用它自己的安装器装的，不是 Homebrew 或 npm。",
    );
  });

  it("gives rustup its sentence and its label, in both locales", () => {
    expect(standaloneSummaryKey("standalone-rustup")).toBe("standalone.summary.standalone-rustup");
    expect(ADAPTER_LABEL_KEYS["standalone-rustup"]).toBe("adapters.standalone-rustup");
    expect(en.standalone.summary["standalone-rustup"]).toBe(
      "Rust's toolchain manager: it installs and updates the Rust compiler and Cargo.",
    );
    expect(zhCN.standalone.summary["standalone-rustup"]).toBe(
      "Rust 的工具链管理器：负责安装和更新 Rust 编译器与 Cargo。",
    );
    expect(en.adapters["standalone-rustup"]).toBe("rustup");
    expect(zhCN.adapters["standalone-rustup"]).toBe("rustup");
  });

  it("labels Antigravity CLI and Grok Build with the command the user types, in both locales", () => {
    // Spec §9.2: the label carries the command name in parentheses, since
    // "Antigravity CLI" and "Grok Build" are not what the user types.
    expect(en.adapters["standalone-agy"]).toBe("Antigravity CLI (agy)");
    expect(zhCN.adapters["standalone-agy"]).toBe("Antigravity CLI（agy）");
    expect(en.adapters["standalone-grok"]).toBe("Grok Build (grok)");
    expect(zhCN.adapters["standalone-grok"]).toBe("Grok Build（grok）");
    expect(ADAPTER_LABEL_KEYS["standalone-agy"]).toBe("adapters.standalone-agy");
    expect(ADAPTER_LABEL_KEYS["standalone-grok"]).toBe("adapters.standalone-grok");
  });

  it("has the two AI CLIs' sentences in both locales, naming the publisher and the installer route", () => {
    expect(en.standalone.summary["standalone-agy"]).toBe(
      "Google's Antigravity coding assistant for the terminal. Installed with its own installer.",
    );
    expect(zhCN.standalone.summary["standalone-agy"]).toBe(
      "Google 的 Antigravity 终端编程助手。用它自己的安装器装的。",
    );
    expect(en.standalone.summary["standalone-grok"]).toBe(
      "xAI's Grok coding assistant for the terminal. Installed with its own installer.",
    );
    expect(zhCN.standalone.summary["standalone-grok"]).toBe(
      "xAI 的 Grok 终端编程助手。用它自己的安装器装的。",
    );
  });
});

describe("artifactBlurb", () => {
  it("gives the artifact's own description first, then a standalone tool's summary, else nothing", () => {
    // The one lookup both pages' rows read, so the Updates row of a tool
    // with no description of its own says what its Installed row says.
    expect(artifactBlurb(fakeT, "Verify system files structure", "brew")).toBe(
      "Verify system files structure",
    );
    expect(artifactBlurb(fakeT, null, "standalone-grok")).toBe(
      "standalone.summary.standalone-grok",
    );
    expect(artifactBlurb(fakeT, undefined, "standalone-rustup")).toBe(
      "standalone.summary.standalone-rustup",
    );
    // A package manager's row with no blurb, and a row whose instance is
    // not in the snapshot: nothing, so the page says it has no description.
    expect(artifactBlurb(fakeT, null, "npm")).toBeNull();
    expect(artifactBlurb(fakeT, null, undefined)).toBeNull();
  });
});

describe("uninstallBlockedCopy", () => {
  it("gives rustup's row its own reason for NoSafeMethod, and every other row B's", () => {
    // The rustup recipe's gate puts `NoSafeMethod` on the artifact when
    // Rust is not in its standard folders (crates/canager-core/src/
    // adapters/standalone/rustup.rs, `uninstall_blocked`); B's sentence
    // for that variant says the tool has no uninstall command, which is
    // false for rustup. The badge stays; the two sentences are rustup's.
    const rustup = uninstallBlockedCopy("NoSafeMethod", "standalone-rustup");
    expect(rustup.badge).toBe(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.badge);
    expect(rustup.description).toBe("installed.blocked.NoSafeMethod.standalone-rustup.description");
    expect(rustup.descriptionSourceUnavailable).toBe(
      "installed.blocked.NoSafeMethod.standalone-rustup.description",
    );
    expect(rustup.refused).toBe("installed.blocked.NoSafeMethod.standalone-rustup.refused");
    expect(
      rustup.command({ instance_id: "standalone-rustup", kind: "Binary", name: "rustup" }, undefined),
    ).toBe("");
    expect(en.installed.blocked.NoSafeMethod["standalone-rustup"].description).toBe(
      "Canager only removes Rust from its standard folders, ~/.cargo and ~/.rustup, and this Mac keeps it, or part of it, somewhere else (CARGO_HOME or RUSTUP_HOME is set, or one of those folders, or something directly inside one, is a link), so it doesn't offer to. rustup's official documentation explains rustup self uninstall.",
    );
    expect(zhCN.installed.blocked.NoSafeMethod["standalone-rustup"].description).toBe(
      "Canager 只会从标准位置（~/.cargo 和 ~/.rustup）删除 Rust，而这台 Mac 把它的全部或一部分放在了别处（设置了 CARGO_HOME 或 RUSTUP_HOME，或者这两个文件夹之一、或它们里面第一层的某一项是链接），所以这里不提供卸载。rustup 的官方文档说明了怎么用 rustup self uninstall 卸载。",
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
