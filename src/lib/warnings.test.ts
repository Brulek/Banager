import { describe, expect, it } from "vitest";
import {
  deletesForGood,
  warningArgs,
  warningDetailKey,
  warningGroup,
  warningKey,
  warningLines,
  warningMessage,
  warningText,
} from "./warnings";
import type { KeptWhat, Warning } from "./types";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";

/** A stub `t`: returns the key with its interpolations inlined, which is
 *  enough to prove `warningText` looked the right key up with the right
 *  values, without coupling this test to the actual English copy. */
function fakeT(key: string, options?: Record<string, unknown>): string {
  return options && Object.keys(options).length > 0 ? `${key}(${JSON.stringify(options)})` : key;
}

describe("warningKey", () => {
  it("gives each fixed warning its own key", () => {
    expect(warningKey("DependentsUnknown")).toBe("warnings.dependentsUnknown");
    expect(warningKey("CompilesLocally")).toBe("warnings.compilesLocally");
    expect(warningKey("NonRegistrySource")).toBe("warnings.nonRegistrySource");
    expect(warningKey({ WouldBreak: { names: ["python@3.13"] } })).toBe("warnings.wouldBreak");
    expect(warningKey({ ThirdPartyRegistry: { host: "modelscope.cn" } })).toBe(
      "warnings.thirdPartyRegistry",
    );
    // A path-list uninstall's items: the key is chosen by what the path
    // is, so each kind can have its own parenthesis.
    expect(warningKey({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toBe(
      "warnings.willTrash.Launcher",
    );
    expect(warningKey({ WillTrash: { path: "~/.local/share/claude", what: "Program" } })).toBe(
      "warnings.willTrash.Program",
    );
    expect(warningKey({ WillTrash: { path: "~/.claude/downloads", what: "Cache" } })).toBe(
      "warnings.willTrash.Cache",
    );
    expect(warningKey({ WillKeep: { path: "~/.claude.json", what: "Settings" } })).toBe(
      "warnings.willKeep.Settings",
    );
    expect(warningKey({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toBe(
      "warnings.willKeep.SettingsAndHistory",
    );
    // Step D's kinds: the backup a self-updater leaves, and the five things
    // the Antigravity and Grok lists keep.
    expect(warningKey({ WillTrash: { path: "~/.local/bin/agy.1727000000.old", what: "Backups" } })).toBe(
      "warnings.willTrash.Backups",
    );
    expect(warningKey({ WillKeep: { path: "~/.gemini/antigravity-cli", what: "ToolState" } })).toBe(
      "warnings.willKeep.ToolState",
    );
    expect(warningKey({ WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } })).toBe(
      "warnings.willKeep.ShellConfigLines",
    );
    expect(warningKey({ WillKeep: { path: "/usr/local/bin/grok", what: "OutsideHome" } })).toBe(
      "warnings.willKeep.OutsideHome",
    );
    expect(warningKey({ WillKeep: { path: "~/.local/bin/agent", what: "NotOurs" } })).toBe(
      "warnings.willKeep.NotOurs",
    );
    expect(warningKey({ WillKeep: { path: "~/.cache/antigravity", what: "InstallerCache" } })).toBe(
      "warnings.willKeep.InstallerCache",
    );
    expect(warningKey({ AlreadyGone: { path: "~/.local/share/claude" } })).toBe(
      "warnings.alreadyGone",
    );
  });

  it("gives each of rustup's uninstall warnings its key, and two of them a second key by payload", () => {
    // No toolchain names (the toolchains directory is missing or empty):
    // the sentence must not read "every toolchain ()" -- it drops the
    // parenthesis instead (spec §6.5). A startup-file line rustup will
    // not remove is "will print an error" only when it is one of the
    // sourcing forms rustup itself writes; any other mention is "may".
    expect(
      warningKey({ RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } }),
    ).toBe("warnings.removesToolchains");
    expect(warningKey({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toBe(
      "warnings.removesToolchainsUnlisted",
    );
    expect(warningKey({ DeletesCargoHome: { path: "~/.cargo" } })).toBe("warnings.deletesCargoHome");
    expect(warningKey({ RemovesCargoInstalled: { names: ["hexyl"] } })).toBe(
      "warnings.removesCargoInstalled",
    );
    expect(warningKey("HomebrewRustupLosesToolchains")).toBe("warnings.homebrewRustupLosesToolchains");
    expect(warningKey("EditsShellConfig")).toBe("warnings.editsShellConfig");
    expect(warningKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: true } })).toBe(
      "warnings.leavesShellConfigLine",
    );
    expect(warningKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toBe(
      "warnings.leavesShellConfigLineMaybe",
    );
  });

  it("has no key for a Message -- its text comes from the wire, not i18n", () => {
    expect(warningKey({ Message: "boom" })).toBeNull();
  });

  it("is null for Message and for nothing else", () => {
    // The runtime half of what `tsc` checks at compile time: every
    // variant of `Warning` is one of these fifteen, and the only one
    // without a `warnings.*` key is the raw-text catch-all. A variant this
    // list does not name is a `never` in `warningKey`'s default branches
    // and does not compile, so there is no "unrecognised variant" to test.
    const all: Warning[] = [
      "DependentsUnknown",
      "CompilesLocally",
      "NonRegistrySource",
      { WouldBreak: { names: ["a"] } },
      { ThirdPartyRegistry: { host: "modelscope.cn" } },
      { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
      { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
      { AlreadyGone: { path: "~/.local/share/claude" } },
      { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
      { DeletesCargoHome: { path: "~/.cargo" } },
      { RemovesCargoInstalled: { names: ["hexyl"] } },
      "HomebrewRustupLosesToolchains",
      "EditsShellConfig",
      { LeavesShellConfigLine: { path: "~/.zshrc", certain: true } },
      { Message: "boom" },
    ];
    const keyless = all.filter((warning) => warningKey(warning) === null);
    expect(keyless).toEqual([{ Message: "boom" }]);
  });
});

describe("warningArgs", () => {
  it("interpolates WouldBreak's names and count for pluralisation", () => {
    expect(warningArgs({ WouldBreak: { names: ["python@3.13"] } })).toEqual({
      count: 1,
      names: "python@3.13",
    });
    expect(warningArgs({ WouldBreak: { names: ["a", "b"] } })).toEqual({
      count: 2,
      names: "a, b",
    });
  });

  it("interpolates the registry host so the copy can name it in either language", () => {
    // The sentence used to be assembled in Rust, in English, and shown
    // verbatim -- including above the Uninstall button, to a zh-CN user
    // pulling from modelscope.cn.
    expect(warningArgs({ ThirdPartyRegistry: { host: "modelscope.cn" } })).toEqual({
      host: "modelscope.cn",
    });
  });

  it("interpolates rustup's two folders, the toolchain names, the cargo-installed programs with a count, and the startup file", () => {
    expect(
      warningArgs({
        RemovesToolchains: {
          path: "~/.rustup",
          names: ["stable-aarch64-apple-darwin", "nightly-aarch64-apple-darwin"],
        },
      }),
    ).toEqual({ path: "~/.rustup", names: "stable-aarch64-apple-darwin, nightly-aarch64-apple-darwin" });
    // No names: the unlisted key has no names slot, only the path.
    expect(warningArgs({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toEqual({
      path: "~/.rustup",
    });
    expect(warningArgs({ DeletesCargoHome: { path: "~/.cargo" } })).toEqual({ path: "~/.cargo" });
    expect(warningArgs({ RemovesCargoInstalled: { names: ["hexyl"] } })).toEqual({
      count: 1,
      names: "hexyl",
    });
    expect(warningArgs({ RemovesCargoInstalled: { names: ["hexyl", "rg"] } })).toEqual({
      count: 2,
      names: "hexyl, rg",
    });
    expect(warningArgs({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toEqual({
      path: "~/.zshrc",
    });
    expect(warningArgs("HomebrewRustupLosesToolchains")).toEqual({});
    expect(warningArgs("EditsShellConfig")).toEqual({});
  });

  it("interpolates the path a trash, keep or already-gone item names", () => {
    // The path arrives with `$HOME` already abbreviated to `~` on the Rust
    // side (`scan::display_path`): data for the sentence, not a path to
    // act on.
    expect(warningArgs({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toEqual({
      path: "~/.local/bin/claude",
    });
    expect(warningArgs({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toEqual({
      path: "~/.claude",
    });
    expect(warningArgs({ AlreadyGone: { path: "~/.local/share/claude" } })).toEqual({
      path: "~/.local/share/claude",
    });
  });

  it("is empty for every other variant", () => {
    expect(warningArgs("DependentsUnknown")).toEqual({});
    expect(warningArgs("CompilesLocally")).toEqual({});
    expect(warningArgs("NonRegistrySource")).toEqual({});
    expect(warningArgs({ Message: "boom" })).toEqual({});
  });
});

describe("warningMessage", () => {
  it("reads a Message's text straight off the wire", () => {
    expect(warningMessage({ Message: "installed from git, cannot check crates.io" })).toBe(
      "installed from git, cannot check crates.io",
    );
  });

  it("is null for every fixed, key-driven variant", () => {
    expect(warningMessage("DependentsUnknown")).toBeNull();
    expect(warningMessage("CompilesLocally")).toBeNull();
    expect(warningMessage("NonRegistrySource")).toBeNull();
    expect(warningMessage({ WouldBreak: { names: ["a"] } })).toBeNull();
    expect(warningMessage({ ThirdPartyRegistry: { host: "modelscope.cn" } })).toBeNull();
  });
});

describe("warningText", () => {
  it("looks a fixed warning up through t(), with its args", () => {
    expect(warningText(fakeT, "DependentsUnknown")).toBe("warnings.dependentsUnknown");
    expect(warningText(fakeT, { WouldBreak: { names: ["a", "b"] } })).toBe(
      'warnings.wouldBreak({"count":2,"names":"a, b"})',
    );
  });

  it("reads a Message's text directly, bypassing t()", () => {
    expect(warningText(fakeT, { Message: "boom" })).toBe("boom");
  });
});

/** Every variant of `Warning` once, for the checks that go over them all. */
const EVERY_VARIANT: Warning[] = [
  "DependentsUnknown",
  "CompilesLocally",
  "NonRegistrySource",
  "HomebrewRustupLosesToolchains",
  "EditsShellConfig",
  { WouldBreak: { names: ["a"] } },
  { ThirdPartyRegistry: { host: "modelscope.cn" } },
  { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
  { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
  { AlreadyGone: { path: "~/.local/share/claude" } },
  { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
  { DeletesCargoHome: { path: "~/.cargo" } },
  { RemovesCargoInstalled: { names: ["hexyl"] } },
  { LeavesShellConfigLine: { path: "~/.zshrc", certain: true } },
  { Message: "boom" },
];

/** A key's text in one locale, or undefined when it has none. */
function lookUp(resource: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (typeof node === "object" && node !== null ? (node as Record<string, unknown>)[part] : undefined),
    resource,
  );
}

describe("warningGroup", () => {
  it("puts what moves to the Trash, and what was already gone from there, in the Trash group", () => {
    expect(warningGroup({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toBe("trash");
    expect(warningGroup({ AlreadyGone: { path: "~/.local/share/claude" } })).toBe("trash");
  });

  it("puts every kind of kept path in the kept group", () => {
    const kinds: KeptWhat[] = [
      "Settings",
      "SettingsAndHistory",
      "ToolState",
      "ShellConfigLines",
      "OutsideHome",
      "NotOurs",
      "InstallerCache",
    ];
    for (const what of kinds) {
      expect(warningGroup({ WillKeep: { path: "~/x", what } })).toBe("keep");
    }
  });

  it("puts everything else under what to know before going on", () => {
    const notes = EVERY_VARIANT.filter(
      (warning) =>
        typeof warning === "string" ||
        !("WillTrash" in warning || "AlreadyGone" in warning || "WillKeep" in warning),
    );
    expect(notes).toHaveLength(12);
    for (const warning of notes) expect(warningGroup(warning)).toBe("note");
  });

  it("makes a variant this build does not know something to note, not something to drop", () => {
    expect(warningGroup("SomeFutureVariant" as unknown as Warning)).toBe("note");
    expect(warningGroup({ SomeFutureVariant: {} } as unknown as Warning)).toBe("note");
  });
});

describe("warningDetailKey", () => {
  it("keeps the why of a kept path behind the line, except for your own settings", () => {
    expect(warningDetailKey({ WillKeep: { path: "~/.claude.json", what: "Settings" } })).toBeNull();
    expect(warningDetailKey({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toBeNull();
    expect(warningDetailKey({ WillKeep: { path: "~/.gemini/antigravity-cli", what: "ToolState" } })).toBe(
      "warnings.willKeep.ToolStateDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } })).toBe(
      "warnings.willKeep.ShellConfigLinesDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "/usr/local/bin/grok", what: "OutsideHome" } })).toBe(
      "warnings.willKeep.OutsideHomeDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "~/.local/bin/agent", what: "NotOurs" } })).toBe(
      "warnings.willKeep.NotOursDetail",
    );
    expect(warningDetailKey({ WillKeep: { path: "~/.cache/antigravity", what: "InstallerCache" } })).toBe(
      "warnings.willKeep.InstallerCacheDetail",
    );
  });

  it("keeps what rustup's deletions mean behind the line, one why for both toolchain sentences", () => {
    expect(warningDetailKey({ RemovesToolchains: { path: "~/.rustup", names: ["stable"] } })).toBe(
      "warnings.removesToolchainsDetail",
    );
    expect(warningDetailKey({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toBe(
      "warnings.removesToolchainsDetail",
    );
    expect(warningDetailKey({ RemovesCargoInstalled: { names: ["hexyl"] } })).toBe(
      "warnings.removesCargoInstalledDetail",
    );
    expect(warningDetailKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: true } })).toBe(
      "warnings.leavesShellConfigLineDetail",
    );
    expect(warningDetailKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toBe(
      "warnings.leavesShellConfigLineMaybeDetail",
    );
  });

  it("has nothing behind a line that already says it all", () => {
    for (const warning of [
      // That the whole Cargo folder goes, and none of it to the Trash, is
      // on the line itself.
      { DeletesCargoHome: { path: "~/.cargo" } },
      "DependentsUnknown",
      "CompilesLocally",
      "NonRegistrySource",
      "HomebrewRustupLosesToolchains",
      "EditsShellConfig",
      { WouldBreak: { names: ["a"] } },
      { ThirdPartyRegistry: { host: "modelscope.cn" } },
      { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
      { AlreadyGone: { path: "~/.local/share/claude" } },
      { Message: "boom" },
    ] as Warning[]) {
      expect(warningDetailKey(warning)).toBeNull();
    }
  });

  it("names only keys both languages have", () => {
    const keys = [
      ...EVERY_VARIANT,
      ...(["ToolState", "ShellConfigLines", "OutsideHome", "NotOurs", "InstallerCache"] as KeptWhat[]).map(
        (what): Warning => ({ WillKeep: { path: "~/x", what } }),
      ),
      { LeavesShellConfigLine: { path: "~/.zshrc", certain: false } },
    ]
      .map(warningDetailKey)
      .filter((key): key is string => key !== null);
    expect(keys.length).toBeGreaterThan(0);
    for (const key of keys) {
      expect(typeof lookUp(en, key), `en: ${key}`).toBe("string");
      expect(typeof lookUp(zhCN, key), `zh-CN: ${key}`).toBe("string");
    }
  });
});

describe("deletesForGood", () => {
  it("is true of rustup's permanent deletions, and of nothing else", () => {
    const forGood = EVERY_VARIANT.filter(deletesForGood);
    expect(forGood).toEqual([
      { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
      { DeletesCargoHome: { path: "~/.cargo" } },
      { RemovesCargoInstalled: { names: ["hexyl"] } },
    ]);
    expect(deletesForGood({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toBe(true);
    // What goes to the Trash can be dragged back out.
    expect(deletesForGood({ WillTrash: { path: "~/.local/share/claude", what: "Program" } })).toBe(false);
  });

  it("is false of a variant this build does not know", () => {
    expect(deletesForGood("SomeFutureVariant" as unknown as Warning)).toBe(false);
    expect(deletesForGood({ SomeFutureVariant: {} } as unknown as Warning)).toBe(false);
  });
});

describe("warningLines", () => {
  it("renders every warning in order, in its group, with its why where it has one", () => {
    const lines = warningLines(fakeT, [
      { WillTrash: { path: "~/.local/share/claude", what: "Program" } },
      { WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } },
      "DependentsUnknown",
      { AlreadyGone: { path: "~/.grok/downloads" } },
      { Message: "boom" },
      { WillKeep: { path: "~/.claude.json", what: "Settings" } },
    ]);
    expect(lines).toEqual({
      trash: [
        { text: 'warnings.willTrash.Program({"path":"~/.local/share/claude"})', detail: null },
        { text: 'warnings.alreadyGone({"path":"~/.grok/downloads"})', detail: null },
      ],
      keep: [
        {
          text: 'warnings.willKeep.ShellConfigLines({"path":"~/.zshrc"})',
          detail: "warnings.willKeep.ShellConfigLinesDetail",
        },
        { text: 'warnings.willKeep.Settings({"path":"~/.claude.json"})', detail: null },
      ],
      note: [
        { text: "warnings.dependentsUnknown", detail: null },
        { text: "boom", detail: null },
      ],
    });
  });

  it("does not say again what the plan's own list of what still needs it says", () => {
    // Homebrew's preview fills `WouldBreak` and `affected` from one
    // `brew uses`; the confirmation shows `affected` as its own list.
    const warnings: Warning[] = ["DependentsUnknown", { WouldBreak: { names: ["wget"] } }];
    expect(warningLines(fakeT, warnings, ["wget"]).note).toEqual([
      { text: "warnings.dependentsUnknown", detail: null },
    ]);
    // Without that list, it is the only place the names are said.
    expect(warningLines(fakeT, warnings).note.map((line) => line.text)).toEqual([
      "warnings.dependentsUnknown",
      'warnings.wouldBreak({"count":1,"names":"wget"})',
    ]);
  });

  it("is empty for an empty list", () => {
    expect(warningLines(fakeT, [])).toEqual({ trash: [], keep: [], note: [] });
  });
});
