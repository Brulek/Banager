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

function instance(id: string, more: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: id.split(":")[0],
    exe_path: "/opt/homebrew/bin/x",
    prefix: id.includes("/usr/local") ? "/usr/local" : "/opt/homebrew",
    scope: "User",
    version: "1.0",
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
      "Terminal settings": ["fine Terminal's login settings were read, and so were all 3 folders it looks in for commands"],
      Sources: [
        "fine Every source answered normally · Homebrew, npm",
        "note Look in Other Programs for command-line programs from none of these sources → unknown",
      ],
      Commands: ["fine Terminal finds every installed tool, and none is installed more than once"],
      Homebrew: ["fine Homebrew hasn't disabled or deprecated any tool, and keeps no other versions"],
      Disk: ["note Installed tools take about\u00a09.8 GB in all"],
    });
  });

  it("and in Chinese", () => {
    expect(shape(toolSetupCheck(zh, input()))).toEqual({
      终端设置: ["fine 已读取终端登录时的设置，它查找命令的3个文件夹也都已读取"],
      来源: [
        "fine 所有来源都正常回应 · Homebrew、npm",
        "note 不属于这些来源的命令行程序，可以在“其他程序”里找 → unknown",
      ],
      命令: ["fine 终端都能找到已安装的工具，也没有装了不止一份的"],
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
      "Couldn't read Terminal's login settings, so which copy runs in Terminal wasn't judged this time",
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
    expect(shape(quiet)["终端设置"]).toEqual([
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
    expect(shape(check)["Terminal settings"]).toEqual([
      "fine Terminal's login settings were read",
      "note Folders Terminal looks in for commands: 3",
    ]);
    // A fixture from before the field: read as null.
    const { path_folders: _, ...older } = FACTS;
    expect(shape(toolSetupCheck(en, input({ facts: older })))["Terminal settings"]).toEqual(
      shape(check)["Terminal settings"],
    );
  });

  it("says one folder in the singular", () => {
    const check = toolSetupCheck(en, input({ facts: { ...FACTS, path_folders: { read: 1, unread: [] } } }));
    expect(check.sections[0].lines[0].text).toBe(
      "Terminal's login settings were read, and so was the 1 folder it looks in for commands",
    );
  });

  it("waits for the facts, and says when they could not be had", () => {
    expect(shape(toolSetupCheck(en, input({ facts: undefined })))["Terminal settings"]).toEqual(["busy Loading…"]);
    expect(shape(toolSetupCheck(zh, input({ facts: null })))["终端设置"]).toEqual(["warning 无法读取终端的设置"]);
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
      "note Look in Other Programs for command-line programs from none of these sources → unknown",
    ]);
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
      "note 1个工具在终端里找不到 → installed:notOnPath",
      "note 1个工具装了不止一份 → installed:twins",
    ]);
    expect(lineOf(check, "commands", "notOnPath").detail).toBe(zh("families.notOnPathNoticeDetail"));
    expect(lineOf(check, "commands", "twins").detail).toMatch(/只会运行其中一份/);
    expect(shape(toolSetupCheck(en, input({ snapshot: twinsSnapshot() })))["Commands"]).toEqual([
      "note 1 tool can't be found in Terminal → installed:notOnPath",
      "note 1 tool is installed more than once → installed:twins",
    ]);
  });

  it("says which half is fine when only the other is not", () => {
    const snapshot = twinsSnapshot();
    snapshot.artifacts[2] = artifact(snapshot.artifacts[2].key, { family: "claude-code", commands: [runs("claude")] });
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "fine Terminal finds every installed tool",
      "note 1 tool is installed more than once → installed:twins",
    ]);
    const lone = fineSnapshot();
    lone.artifacts.push(
      artifact({ instance_id: NPM, kind: "Package", name: "tsx" }, { commands: [notOnPath("tsx")] }),
    );
    expect(shape(toolSetupCheck(zh, input({ snapshot: lone })))["命令"]).toEqual([
      "note 1个工具在终端里找不到 → installed:notOnPath",
      "fine 没有装了不止一份的工具",
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
      "note 1个工具装了不止一份 → installed:twins",
    ]);
    for (const a of snapshot.artifacts) a.facts = { ...a.facts, commands: [] };
    expect(shape(toolSetupCheck(en, input({ snapshot })))["Commands"]).toEqual([
      "note Not found in Terminal: not looked at this time",
      "note Tools installed more than once: not looked at this time",
    ]);
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
      "note 2 tools keep other versions, about\u00a0500 MB in all → installed:otherVersions",
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
      "2 tools keep other versions",
    );
    expect(
      lineOf(toolSetupCheck(en, input({ snapshot, sizes: { ...sizes, round: 3 } })), "homebrew", "otherVersions").text,
    ).toBe("2 tools keep other versions");
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
      expect(shape(toolSetupCheck(zh, input({ snapshot, sizes })))["磁盘"]).toEqual(["busy 占用的空间正在计算…"]);
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
      终端设置: ["fine 已读取终端登录时的设置", "note 终端查找命令的文件夹：3个"],
      来源: [
        `warning npm：没有响应 → source:${NPM}`,
        // No source has answered a check yet: nothing found, not "answered normally".
        "fine 目前没有发现问题 · Homebrew",
        "note 不属于这些来源的命令行程序，可以在“其他程序”里找 → unknown",
      ],
      命令: ["busy 检查完成后会显示在这里"],
      Homebrew: ["fine 没有Homebrew已停用或弃用的工具，也没有保留的其他版本"],
      磁盘: ["busy 占用的空间正在计算…"],
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
      "Terminal settings": ["fine Terminal's login settings were read, and so were all 3 folders it looks in for commands"],
      Sources: ["busy Checking…"],
      Commands: ["busy These appear here when the check finishes"],
    });
  });
});
