import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { diagnosticsText, diagnosticsTime, withoutHomePaths, type DiagnosticsInput } from "./diagnostics";
import type {
  ArtifactKey,
  CommandFact,
  InstalledArtifact,
  ManagerInstance,
  Sizes,
  Snapshot,
  SystemFacts,
} from "./types";
import { NO_FACTS } from "./types";

function instance(id: string, exe: string, more: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: id.split(":")[0],
    exe_path: exe,
    prefix: "/opt/homebrew",
    scope: "User",
    version: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...more,
  };
}

function artifact(
  key: ArtifactKey,
  version: string,
  family: string | null = null,
  commands: CommandFact[] = [],
): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version,
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family, commands },
  };
}

const BREW = "brew:/opt/homebrew";
const NPM = "npm:/opt/homebrew";
const UV = "uv:/Users/alice/.local/bin/uv";
const CLAUDE = "standalone-claude:/Users/alice/.local/bin/claude";
const npmClaude: ArtifactKey = { instance_id: NPM, kind: "Package", name: "@anthropic-ai/claude-code" };

/**
 * A small Mac: Homebrew with a stale list, npm read-only, uv not
 * answering, Claude Code on a version Banager has not been tested with --
 * installed twice, once through npm -- and ruff, which Terminal cannot
 * find. Claude Code's source is not among the facts' (found after they
 * were read), so its path comes from the snapshot, home and all.
 */
const SNAPSHOT: Snapshot = {
  generation: 4,
  round: 4,
  detect: "Found",
  instances: [
    instance(BREW, "/opt/homebrew/bin/brew", { version: "7.0.3", status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
    instance(NPM, "/opt/homebrew/bin/npm", { version: "11.6.2", read_only_reason: "PrefixNotWritable" }),
    instance(UV, "/Users/alice/.local/bin/uv", {
      version: "0.9.1",
      status: { unavailable: "NotResponding", notes: [] },
    }),
    instance(CLAUDE, "/Users/alice/.local/bin/claude", { unverified_version: "2.1.0" }),
  ],
  artifacts: [
    artifact({ instance_id: BREW, kind: "Formula", name: "jq" }, "1.8.2"),
    artifact({ instance_id: BREW, kind: "Formula", name: "ffmpeg" }, "9.0.1_1"),
    artifact(npmClaude, "2.1.0", "claude-code", [{ name: "claude", state: "Runs" }]),
    artifact({ instance_id: UV, kind: "Tool", name: "ruff" }, "0.14.0", null, [
      { name: "ruff", state: { NotOnPath: { dir: "~/.local/share/uv/bin" } } },
    ]),
    artifact({ instance_id: CLAUDE, kind: "Binary", name: "claude-code" }, "2.1.0", "claude-code", [
      { name: "claude", state: { ShadowedBy: { by: npmClaude } } },
    ]),
  ],
  updates: [],
  refreshed_at: new Date(2026, 9, 1, 13, 58).getTime() / 1000,
  stale: true,
  errors: [{ instance_id: NPM, message: "npm outdated timed out" }],
};

const FACTS: SystemFacts = {
  macos_version: "27.0",
  chip: "Apple M2 Pro",
  arch: "aarch64",
  login_path: true,
  path_dirs: ["/opt/homebrew/bin", "~/.local/bin", "/usr/bin", "/bin"],
  sources: [
    { instance_id: BREW, exe_path: "/opt/homebrew/bin/brew" },
    { instance_id: NPM, exe_path: "/opt/homebrew/bin/npm" },
    { instance_id: UV, exe_path: "~/.local/bin/uv" },
  ],
};

const SIZES: Sizes = {
  round: 4,
  done: true,
  artifacts: [],
  models: [],
  total: { bytes: 1_234_000_000, partial: false, at_least: false },
  sources: [],
};

function input(more: Partial<DiagnosticsInput> = {}): DiagnosticsInput {
  return {
    now: new Date(2026, 9, 1, 14, 3),
    appName: "Banager",
    appVersion: "0.1.0",
    languageName: "English",
    facts: FACTS,
    snapshot: SNAPSHOT,
    sizes: SIZES,
    includeTools: false,
    ...more,
  };
}

const en = i18n.getFixedT("en");
const zh = i18n.getFixedT("zh-CN");

describe("diagnosticsText", () => {
  it("says, in English, what a helper needs: the Mac, each source, the search folders, the last check and the counts", () => {
    expect(diagnosticsText(en, input())).toBe(
      [
        "Diagnostic info",
        "Time: 2026-10-01 14:03",
        "Banager: 0.1.0",
        "macOS: 27.0",
        "Chip: Apple M2 Pro",
        "Language: English",
        "",
        "Sources: 4",
        "Homebrew",
        "  Version: 7.0.3",
        "  Location: /opt/homebrew/bin/brew",
        "  Status: OK",
        "  Notes: 1",
        "  Tools: 2",
        "npm",
        "  Version: 11.6.2",
        "  Location: /opt/homebrew/bin/npm",
        "  Status: View only",
        "  Tools: 1",
        "uv",
        "  Version: 0.9.1",
        "  Location: ~/.local/bin/uv",
        "  Status: Not responding",
        "  Tools: 1",
        "Claude Code",
        "  Version: 2.1.0",
        "  Location: ~/.local/bin/claude",
        "  Status: Version not tested",
        "  Tools: 1",
        "",
        "Command search folders: 4",
        "  Read from the login settings",
        "  /opt/homebrew/bin",
        "  ~/.local/bin",
        "  /usr/bin",
        "  /bin",
        "",
        "Last check: 2026-10-01 13:58",
        "Check: incomplete, Homebrew, npm and uv didn't finish",
        "Not found in Terminal: 1",
        "Tools installed more than once: 1",
        "Space used: 1.2 GB or more",
        "",
      ].join("\n"),
    );
  });

  it("says the same in Chinese, in the window's language", () => {
    expect(diagnosticsText(zh, input({ languageName: "简体中文" }))).toBe(
      [
        "诊断信息",
        "时间：2026-10-01 14:03",
        "Banager：0.1.0",
        "macOS：27.0",
        "芯片：Apple M2 Pro",
        "界面语言：简体中文",
        "",
        "来源：4个",
        "Homebrew",
        "  版本：7.0.3",
        "  位置：/opt/homebrew/bin/brew",
        "  状态：正常",
        "  提示：1条",
        "  工具：2个",
        "npm",
        "  版本：11.6.2",
        "  位置：/opt/homebrew/bin/npm",
        "  状态：仅供查看",
        "  工具：1个",
        "uv",
        "  版本：0.9.1",
        "  位置：~/.local/bin/uv",
        "  状态：没有响应",
        "  工具：1个",
        "Claude Code",
        "  版本：2.1.0",
        "  位置：~/.local/bin/claude",
        "  状态：未经测试的版本",
        "  工具：1个",
        "",
        "查找命令的文件夹：4个",
        "  已读取登录时的设置",
        "  /opt/homebrew/bin",
        "  ~/.local/bin",
        "  /usr/bin",
        "  /bin",
        "",
        "上次检查：2026-10-01 13:58",
        "检查结果：不完整，Homebrew、npm和uv未检查完",
        "终端找不到：1个",
        "装了不止一份的工具：1个",
        "占用空间：1.2 GB以上",
        "",
      ].join("\n"),
    );
  });

  it("lists each source's tools, name and version, only when asked to", () => {
    const without = diagnosticsText(en, input());
    const withTools = diagnosticsText(en, input({ includeTools: true }));
    for (const line of ["    ffmpeg 9.0.1_1", "    jq 1.8.2", "    @anthropic-ai/claude-code 2.1.0", "    ruff 0.14.0"]) {
      expect(without.split("\n")).not.toContain(line);
      expect(withTools.split("\n")).toContain(line);
    }
    // Under their source, by name.
    const lines = withTools.split("\n");
    expect(lines.slice(lines.indexOf("  Tools: 2"), lines.indexOf("  Tools: 2") + 3)).toEqual([
      "  Tools: 2",
      "    ffmpeg 9.0.1_1",
      "    jq 1.8.2",
    ]);
    // Nothing else changes.
    expect(withTools.split("\n").filter((line) => !line.startsWith("    "))).toEqual(without.split("\n"));
  });

  it("counts a tool installed twice once, however many copies it has, and each such tool", () => {
    // A third Claude Code, from Homebrew; and ruff twice more, from pipx and Homebrew.
    const brewClaude = artifact({ instance_id: BREW, kind: "Cask", name: "claude-code" }, "2.1.0", "claude-code", [
      { name: "claude", state: { ShadowedBy: { by: npmClaude } } },
    ]);
    const pipxRuff = artifact({ instance_id: "pipx:/opt/homebrew/bin/pipx", kind: "Package", name: "ruff" }, "0.14.0", "ruff", [
      { name: "ruff", state: "Runs" },
    ]);
    const brewRuff = artifact({ instance_id: BREW, kind: "Formula", name: "ruff" }, "0.14.0", "ruff", [
      { name: "ruff", state: { ShadowedBy: { by: pipxRuff.key } } },
    ]);
    const three = diagnosticsText(en, input({ snapshot: { ...SNAPSHOT, artifacts: [...SNAPSHOT.artifacts, brewClaude] } }));
    expect(three).toContain("\nTools installed more than once: 1\n");
    const two = diagnosticsText(
      zh,
      input({ snapshot: { ...SNAPSHOT, artifacts: [...SNAPSHOT.artifacts, brewClaude, pipxRuff, brewRuff] } }),
    );
    expect(two).toContain("\n装了不止一份的工具：2个\n");
  });

  it("cuts an Ollama model's digest to the twelve digits ollama list shows", () => {
    const digest = "8e4cdead7463ce276b20d4e33341950d7bb40847f70a9882567a188e24ec1f66";
    const model = artifact({ instance_id: BREW, kind: "Model", name: "llama3.2:3b" }, digest);
    const text = diagnosticsText(
      en,
      input({ includeTools: true, snapshot: { ...SNAPSHOT, artifacts: [...SNAPSHOT.artifacts, model] } }),
    );
    expect(text).toContain("\n    llama3.2:3b 8e4cdead7463\n");
    expect(text).not.toContain(digest);
  });

  it("never names a home folder, in either language, with or without the tools", () => {
    for (const t of [en, zh]) {
      for (const includeTools of [false, true]) {
        const text = diagnosticsText(t, input({ includeTools }));
        expect(text).not.toContain("/Users/");
        expect(text).not.toContain("alice");
      }
    }
  });

  it("says when the login settings could not be read, and names the chip by Banager's build when the kernel would not", () => {
    const text = diagnosticsText(
      en,
      input({ facts: { ...FACTS, login_path: false, chip: null, macos_version: null, arch: "x86_64" } }),
    );
    expect(text).toContain("\nmacOS: couldn't read\nChip: Intel\n");
    expect(text).toContain("\n  Couldn't read the login settings, so defaults were used, which Terminal may not use\n");
    const apple = diagnosticsText(zh, input({ facts: { ...FACTS, chip: null } }));
    expect(apple).toContain("\n芯片：Apple芯片\n");
  });

  it("still says what the snapshot knows when the facts could not be had, without a word on the search folders", () => {
    const text = diagnosticsText(en, input({ facts: null }));
    expect(text).toContain("Banager: 0.1.0\nSystem info: couldn't read\nLanguage: English\n");
    expect(text).not.toContain("Command search folders");
    // Every path from the snapshot, with `~` all the same.
    expect(text).toContain("  Location: ~/.local/bin/uv\n");
    expect(text).not.toContain("/Users/");
  });

  it("says a check that finished is complete, one never made, and no disk total until it is measured", () => {
    // Every source answered, and none has a note that left its updates
    // unchecked: complete.
    const allClean = SNAPSHOT.instances.map((one) => ({ ...one, status: { unavailable: null, notes: [] } }));
    const clean = { ...SNAPSHOT, stale: false, errors: [], instances: allClean };
    const finished = diagnosticsText(en, input({ snapshot: clean }));
    expect(finished).toContain("\nCheck: complete\n");
    const never = diagnosticsText(
      zh,
      input({ snapshot: { ...SNAPSHOT, refreshed_at: null, stale: false, errors: [] }, sizes: { ...SIZES, done: false } }),
    );
    expect(never).toContain("\n上次检查：从未\n");
    expect(never).not.toContain("检查结果");
    expect(never).not.toContain("占用空间");
    // Codex's own install is listed, and its updates never checked: the
    // check is complete but for it, and the text says so.
    const codex = {
      ...SNAPSHOT.instances[0],
      id: "standalone-codex",
      adapter_id: "standalone-codex",
      exe_path: "/Users/you/.local/bin/codex",
      prefix: "/Users/you/.codex/packages/standalone",
    };
    const withCodex = { ...clean, instances: [...allClean, codex] };
    expect(diagnosticsText(en, input({ snapshot: withCodex }))).toContain(
      "\nCheck: complete, except for updates to Codex\n",
    );
    expect(diagnosticsText(zh, input({ snapshot: withCodex }))).toContain("\n检查结果：完整，不含Codex的更新\n");
    // And when another source did not finish, both are said, one after the other.
    const staleWithCodex = { ...SNAPSHOT, instances: [...allClean, codex] };
    expect(diagnosticsText(en, input({ snapshot: staleWithCodex }))).toContain(
      "\nCheck: incomplete, npm didn't finish; updates to Codex aren't checked either\n",
    );
    expect(diagnosticsText(zh, input({ snapshot: staleWithCodex }))).toContain(
      "\n检查结果：不完整，npm未检查完；也不含Codex的更新\n",
    );
    const empty = diagnosticsText(en, input({ snapshot: null, sizes: null }));
    expect(empty).toContain("\nSources: 0\n\nCommand search folders: 4\n");
    expect(empty).toContain("\nNot found in Terminal: 0\nTools installed more than once: 0\n");
  });
});

describe("the text's helpers", () => {
  it("writes the time the same way in both languages, local, to the minute", () => {
    expect(diagnosticsTime(new Date(2026, 0, 5, 9, 7, 59))).toBe("2026-01-05 09:07");
  });

  it("writes any home folder left as ~, but macOS's shared one", () => {
    expect(withoutHomePaths("/Users/alice/.local/bin and /Users/bob")).toBe("~/.local/bin and ~");
    expect(withoutHomePaths("/Users/Shared/tools/bin")).toBe("/Users/Shared/tools/bin");
    expect(withoutHomePaths("/opt/homebrew/bin")).toBe("/opt/homebrew/bin");
  });
});

describe("the check line", () => {
  // No call failed this round, yet a source was not checked in full: the
  // Updates page then says it found no update among the sources it could
  // check (`everySourceChecked`), and the pasted text must not say
  // "complete" either.
  it("names a source that answered without its updates checked, as the Updates page counts it", () => {
    const quiet = (notes: ManagerInstance["status"]["notes"], unavailable: ManagerInstance["status"]["unavailable"] = null) => ({
      ...SNAPSHOT,
      stale: false,
      errors: [],
      instances: [
        instance(BREW, "/opt/homebrew/bin/brew", { status: { unavailable: null, notes } }),
        instance("ollama:/opt/homebrew/bin/ollama", "/opt/homebrew/bin/ollama", { status: { unavailable, notes: [] } }),
      ],
    });
    expect(diagnosticsText(en, input({ snapshot: quiet(["IndexUpdating"]) }))).toContain(
      "\nCheck: incomplete, Homebrew didn't finish\n",
    );
    expect(diagnosticsText(zh, input({ snapshot: quiet([], "NotRunning") }))).toContain(
      "\n检查结果：不完整，Ollama未检查完\n",
    );
    expect(diagnosticsText(en, input({ snapshot: quiet([]) }))).toContain("\nCheck: complete\n");
  });
});

describe("the disk line", () => {
  // The toolbar's number and hedge: 「约」 only when every tool listed has
  // its size in the total, and nothing from a round other than the
  // snapshot's.
  it("hedges as the toolbar does, and says nothing for another round's sizes", () => {
    const measured = (artifact: InstalledArtifact) => ({
      key: artifact.key,
      version: artifact.version,
      measured: { bytes: 1, partial: false, at_least: false },
      old_versions: null,
    });
    const whole = { ...SIZES, artifacts: SNAPSHOT.artifacts.map(measured) };
    expect(diagnosticsText(zh, input({ sizes: whole }))).toContain("\n占用空间：约1.2 GB\n");
    expect(diagnosticsText(en, input({ sizes: whole }))).toContain("\nSpace used: about 1.2 GB\n");
    // A pip package, say, with no size: the total is short of what they take.
    const one = { ...whole, artifacts: whole.artifacts.slice(1) };
    expect(diagnosticsText(zh, input({ sizes: one }))).toContain("\n占用空间：1.2 GB以上\n");
    // Measured for an earlier round: no line rather than a stale number.
    expect(diagnosticsText(zh, input({ sizes: { ...whole, round: 3 } }))).not.toContain("占用空间");
  });
});
