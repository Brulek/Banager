import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { toolSetupCheck, type SetupLine, type ToolSetupCheck, type ToolSetupInput } from "./toolSetupCheck";
import type {
  ArtifactKey,
  CommandFact,
  HomebrewFacts,
  InstalledArtifact,
  ManagerInstance,
  Sizes,
  Snapshot,
  SystemFacts,
} from "./types";
import { NO_FACTS } from "./types";

const en = i18n.getFixedT("en");
const zh = i18n.getFixedT("zh-CN");
const zhHant = i18n.getFixedT("zh-Hant");

function instance(id: string, more: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: id.split(":")[0],
    exe_path: "/opt/homebrew/bin/x",
    prefix: id.includes("/usr/local") ? "/usr/local" : "/opt/homebrew",
    scope: "User",
    version: "1.0",
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...more,
  };
}

function artifact(
  key: ArtifactKey,
  more: { version?: string; family?: string | null; commands?: CommandFact[]; homebrew?: HomebrewFacts | null } = {},
): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version: more.version ?? "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family: more.family ?? null, commands: more.commands ?? [], homebrew: more.homebrew ?? null },
  };
}

function brewFacts(more: Partial<HomebrewFacts> = {}): HomebrewFacts {
  return { deprecated: null, disabled: null, caveats: null, other_versions: [], ...more };
}

const BREW = "brew:/opt/homebrew";
const INTEL = "brew:/usr/local";
const NPM = "npm:/opt/homebrew";
const OLLAMA = "ollama:http://127.0.0.1:11434";
const CLAUDE = "standalone-claude:/Users/alice/.local/bin/claude";
const CODEX = "standalone-codex:/Users/alice/.local/bin/codex";

const runs = (name: string): CommandFact => ({ name, state: "Runs" });
const notOnPath = (name: string): CommandFact => ({ name, state: { NotOnPath: { dir: "~/.npm-global/bin" } } });

/** A Mac with nothing to say: every source answers, every command runs, Homebrew keeps nothing. */
function fineSnapshot(): Snapshot {
  return {
    generation: 3,
    round: 4,
    detect: "Found",
    instances: [instance(BREW), instance(NPM)],
    artifacts: [
      artifact({ instance_id: BREW, kind: "Formula", name: "jq" }, { commands: [runs("jq")], homebrew: brewFacts() }),
      artifact({ instance_id: NPM, kind: "Package", name: "prettier" }, { commands: [runs("prettier")] }),
    ],
    updates: [],
    refreshed_at: 1_790_000_000,
    stale: false,
    errors: [],
  };
}

const FACTS: SystemFacts = {
  macos_version: "27.0",
  chip: "Apple M2 Pro",
  arch: "aarch64",
  login_path: true,
  path_dirs: ["/opt/homebrew/bin", "~/.local/bin", "/usr/bin"],
  sources: [],
  path_folders: { read: 3, unread: [] },
};

function sizesFor(snapshot: Snapshot, more: Partial<Sizes> = {}): Sizes {
  return {
    round: snapshot.round,
    done: true,
    artifacts: snapshot.artifacts.map((a) => ({
      key: a.key,
      version: a.version,
      measured: { bytes: 1_000_000, partial: false, at_least: false },
      old_versions: null,
    })),
    models: [],
    total: { bytes: 9_800_000_000, partial: false, at_least: false },
    sources: [],
    ...more,
  };
}

function input(more: Partial<ToolSetupInput> = {}): ToolSetupInput {
  const snapshot = more.snapshot === undefined ? fineSnapshot() : more.snapshot;
  return {
    snapshot,
    pending: false,
    facts: FACTS,
    sizes: snapshot === null ? null : sizesFor(snapshot),
    technicalDetails: false,
    language: "en",
    nowMs: Date.now(),
    ...more,
  };
}

/** Each section's lines as `symbol text`, with ` · secondary` and ` → view` where they have them. */
function shape(check: ToolSetupCheck): Record<string, string[]> {
  const said = (line: SetupLine) => {
    const view =
      line.view === null
        ? ""
        : line.view.kind === "installed"
          ? ` → installed:${line.view.show}`
          : line.view.kind === "source"
            ? ` → source:${line.view.instanceId}`
            : " → unknown";
    return `${line.symbol} ${line.text}${line.secondary === null ? "" : ` · ${line.secondary}`}${view}`;
  };
  return Object.fromEntries(check.sections.map((section) => [section.title, section.lines.map(said)]));
}

function lineOf(check: ToolSetupCheck, section: string, id: string): SetupLine {
  const found = check.sections.find((s) => s.id === section)?.lines.find((l) => l.id === id);
  if (found === undefined) throw new Error(`no ${section}/${id}`);
  return found;
}

describe("toolSetupCheck, on a Mac with nothing wrong", () => {
  it("says each section is fine in one line, in English", () => {
    expect(shape(toolSetupCheck(en, input()))).toEqual({
      "Finding Commands in Terminal": ["fine Terminal's settings were read normally, and so were all 3 folders it looks in for commands"],
      Sources: [
        "fine Every source answered normally · Homebrew, npm",
        "note Command-line programs from other sources are listed separately → unknown",
      ],
      Commands: ["fine Terminal finds every installed tool, and no AI coding tool is installed more than once"],
      Homebrew: ["fine Homebrew hasn't disabled or deprecated any tool, and keeps no other versions"],
      Disk: ["note Installed tools take about\u00a09.8 GB in all"],
    });
  });

  it("and in Chinese", () => {
    expect(shape(toolSetupCheck(zh, input()))).toEqual({
      在终端里查找命令: ["fine 终端设置读取正常，查找命令的3个文件夹也都能读取"],
      来源: [
        "fine 所有来源都正常回应 · Homebrew、npm",
        "note 不属于这些来源的命令行程序，可以在“其他程序”里找 → unknown",
      ],
      命令: ["fine 终端都能找到已安装的工具，也没有装了不止一份的AI编程工具"],
      Homebrew: ["fine 没有Homebrew已停用或弃用的工具，也没有保留的其他版本"],
      磁盘: ["note 已安装的工具共占用约9.8 GB"],
    });
  });

  it("gives no score, no grade and no 健康 anywhere", () => {
    for (const t of [en, zh]) {
      const words = JSON.stringify(toolSetupCheck(t, input()));
      expect(words).not.toMatch(/健康|体检|分数|\bscore\b|\bgrade\b|\bhealth/i);
    }
  });

  it("leaves Homebrew out on a Mac without one", () => {
    const snapshot = { ...fineSnapshot(), instances: [instance(NPM)] };
    snapshot.artifacts = snapshot.artifacts.filter((a) => a.key.instance_id === NPM);
    expect(toolSetupCheck(en, input({ snapshot })).sections.map((s) => s.id)).toEqual([
      "terminal",
      "sources",
      "commands",
      "disk",
    ]);
  });
});

describe("toolSetupCheck's terminal lines", () => {
  it("says, when the login shell's settings could not be read, that which copy runs was not judged, and why behind its ⓘ", () => {
    const check = toolSetupCheck(en, input({ facts: { ...FACTS, login_path: false, path_folders: null } }));
    const line = lineOf(check, "terminal", "loginNotRead");
    expect(line.symbol).toBe("warning");
    expect(line.text).toBe(
      "Couldn't read Terminal's login settings, so which copy runs in Terminal wasn't checked this time",
    );
    expect(line.detail).toMatch(/opened from Finder/);
    // Its one line: the system's few folders are not the user's.
    expect(check.sections[0].lines).toHaveLength(1);
    expect(toolSetupCheck(zh, input({ facts: { ...FACTS, login_path: false } })).sections[0].lines[0].text).toBe(
      "无法读取终端登录时的设置，所以这次没有判断终端里运行的是哪一份",
    );
  });

  it("counts the folders read and not read, and names the unread ones only with technical details on", () => {
    const facts: SystemFacts = { ...FACTS, path_folders: { read: 9, unread: ["~/Documents/bin", "/Volumes/x/bin"] } };
    const quiet = toolSetupCheck(zh, input({ facts }));
    expect(shape(quiet)["在终端里查找命令"]).toEqual([
      "fine 已读取终端登录时的设置",
      "note 终端查找命令的文件夹中，9个已读取，2个无法读取",
    ]);
    expect(lineOf(quiet, "terminal", "foldersUnread").detail).toMatch(/受保护的位置/);
    const shown = toolSetupCheck(en, input({ facts, technicalDetails: true }));
    expect(lineOf(shown, "terminal", "foldersUnread")).toMatchObject({
      text: "Folders Terminal looks in for commands: 9 read, 2 couldn't be read",
      secondary: "Couldn't read: ~/Documents/bin, /Volumes/x/bin",
    });
  });

  it("says how many folders there are, and nothing of which were read, before a round has read them", () => {
    const check = toolSetupCheck(en, input({ facts: { ...FACTS, path_folders: null } }));
    expect(shape(check)["Finding Commands in Terminal"]).toEqual([
      "fine Terminal's login settings were read",
      "note Folders Terminal looks in for commands: 3",
    ]);
    // A fixture from before the field: read as null.
    const { path_folders: _, ...older } = FACTS;
    expect(shape(toolSetupCheck(en, input({ facts: older })))["Finding Commands in Terminal"]).toEqual(
      shape(check)["Finding Commands in Terminal"],
    );
  });

  it("says one folder in the singular", () => {
    const check = toolSetupCheck(en, input({ facts: { ...FACTS, path_folders: { read: 1, unread: [] } } }));
    expect(check.sections[0].lines[0].text).toBe(
      "Terminal's settings were read normally, and so was the 1 folder it looks in for commands",
    );
  });

  it("waits for the facts, and says when they could not be had", () => {
    expect(shape(toolSetupCheck(en, input({ facts: undefined })))["Finding Commands in Terminal"]).toEqual(["busy Loading…"]);
    expect(shape(toolSetupCheck(zh, input({ facts: null })))["在终端里查找命令"]).toEqual(["warning 无法读取终端的设置"]);
  });
});

describe("toolSetupCheck's source lines", () => {
  it("gives each source with something to say a line in the diagnostic text's words, what stops it first, with 查看 to its page", () => {
    const snapshot: Snapshot = {
      ...fineSnapshot(),
      instances: [
        instance(BREW),
        instance(INTEL, { status: { unavailable: "NotResponding", notes: [] } }),
        instance(NPM, { read_only_reason: "PrefixNotWritable", unverified_version: "12.0.0" }),
        instance("pip:/usr/bin/python3", { read_only_reason: "ByDesign" }),
        instance(OLLAMA, { status: { unavailable: "NotRunning", notes: [] } }),
        instance(CLAUDE),
      ],
      errors: [{ instance_id: CLAUDE, message: "timed out" }],
    };
    const check = toolSetupCheck(zh, input({ snapshot }));
    expect(shape(check)["来源"]).toEqual([
      // Two Homebrews, named as the sidebar names them.
      `warning Homebrew（Intel）：没有响应 → source:${INTEL}`,
      `warning Ollama：没有运行 → source:${OLLAMA}`,
      `warning Claude Code：这次没有检查完 → source:${CLAUDE}`,
      `note npm：仅供查看、未经测试的版本 → source:${NPM}`,
      "note pip：仅供查看 → source:pip:/usr/bin/python3",
      "fine 其他来源都正常回应 · Homebrew（Apple芯片）",
      "note 不属于这些来源的命令行程序，可以在“其他程序”里找 → unknown",
    ]);
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Sources"].slice(0, 4)).toEqual([
      `warning Homebrew (Intel): Not responding → source:${INTEL}`,
      `warning Ollama: Not running → source:${OLLAMA}`,
      `warning Claude Code: Check didn't finish → source:${CLAUDE}`,
      `note npm: View only, version not tested → source:${NPM}`,
    ]);
  });

  it("has no line saying the others are fine when none is", () => {
    const snapshot: Snapshot = {
      ...fineSnapshot(),
      instances: [instance(BREW, { status: { unavailable: "NotResponding", notes: [] } })],
    };
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Sources"]).toEqual([
      `warning Homebrew: Not responding → source:${BREW}`,
      "note Command-line programs from other sources are listed separately → unknown",
    ]);
  });

  it("says under a source that did not answer when it last did, and no time Banager does not have (R12)", () => {
    const nowMs = new Date(2026, 9, 2, 10, 0).getTime();
    const at = new Date(2026, 9, 2, 9, 12).getTime() / 1000;
    const answered = { answered_at: at };
    const snapshot: Snapshot = {
      ...fineSnapshot(),
      instances: [
        instance(BREW, { ...answered, status: { unavailable: "NotResponding", notes: [] } }),
        // Not heard this session: no time.
        instance(INTEL, { status: { unavailable: "NotResponding", notes: [] } }),
        // Heard, but its line says something else than not answering.
        instance(OLLAMA, { ...answered, status: { unavailable: "NotRunning", notes: [] } }),
        instance(CLAUDE, answered),
      ],
      errors: [{ instance_id: CLAUDE, message: "timed out" }],
    };
    expect(shape(toolSetupCheck(zh, input({ snapshot, nowMs, language: "zh-CN" })))["来源"].slice(0, 4)).toEqual([
      `warning Homebrew（Apple芯片）：没有响应 · 上次响应：今天09:12 → source:${BREW}`,
      `warning Homebrew（Intel）：没有响应 → source:${INTEL}`,
      `warning Ollama：没有运行 → source:${OLLAMA}`,
      `warning Claude Code：这次没有检查完 → source:${CLAUDE}`,
    ]);
    const nine = new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(new Date(at * 1000));
    expect(shape(toolSetupCheck(en, input({ snapshot, nowMs, language: "en" })))["Sources"][0]).toBe(
      `warning Homebrew (Apple silicon): Not responding · Last responded at ${nine} today → source:${BREW}`,
    );
  });

  it("keeps the last response of a Mac whose clock reads before 1970 instead of treating it as missing", () => {
    // Banager stamps that clock's answers 0 (`Session::clock`,
    // crates/banager-core/src/session/mod.rs), and the window reads the
    // same clock, half a minute before 1970.
    const snapshot: Snapshot = {
      ...fineSnapshot(),
      instances: [instance(BREW, { answered_at: 0, status: { unavailable: "NotResponding", notes: [] } })],
    };
    const check = toolSetupCheck(en, input({ snapshot, nowMs: -30_000 }));
    const time = new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(new Date(0));
    expect(lineOf(check, "sources", `source:${BREW}`).secondary).toBe(`Last responded at ${time} today`);
  });

  it("says so when no source was found", () => {
    const snapshot: Snapshot = { ...fineSnapshot(), instances: [], artifacts: [] };
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["来源"]).toEqual(["note 没有找到可以管理的来源"]);
  });
});

describe("toolSetupCheck's command lines", () => {
  /** Two copies of Claude Code, the npm one not on Terminal's search path. */
  function twinsSnapshot(): Snapshot {
    const snapshot = fineSnapshot();
    snapshot.instances.push(instance(CLAUDE));
    snapshot.artifacts.push(
      artifact(
        { instance_id: NPM, kind: "Package", name: "@anthropic-ai/claude-code" },
        { family: "claude-code", commands: [notOnPath("claude")] },
      ),
      artifact({ instance_id: CLAUDE, kind: "Binary", name: "claude" }, { family: "claude-code", commands: [runs("claude")] }),
    );
    return snapshot;
  }

  it("counts the tools Terminal can't find and those installed more than once, each with 查看 to its 显示 choice", () => {
    const check = toolSetupCheck(zh, input({ snapshot: twinsSnapshot() }));
    expect(shape(check)["命令"]).toEqual([
      "warning 1个工具在终端里找不到 → installed:notOnPath",
      "note 1个AI编程工具装了不止一份，共2份 → installed:twins",
    ]);
    expect(lineOf(check, "commands", "notOnPath").detail).toBe(zh("notOnPathMore.detailOne"));
    expect(lineOf(check, "commands", "twins").detail).toMatch(/只会运行其中一份/);
    expect(shape(toolSetupCheck(en, input({ snapshot: twinsSnapshot() })))["Commands"]).toEqual([
      "warning 1 tool can't be found in Terminal → installed:notOnPath",
      "note 1 AI coding tool is installed more than once, 2 copies in all → installed:twins",
    ]);
  });

  it("says how many copies the tools installed more than once are, the rows its 查看 lists (r24 W3)", () => {
    // Claude Code twice, and Codex three times: two tools, five copies --
    // the five rows 「装了不止一份（5）」 shows.
    const snapshot = twinsSnapshot();
    snapshot.instances.push(instance(CODEX));
    snapshot.artifacts.push(
      artifact({ instance_id: NPM, kind: "Package", name: "@openai/codex" }, { family: "codex", commands: [runs("codex")] }),
      artifact({ instance_id: BREW, kind: "Cask", name: "codex" }, { family: "codex", commands: [runs("codex")] }),
      artifact({ instance_id: CODEX, kind: "Binary", name: "codex" }, { family: "codex", commands: [runs("codex")] }),
    );
    expect(lineOf(toolSetupCheck(zh, input({ snapshot })), "commands", "twins").text).toBe("2个AI编程工具装了不止一份，共5份");
    expect(lineOf(toolSetupCheck(en, input({ snapshot })), "commands", "twins").text).toBe(
      "2 AI coding tools are installed more than once, 5 copies in all",
    );
    expect(lineOf(toolSetupCheck(zhHant, input({ snapshot })), "commands", "twins").text).toBe(
      "2個AI程式設計工具裝了不止一份，共5份",
    );
  });

  it("says which half is fine when only the other is not", () => {
    const snapshot = twinsSnapshot();
    snapshot.artifacts[2] = artifact(snapshot.artifacts[2].key, { family: "claude-code", commands: [runs("claude")] });
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "fine Terminal finds every installed tool",
      "note 1 AI coding tool is installed more than once, 2 copies in all → installed:twins",
    ]);
    const lone = fineSnapshot();
    lone.artifacts.push(
      artifact({ instance_id: NPM, kind: "Package", name: "tsx" }, { commands: [notOnPath("tsx")] }),
    );
    expect(shape(toolSetupCheck(zh, input({ snapshot: lone })))["命令"]).toEqual([
      "warning 1个工具在终端里找不到 → installed:notOnPath",
      "fine 没有装了不止一份的AI编程工具",
    ]);
  });

  it("says the commands were not judged when no verdict was made, as with the login shell's settings unread", () => {
    const snapshot = twinsSnapshot();
    for (const a of snapshot.artifacts) {
      a.facts = { ...a.facts, commands: a.facts.commands.map((c) => ({ ...c, state: null })) };
    }
    // The names are still there: two copies can still be told apart.
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["命令"]).toEqual([
      "note 终端找不到：这次没有判断",
      "note 1个AI编程工具装了不止一份，共2份 → installed:twins",
    ]);
    for (const a of snapshot.artifacts) a.facts = { ...a.facts, commands: [] };
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "note Not found in Terminal: not looked at this time",
      "note AI coding tools installed more than once: not looked at this time",
    ]);
  });

  /**
   * jq runs; two more tools' launchers could not be checked -- one behind a
   * folder Terminal looks in that could not be read, one whose link was
   * replaced -- and so have no verdict, as Astra's round-5 review (#5) found
   * the sheet then said every installed tool was found.
   */
  function partlyCheckedSnapshot(): Snapshot {
    const snapshot = fineSnapshot();
    snapshot.instances.push(instance(CLAUDE));
    snapshot.artifacts.push(
      artifact({ instance_id: CLAUDE, kind: "Binary", name: "claude" }, { commands: [{ name: "claude", state: null }] }),
      artifact(
        { instance_id: NPM, kind: "Package", name: "eslint" },
        { commands: [runs("eslint"), { name: "eslint-config-inspector", state: null }] },
      ),
    );
    return snapshot;
  }

  it("says Terminal finds the tools that were checked, and how many could not be, when only some were", () => {
    const facts = { ...FACTS, path_folders: { read: 2, unread: ["~/Documents/bin"] } };
    expect(shape(toolSetupCheck(en, input({ snapshot: partlyCheckedSnapshot(), facts })))["Commands"]).toEqual([
      "fine Terminal finds every tool that was checked",
      "note Couldn't check whether Terminal finds 2 tools",
      "fine No AI coding tool is installed more than once",
    ]);
    const zhCheck = toolSetupCheck(zh, input({ snapshot: partlyCheckedSnapshot(), facts }));
    expect(shape(zhCheck)["命令"]).toEqual([
      "fine 终端都能找到检查过的工具",
      "note 2个工具无法确认终端能否找到",
      "fine 没有装了不止一份的AI编程工具",
    ]);
    expect(lineOf(zhCheck, "commands", "notChecked").detail).toBe(
      "无法判断终端会运行这些工具的哪一份，例如终端查找命令的某个文件夹无法读取。",
    );
    // Never the green sentence about every installed tool, folders read or not.
    const enCheck = toolSetupCheck(en, input({ snapshot: partlyCheckedSnapshot() }));
    for (const [check, said] of [
      [zhCheck, zh],
      [enCheck, en],
    ] as const) {
      const texts = check.sections.find((s) => s.id === "commands")!.lines.map((l) => l.text);
      expect(texts).not.toContain(said("setupCheck.commands.allFine"));
      expect(texts).not.toContain(said("setupCheck.commands.notOnPathFine"));
    }
  });

  it("counts a pipx tool whose protected command claim was dropped beside a running jq", () => {
    const snapshot = fineSnapshot();
    snapshot.artifacts = snapshot.artifacts.slice(0, 1);
    const pipx = instance("pipx:home");
    snapshot.instances.push(pipx);
    const dropped = artifact({ instance_id: pipx.id, kind: "Tool", name: "cowsay" });
    dropped.facts.commands_unavailable = true;
    snapshot.artifacts.push(dropped);
    // Its commands could not all be listed, so whether it is a second copy
    // of one is not known: the twins sentence is of the tools checked only
    // (Opus review finding 8).
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "fine Terminal finds every tool that was checked",
      "note Couldn't check whether Terminal finds 1 tool",
      "fine No AI coding tool that was checked is installed more than once",
    ]);
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["命令"]).toEqual([
      "fine 终端都能找到检查过的工具",
      "note 1个工具无法确认终端能否找到",
      "fine 检查过的工具中没有装了不止一份的AI编程工具",
    ]);
    const hant = toolSetupCheck(i18n.getFixedT("zh-Hant"), input({ snapshot }));
    expect(hant.sections.find((section) => section.id === "commands")!.lines.map((l) => l.text)).toContain(
      "檢查過的工具中沒有裝了不止一份的AI程式設計工具",
    );
  });

  it("says one in the singular, and the count beside the tools Terminal can't find", () => {
    const snapshot = partlyCheckedSnapshot();
    snapshot.artifacts.pop();
    snapshot.artifacts.push(
      artifact({ instance_id: NPM, kind: "Package", name: "tsx" }, { commands: [notOnPath("tsx")] }),
    );
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "warning 1 tool can't be found in Terminal → installed:notOnPath",
      "note Couldn't check whether Terminal finds 1 tool",
      "fine No AI coding tool is installed more than once",
    ]);
  });

  it("does not count what is never checked: a Homebrew dependency, or a tool already counted as not found", () => {
    const snapshot = fineSnapshot();
    snapshot.artifacts.push(
      {
        ...artifact({ instance_id: BREW, kind: "Formula", name: "openssl@3" }, { commands: [{ name: "openssl", state: null }] }),
        reason: "Dependency",
      },
      artifact(
        { instance_id: NPM, kind: "Package", name: "tsx" },
        { commands: [notOnPath("tsx"), { name: "tsx-watch", state: null }] },
      ),
    );
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "warning 1 tool can't be found in Terminal → installed:notOnPath",
      "fine No AI coding tool is installed more than once",
    ]);
    snapshot.artifacts.pop();
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "fine Terminal finds every installed tool, and no AI coding tool is installed more than once",
    ]);
  });

  it("counts a formula Homebrew didn't link as a second copy, not as a tool it couldn't check (r36 V5)", () => {
    // npm's `gemini`, then `brew install gemini-cli`, whose link step
    // stopped at npm's file: its commands come from its keg, with no
    // verdict. It used to have none, and this said 「终端都能找到已安装的
    // 工具，也没有装了不止一份的」.
    const snapshot = fineSnapshot();
    snapshot.artifacts.push(
      artifact({ instance_id: NPM, kind: "Package", name: "@google/gemini-cli" }, { family: "gemini-cli", commands: [runs("gemini")] }),
    );
    const formula = artifact(
      { instance_id: BREW, kind: "Formula", name: "gemini-cli" },
      { family: "gemini-cli", commands: [{ name: "gemini", state: null }] },
    );
    formula.facts.unlinked = true;
    snapshot.artifacts.push(formula);
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "fine Terminal finds every installed tool",
      "note 1 AI coding tool is installed more than once, 2 copies in all → installed:twins",
    ]);
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["命令"]).toEqual([
      "fine 终端都能找到已安装的工具",
      "note 1个AI编程工具装了不止一份，共2份 → installed:twins",
    ]);
    // Its keg could not all be read: then it is one it couldn't check.
    formula.facts.commands_unavailable = true;
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toContain(
      "note Couldn't check whether Terminal finds 1 tool",
    );
  });

  it("names a tool Homebrew didn't link that is no copy of another, not saying Terminal finds all (q1b skeptic 2)", () => {
    // `brew unlink ollama`, or Ollama.app's own CLI in the way of `brew
    // install ollama`: the formula's `ollama`, named from its keg, has no
    // verdict, and no other copy runs it. Its details say Homebrew didn't
    // link it; this used to say 「终端都能找到已安装的工具…」 beside it.
    const snapshot = fineSnapshot();
    const formula = artifact(
      { instance_id: BREW, kind: "Formula", name: "ollama" },
      { family: "ollama", commands: [{ name: "ollama", state: null }] },
    );
    formula.facts.unlinked = true;
    snapshot.artifacts.push(formula);
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "note Homebrew didn't link 1 tool where Terminal looks · ollama",
      "fine No AI coding tool is installed more than once",
    ]);
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["命令"]).toEqual([
      "note Homebrew没有把1个工具链接到终端能找到的地方 · ollama",
      "fine 没有装了不止一份的AI编程工具",
    ]);
    expect(shape(toolSetupCheck(zhHant, input({ snapshot })))["指令"]).toEqual([
      "note Homebrew沒有把1個工具連結到終端機找得到的地方 · ollama",
      "fine 沒有裝了不止一份的AI程式設計工具",
    ]);
    const line = toolSetupCheck(en, input({ snapshot })).sections.find((s) => s.id === "commands")!.lines[0];
    expect(line.detail).toBe(
      "Usually a file of the same name was in the way when Homebrew installed it. Typing its commands in Terminal doesn't run it.",
    );
    // Beside a tool Terminal can't find, both.
    snapshot.artifacts.push(
      artifact({ instance_id: NPM, kind: "Package", name: "tsx" }, { commands: [notOnPath("tsx")] }),
    );
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "warning 1 tool can't be found in Terminal → installed:notOnPath",
      "note Homebrew didn't link 1 tool where Terminal looks · ollama",
      "fine No AI coding tool is installed more than once",
    ]);
    // Its keg first on PATH after all: it has a verdict, and is found.
    snapshot.artifacts.pop();
    formula.facts.commands = [runs("ollama")];
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "fine Terminal finds every installed tool, and no AI coding tool is installed more than once",
    ]);
  });

  /**
   * Ruff from Homebrew and from pipx, both on Terminal's search path: two
   * copies of one program, but no AI coding tool (`families.rs` has no
   * Ruff), so `twinsByArtifact` pairs neither -- and a matching command name
   * alone is no proof of one tool. What is said of copies is said of the AI
   * coding tools only; what Terminal finds stays said of every tool (R82-3).
   */
  function ruffTwiceSnapshot(): Snapshot {
    const snapshot = fineSnapshot();
    const pipx = instance("pipx:home");
    snapshot.instances.push(pipx);
    const brewRuff: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "ruff" };
    snapshot.artifacts.push(
      artifact(brewRuff, { commands: [runs("ruff")], homebrew: brewFacts() }),
      artifact(
        { instance_id: pipx.id, kind: "Tool", name: "ruff" },
        { commands: [{ name: "ruff", state: { ShadowedBy: { by: brewRuff } } }] },
      ),
    );
    return snapshot;
  }

  it("says what it says of copies of the AI coding tools only, with Ruff installed twice (R82-3)", () => {
    const snapshot = ruffTwiceSnapshot();
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "fine Terminal finds every installed tool, and no AI coding tool is installed more than once",
    ]);
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["命令"]).toEqual([
      "fine 终端都能找到已安装的工具，也没有装了不止一份的AI编程工具",
    ]);
    expect(lineOf(toolSetupCheck(zhHant, input({ snapshot })), "commands", "allFine").text).toBe(
      "終端機都能找到已安裝的工具，也沒有裝了不止一份的AI程式設計工具",
    );
    // Beside a tool Terminal can't find, the twins line of its own.
    snapshot.artifacts.push(
      artifact({ instance_id: NPM, kind: "Package", name: "tsx" }, { commands: [notOnPath("tsx")] }),
    );
    for (const [t, want] of [
      [en, "No AI coding tool is installed more than once"],
      [zh, "没有装了不止一份的AI编程工具"],
      [zhHant, "沒有裝了不止一份的AI程式設計工具"],
    ] as const) {
      expect(lineOf(toolSetupCheck(t, input({ snapshot })), "commands", "twins").text).toBe(want);
    }
    // And with a tool whose commands could not all be listed.
    const dropped = artifact({ instance_id: "pipx:home", kind: "Tool", name: "cowsay" });
    dropped.facts.commands_unavailable = true;
    snapshot.artifacts.push(dropped);
    for (const [t, want] of [
      [en, "No AI coding tool that was checked is installed more than once"],
      [zh, "检查过的工具中没有装了不止一份的AI编程工具"],
      [zhHant, "檢查過的工具中沒有裝了不止一份的AI程式設計工具"],
    ] as const) {
      expect(lineOf(toolSetupCheck(t, input({ snapshot })), "commands", "twins").text).toBe(want);
    }
  });

  it("says the copies of an AI coding tool are more than one, not where they came from or that one runs (R82-3)", () => {
    // Two copies from one source -- Homebrew's ollama formula and cask --
    // neither found in Terminal: still two copies of one tool.
    const snapshot = fineSnapshot();
    snapshot.artifacts.push(
      artifact({ instance_id: BREW, kind: "Formula", name: "ollama" }, { family: "ollama", commands: [notOnPath("ollama")] }),
      artifact({ instance_id: BREW, kind: "Cask", name: "ollama-app" }, { family: "ollama", commands: [notOnPath("ollama")] }),
    );
    for (const [t, text, detail] of [
      [
        en,
        "1 AI coding tool is installed more than once, 2 copies in all",
        "More than one copy of the same AI coding tool is installed. Typing its command in Terminal runs one of them at most.",
      ],
      [zh, "1个AI编程工具装了不止一份，共2份", "同一个AI编程工具装了不止一份。在终端里输入它的命令，最多只会运行其中一份。"],
      [
        zhHant,
        "1個AI程式設計工具裝了不止一份，共2份",
        "同一個AI程式設計工具裝了不止一份。在終端機裡輸入它的指令，最多只會執行其中一份。",
      ],
    ] as const) {
      const twins = lineOf(toolSetupCheck(t, input({ snapshot })), "commands", "twins");
      expect(twins.text).toBe(text);
      expect(twins.detail).toBe(detail);
    }
  });
});

describe("toolSetupCheck's Homebrew lines", () => {
  function brewSnapshot(): Snapshot {
    const snapshot = fineSnapshot();
    snapshot.instances.push(instance(INTEL));
    snapshot.artifacts.push(
      artifact(
        { instance_id: BREW, kind: "Formula", name: "youtube-dl" },
        { homebrew: brewFacts({ disabled: { date: "2024-10-24", reason: "unmaintained", replacement: "yt-dlp" } }) },
      ),
      artifact(
        { instance_id: INTEL, kind: "Cask", name: "old-cask" },
        { homebrew: brewFacts({ deprecated: { date: null, reason: null, replacement: null } }) },
      ),
      artifact(
        { instance_id: BREW, kind: "Formula", name: "node@22" },
        { version: "22.23.3", homebrew: brewFacts({ other_versions: ["22.22.0", "22.21.1"] }) },
      ),
      artifact(
        { instance_id: INTEL, kind: "Formula", name: "python@3.13" },
        { version: "3.13.7", homebrew: brewFacts({ other_versions: ["3.13.5"] }) },
      ),
    );
    return snapshot;
  }

  it("excludes old-version measurements for a different installed version from the total", () => {
    const snapshot = brewSnapshot();
    const sizes = sizesFor(snapshot);
    sizes.artifacts = sizes.artifacts.map((size) =>
      size.key.name === "node@22"
        ? { ...size, old_versions: { bytes: 400_000_000, partial: false, at_least: false } }
        : size.key.name === "python@3.13"
          ? { ...size, version: "3.13.6", old_versions: { bytes: 100_000_000, partial: false, at_least: false } }
          : size,
    );
    expect(lineOf(toolSetupCheck(zh, input({ snapshot, sizes })), "homebrew", "otherVersions").text).toBe(
      "2个工具保留了其他版本，共400 MB以上",
    );
    // Once the measured version matches, both amounts can be claimed.
    const python = sizes.artifacts.find((size) => size.key.name === "python@3.13")!;
    python.version = "3.13.7";
    expect(lineOf(toolSetupCheck(zh, input({ snapshot, sizes })), "homebrew", "otherVersions").text).toBe(
      "2个工具保留了其他版本，共约500 MB",
    );
  });

  it("counts what every Homebrew disabled or deprecated, with 查看, and what keeps other versions with their size", () => {
    const snapshot = brewSnapshot();
    const sizes = sizesFor(snapshot);
    sizes.artifacts = sizes.artifacts.map((size) =>
      size.key.name === "node@22"
        ? { ...size, old_versions: { bytes: 400_000_000, partial: false, at_least: false } }
        : size.key.name === "python@3.13"
          ? { ...size, old_versions: { bytes: 100_000_000, partial: false, at_least: false } }
          : size,
    );
    const check = toolSetupCheck(zh, input({ snapshot, sizes }));
    expect(shape(check)["Homebrew"]).toEqual([
      "note 2个工具已被Homebrew停用或弃用 → installed:brewRetired",
      "note 2个工具保留了其他版本，共约500 MB → installed:otherVersions",
    ]);
    expect(lineOf(check, "homebrew", "retired").detail).toBe(zh("families.brewRetiredNoticeDetail"));
    expect(lineOf(check, "homebrew", "otherVersions").detail).toBe(zh("clarity.otherVersionsDetail"));
    expect(shape(toolSetupCheck(en, input({ snapshot, sizes })))["Homebrew"]).toEqual([
      "note 2 tools were disabled or deprecated by Homebrew → installed:brewRetired",
      "note Homebrew keeps other versions of 2 tools, about\u00a0500 MB in all → installed:otherVersions",
    ]);
  });

  it("says 以上 when one of them has no measured size, and no size at all before the round is measured", () => {
    const snapshot = brewSnapshot();
    const sizes = sizesFor(snapshot);
    sizes.artifacts = sizes.artifacts.map((size) =>
      size.key.name === "node@22" ? { ...size, old_versions: { bytes: 400_000_000, partial: false, at_least: false } } : size,
    );
    expect(lineOf(toolSetupCheck(zh, input({ snapshot, sizes })), "homebrew", "otherVersions").text).toBe(
      "2个工具保留了其他版本，共400 MB以上",
    );
    expect(lineOf(toolSetupCheck(en, input({ snapshot, sizes: null })), "homebrew", "otherVersions").text).toBe(
      "Homebrew keeps other versions of 2 tools",
    );
    expect(
      lineOf(toolSetupCheck(en, input({ snapshot, sizes: { ...sizes, round: 3 } })), "homebrew", "otherVersions").text,
    ).toBe("Homebrew keeps other versions of 2 tools");
  });

  it("says which half is fine when only the other is not", () => {
    const snapshot = brewSnapshot();
    snapshot.artifacts = snapshot.artifacts.filter((a) => !["node@22", "python@3.13"].includes(a.key.name));
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Homebrew"]).toEqual([
      "note 2 tools were disabled or deprecated by Homebrew → installed:brewRetired",
      "fine No other versions are kept",
    ]);
    const keeping = brewSnapshot();
    keeping.artifacts = keeping.artifacts.filter((a) => !["youtube-dl", "old-cask"].includes(a.key.name));
    expect(shape(toolSetupCheck(zh, input({ snapshot: keeping, sizes: null })))["Homebrew"]).toEqual([
      "fine 没有Homebrew已停用或弃用的工具",
      "note 2个工具保留了其他版本 → installed:otherVersions",
    ]);
  });
});

describe("toolSetupCheck's Homebrew lines when a Homebrew wasn't fully checked (R59-3)", () => {
  it("claims nothing is disabled or kept when the only Homebrew didn't answer the first check", () => {
    // First launch, a Homebrew that doesn't answer: no list was read, and
    // there is no earlier one to carry, so "nothing disabled, no other
    // versions" would be said of nothing at all.
    const snapshot = fineSnapshot();
    snapshot.instances = snapshot.instances.map((i) =>
      i.id === BREW ? { ...i, status: { unavailable: "NotResponding", notes: [] } } : i,
    );
    snapshot.artifacts = snapshot.artifacts.filter((a) => a.key.instance_id !== BREW);
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Homebrew"]).toEqual([
      "note Couldn't fully check Homebrew this time, so what's here about it may be incomplete",
    ]);
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["Homebrew"]).toEqual([
      "note Homebrew这次没有检查完，这里关于它的结果可能不完整",
    ]);
    expect(shape(toolSetupCheck(zhHant, input({ snapshot })))["Homebrew"]).toEqual([
      "note Homebrew這次沒有檢查完，這裡關於它的結果可能不完整",
    ]);
  });

  it("says the same when its check didn't finish, and keeps what was counted", () => {
    const snapshot = fineSnapshot();
    snapshot.errors = [{ instance_id: BREW, message: "timed out" }];
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Homebrew"]).toEqual([
      "note Couldn't fully check Homebrew this time, so what's here about it may be incomplete",
    ]);
    snapshot.artifacts.push(
      artifact(
        { instance_id: BREW, kind: "Formula", name: "node@22" },
        { version: "22.23.3", homebrew: brewFacts({ other_versions: ["22.22.0"] }) },
      ),
    );
    expect(shape(toolSetupCheck(en, input({ snapshot, sizes: null })))["Homebrew"]).toEqual([
      "note Couldn't fully check Homebrew this time, so what's here about it may be incomplete",
      "note Homebrew keeps other versions of 1 tool → installed:otherVersions",
    ]);
  });

  it("says the same when its list wasn't read because brew update was running", () => {
    // `IndexUpdating`: no error and no ⚠︎ (nothing failed), but the list is
    // last check's -- on a first launch, none.
    const snapshot = fineSnapshot();
    snapshot.instances = snapshot.instances.map((i) =>
      i.id === BREW ? { ...i, status: { unavailable: null, notes: ["IndexUpdating"] } } : i,
    );
    snapshot.artifacts = snapshot.artifacts.filter((a) => a.key.instance_id !== BREW);
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Homebrew"]).toEqual([
      "note Couldn't fully check Homebrew this time, so what's here about it may be incomplete",
    ]);
  });

  it("doesn't let one Homebrew's clean list speak for another that didn't answer", () => {
    const snapshot = fineSnapshot();
    snapshot.instances.push(instance(INTEL, { status: { unavailable: "NotResponding", notes: [] } }));
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Homebrew"]).toEqual([
      "note Couldn't fully check Homebrew (Intel) this time, so what's here about it may be incomplete",
    ]);
  });
});

describe("toolSetupCheck's disk lines", () => {
  it("says the toolbar's total with its hedge, and each Ollama's models", () => {
    const snapshot = fineSnapshot();
    snapshot.instances.push(instance(OLLAMA));
    snapshot.artifacts.push(artifact({ instance_id: OLLAMA, kind: "Model", name: "llama3:8b" }));
    const sizes: Sizes = {
      ...sizesFor(snapshot),
      models: [{ instance_id: OLLAMA, measured: { bytes: 41_000_000_000, partial: false, at_least: false } }],
      total: { bytes: 52_000_000_000, partial: false, at_least: true },
    };
    const check = toolSetupCheck(zh, input({ snapshot, sizes }));
    expect(shape(check)["磁盘"]).toEqual(["note 已安装的工具共占用52 GB以上", "note Ollama模型共约41 GB"]);
    expect(lineOf(check, "disk", "total").detail).toBe(zh("sizeTotals.note"));
  });

  it("says it is measuring until the snapshot's round is measured", () => {
    const snapshot = fineSnapshot();
    for (const sizes of [null, { ...sizesFor(snapshot), done: false }, { ...sizesFor(snapshot), round: 1 }]) {
      expect(shape(toolSetupCheck(zh, input({ snapshot, sizes })))["磁盘"]).toEqual(["busy 正在计算占用的空间…"]);
    }
  });
});

describe("toolSetupCheck while the first check runs", () => {
  it("says what is known from the first list, and that the commands and sizes come once it finishes", () => {
    const snapshot = { ...fineSnapshot(), generation: 0, round: 0, refreshed_at: null };
    snapshot.instances[1] = instance(NPM, { status: { unavailable: "NotResponding", notes: [] } });
    const check = toolSetupCheck(zh, input({ snapshot, pending: true, facts: { ...FACTS, path_folders: null } }));
    expect(check.pending).toBe(true);
    expect(shape(check)).toEqual({
      在终端里查找命令: ["fine 已读取终端登录时的设置", "note 终端查找命令的文件夹：3个"],
      来源: [
        `warning npm：没有响应 → source:${NPM}`,
        // No source has answered a check yet: nothing found, not "answered normally".
        "fine 目前没有发现问题 · Homebrew",
        "note 不属于这些来源的命令行程序，可以在“其他程序”里找 → unknown",
      ],
      命令: ["busy 检查完成后会显示在这里"],
      Homebrew: ["fine 没有Homebrew已停用或弃用的工具，也没有保留的其他版本"],
      磁盘: ["busy 正在计算占用的空间…"],
    });
  });

  it("says no problems so far, in English too, rather than that every source answered", () => {
    const snapshot = { ...fineSnapshot(), generation: 0, round: 0, refreshed_at: null };
    const check = toolSetupCheck(en, input({ snapshot, pending: true }));
    expect(shape(check).Sources[0]).toBe("fine No problems found so far · Homebrew, npm");
  });

  it("says it is checking while nothing is listed yet", () => {
    const check = toolSetupCheck(en, input({ snapshot: null, pending: true }));
    expect(shape(check)).toEqual({
      "Finding Commands in Terminal": ["fine Terminal's settings were read normally, and so were all 3 folders it looks in for commands"],
      Sources: ["busy Checking…"],
      Commands: ["busy These appear here when the check finishes"],
    });
  });
});

describe("toolSetupCheck's order", () => {
  it("puts the sections with a ⚠︎ line first, and counts the ⚠︎ lines", () => {
    const fine = toolSetupCheck(en, input());
    expect(fine.attention).toBe(0);
    expect(fine.sections.map((section) => section.id)).toEqual(["terminal", "sources", "commands", "homebrew", "disk"]);
  });
});

describe("toolSetupCheck's sources whose lists or checks are incomplete (R82-1)", () => {
  const PIPX = "pipx:/opt/homebrew/bin/pipx";
  const partial = (): Snapshot => ({
    ...fineSnapshot(),
    instances: [
      instance(BREW, { status: { unavailable: null, notes: ["FormulaeNotListed"] } }),
      instance(NPM),
      instance(PIPX, { status: { unavailable: null, notes: ["SomeNotListed"] } }),
    ],
  });

  it("warns once for each, with its page, and never counts it among the sources that answered normally", () => {
    const snapshot = partial();
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Sources"]).toEqual([
      `warning Homebrew: Some packages not shown → source:${BREW}`,
      `warning pipx: Some tools not shown → source:${PIPX}`,
      "fine The other sources answered normally · npm",
      "note Command-line programs from other sources are listed separately → unknown",
    ]);
    expect(shape(toolSetupCheck(zh, input({ snapshot })))["来源"].slice(0, 3)).toEqual([
      `warning Homebrew：部分软件包没有显示 → source:${BREW}`,
      `warning pipx：部分工具没有显示 → source:${PIPX}`,
      "fine 其他来源都正常回应 · npm",
    ]);
    expect(shape(toolSetupCheck(zhHant, input({ snapshot })))["來源"].slice(0, 3)).toEqual([
      `warning Homebrew：部分套件沒有顯示 → source:${BREW}`,
      `warning pipx：部分工具沒有顯示 → source:${PIPX}`,
      "fine 其他來源都正常回應 · npm",
    ]);
    expect(toolSetupCheck(en, input({ snapshot })).attention).toBe(2);
  });

  it("warns for a list that may be out of date, and only notes a list Homebrew is still downloading", () => {
    const snapshot: Snapshot = {
      ...fineSnapshot(),
      instances: [
        instance(BREW, { status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
        instance(INTEL, { status: { unavailable: null, notes: ["IndexUpdating"] } }),
        instance(NPM),
      ],
    };
    const check = toolSetupCheck(en, input({ snapshot }));
    expect(shape(check)["Sources"].slice(0, 3)).toEqual([
      `warning Homebrew (Apple silicon): Updates not fully checked → source:${BREW}`,
      `note Homebrew (Intel): Checking online for new versions → source:${INTEL}`,
      "fine The other sources answered normally · npm",
    ]);
    expect(check.attention).toBe(1);
  });

  it("says an error whose source isn't listed, once, without a page it doesn't have, and claims no source is fine for it", () => {
    const bare = { ...fineSnapshot(), instances: [], artifacts: [], errors: [{ instance_id: "npm", message: "internal error detecting this source" }] };
    expect(shape(toolSetupCheck(en, input({ snapshot: bare })))["Sources"]).toEqual([
      "warning npm: Check didn't finish",
      "note Command-line programs from other sources are listed separately → unknown",
    ]);
    expect(toolSetupCheck(en, input({ snapshot: bare })).attention).toBe(1);

    // Beside a healthy Homebrew: no 「所有来源都正常回应」.
    const beside: Snapshot = {
      ...fineSnapshot(),
      instances: [instance(BREW)],
      errors: [
        { instance_id: "npm", message: "internal error detecting this source" },
        { instance_id: "npm", message: "again" },
      ],
    };
    expect(shape(toolSetupCheck(zh, input({ snapshot: beside })))["来源"].slice(0, 2)).toEqual([
      "warning npm：这次没有检查完",
      "fine 其他来源都正常回应 · Homebrew",
    ]);
    expect(toolSetupCheck(en, input({ snapshot: beside })).attention).toBe(1);

    // A Homebrew the snapshot no longer lists: its error isn't the live one's.
    const gone: Snapshot = { ...fineSnapshot(), errors: [{ instance_id: INTEL, message: "timed out" }] };
    expect(shape(toolSetupCheck(en, input({ snapshot: gone })))["Sources"].slice(0, 2)).toEqual([
      "warning Homebrew (/usr/local): Check didn't finish",
      "fine The other sources answered normally · Homebrew, npm",
    ]);
    expect(shape(toolSetupCheck(zhHant, input({ snapshot: gone })))["來源"][0]).toBe(
      "warning Homebrew（/usr/local）：這次沒有檢查完",
    );
  });

  it("adds nothing for a bare source's error when its sources already warn, and gives it to the ones that don't", () => {
    const retained: Snapshot = {
      ...fineSnapshot(),
      instances: [
        instance(BREW, { status: { unavailable: "NotResponding", notes: [] } }),
        instance(INTEL, { status: { unavailable: "NotResponding", notes: [] } }),
      ],
      errors: [{ instance_id: "brew", message: "internal error detecting this source" }],
    };
    expect(shape(toolSetupCheck(en, input({ snapshot: retained })))["Sources"]).toEqual([
      `warning Homebrew (Apple silicon): Not responding → source:${BREW}`,
      `warning Homebrew (Intel): Not responding → source:${INTEL}`,
      "note Command-line programs from other sources are listed separately → unknown",
    ]);
    expect(toolSetupCheck(en, input({ snapshot: retained })).attention).toBe(2);
    const answered: Snapshot = { ...retained, instances: [instance(BREW), instance(NPM)] };
    expect(shape(toolSetupCheck(en, input({ snapshot: answered })))["Sources"].slice(0, 2)).toEqual([
      `warning Homebrew: Check didn't finish → source:${BREW}`,
      "fine The other sources answered normally · npm",
    ]);
  });

  it("claims nothing is disabled or kept of a Homebrew whose list left formulae out or may be out of date, and keeps what it counted", () => {
    for (const note of ["FormulaeNotListed", "IndexMayBeStale"] as const) {
      const snapshot = fineSnapshot();
      snapshot.instances = snapshot.instances.map((i) => (i.id === BREW ? { ...i, status: { unavailable: null, notes: [note] } } : i));
      expect(shape(toolSetupCheck(en, input({ snapshot })))["Homebrew"]).toEqual([
        "note Couldn't fully check Homebrew this time, so what's here about it may be incomplete",
      ]);
      snapshot.artifacts.push(
        artifact(
          { instance_id: BREW, kind: "Formula", name: "node@22" },
          { version: "22.23.3", homebrew: brewFacts({ other_versions: ["22.22.0"] }) },
        ),
      );
      expect(shape(toolSetupCheck(zh, input({ snapshot, sizes: null })))["Homebrew"]).toEqual([
        "note Homebrew这次没有检查完，这里关于它的结果可能不完整",
        "note 1个工具保留了其他版本 → installed:otherVersions",
      ]);
    }
    // One of two Homebrews: the other's clean list doesn't speak for it.
    const two = fineSnapshot();
    two.instances.push(instance(INTEL, { status: { unavailable: null, notes: ["FormulaeNotListed"] } }));
    expect(shape(toolSetupCheck(en, input({ snapshot: two })))["Homebrew"]).toEqual([
      "note Couldn't fully check Homebrew (Intel) this time, so what's here about it may be incomplete",
    ]);
  });
});
